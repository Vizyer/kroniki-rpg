use crate::domain::*;
use crate::lore::{self, LoreQuery};
use crate::systems::{default_mechanical_patch, infer_intent, magic_semantics};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub mode: String,
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u64,
    pub max_context_tokens: usize,
    pub thinking: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            mode: "local".into(),
            endpoint: "http://127.0.0.1:8080/v1/chat/completions".into(),
            model: "Qwen3-8B-Q5_K_M".into(),
            timeout_ms: 15_000,
            max_context_tokens: 14_000,
            thinking: true,
        }
    }
}

pub fn build_context(state: &GameState, action: &PlayerAction, resolution: &Resolution) -> Value {
    let visible_facts: Vec<_> = state.character.knowledge.iter().rev().take(12).cloned().collect();
    let active_npcs: Vec<_> = state.world.npcs.values()
        .filter(|n| n.active && n.location == state.world.location)
        .map(|n| json!({
            "id":n.id,
            "name":n.name,
            "role":n.role,
            "personality":n.personality,
            "public_goal":n.public_goal,
            "relation_to_player":n.relations.get("player"),
            "known_facts":n.knowledge.iter().take(8).collect::<Vec<_>>()
        })).collect();
    let quests: Vec<_> = state.world.quests.values().filter(|q| q.status == "active").take(8).collect();
    let lore_facts = lore::starter_facts();
    let lore_query = LoreQuery {
        text: action.text.clone(),
        year: state.world.clock.year,
        month: state.world.clock.month,
        day: state.world.clock.day,
        character_only: false,
        known_fact_ids: state.character.knowledge.iter().map(|k| k.id.clone()).collect(),
        limit: 6,
    };
    let relevant_lore = lore::search(&lore_facts, &lore_query);
    json!({
        "rules": {
            "player_controls": ["intentions","speech","thoughts","attempts"],
            "gm_controls": ["world_truth","npc_actions","outcomes","consequences"],
            "never_confirm_player_claim_as_world_truth_without_state_support": true,
            "do_not_ask_for_detail_when_reasonable_inference_is_possible": true,
            "autonomous_scene_progression": true
        },
        "character": &state.character,
        "scene": {
            "location":&state.world.location,
            "weather":&state.world.weather,
            "clock":&state.world.clock,
            "tension":state.world.tension
        },
        "director": &state.director,
        "active_npcs": active_npcs,
        "active_quests": quests,
        "relevant_lore": relevant_lore,
        "player_known_facts": visible_facts,
        "action": action,
        "mechanical_resolution": resolution,
        "magic_semantics": if resolution.cost_vigor > 0 { magic_semantics(&action.text) } else { json!(null) }
    })
}

fn system_prompt() -> &'static str {
    r#"Jesteś autonomicznym Mistrzem Gry w mrocznym słowiańskim fantasy inspirowanym realiami Wiedźmina. Nie jesteś asystentem gracza.
Gracz kontroluje tylko zamiary, słowa, myśli i próby swojej postaci. Ty kontrolujesz NPC, świat, konsekwencje i rozwój sceny.
Nie potwierdzaj deklaracji gracza jako faktów świata. Mechanika i WORLD TRUTH z kontekstu są nadrzędne.
Jeżeli intencję można rozsądnie wywnioskować, nie pytaj gracza o doprecyzowanie. Prowadź scenę do przodu.
Autonomiczne wydarzenia muszą wynikać z istniejącego NPC, wątku, zegara, frakcji, miejsca, zagrożenia albo wcześniejszej decyzji.
Nie ujawniaj ukrytej wiedzy, której bohater nie może znać.
Zwróć WYŁĄCZNIE JSON zgodny ze schematem: {interpretation,narration,suggestions,patch,npc_moves,world_events,knowledge_revealed}. Patch ma używać tylko dozwolonych operacji, nie twórz nieistniejących ID."#
}

pub async fn propose_with_model(config: &AiConfig, state: &GameState, action: &PlayerAction, resolution: &Resolution) -> Result<AiProposal, String> {
    if config.mode == "off" { return Err("AI disabled".into()); }
    let ctx = build_context(state, action, resolution);
    let intent = crate::systems::infer_intent(&action.text);
    let use_thinking = config.thinking && (
        intent.magical ||
        intent.kind == "combat_action" ||
        !resolution.complications.is_empty() ||
        state.director.tension >= 60
    );
    let mode_prefix = if use_thinking { "/think\n" } else { "/no_think\n" };
    let user_content = format!("{}{}", mode_prefix, serde_json::to_string(&ctx).map_err(|e|e.to_string())?);
    let body = json!({
        "model": config.model,
        "temperature": 0.72,
        "response_format": {"type":"json_object"},
        "messages": [
            {"role":"system","content":system_prompt()},
            {"role":"user","content":user_content}
        ]
    });
    let client = Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .map_err(|e|e.to_string())?;
    let response = client.post(&config.endpoint).json(&body).send().await.map_err(|e|e.to_string())?;
    if !response.status().is_success() { return Err(format!("AI HTTP {}", response.status())); }
    let raw: Value = response.json().await.map_err(|e|e.to_string())?;
    let content = raw.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or("AI response missing content")?;
    serde_json::from_str::<AiProposal>(content).map_err(|e|format!("Invalid AI proposal JSON: {e}"))
}

pub fn local_fallback(_state: &GameState, action: &PlayerAction, resolution: &Resolution) -> AiProposal {
    let intent = infer_intent(&action.text);
    let mut patch = default_mechanical_patch(resolution);
    let outcome = match resolution.degree.as_str() {
        "critical_success" => "próba udaje się wyjątkowo dobrze",
        "success" => "próba się udaje",
        "success_with_complication" => "próba się udaje, ale pojawia się koszt lub komplikacja",
        _ => "świat stawia opór i próba nie daje zamierzonego rezultatu",
    };
    let detail = if resolution.revealed.is_empty() {
        String::new()
    } else {
        format!(" {}", resolution.revealed.join(" "))
    };
    let narration = if intent.magical {
        format!("Inkantacja przecina ciszę. {}.{} Energia reaguje zgodnie z prawami tego miejsca, nie z samym życzeniem maga.", capitalize(outcome), detail)
    } else {
        format!("Bohater działa bez zawahania. {}.{} Świat odpowiada konsekwencją tej decyzji.", capitalize(outcome), detail)
    };
    patch.ops.push(PatchOp::AddChronicle {
        text: format!("{} → {}", action.text.trim(), resolution.degree)
    });
    AiProposal {
        interpretation: intent,
        narration,
        suggestions: vec![
            "Kontynuuj ostrożnie".into(),
            "Zbadaj konsekwencje".into(),
            "Zmień podejście".into()
        ],
        patch,
        npc_moves: vec![],
        world_events: resolution.complications.clone(),
        knowledge_revealed: resolution.revealed.clone(),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str()
    }
}
