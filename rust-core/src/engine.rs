use crate::ai::{local_fallback, propose_with_model, AiConfig};
use crate::domain::*;
use crate::systems::{apply_patch, infer_intent, resolve_action, simulate_background, validate_ai_patch};
use crate::store::Store;
use crate::runtime::LocalAiRuntime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct Engine {
    pub state: Arc<RwLock<GameState>>,
    pub store: Store,
    pub ai_config: Arc<RwLock<AiConfig>>,
    pub action_lock: Arc<Mutex<()>>,
    pub local_ai: LocalAiRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub ok: bool,
    pub source: String,
    pub intent: Intent,
    pub resolution: Resolution,
    pub narration: String,
    pub suggestions: Vec<String>,
    pub state: GameState,
    pub ai_error: Option<String>,
}

impl Engine {
    pub fn new(store: Store) -> Self {
        let cfg = store.get_ai_config();
        Self {
            state: Arc::new(RwLock::new(GameState::default())),
            store,
            ai_config: Arc::new(RwLock::new(cfg)),
            action_lock: Arc::new(Mutex::new(())),
            local_ai: LocalAiRuntime::default(),
        }
    }

    pub async fn act(&self, action: PlayerAction) -> Result<ActionResult, String> {
        let _single = self.action_lock.lock().await;
        if action.text.trim().is_empty() {
            return Err("Pusta akcja gracza".into());
        }

        let snapshot = self.state.read().await.clone();
        let intent = infer_intent(&action.text);
        let resolution = resolve_action(&snapshot, &action, &intent);
        let cfg = self.ai_config.read().await.clone();
        if cfg.mode == "local" {
            let _ = self.local_ai.ensure_started().await;
        }

        let (mut proposal, source, ai_error) =
            match propose_with_model(&cfg, &snapshot, &action, &resolution).await {
                Ok(p) => (p, "model".to_string(), None),
                Err(e) => (
                    local_fallback(&snapshot, &action, &resolution),
                    "local_fallback".to_string(),
                    Some(e),
                ),
            };

        proposal.interpretation = intent.clone();
        if proposal.patch.ops.is_empty() {
            proposal.patch = crate::systems::default_mechanical_patch(&resolution);
        }
        validate_ai_patch(&snapshot, &proposal)?;

        let mut next = snapshot.clone();
        apply_patch(&mut next, &proposal.patch)?;
        let director_events = crate::director::advance(&mut next);
        if !director_events.is_empty() {
            next.world.chronicle.extend(director_events);
        }
        next.last_narration = proposal.narration.clone();
        next.last_suggestions = proposal.suggestions.clone();

        for f in &resolution.revealed {
            if !next.character.knowledge.iter().any(|k| k.statement == *f) {
                next.character.knowledge.push(KnowledgeFact {
                    id: format!("reveal:{}:{}", next.revision, next.character.knowledge.len()),
                    statement: f.clone(),
                    source: "observation".into(),
                    confidence: 80,
                    canon: false,
                });
            }
        }

        *self.state.write().await = next.clone();
        self.store.event(&next, "action", &action.text);

        Ok(ActionResult {
            ok: true,
            source,
            intent,
            resolution,
            narration: proposal.narration,
            suggestions: proposal.suggestions,
            state: next,
            ai_error,
        })
    }

    pub async fn tick(&self, minutes: i64) -> Vec<String> {
        let mut s = self.state.write().await;
        let mut events = simulate_background(&mut s, minutes);
        let director_events = crate::director::advance(&mut s);
        if !director_events.is_empty() {
            s.world.chronicle.extend(director_events.clone());
            events.extend(director_events);
        }
        self.store.event(&s, "tick", &format!("{minutes}"));
        events
    }
}
