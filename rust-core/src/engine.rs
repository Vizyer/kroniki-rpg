use crate::ai::{local_fallback, plan_turn, propose_with_model, AiConfig};
use crate::domain::*;
use crate::dm::{self, TurnMemory, TurnPlan};
use crate::systems::{apply_patch, default_mechanical_patch, resolve_action, simulate_background};
use crate::store::Store;
use crate::runtime::LocalAiRuntime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, watch};
use std::time::Duration;

#[derive(Clone)]
pub struct Engine {
    pub state: Arc<RwLock<GameState>>,
    pub store: Store,
    pub ai_config: Arc<RwLock<AiConfig>>,
    pub action_lock: Arc<Mutex<()>>,
    pub local_ai: LocalAiRuntime,
    cancelled: watch::Sender<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub ok: bool,
    pub source: String,
    pub intent: Intent,
    pub resolution: Resolution,
    pub narration: String,
    pub suggestions: Vec<String>,
    pub state: Value,
    pub ai_error: Option<String>,
}

impl Engine {
    pub fn new(store: Store) -> Self {
        let cfg = store.get_ai_config();
        let state = store.latest().expect("Autosave cannot be read; refusing to overwrite it").unwrap_or_default();
        let (cancelled, _) = watch::channel(String::new());
        Self { state:Arc::new(RwLock::new(state)),store,ai_config:Arc::new(RwLock::new(cfg)),
            action_lock:Arc::new(Mutex::new(())),local_ai:LocalAiRuntime::default(),cancelled }
    }
    pub fn cancel(&self, id: &str) {
        if !id.is_empty() { self.cancelled.send_replace(id.to_string()); }
    }
    pub async fn act(&self, action: PlayerAction) -> Result<ActionResult,String> {
        self.act_with_id(action, format!("internal-{:?}",std::time::SystemTime::now())).await
    }
    pub async fn act_with_id(&self, action: PlayerAction, id: String) -> Result<ActionResult,String> {
        if action.text.trim().is_empty() || action.text.chars().count()>4000 { return Err("Akcja musi mieć od 1 do 4000 znaków.".into()); }
        let mut cancel = self.cancelled.subscribe();
        let _single = self.action_lock.lock().await;
        let cancellation = async {
            loop {
                if !id.is_empty() && *cancel.borrow_and_update() == id { return; }
                if cancel.changed().await.is_err() { return; }
            }
        };
        tokio::select! {
            biased;
            _ = cancellation => Err("Tura anulowana przed zatwierdzeniem.".into()),
            result = tokio::time::timeout(Duration::from_secs(150), self.prepare_turn(&action)) => {
                match result {
                    Ok(result) => result,
                    Err(_) => Err("Przekroczono czas odpowiedzi MG. Tura nie została zapisana; spróbuj ponownie.".into()),
                }
            }
        }
    }

    async fn prepare_turn(&self, action: &PlayerAction) -> Result<ActionResult,String> {
        let snapshot = self.state.read().await.clone();
        if snapshot.campaign.bell.as_ref().and_then(|b|b.outcome.as_ref()).is_some() {return Err("Ta przygoda ma już zakończenie. Wczytaj wcześniejszy zapis albo rozpocznij nową kampanię.".into());}
        let cfg = self.ai_config.read().await.clone();
        let mut errors=Vec::new();
        let ready = if cfg.mode == "off" { false }
            else if cfg.mode == "local" {
                match self.local_ai.ensure_ready().await { Ok(true)=>true,Ok(false)=>{errors.push("Brak lokalnego modelu; pobierz go w launcherze.".into());false},Err(e)=>{errors.push(e);false} }
            } else { true };
        let plan = if crate::bell::resolution(&snapshot,action).is_some() {
            dm::fallback_plan(&snapshot,action)
        } else if ready {
            match plan_turn(&cfg,&snapshot,action).await { Ok(p)=>p,Err(e)=>{errors.push(e);dm::fallback_plan(&snapshot,action)} }
        } else { dm::fallback_plan(&snapshot,action) };
        let (mut next,intent,resolution)=Self::resolve_turn(&snapshot,action,&plan)?;
        let (proposal,source)=if ready {
            let narration=tokio::time::timeout(Duration::from_secs(65),propose_with_model(&cfg,&next,action,&resolution)).await
                .unwrap_or_else(|_|Err("Model nie zakończył narracji w 65 sekund; użyto narracji awaryjnej.".into()));
            match narration {
                Ok(p)=>(p,"model"),Err(e)=>{errors.push(e);(local_fallback(&next,action,&resolution),"local_fallback")}
            }
        } else { (local_fallback(&next,action,&resolution),"local_fallback") };
        // Narrator prose cannot supply a patch. Mechanics and approved moves were applied above.
        next.last_narration=proposal.narration.clone();
        next.last_suggestions=proposal.suggestions.clone();
        if source=="local_fallback" {
            next.last_suggestions=next.campaign.exits.get(&next.world.location).into_iter().flatten().take(2)
                .map(|p|format!("Idę do miejsca {p}")).chain(["Badam otoczenie w poszukiwaniu wskazówek".into(),"Rozglądam się".into()]).collect();
        }
        if next.campaign.bell.is_some() {
            let mut suggestions:Vec<String>=crate::bell::choices(&next).iter().map(|c|c.text.to_string()).collect();
            suggestions.extend(next.campaign.exits.get(&next.world.location).into_iter().flatten().map(|p|format!("Idę do miejsca {p}")));
            next.last_suggestions=suggestions;
            if let Some(b)=next.campaign.bell.as_ref().filter(|b|b.outcome.is_some()) {
                next.last_narration=b.epilogue.clone();next.last_suggestions.clear();
            }
        }
        let turn=TurnMemory {revision:next.revision,location:next.world.location.clone(),action:action.text.clone(),outcome:resolution.degree.clone(),
            narration:dm::short(&next.last_narration,3000),events:next.director.recent_events.clone()};
        next.memory.record(turn.clone());
        next.world.chronicle.push(format!("T{}: {} → {}",next.revision,dm::short(&action.text,400),resolution.degree));
        if next.world.chronicle.len()>256 { let n=next.world.chronicle.len()-256;next.world.chronicle.drain(..n); }
        // No await between persistent commit and in-memory replacement: cancellation cannot split them.
        let mut current=self.state.write().await;
        if current.revision != snapshot.revision || current.campaign_id != snapshot.campaign_id { return Err("Stan kampanii zmienił się podczas tury.".into()); }
        self.store.checkpoint(&next,"turn",&serde_json::to_string(&turn).map_err(|e|e.to_string())?)?;
        *current=next.clone();
        Ok(ActionResult{ok:true,source:source.into(),intent,resolution,narration:next.last_narration.clone(),suggestions:next.last_suggestions.clone(),
            state:state_summary(&next),ai_error:if errors.is_empty(){None}else{Some(errors.join("; "))}})
    }

