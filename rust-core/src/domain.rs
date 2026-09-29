use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Clock {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub origin: String,
    pub profession: String,
    pub hp: i32,
    pub stamina: i32,
    pub vigor: i32,
    pub chaos: i32,
    pub stress: i32,
    pub fatigue: i32,
    pub attributes: BTreeMap<String, i32>,
    pub skills: BTreeMap<String, i32>,
    pub inventory: Vec<ItemStack>,
    pub known_formulas: Vec<String>,
    pub knowledge: Vec<KnowledgeFact>,
}

impl Default for Character {
    fn default() -> Self {
        let attributes = [
            ("STR", 2), ("DEX", 2), ("CON", 2),
            ("INT", 2), ("PER", 2), ("CHA", 2),
        ].into_iter().map(|(k,v)|(k.to_string(),v)).collect();
        Self {
            name: "BezImienny".into(),
            origin: "Wędrowiec".into(),
            profession: "Poszukiwacz".into(),
            hp: 24,
            stamina: 20,
            vigor: 12,
            chaos: 0,
            stress: 0,
            fatigue: 0,
            attributes,
            skills: BTreeMap::new(),
            inventory: Vec::new(),
            known_formulas: Vec::new(),
            knowledge: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemStack {
    pub id: String,
    pub name: String,
    pub quantity: i32,
    pub quality: i32,
    pub freshness: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeFact {
    pub id: String,
    pub statement: String,
    pub source: String,
    pub confidence: i32,
    pub canon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NpcMemory {
    pub id: String,
    pub summary: String,
    pub importance: i32,
    pub valence: i32,
    pub at: Clock,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Relation {
    pub trust: i32,
    pub respect: i32,
    pub fear: i32,
    pub liking: i32,
    pub debt: i32,
    pub hostility: i32,
    pub leverage: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Npc {
    #[serde(default)]
    pub canon: bool,
    pub id: String,
    pub name: String,
    pub role: String,
    pub personality: String,
    pub public_goal: String,
    pub hidden_goal: String,
    pub secret: String,
    pub risk_tolerance: i32,
    pub knowledge: Vec<KnowledgeFact>,
    pub memories: Vec<NpcMemory>,
    pub relations: BTreeMap<String, Relation>,
    pub plan: Vec<String>,
    pub resources: BTreeMap<String, i32>,
    pub location: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Faction {
    #[serde(default)]
    pub canon: bool,
    pub id: String,
    pub name: String,
    pub power: i32,
    pub wealth: i32,
    pub influence: i32,
    pub public_goal: String,
    pub private_goal: String,
    pub clock: i32,
    pub clock_max: i32,
    pub resources: BTreeMap<String, i32>,
    pub attitude_to_player: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuestObjective {
    pub id: String,
    pub text: String,
    pub status: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Quest {
    #[serde(default)]
    pub canon: bool,
    pub id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub stage: i32,
    pub summary: String,
    pub urgency: i32,
    pub deadline_minutes: Option<i64>,
    pub objectives: Vec<QuestObjective>,
    pub consequences: Vec<String>,
    pub rewards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CombatState {
    pub active: bool,
    pub tempo: i32,
    pub guard: i32,
    pub pressure: i32,
    pub range: String,
    pub stance: String,
    pub morale: i32,
    pub wounds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HuntState {
    pub active: bool,
    pub contract: String,
    pub confidence: i32,
    pub preparation: i32,
    pub clues: Vec<String>,
    pub hypotheses: Vec<String>,
    pub contradictions: Vec<String>,
    pub target_truth: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlchemyState {
    pub toxicity: i32,
    pub recipes: Vec<String>,
    pub prepared: Vec<ItemStack>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CraftState {
    pub recipes: Vec<String>,
    pub queue: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    pub clock: Clock,
    pub location: String,
    pub weather: String,
    pub tension: i32,
    pub canon_mode: String,
    pub truth: BTreeMap<String, Value>,
    pub factions: BTreeMap<String, Faction>,
    pub npcs: BTreeMap<String, Npc>,
    pub quests: BTreeMap<String, Quest>,
    pub combat: CombatState,
    pub hunt: HuntState,
    pub alchemy: AlchemyState,
    pub crafting: CraftState,
    pub chronicle: Vec<String>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            clock: Clock { year: 1272, month: 6, day: 17, hour: 18, minute: 0 },
            location: "Północne Królestwa".into(),
            weather: "pochmurno".into(),
            tension: 20,
            canon_mode: "stable".into(),
            truth: BTreeMap::new(),
            factions: BTreeMap::new(),
            npcs: BTreeMap::new(),
            quests: BTreeMap::new(),
            combat: CombatState { active: false, tempo: 0, guard: 5, pressure: 0, range: "near".into(), stance: "balanced".into(), morale: 100, wounds: vec![] },
            hunt: HuntState::default(),
            alchemy: AlchemyState::default(),
            crafting: CraftState::default(),
            chronicle: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DirectorState {
    pub scene_goal: String,
    pub tension: i32,
    pub open_threads: Vec<String>,
    pub hidden_facts: Vec<String>,
    pub clocks: BTreeMap<String, i32>,
    pub recent_events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    #[serde(default)]
    pub memory: crate::dm::CampaignMemory,
    #[serde(default)]
    pub campaign: crate::dm::Campaign,
    pub schema: i32,
    pub campaign_id: String,
    pub character: Character,
    pub world: World,
    pub director: DirectorState,
    pub last_narration: String,
    pub last_suggestions: Vec<String>,
    pub revision: i64,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            memory: Default::default(),
            campaign: Default::default(),
            schema: 10,
            campaign_id: "default".into(),
            character: Character::default(),
            world: World::default(),
            director: DirectorState::default(),
            last_narration: "Świat czeka na pierwszy krok bohatera.".into(),
            last_suggestions: vec!["Rozejrzyj się".into(), "Porozmawiaj z kimś".into(), "Sprawdź ekwipunek".into()],
            revision: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerAction {
    pub text: String,
    #[serde(default)]
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Intent {
    pub kind: String,
    pub verb: String,
    pub target: String,
    pub method: String,
    pub stakes: String,
    pub magical: bool,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Resolution {
    pub ok: bool,
    pub degree: String,
    pub difficulty: i32,
    pub roll: i32,
    pub cost_stamina: i32,
    pub cost_vigor: i32,
    pub chaos_gain: i32,
    pub instability: i32,
    pub duration_seconds: i32,
    pub revealed: Vec<String>,
    pub complications: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatePatch {
    pub ops: Vec<PatchOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOp {
    AdvanceTime { minutes: i64 },
    SetLocation { location: String },
    AddChronicle { text: String },
    LearnFact { fact: KnowledgeFact },
    NpcRemember { npc_id: String, memory: NpcMemory },
    RelationDelta { npc_id: String, subject_id: String, trust: i32, respect: i32, fear: i32, liking: i32, debt: i32, hostility: i32, leverage: i32 },
    FactionClock { faction_id: String, delta: i32 },
    QuestStage { quest_id: String, stage: i32, status: Option<String> },
    CharacterResource { resource: String, delta: i32 },
    CombatPulse { tempo: i32, guard: i32, pressure: i32, morale: i32, wound: Option<String> },
    AddClue { clue: String },
    AlchemyToxicity { delta: i32 },
    AddItem { item: ItemStack },
    RemoveItem { item_id: String, quantity: i32 },
}

impl Default for PatchOp {
    fn default() -> Self { Self::AddChronicle { text: String::new() } }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiProposal {
    pub interpretation: Intent,
    pub narration: String,
    pub suggestions: Vec<String>,
    pub patch: StatePatch,
    pub npc_moves: Vec<String>,
    pub world_events: Vec<String>,
    pub knowledge_revealed: Vec<String>,
}

pub fn state_summary(s: &GameState) -> Value {
    json!({
        "schema": s.schema,
        "campaign": crate::dm::public_campaign(s),
        "recent_turns": &s.memory.recent,
        "revision": s.revision,
        "campaign_id": &s.campaign_id,
        "character": {"name": &s.character.name, "hp": s.character.hp, "stamina": s.character.stamina, "vigor": s.character.vigor, "chaos": s.character.chaos},
        "world": {"location": &s.world.location, "clock": &s.world.clock, "weather": &s.world.weather, "tension": s.world.tension},
        "combat": &s.world.combat,
        "hunt": {"active":s.world.hunt.active,"contract":s.world.hunt.contract,"confidence":s.world.hunt.confidence,"clues":s.world.hunt.clues,"hypotheses":s.world.hunt.hypotheses},
        "last_narration": &s.last_narration,
        "suggestions": &s.last_suggestions,
    })
}

