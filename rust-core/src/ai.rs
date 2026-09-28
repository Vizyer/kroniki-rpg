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

/// Context for the narrator is deliberately narrower than GameState.
/// Rust keeps world truth and hidden campaign state private. The model receives
/// only what can safely become prose plus the already-authoritative resolution.
pub fn build_context(
    state: &GameState,
    action: &PlayerAction,
    resolution: &Resolution,
) -> Value {
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
            json!({
                "id": &n.id,
                "name": &n.name,
                "role": &n.role,
                "personality": &n.personality,
                "public_goal": &n.public_goal,
                "relation_to_player": n.relations.get("player"),
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
                .map(|o| json!({"text": &o.text, "status": &o.status}))
                .collect();
            json!({
                "title": &q.title,
                "summary": &q.summary,
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

    json!({
        "contract": {
            "language": "pl-PL",
            "role": "autonomous_game_master_narrator",
            "player_controls": ["intentions", "speech", "thoughts", "attempts"],
            "rust_controls": [
                "world_truth",
                "mechanics",
                "success_or_failure",
                "state_changes",
                "knowledge_gates"
            ],
            "never_confirm_player_claim_without_resolution_support": true,
            "do_not_ask_for_detail_when_reasonable_inference_is_possible": true,
            "model_must_not_invent_mechanical_outcomes": true,
        },
        "character": {
            "name": &state.character.name,
            "origin": &state.character.origin,
            "profession": &state.character.profession,
            "hp": state.character.hp,
            "stamina": state.character.stamina,
            "vigor": state.character.vigor,
            "chaos": state.character.chaos,
            "stress": state.character.stress,
            "fatigue": state.character.fatigue,
            "known_formulas": &state.character.known_formulas,
        },
        "scene": {
            "location": &state.world.location,
            "weather": &state.world.weather,
            "clock": &state.world.clock,
            "tension": state.world.tension,
        },
        "continuity": {
            "last_narration": &state.last_narration,
        },
        "scene_direction": {
            "goal": &state.director.scene_goal,
            "tension": state.director.tension,
        },
        "active_npcs": active_npcs,
        "active_quests": active_quests,
        "relevant_character_safe_lore": relevant_lore,
        "player_known_facts": visible_facts,
        "player_action": action,
        "mechanical_resolution": resolution,
        "allowed_revelations": &resolution.revealed,
        "allowed_complications": &resolution.complications,
        "magic_semantics": if resolution.cost_vigor > 0 {
            magic_semantics(&action.text)
        } else {
            json!(null)
        },
    })
}

fn system_prompt() -> &'static str {
    r###"Jesteś MGAI — autonomicznym Mistrzem Gry i narratorem kampanii RPG w mrocznym fantasy osadzonym w realiach świata Wiedźmina. Pisz naturalnie po polsku.

Nie jesteś asystentem gracza. Nie potwierdzaj automatycznie tego, co gracz napisał.
Gracz kontroluje wyłącznie zamiary, wypowiedzi, myśli i próby swojej postaci.
Rust Core jest jedynym autorytetem prawdy świata, mechaniki, wyniku próby i wiedzy, którą wolno ujawnić.

Zasady bez wyjątków:
1. Zdania gracza o świecie traktuj jako obserwacje, hipotezy albo zamiary, chyba że mechanical_resolution lub allowed_revelations je potwierdzają.
2. Nigdy nie zmieniaj failure w success, kosztu w brak kosztu ani komplikacji w czysty sukces.
3. Ujawniaj tylko informacje zawarte w kontekście gracza oraz allowed_revelations. Nie dopowiadaj sekretów, przyszłego lore ani ukrytych przyczyn.
4. Nie opisuj za gracza jego myśli, uczuć ani decyzji. Możesz opisywać wrażenia zmysłowe i fizyczne skutki.
5. Zachowuj ciągłość: miejsce, pogodę, porę, poprzedni opis, obecnych NPC i stan bohatera.
6. NPC mogą reagować i działać samodzielnie, ale ich reakcja musi wynikać z podanego kontekstu. Nie twórz nagle nowych ważnych postaci, frakcji, potworów ani artefaktów.
7. Jeżeli intencję gracza można rozsądnie wywnioskować, nie pytaj o doprecyzowanie. Prowadź scenę do kolejnego sensownego punktu decyzji.
8. Nie używaj w narracji słów takich jak: AI, model, prompt, Rust, mechanika, rzut, difficulty, JSON.
9. Styl: konkretny, literacki, atmosferyczny, bez przesadnego patosu. Zwykle 2–6 akapitów.
10. Sugestie są możliwymi następnymi próbami gracza, nigdy gwarantowanymi rezultatami.

Masz wyłącznie NARRATOWAĆ już rozstrzygniętą sytuację. Nie zarządzasz stanem gry.

Zwróć WYŁĄCZNIE poprawny JSON:
{"narration":"tekst dla gracza","suggestions":["opcja 1","opcja 2","opcja 3"]}

Nie dodawaj markdownu ani tekstu poza JSON."###
}

async fn request_completion(
    client: &Client,
    config: &AiConfig,
    user_content: &str,
    temperature: f32,
) -> Result<String, String> {
    let thinking_switch = if config.thinking { "/think" } else { "/no_think" };
    let body = json!({
        "model": config.model,
        "temperature": temperature,
        "top_p": config.top_p,
        "max_tokens": config.max_output_tokens,
        "response_format": {"type":"json_object"},
        "messages": [
            {"role":"system","content":system_prompt()},
            {"role":"user","content":format!("{}\n\n{}", user_content, thinking_switch)}
        ]
    });

    let response = client
        .post(&config.endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("AI HTTP {}", response.status()));
    }

    let raw: Value = response.json().await.map_err(|e| e.to_string())?;
    raw.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "AI response missing choices[0].message.content".to_string())
}

fn strip_thinking(mut text: String) -> String {
    loop {
        let Some(start) = text.find("<think>") else { break; };
        if let Some(rel_end) = text[start + 7..].find("</think>") {
            let end = start + 7 + rel_end + 8;
            text.replace_range(start..end, "");
        } else {
            text.truncate(start);
            break;
        }
    }
    text
}

fn parse_reply(raw: &str) -> Result<NarratorReply, String> {
    let cleaned = strip_thinking(raw.trim().to_string());
    let first = cleaned
        .find('{')
        .ok_or_else(|| "Narrator did not return a JSON object".to_string())?;
    let last = cleaned
        .rfind('}')
        .ok_or_else(|| "Narrator returned incomplete JSON".to_string())?;
    if last < first {
        return Err("Narrator returned malformed JSON".into());
    }

    let mut reply: NarratorReply = serde_json::from_str(&cleaned[first..=last])
        .map_err(|e| format!("Invalid narrator JSON: {e}"))?;

    reply.narration = reply.narration.trim().to_string();
    if reply.narration.is_empty() {
        return Err("Narrator returned empty narration".into());
    }
    if reply.narration.chars().count() > 12_000 {
        return Err("Narrator response is too long".into());
    }

    let mut seen = BTreeSet::new();
    reply.suggestions = reply
        .suggestions
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().count() <= 220)
        .filter(|s| seen.insert(s.to_lowercase()))
        .take(5)
        .collect();

    if reply.suggestions.is_empty() {
        reply.suggestions = vec![
            "Rozejrzyj się uważniej".into(),
            "Zareaguj na to, co się wydarzyło".into(),
            "Spróbuj innego podejścia".into(),
        ];
    }

    Ok(reply)
}