    pub fn resolve_turn(snapshot: &GameState, action: &PlayerAction, plan: &TurnPlan) -> Result<(GameState,Intent,Resolution),String> {
        dm::validate_plan(snapshot,plan)?;
        let intent=dm::intent_from_plan(action,plan);
        let mut resolution=crate::bell::resolution(snapshot,action).unwrap_or_else(||resolve_action(snapshot,action,&intent));
        let mut next=snapshot.clone();
        apply_patch(&mut next,&default_mechanical_patch(&resolution))?;
        // Time was advanced by mechanics; simulate background without advancing it twice.
        let clock=next.world.clock.clone();
        let mut events=simulate_background(&mut next,i64::from((resolution.duration_seconds.max(1)+59)/60));
        next.world.clock=clock;
        events.extend(dm::apply_turn(&mut next,plan,action,&intent,&mut resolution));
        events.extend(crate::bell::apply_choice(&mut next,snapshot,action,&mut resolution));
        if next.campaign.bell.as_ref().and_then(|b|b.outcome.as_ref()).is_some() {next.campaign.scene="epilogue".into();next.director.scene_goal="Przygoda zakończona. Opisz zapisany epilog, nie otwieraj ponownie sprawy.".into();}
        for f in &resolution.revealed {
            if !next.character.knowledge.iter().any(|k|k.statement==*f) {
                next.character.knowledge.push(KnowledgeFact{id:format!("reveal:{}:{}",next.revision,next.character.knowledge.len()),statement:f.clone(),source:"observation".into(),confidence:80,canon:false});
            }
        }
        next.director.recent_events=events;
        Ok((next,intent,resolution))
    }

    pub async fn tick(&self, minutes:i64) -> Result<Vec<String>,String> {
        let _single=self.action_lock.lock().await;
        let mut current=self.state.write().await;
        let mut next=current.clone();
        let events=simulate_background(&mut next,minutes.clamp(0,1440));
        next.revision+=1;
        next.director.recent_events=events.clone();
        self.store.checkpoint(&next,"tick",&format!("{minutes}"))?;
        *current=next;
        Ok(events)
    }

    pub async fn mutate<T, F>(&self, kind:&str, change:F) -> Result<T,String>
    where F: FnOnce(&mut GameState) -> Result<T,String> {
        let _single=self.action_lock.lock().await;
        let mut current=self.state.write().await;
        let mut next=current.clone();
        let old_clock=&current.world.clock;
        let ordinal=|c:&Clock| (((i64::from(c.year)*12+i64::from(c.month))*30+i64::from(c.day))*24+i64::from(c.hour))*60+i64::from(c.minute);
        let before=ordinal(old_clock);
        let result=change(&mut next)?;
        let elapsed=(ordinal(&next.world.clock)-before).max(0);
        let events=crate::bell::advance(&mut next,elapsed);
        if !events.is_empty(){next.director.recent_events=events;}

        next.revision+=1;
        self.store.checkpoint(&next,kind,"")?;
        *current=next;
        Ok(result)
    }

    pub async fn replace(&self, mut next:GameState, kind:&str) -> Result<(),String> {
        let _single=self.action_lock.lock().await;
        let mut current=self.state.write().await;
        next.revision=next.revision.max(current.revision)+1;
        self.store.checkpoint(&next,kind,"")?;
        *current=next;
        Ok(())
    }
}
