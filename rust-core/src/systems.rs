use crate::domain::*;
use serde_json::Value;
use std::cmp::{max, min};

pub fn advance_time(clock: &mut Clock, minutes: i64) {
    let mut total = i64::from(clock.hour) * 60 + i64::from(clock.minute) + minutes.max(0);
    let days = total / 1440;
    total %= 1440;
    clock.hour = (total / 60) as u8;
    clock.minute = (total % 60) as u8;
    if days > 0 {
        let mut d = i64::from(clock.day) + days;
        while d > 30 {
            d -= 30;
            clock.month = if clock.month >= 12 { 1 } else { clock.month + 1 };
            if clock.month == 1 { clock.year += 1; }
        }
        clock.day = d as u8;
    }
}

pub fn infer_intent(text: &str) -> Intent {
    let l = text.to_lowercase();
    let magical = l.contains("inkant") || l.contains("zakl") || l.contains("aard") || l.contains("igni") || l.contains("quen") || l.contains("yrden") || l.contains("axii") || l.contains("chaos") || l.contains("pash ");
    let (kind, verb) = if magical {
        ("magic_attempt", "cast")
    } else if l.contains("atak") || l.contains("tnę") || l.contains("uderz") || l.contains("strzel") {
        ("combat_action", "attack")
    } else if l.contains("pytam") || l.contains("mówię") || l.contains("rozmaw") {
        ("social", "speak")
    } else if l.contains("szuk") || l.contains("badam") || l.contains("ogląd") || l.contains("sprawdz") {
        ("investigation", "inspect")
    } else if l.contains("idę") || l.contains("jadę") || l.contains("ruszam") {
        ("travel", "move")
    } else {
        ("freeform", "act")
    };
    let method = if magical && (l.contains("wyczu") || l.contains("energia") || l.contains("rezon")) { "sensing" } else if magical { "improvised" } else { "natural_language" };
    Intent { kind: kind.into(), verb: verb.into(), target: String::new(), method: method.into(), stakes: "normal".into(), magical, confidence: 0.72 }
}

fn deterministic_roll(seed: &str, revision: i64) -> i32 {
    let mut h: u64 = 1469598103934665603;
    for b in seed.bytes().chain(revision.to_string().bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(1099511628211);
    }
    1 + (h % 20) as i32
}

pub fn resolve_action(state: &GameState, action: &PlayerAction, intent: &Intent) -> Resolution {
    let social_pressure = ["przekon", "groż", "groź", "kłam", "oszuk", "zastrasz"].iter().any(|w|action.text.to_lowercase().contains(w));
    if matches!(intent.kind.as_str(), "observe" | "rest") || (intent.kind == "travel" && !intent.target.is_empty()) || (intent.kind == "social" && !social_pressure) {
        return Resolution { ok:true, degree:"no_check".into(), duration_seconds:if intent.kind=="rest"{1800}else if intent.kind=="travel"{600}else{30}, ..Default::default() };
    }
    let roll = deterministic_roll(&action.text, state.revision);
    let per = *state.character.attributes.get("PER").unwrap_or(&2);
    let int_ = *state.character.attributes.get("INT").unwrap_or(&2);
    let dex = *state.character.attributes.get("DEX").unwrap_or(&2);
    let cha = *state.character.attributes.get("CHA").unwrap_or(&2);
    let skill = if intent.magical { int_ + *state.character.skills.get("magic").unwrap_or(&0) }
        else if intent.kind == "combat_action" { dex + *state.character.skills.get("combat").unwrap_or(&0) }
        else if intent.kind == "social" { cha + *state.character.skills.get("social").unwrap_or(&0) }
        else { per + *state.character.skills.get("investigation").unwrap_or(&0) };
    let mut difficulty = 11 + state.world.tension / 25;
    if intent.magical { difficulty += state.character.chaos / 25; }
    let score = roll + skill;
    let degree = if score >= difficulty + 7 { "critical_success" }
        else if score >= difficulty { "success" }
        else if score + 3 >= difficulty { "success_with_complication" }
        else { "failure" };
    let ok = degree != "failure";
    let mut revealed = Vec::new();
    let mut complications = Vec::new();
    if intent.magical && intent.method == "sensing" && ok {
        if let Some(v) = state.world.truth.get("local_magic_disturbance") {
            if v.as_bool().unwrap_or(false) {
                revealed.push("Pole magiczne rzeczywiście jest zaburzone.".into());
                if degree == "critical_success" { revealed.push("Zakłócenie ma uporządkowany rytm i pochodzi z głębi miejsca.".into()); }
            } else {
                revealed.push("Nie wykrywasz wiarygodnego zaburzenia pola magicznego.".into());
            }
        } else {
            revealed.push("Wyczuwasz ślady lokalnego rezonansu, ale ich źródło pozostaje niepewne.".into());
        }
    }
    if degree == "success_with_complication" { complications.push("Sukces ma koszt lub zwraca uwagę otoczenia.".into()); }
    if degree == "failure" { complications.push("Świat nie potwierdza deklarowanego rezultatu.".into()); }
    Resolution {
        ok,
        degree: degree.into(),
        difficulty,
        roll,
        cost_stamina: if intent.kind == "combat_action" { 2 } else { 0 },
        cost_vigor: if intent.magical { 2 } else { 0 },
        chaos_gain: if intent.magical { if ok { 2 } else { 4 } } else { 0 },
        instability: if intent.magical { if degree == "failure" { 3 } else { 1 } } else { 0 },
        duration_seconds: if intent.kind == "social" { 30 } else if intent.kind == "travel" { 600 } else { 4 },
        revealed,
        complications,
    }
}

