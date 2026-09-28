use crate::domain::*;
use crate::lore::{self, LoreQuery};
use crate::systems::{default_mechanical_patch, infer_intent, magic_semantics};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub mode: String,
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u64,
    pub max_context_tokens: usize,
    pub thinking: bool,
    pub max_output_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub retry_invalid_json: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            mode: "local".into(),
            endpoint: "http://127.0.0.1:8080/v1/chat/completions".into(),
            model: "Qwen3-8B-Q5_K_M".into(),
            timeout_ms: 30_000,
            max_context_tokens: 14_000,
            thinking: true,
            max_output_tokens: 720,
            temperature: 0.72,
            top_p: 0.90,
            retry_invalid_json: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct NarratorReply {
    narration: String,
    suggestions: Vec<String>,
}

/// Builds a narrator-safe context. The local model gets enough information to
/// maintain continuity and play present NPCs, but it does not receive raw
/// World Truth, Director hidden facts, hidden quest objectives, or arbitrary
/// future lore. Hidden state stays in Rust; the model only sees mechanically
/// revealed information and character-safe lore.
pub fn build_context(state: &GameState, action: &PlayerAction, resolution: &Resolution) -> Value {
    let visible_facts: Vec<_> = state
        .character
        .knowledge
        .iter()
        .rev()
        .take(16)
        .cloned()
        .collect();

    let active_npcs: Vec<_> = state
        .world
        .npcs
        .values()
        .filter(|n| n.active && n.location == state.world.location)
        .map(|n| {
            let recent_memories: Vec<_> = n
                .memories
                .iter()
                .take(4)
                .map(|m| json!({
                    "summary": m.summary,
                    "importance": m.importance,
                    "valence": m.valence,
                }))
                .collect();
            json!({
                "id": n.id,
                "name": n.name,
                "role": n.role,
                "personality": n.personality,
                "public_goal": n.public_goal,
                "relation_to_player": n.relations.get("player"),
                "known_facts": n.knowledge.iter().take(8).collect::<Vec<_>>(),
                "recent_memories": recent_memories,
            })
        })
        .collect();

    let active_quests: Vec<_> = state
        .world
        .quests
        .values()
        .filter(|q| q.status == "active")
        .take(8)
        .map(|q| {
            let objectives: Vec<_> = q
                .objectives
                .iter()
                .filter(|o| !o.hidden)
                .map(|o| json!({"text": o.text, "status": o.status}))
                .collect();
            json!({
                "title": q.title,
                "summary": q.summary,
                "stage": q.stage,
                "urgency": q.urgency,
                "objectives": objectives,
            })
        })
        .collect();

    let lore_facts = lore::starter_facts();
    let lore_query = LoreQuery {
        text: action.text.clone(),
        year: state.world.clock.year,
        month: state.world.clock.month,
        day: state.world.clock.day,
        // Narrator is not allowed to learn lore the character could not know.
        // Secret world facts must enter narration only through Resolution.revealed.
        character_only: true,
        known_fact_ids: state
            .character
            .knowledge
            .iter()
            .map(|k| k.id.clone())
            .collect(),
        limit: 6,
    };
    let relevant_lore = lore::search(&lore_facts, &lore_query);

    let recent_chronicle: Vec<_> = state
        .world
        .chronicle
        .iter()
        .rev()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let recent_director_events: Vec<_> = state
        .director
        .recent_events
        .iter()
        .rev()
        .take(6)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    json!({
        "contract": {
            "language": "pl-PL",
            "role": "autonomous_game_master_narrator",
            "player_controls": ["intentions", "speech", "thoughts", "attempts"],
            "gm_controls": ["npc_reactions", "scene_pacing", "description"],
            "rust_controls": ["world_truth", "mechanics", "success_or_failure", "state_changes", "knowledge_gates"],
            "never_confirm_player_claim_as_world_truth_without_resolution_support": true,
            "do_not_ask_for_detail_when_reasonable_inference_is_possible": true,
            "do_not_invent_mechanical_success": true,
            "do_not_expose_hidden_information": true,
        },
        "character": {
            "name": state.character.name,
            "origin": state.character.origin,
            "profession": state.character.profession,
            "hp": state.character.hp,
            "stamina": state.character.stamina,
            "vigor": state.character.vigor,
            "chaos": state.character.chaos,
            "stress": state.character.stress,
            "fatigue": state.character.fatigue,
            "known_formulas": state.character.known_formulas,
        },
        "scene": {
            "location": state.world.location,
            "weather": state.world.weather,
            "clock": state.world.clock,
            "tension": state.world.tension,
        },
        "continuity": {
            "last_narration": state.last_narration,
            "recent_chronicle": recent_chronicle,
        },
        "director_cues": {
            "scene_goal": state.director.scene_goal,
            "tension": state.director.tension,
            "open_threads": state.director.open_threads,
            "recent_events": recent_director_events,
        },
        "active_npcs": active_npcs,
        "active_quests": active_quests,
        "relevant_character_safe_lore": relevant_lore,
        "player_known_facts": visible_facts,
        "player_action": action,
        "mechanical_resolution": resolution,
        "allowed_revelations": resolution.revealed,
        "allowed_complications": resolution.complications,
        "magic_semantics": if resolution.cost_vigor > 0 {
            magic_semantics(&action.text)
        } else {
            json!(null)
        },
    })
}

fn system_prompt() -> &'static str {
    r#"Jesteś MGAI — autonomicznym Mistrzem Gry i narratorem kampanii RPG w mrocznym fantasy osadzonym w realiach świata Wiedźmina. Pisz po polsku.

NIE jesteś asystentem gracza i NIE masz grzecznie potwierdzać wszystkiego, co gracz napisze.
Gracz kontroluje wyłącznie zamiary, wypowiedzi, myśli oraz próby swojej postaci. Rust Core jest jedynym autorytetem prawdy świata, mechaniki, wyniku próby i wiedzy, którą wolno ujawnić.

Twarde zasady:
1. Traktuj zdania gracza o świecie jako obserwacje, hipotezy lub zamiary, dopóki mechanical_resolution / allowed_revelations ich nie potwierdzą.