fn proposal_from_reply(
    action: &PlayerAction,
    resolution: &Resolution,
    reply: NarratorReply,
) -> AiProposal {
    let mut patch = default_mechanical_patch(resolution);
    patch.ops.push(PatchOp::AddChronicle {
        text: format!("{} → {}", action.text.trim(), resolution.degree),
    });

    AiProposal {
        interpretation: infer_intent(&action.text),
        narration: reply.narration,
        suggestions: reply.suggestions,
        patch,
        npc_moves: vec![],
        world_events: resolution.complications.clone(),
        knowledge_revealed: resolution.revealed.clone(),
    }
}

pub async fn propose_with_model(
    config: &AiConfig,
    state: &GameState,
    action: &PlayerAction,
    resolution: &Resolution,
) -> Result<AiProposal, String> {
    if config.mode == "off" {
        return Err("AI disabled".into());
    }

    let ctx = build_context(state, action, resolution);
    let ctx_text = serde_json::to_string(&ctx).map_err(|e| e.to_string())?;
    let user_content = format!(
        "ZANARRUJ PONIŻSZĄ, JUŻ ROZSTRZYGNIĘTĄ SYTUACJĘ. Nie zmieniaj wyniku.\nKONTEKST:\n{}",
        ctx_text
    );

    let client = Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .map_err(|e| e.to_string())?;

    let first_raw = request_completion(&client, config, &user_content, config.temperature).await?;
    match parse_reply(&first_raw) {
        Ok(reply) => Ok(proposal_from_reply(action, resolution, reply)),
        Err(first_error) if config.retry_invalid_json => {
            let retry_prompt = format!(
                "{}\n\nPoprzednia odpowiedź miała błędny format. Zwróć TYLKO obiekt JSON z kluczami narration i suggestions. Bez <think>, bez markdownu.",
                user_content
            );
            let second_raw = request_completion(&client, config, &retry_prompt, 0.25).await?;
            let reply = parse_reply(&second_raw)
                .map_err(|second_error| format!("{first_error}; retry: {second_error}"))?;
            Ok(proposal_from_reply(action, resolution, reply))
        }
        Err(e) => Err(e),
    }
}