pub fn apply_patch(state: &mut GameState, patch: &StatePatch) -> Result<(), String> {
    for op in &patch.ops {
        match op {
            PatchOp::AdvanceTime { minutes } => advance_time(&mut state.world.clock, *minutes),
            PatchOp::SetLocation { location } => state.world.location = location.clone(),
            PatchOp::AddChronicle { text } => if !text.trim().is_empty() { state.world.chronicle.push(text.clone()); },
            PatchOp::LearnFact { fact } => {
                if !state.character.knowledge.iter().any(|x| x.id == fact.id) { state.character.knowledge.push(fact.clone()); }
            }
            PatchOp::NpcRemember { npc_id, memory } => {
                let npc = state.world.npcs.get_mut(npc_id).ok_or_else(|| format!("Unknown NPC: {npc_id}"))?;
                npc.memories.push(memory.clone());
                npc.memories.sort_by_key(|m| -m.importance);
                npc.memories.truncate(24);
            }
            PatchOp::RelationDelta { npc_id, subject_id, trust, respect, fear, liking, debt, hostility, leverage } => {
                let npc = state.world.npcs.get_mut(npc_id).ok_or_else(|| format!("Unknown NPC: {npc_id}"))?;
                let r = npc.relations.entry(subject_id.clone()).or_default();
                r.trust = clamp100(r.trust + trust);
                r.respect = clamp100(r.respect + respect);
                r.fear = clamp100(r.fear + fear);
                r.liking = clamp100(r.liking + liking);
                r.debt = clamp100(r.debt + debt);
                r.hostility = clamp100(r.hostility + hostility);
                r.leverage = clamp100(r.leverage + leverage);
            }
            PatchOp::FactionClock { faction_id, delta } => {
                let f = state.world.factions.get_mut(faction_id).ok_or_else(|| format!("Unknown faction: {faction_id}"))?;
                f.clock = min(f.clock_max.max(1), max(0, f.clock + delta));
            }
            PatchOp::QuestStage { quest_id, stage, status } => {
                let q = state.world.quests.get_mut(quest_id).ok_or_else(|| format!("Unknown quest: {quest_id}"))?;
                q.stage = max(q.stage, *stage);
                if let Some(s) = status { q.status = s.clone(); }
            }
            PatchOp::CharacterResource { resource, delta } => match resource.as_str() {
                "hp" => state.character.hp = max(0, state.character.hp + delta),
                "stamina" => state.character.stamina = max(0, state.character.stamina + delta),
                "vigor" => state.character.vigor = max(0, state.character.vigor + delta),
                "chaos" => state.character.chaos = min(100, max(0, state.character.chaos + delta)),
                "stress" => state.character.stress = min(100, max(0, state.character.stress + delta)),
                "fatigue" => state.character.fatigue = min(100, max(0, state.character.fatigue + delta)),
                _ => return Err(format!("Unknown resource: {resource}")),
            },
            PatchOp::CombatPulse { tempo, guard, pressure, morale, wound } => {
                let c = &mut state.world.combat;
                c.tempo = max(-3, min(3, c.tempo + tempo));
                c.guard = max(0, c.guard + guard);
                c.pressure = max(0, min(10, c.pressure + pressure));
                c.morale = max(0, min(100, c.morale + morale));
                if let Some(w) = wound { c.wounds.push(w.clone()); }
            }
            PatchOp::AddClue { clue } => if !state.world.hunt.clues.contains(clue) { state.world.hunt.clues.push(clue.clone()); },
            PatchOp::AlchemyToxicity { delta } => state.world.alchemy.toxicity = max(0, min(100, state.world.alchemy.toxicity + delta)),
            PatchOp::AddItem { item } => {
                if let Some(existing) = state.character.inventory.iter_mut().find(|x| x.id == item.id && x.quality == item.quality) { existing.quantity += item.quantity; }
                else { state.character.inventory.push(item.clone()); }
            }
            PatchOp::RemoveItem { item_id, quantity } => {
                let it = state.character.inventory.iter_mut().find(|x| x.id == *item_id).ok_or_else(|| format!("Missing item: {item_id}"))?;
                if it.quantity < *quantity { return Err(format!("Not enough item: {item_id}")); }
                it.quantity -= quantity;
                state.character.inventory.retain(|x| x.quantity > 0);
            }
        }
    }
    state.revision += 1;
    Ok(())
}

