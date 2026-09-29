//! Campaign memory and a constrained, engine-owned game-master vocabulary.
use crate::domain::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TurnMemory {
    pub revision: i64,
    pub location: String,
    pub action: String,
    pub outcome: String,
    pub narration: String,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CampaignMemory {
    pub recent: Vec<TurnMemory>,
    pub archive: Vec<TurnMemory>,
    pub summaries: Vec<String>,
    pub turns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StoryThread {
    pub id: String,
    pub title: String,
    pub question: String,
    pub clock: u32,
    pub urgency: u32,
    pub resolved: bool,
    pub required_clues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Clue {
    pub id: String,
    pub location: String,
    pub text: String,
    pub requires: Vec<String>,
    pub discovered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Campaign {
    pub title: String,
    pub setting: String,
    pub tone: String,
    pub boundaries: String,
    pub scene: String,
    pub scene_turns: u32,
    pub threads: BTreeMap<String, StoryThread>,
    pub clues: BTreeMap<String, Clue>,
    pub exits: BTreeMap<String, Vec<String>>,
    pub bell: Option<crate::bell::BellAdventure>,
}
impl Default for Campaign {
    fn default() -> Self {
        Self { title: "Kampania".into(), setting: "Mroczne fantasy inspirowane światem Wiedźmina".into(),
            tone: "Przygodowy, konkretny; decyzje należą do gracza".into(), boundaries: String::new(),
            scene: "opening".into(), scene_turns: 0, threads: BTreeMap::new(), clues: BTreeMap::new(), exits: BTreeMap::new(), bell: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct TurnPlan {
    pub intent_kind: String,
    pub target: String,
    pub npc_moves: Vec<NpcMove>,
    pub thread_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcMove {
    pub npc_id: String,
    pub kind: NpcMoveKind,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcMoveKind { Ask, Help, Refuse, Warn }

pub fn short(s: &str, n: usize) -> String { s.chars().take(n).collect() }
fn words(s: &str) -> BTreeSet<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 3)
        .map(|w| w.chars().take(5).collect()).collect()
}

impl CampaignMemory {
    pub fn record(&mut self, turn: TurnMemory) {
        self.turns += 1;
        self.recent.push(turn);
        if self.recent.len() > 12 {
            let mut old = self.recent.remove(0);
            old.narration = short(&old.narration, 700);
            old.action = short(&old.action, 400);
            self.archive.push(old);
        }
        // Compact only old entries. Original complete turns also remain in SQLite events.
        if self.archive.len() > 256 {
            let group: Vec<_> = self.archive.drain(..16).collect();
            let summary = group.iter().map(|t| format!("T{} [{}]: deklaracja: {}; wynik: {}; zdarzenia: {}",
                t.revision, t.location, short(&t.action, 120), t.outcome, t.events.join("; "))).collect::<Vec<_>>().join("\n");
            self.summaries.push(summary);
            if self.summaries.len() > 64 { self.summaries.remove(0); }
        }
    }
    pub fn context(&self, query: &str) -> Value {
        let q = words(query);
        let mut scored: Vec<_> = self.archive.iter().map(|m| {
            let text = format!("{} {} {}", m.action, m.location, m.events.join(" "));
            (words(&text).intersection(&q).count(), m)
        }).filter(|(score,_)| *score > 0).collect();
        scored.sort_by(|a,b| b.0.cmp(&a.0).then(b.1.revision.cmp(&a.1.revision)));
        let recall: Vec<_> = scored.into_iter().take(6).map(|(_,m)|m).collect();
        let recent: Vec<_> = self.recent.iter().rev().take(6).rev().collect();
        json!({"recent_turns":recent,"relevant_older_turns":recall,
            "session_summaries":self.summaries.iter().rev().take(2).collect::<Vec<_>>(),
            "rule":"Actions are attempts, narration is prose; only engine state and approved events establish facts."})
    }
}

pub fn validate_plan(state: &GameState, plan: &TurnPlan) -> Result<(), String> {
    if !["observe","social","investigation","travel","combat_action","magic_attempt","rest","freeform"].contains(&plan.intent_kind.as_str()) {
        return Err("Unknown intent category".into());
    }
    if plan.target.chars().count() > 160 || plan.npc_moves.len() > 2 { return Err("Plan exceeds bounds".into()); }
    if plan.intent_kind == "travel" && !plan.target.is_empty() && !state.campaign.exits.get(&state.world.location).map(|v|v.contains(&plan.target)).unwrap_or(false) {
        return Err("Travel target is not an established adjacent location".into());
    }
    if !plan.thread_id.is_empty() && !state.campaign.threads.get(&plan.thread_id).map(|t|!t.resolved).unwrap_or(false) { return Err("Unknown or closed thread".into()); }
    let mut seen = BTreeSet::new();
    for m in &plan.npc_moves {
        if !seen.insert(&m.npc_id) { return Err("Duplicate NPC move".into()); }
        if !state.world.npcs.get(&m.npc_id).map(|n|n.active && n.location == state.world.location).unwrap_or(false) { return Err("NPC is not present".into()); }
    }
    Ok(())
}

pub fn fallback_plan(state: &GameState, action: &PlayerAction) -> TurnPlan {
    let mut intent = crate::systems::infer_intent(&action.text);
    let l = action.text.to_lowercase();
    if l.contains("odpoczy") || l.contains("czekam") { intent.kind = "rest".into(); }
    else if l.contains("rozglądam") || l.contains("rozejrz") { intent.kind = "observe".into(); }
    let target = if intent.kind == "travel" {
        state.campaign.exits.get(&state.world.location).into_iter().flatten().find(|p| l.contains(&p.to_lowercase())).cloned().unwrap_or_default()
    } else { String::new() };
    TurnPlan { intent_kind:intent.kind, target, npc_moves:vec![], thread_id:String::new() }
}

pub fn intent_from_plan(action: &PlayerAction, plan: &TurnPlan) -> Intent {
    let mut inferred = crate::systems::infer_intent(&action.text);
    // A model cannot turn an explicit attack or spell into a cost-free observation.
    if !inferred.magical && inferred.kind != "combat_action" { inferred.kind = plan.intent_kind.clone(); }
    inferred.magical = inferred.kind == "magic_attempt";
    inferred.target = plan.target.clone();
    inferred
}

/// Apply only engine-defined moves. AI supplies IDs/enums, never arbitrary patches or secrets.
pub fn apply_turn(state: &mut GameState, plan: &TurnPlan, action: &PlayerAction, intent: &Intent, resolution: &mut Resolution) -> Vec<String> {
    let mut events = Vec::new();
    state.campaign.scene_turns += 1;
    state.campaign.scene = match intent.kind.as_str() {
        "combat_action" => "confrontation", "social" => "negotiation", "investigation"|"magic_attempt" => "investigation", "rest" => "aftermath", _ => "exploration"
    }.into();
    if intent.kind == "travel" && !plan.target.is_empty() && state.campaign.exits.get(&state.world.location).map(|v|v.contains(&plan.target)).unwrap_or(false) {
        state.world.location = plan.target.clone();
        state.campaign.scene_turns = 0;
        events.push(format!("Docierasz do miejsca: {}.", state.world.location));
    }
    if (intent.kind == "investigation" || (intent.magical && intent.method == "sensing")) && resolution.ok {
        let found = state.campaign.clues.values().find(|c| !c.discovered && c.location == state.world.location && c.requires.iter().all(|id| state.campaign.clues.get(id).map(|c|c.discovered).unwrap_or(false))).map(|c|c.id.clone());
        if let Some(id) = found {
            let clue = state.campaign.clues.get_mut(&id).unwrap();
            clue.discovered = true;
            resolution.revealed.push(clue.text.clone());
            state.character.knowledge.push(KnowledgeFact { id:format!("clue:{id}"), statement:clue.text.clone(), source:"investigation".into(), confidence:100, canon:false });
            events.push(format!("Odkryta wskazówka: {}", clue.text));
        }
    }
    for m in &plan.npc_moves {
        if let Some(npc) = state.world.npcs.get_mut(&m.npc_id).filter(|n|n.active && n.location == state.world.location) {
            let (verb, delta) = match m.kind { NpcMoveKind::Ask => ("prosi o wyjaśnienie twoich zamiarów",0), NpcMoveKind::Help => ("deklaruje chęć pomocy",1), NpcMoveKind::Refuse => ("odmawia współpracy",-1), NpcMoveKind::Warn => ("ostrzega przed dalszym ryzykiem",0) };
            let event = format!("{} {}.",npc.name,verb);
            events.push(event.clone());
            let r = npc.relations.entry("player".into()).or_default();
            r.trust = (r.trust + delta).clamp(-100,100);
            npc.memories.push(NpcMemory { id:format!("turn:{}",state.revision),summary:format!("Gracz próbował: {}. Reakcja: {}",short(&action.text,240),verb),importance:2,valence:delta,at:state.world.clock.clone() });
            if npc.memories.len()>24 { npc.memories.remove(0); }
        }
    }
    let discovered: BTreeSet<_> = state.campaign.clues.values().filter(|c|c.discovered).map(|c|c.id.clone()).collect();
    for thread in state.campaign.threads.values_mut().filter(|t|!t.resolved) {
        thread.clock += 1;
        if state.campaign.bell.is_some() && thread.id=="missing-courier" {continue;}
        if !thread.required_clues.is_empty() && thread.required_clues.iter().all(|id|discovered.contains(id)) {
            thread.resolved = true;
            events.push(format!("Zebrane wskazówki pozwalają rozwiązać wątek: {}.",thread.title));
            if let Some(q)=state.world.quests.get_mut(&thread.id) { q.status="completed".into(); q.stage+=1; }
        } else if thread.clock % 6 == 0 {
            thread.urgency = (thread.urgency+1).min(5);
            events.push(format!("Sprawa „{}” staje się pilniejsza; czas działa na niekorzyść bohatera.",thread.title));
        }
    }
    state.director.scene_goal = state.campaign.threads.get(&plan.thread_id).filter(|t|!t.resolved)
        .or_else(||state.campaign.threads.values().filter(|t|!t.resolved).max_by_key(|t|t.urgency))
        .map(|t|t.question.clone()).unwrap_or_else(||"Pokaż konsekwencje ostatnich decyzji i pozostaw wybór graczowi.".into());
    state.director.tension = if state.campaign.scene == "aftermath" { (state.director.tension-10).max(0) } else { (state.director.tension+2).min(100) };
    events
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct NewCampaign {
    pub title: String,
    pub character_name: String,
    pub tone: String,
    pub boundaries: String,
}

pub fn new_campaign(req: NewCampaign) -> GameState {
    let mut s = GameState::default();
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    s.campaign_id=format!("campaign-{stamp}");
    s.campaign.title=if req.title.trim().is_empty(){"Dzwon nad brodem".into()}else{short(req.title.trim(),120)};
    if !req.character_name.trim().is_empty(){s.character.name=short(req.character_name.trim(),80);}
    if !req.tone.trim().is_empty(){s.campaign.tone=short(&req.tone,240);}
    s.campaign.boundaries=short(&req.boundaries,1000);
    s.world.location="Gospoda nad brodem".into();
    s.campaign.bell=Some(crate::bell::BellAdventure::default());
    s.campaign.exits=BTreeMap::from([
        ("Gospoda nad brodem".into(),vec!["Stary most".into(),"Młyn".into()]),
        ("Stary most".into(),vec!["Gospoda nad brodem".into(),"Młyn".into()]),
        ("Młyn".into(),vec!["Stary most".into(),"Gospoda nad brodem".into()])]);
    for (id,name,role,personality,goal,location,secret) in [
        ("marta","Marta","gospodyni","ostrożna, rzeczowa","odnaleźć zaginionego kuriera","Gospoda nad brodem","Ukryła list kuriera przed poborcą."),
        ("bor","Bor","strażnik mostu","dumny, zmęczony","utrzymać bezpieczną przeprawę","Stary most","Przepuścił wóz bez sprawdzenia ładunku."),
        ("ela","Ela","młynarka","spostrzegawcza, nieufna","ochronić rodzinę","Młyn","Pomaga rannemu kurierowi ukrytemu na strychu.")
    ] {
        s.world.npcs.insert(id.into(),Npc { id:id.into(),name:name.into(),role:role.into(),personality:personality.into(),public_goal:goal.into(),secret:secret.into(),location:location.into(),active:true,plan:vec!["sprawdzić najbliższe otoczenie".into()],..Default::default() });
    }
    for (id,name,role,location,goal,active) in [
        ("jan","Jan","kurier","Młyn","odzyskać bezpieczeństwo",false),
        ("poborca","Olgierd","poborca myta","Stary most","uzyskać poręczenie lub zatrzymać dłużnika",false)
    ] {
        s.world.npcs.insert(id.into(),Npc{id:id.into(),name:name.into(),role:role.into(),location:location.into(),public_goal:goal.into(),active,..Default::default()});
    }
    for (id,location,text,requires) in [
        ("tracks","Stary most","Ślady wozu prowadzą spod mostu do młyna; na poręczy została tkanina kuriera.",vec![]),
        ("letter","Gospoda nad brodem","Na stole pod rachunkami leży list: kurier przewoził lekarstwo dla młynarki.",vec![]),
        ("courier","Młyn","Za workami widzisz rannego, żywego kuriera. Potwierdza, że uciekł przed poborcą.",vec!["tracks".into()])
    ] { s.campaign.clues.insert(id.into(),Clue{id:id.into(),location:location.into(),text:text.into(),requires,discovered:false}); }
    let id="missing-courier";
    s.campaign.threads.insert(id.into(),StoryThread{id:id.into(),title:"Zaginiony kurier".into(),question:"Co stało się z kurierem i komu można zaufać?".into(),required_clues:vec!["tracks".into(),"courier".into()],urgency:1,..Default::default()});
    s.world.quests.insert(id.into(),Quest{id:id.into(),title:"Zaginiony kurier".into(),status:"active".into(),summary:"Marta prosi o ustalenie losu kuriera.".into(),..Default::default()});
    s.director.scene_goal="Poznaj Martę i wybierz pierwszy trop.".into();
    s.last_narration="Deszcz bębni o dach gospody nad brodem. Marta odkłada nietkniętą miskę. «Kurier miał wrócić przed zmrokiem. Strażnik widział go przy starym moście, ale młyn też dziś milczy». Przesuwa ku tobie lampę i czeka na odpowiedź. Co robisz?".into();
    s.last_suggestions=vec!["Pytam Martę, co mogę zrobić dla kuriera".into(),"Idę do miejsca Stary most".into(),"Badam pozostawione na stole rzeczy".into()];
    s
}

pub fn public_campaign(s: &GameState) -> Value {
    json!({"title":s.campaign.title,"tone":s.campaign.tone,"scene":s.campaign.scene,
        "threads":s.campaign.threads.values().map(|t|json!({"id":t.id,"title":t.title,"question":t.question,"urgency":t.urgency,"resolved":t.resolved})).collect::<Vec<_>>(),
        "clues":s.campaign.clues.values().filter(|c|c.discovered).map(|c|json!({"id":c.id,"text":c.text})).collect::<Vec<_>>(),
        "exits":s.campaign.exits.get(&s.world.location).cloned().unwrap_or_default(),
        "adventure":crate::bell::public(s),
        "turns":s.memory.turns})
}