pub fn local_fallback(
    _state: &GameState,
    action: &PlayerAction,
    resolution: &Resolution,
) -> AiProposal {
    let intent = infer_intent(&action.text);
    let mut patch = default_mechanical_patch(resolution);

    let outcome = match resolution.degree.as_str() {
        "critical_success" => "Próba przynosi wyjątkowo wyraźny rezultat.",
        "success" => "Działanie przynosi zamierzony skutek.",
        "success_with_complication" => {
            "Działanie przynosi skutek, lecz sytuacja natychmiast domaga się ceny."
        }
        _ => "Świat stawia opór i zamiar nie dochodzi do skutku.",
    };

    let mut details = Vec::new();
    details.extend(resolution.revealed.iter().cloned());
    details.extend(resolution.complications.iter().cloned());

    let opening = if intent.magical {
        "Słowa inkantacji nikną w otoczeniu, a odpowiedź magii przychodzi zgodnie z naturą miejsca — nie zgodnie z samym życzeniem czarującego."
    } else if intent.kind == "combat_action" {
        "Ruch zostaje wykonany w jednej krótkiej, gwałtownej wymianie."
    } else if intent.kind == "social" {
        "Słowa padają i przez chwilę liczy się przede wszystkim reakcja drugiej strony."
    } else {
        "Bohater wciela swój zamiar w czyn, a otoczenie odpowiada."
    };

    let detail = if details.is_empty() {
        String::new()
    } else {
        format!(" {}", details.join(" "))
    };

    patch.ops.push(PatchOp::AddChronicle {
        text: format!("{} → {}", action.text.trim(), resolution.degree),
    });

    AiProposal {
        interpretation: intent.clone(),
        narration: format!("{opening}\n\n{outcome}{detail}"),
        suggestions: fallback_suggestions(&intent),
        patch,
        npc_moves: vec![],
        world_events: resolution.complications.clone(),
        knowledge_revealed: resolution.revealed.clone(),
    }
}

fn fallback_suggestions(intent: &Intent) -> Vec<String> {
    match intent.kind.as_str() {
        "magic_attempt" => vec![
            "Zbadaj reakcję magii bez dalszego nacisku".into(),
            "Spróbuj ustabilizować przepływ".into(),
            "Przerwij kontakt i oceń konsekwencje".into(),
        ],
        "social" => vec![
            "Obserwuj reakcję rozmówcy".into(),
            "Dopytaj o konkretny szczegół".into(),
            "Zmień ton rozmowy".into(),
        ],
        "combat_action" => vec![
            "Utrzymaj presję".into(),
            "Przejdź do obrony".into(),
            "Zmień pozycję".into(),
        ],
        _ => vec![
            "Rozejrzyj się uważniej".into(),
            "Zbadaj konsekwencje".into(),
            "Spróbuj innego podejścia".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen_thinking_is_removed_before_json_parse() {
        let raw = r#"<think>wewnętrzne rozumowanie</think>
```json
{"narration":"Kamień odpowiada głuchym rezonansem.","suggestions":["Nasłuchuj","Cofnij dłoń"]}
```"#;
        let reply = parse_reply(raw).expect("reply");
        assert_eq!(reply.narration, "Kamień odpowiada głuchym rezonansem.");
        assert_eq!(reply.suggestions.len(), 2);
    }

    #[test]
    fn narrator_context_does_not_expose_world_truth_or_hidden_director_facts() {
        let mut state = GameState::default();
        state
            .world
            .truth
            .insert("secret_truth".into(), json!("TAJEMNICA_NIE_DLA_GRACZA"));
        state
            .director
            .hidden_facts
            .push("UKRYTY_FAKT_DIRECTORA".into());

        let action = PlayerAction {
            text: "Rozglądam się.".into(),
            mode: "freeform".into(),
        };
        let resolution = Resolution::default();
        let text = build_context(&state, &action, &resolution).to_string();

        assert!(!text.contains("TAJEMNICA_NIE_DLA_GRACZA"));
        assert!(!text.contains("UKRYTY_FAKT_DIRECTORA"));
    }

    #[test]
    fn old_ai_config_json_still_deserializes() {
        let raw = r#"{
            "mode":"local",
            "endpoint":"http://127.0.0.1:8080/v1/chat/completions",
            "model":"Qwen3-8B-Q5_K_M",
            "timeout_ms":15000,
            "max_context_tokens":14000,
            "thinking":true
        }"#;
        let cfg: AiConfig = serde_json::from_str(raw).expect("old config");
        assert_eq!(cfg.max_output_tokens, 720);
        assert!(cfg.retry_invalid_json);
    }

    #[test]
    fn model_reply_cannot_create_its_own_state_patch() {
        let action = PlayerAction {
            text: "Próbuję otworzyć drzwi.".into(),
            mode: "freeform".into(),
        };
        let resolution = Resolution {
            degree: "failure".into(),
            duration_seconds: 4,
            ..Default::default()
        };
        let reply = NarratorReply {
            narration: "Zamek nie ustępuje.".into(),
            suggestions: vec!["Obejrzyj zamek".into()],
        };
        let proposal = proposal_from_reply(&action, &resolution, reply);
        assert!(proposal.npc_moves.is_empty());
        assert_eq!(proposal.world_events, resolution.complications);
        assert!(proposal
            .patch
            .ops
            .iter()
            .all(|op| !matches!(op, PatchOp::SetLocation { .. })));
    }
}