fn clamp100(v: i32) -> i32 { max(-100, min(100, v)) }

pub fn simulate_background(state: &mut GameState, elapsed_minutes: i64) -> Vec<String> {
    let mut events = Vec::new();
    if elapsed_minutes <= 0 { return events; }
    advance_time(&mut state.world.clock, elapsed_minutes);
    for f in state.world.factions.values_mut() {
        if f.clock_max > 0 && f.clock < f.clock_max && elapsed_minutes >= 60 {
            f.clock = min(f.clock_max, f.clock + (elapsed_minutes / 180) as i32);
            if f.clock == f.clock_max { events.push(format!("Plan frakcji '{}' osiągnął punkt przełomowy.", f.name)); }
        }
    }
    for npc in state.world.npcs.values_mut().filter(|n| n.active) {
        if !npc.plan.is_empty() && elapsed_minutes >= 60 {
            npc.plan.remove(0);
            // Private plans are never player-visible narration input.
            if npc.location == state.world.location { events.push(format!("{} zajmuje się swoimi sprawami.",npc.name)); }
        }
    }
    for q in state.world.quests.values_mut() {
        if let Some(d) = q.deadline_minutes.as_mut() {
            *d -= elapsed_minutes;
            if *d <= 0 && q.status == "active" {
                q.status = "failed".into();
                events.push(format!("Upłynął termin zadania '{}'.", q.title));
            }
        }
    }
    state.world.chronicle.extend(events.clone());
    if !events.is_empty() { state.revision += 1; }
    events
}

pub fn validate_ai_patch(state: &GameState, proposal: &AiProposal) -> Result<(), String> {
    for op in &proposal.patch.ops {
        match op {
            PatchOp::NpcRemember { npc_id, .. } | PatchOp::RelationDelta { npc_id, .. } if !state.world.npcs.contains_key(npc_id) => return Err(format!("AI referenced unknown NPC: {npc_id}")),
            PatchOp::FactionClock { faction_id, .. } if !state.world.factions.contains_key(faction_id) => return Err(format!("AI referenced unknown faction: {faction_id}")),
            PatchOp::QuestStage { quest_id, .. } if !state.world.quests.contains_key(quest_id) => return Err(format!("AI referenced unknown quest: {quest_id}")),
            PatchOp::CharacterResource { resource, delta } if resource == "hp" && state.character.hp + delta < 0 => return Err("AI attempted invalid HP patch".into()),
            _ => {}
        }
    }
    if proposal.narration.len() > 18_000 { return Err("Narration too long".into()); }
    Ok(())
}

pub fn default_mechanical_patch(resolution: &Resolution) -> StatePatch {
    let mut ops = vec![PatchOp::AdvanceTime { minutes: i64::from((resolution.duration_seconds.max(1) + 59) / 60) }];
    if resolution.cost_stamina != 0 { ops.push(PatchOp::CharacterResource { resource: "stamina".into(), delta: -resolution.cost_stamina }); }
    if resolution.cost_vigor != 0 { ops.push(PatchOp::CharacterResource { resource: "vigor".into(), delta: -resolution.cost_vigor }); }
    if resolution.chaos_gain != 0 { ops.push(PatchOp::CharacterResource { resource: "chaos".into(), delta: resolution.chaos_gain }); }
    StatePatch { ops }
}

pub fn craft(state: &mut GameState, recipe: &str) -> Result<ItemStack, String> {
    if !state.world.crafting.recipes.iter().any(|r| r == recipe) { return Err("Nieznana receptura".into()); }
    let item = ItemStack { id: format!("crafted:{}", recipe.to_lowercase().replace(' ', "_")), name: recipe.into(), quantity: 1, quality: 1 + *state.character.skills.get("crafting").unwrap_or(&0) / 2, freshness: None };
    state.character.inventory.push(item.clone());
    advance_time(&mut state.world.clock, 60);
    state.revision += 1;
    Ok(item)
}

pub fn brew(state: &mut GameState, recipe: &str) -> Result<ItemStack, String> {
    if !state.world.alchemy.recipes.iter().any(|r| r == recipe) { return Err("Nieznana receptura alchemiczna".into()); }
    let item = ItemStack { id: format!("alchemy:{}", recipe.to_lowercase().replace(' ', "_")), name: recipe.into(), quantity: 1, quality: 1 + *state.character.skills.get("alchemy").unwrap_or(&0) / 2, freshness: Some(72) };
    state.world.alchemy.prepared.push(item.clone());
    advance_time(&mut state.world.clock, 30);
    state.revision += 1;
    Ok(item)
}

pub fn add_hunt_evidence(state: &mut GameState, clue: &str, reliability: i32) {
    if !state.world.hunt.clues.contains(&clue.to_string()) { state.world.hunt.clues.push(clue.into()); }
    state.world.hunt.confidence = min(100, state.world.hunt.confidence + max(1, reliability / 10));
    state.revision += 1;
}

pub fn magic_semantics(text: &str) -> Value {
    let l = text.to_lowercase();
    let operation = if l.contains("wyczu") || l.contains("słuch") || l.contains("energia") { "sense" }
        else if l.contains("tłumi") || l.contains("zdusi") { "suppress" }
        else if l.contains("wiąż") || l.contains("unieruch") { "bind" }
        else if l.contains("pch") || l.contains("odrz") { "force" }
        else { "shape" };
    serde_json::json!({
        "operation": operation,
        "element": if l.contains("ogie") || l.contains("igni") { "fire" } else if l.contains("aard") || l.contains("powiet") { "air" } else { "chaos" },
        "target": "contextual",
        "scale": if l.contains("potęż") || l.contains("całe") { "large" } else { "small" },
        "precision": if l.contains("ostroż") || l.contains("precy") { "high" } else { "medium" },
        "channeling": l.contains("skup") || l.contains("powoli") || l.contains("zamykam oczy")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_advances_across_midnight() {
        let mut c = Clock { year: 1272, month: 6, day: 17, hour: 23, minute: 50 };
        advance_time(&mut c, 20);
        assert_eq!((c.day, c.hour, c.minute), (18, 0, 10));
    }

    #[test]
    fn free_magic_is_intent_not_world_truth() {
        let i = infer_intent("Energia jest wzburzona. Pash iritor — próbuję ją wyczuć.");
        assert!(i.magical);
        assert_eq!(i.method, "sensing");
    }

    #[test]
    fn npc_memory_requires_existing_npc() {
        let s = GameState::default();
        let p = AiProposal { patch: StatePatch { ops: vec![PatchOp::NpcRemember { npc_id: "ghost".into(), memory: NpcMemory::default() }] }, ..Default::default() };
        assert!(validate_ai_patch(&s, &p).is_err());
    }
}

