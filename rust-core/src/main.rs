use axum::{extract::State, http::Method, routing::{get, post}, Json, Router};
use kroniki_core::{
    ai::AiConfig,
    creator::{self, CreateCharacterRequest},
    dm::{self, NewCampaign},
    domain::{state_summary, GameState, PlayerAction},
    engine::Engine,
    lore::{self, LoreQuery},
    runtime::LocalAiRuntime,
    store::Store,
    systems,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    engine: Arc<Engine>,
}

#[tokio::main]
async fn main() {
    let store = Store::open_default().expect("Cannot open KronikiRPG SQLite database");
    let engine = Arc::new(Engine::new(store));
    let app_state = AppState { engine: engine.clone() };
    let local_ai = engine.local_ai.clone();
    tokio::spawn(async move {
        let _ = local_ai.ensure_started().await;
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/state", get(get_state).post(set_state))
        .route("/action", post(action))
        .route("/action/cancel", post(cancel_action))
        .route("/campaign/new", post(new_campaign))
        .route("/tick", post(tick))
        .route("/save", post(save))
        .route("/saves", get(list_saves))
        .route("/save/load", post(load_save))
        .route("/ai/config", get(ai_config_get).post(ai_config_set))
        .route("/ai/local/status", get(ai_local_status))
        .route("/ai/local/start", post(ai_local_start))
        .route("/character/create", post(character_create))
        .route("/lore/search", post(lore_search))
        .route("/magic/parse", post(magic_parse))
        .route("/craft", post(craft))
        .route("/alchemy/brew", post(brew))
        .route("/hunt/evidence", post(hunt_evidence))
        .route("/shutdown", post(shutdown))
        .layer(cors)
        .with_state(app_state);

    let addr: SocketAddr = "127.0.0.1:17377".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Cannot bind Rust Core port");
    axum::serve(listener, app)
        .await
        .expect("Rust Core server failed");
}

async fn health(State(s): State<AppState>) -> Json<Value> {
    let st = s.engine.state.read().await;
    Json(json!({
        "ok": true,
        "core": "0.10.0",
        "schema": st.schema,
        "revision": st.revision,
        "ai": s.engine.ai_config.read().await.clone(),
        "local_ai": LocalAiRuntime::status_json()
    }))
}

async fn get_state(State(s): State<AppState>) -> Json<Value> {
    let guard = s.engine.state.read().await;
    Json(state_summary(&guard))
}

async fn set_state(
    State(s): State<AppState>,
    Json(v): Json<GameState>,
) -> Json<Value> {
    match s.engine.replace(v,"import").await {
        Ok(()) => { let st=s.engine.state.read().await; Json(json!({"ok":true,"state":state_summary(&st)})) },
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

#[derive(Deserialize)]
struct ActionReq {
    #[serde(flatten)] action: PlayerAction,
    #[serde(default)] request_id: String,
}
#[derive(Deserialize)]
struct CancelReq { request_id: String }
async fn cancel_action(State(s): State<AppState>, Json(p): Json<CancelReq>) -> Json<Value> {
    s.engine.cancel(&p.request_id);
    Json(json!({"ok":true,"cancel_requested":true}))
}
async fn new_campaign(State(s): State<AppState>, Json(p): Json<NewCampaign>) -> Json<Value> {
    match s.engine.replace(dm::new_campaign(p),"new_campaign").await {
        Ok(()) => { let st=s.engine.state.read().await; Json(json!({"ok":true,"state":state_summary(&st)})) },
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

async fn action(
    State(s): State<AppState>,
    Json(p): Json<ActionReq>,
) -> Json<Value> {
    match s.engine.act_with_id(p.action,p.request_id).await {
        Ok(x) => Json(serde_json::to_value(x).unwrap_or(json!({"ok":false}))),
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

#[derive(Deserialize)]
struct TickReq {
    minutes: i64,
}

async fn tick(
    State(s): State<AppState>,
    Json(p): Json<TickReq>,
) -> Json<Value> {
    let events = match s.engine.tick(p.minutes).await { Ok(e)=>e, Err(e)=>return Json(json!({"ok":false,"error":e})) };
    let guard = s.engine.state.read().await;
    Json(json!({
        "ok":true,
        "events":events,
        "state":state_summary(&guard)
    }))
}

#[derive(Deserialize)]
struct SaveReq {
    name: String,
    id: Option<i64>,
    state: Option<GameState>,
}

async fn save(
    State(s): State<AppState>,
    Json(p): Json<SaveReq>,
) -> Json<Value> {
    let _single = s.engine.action_lock.lock().await;
    let st = match p.state {
        Some(x) => x,
        None => s.engine.state.read().await.clone(),
    };
    let name = if p.name.trim().is_empty() {
        "Zapis"
    } else {
        p.name.trim()
    };

    match s.engine.store.save(name, &st, p.id) {
        Ok(id) => Json(json!({"ok":true,"id":id,"schema":st.schema})),
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

async fn list_saves(State(s): State<AppState>) -> Json<Value> {
    match s.engine.store.list() {
        Ok(v) => Json(json!({
            "ok":true,
            "saves":v.into_iter()
                .map(|(id,name,updated_at)|json!({
                    "id":id,
                    "name":name,
                    "updated_at":updated_at
                }))
                .collect::<Vec<_>>()
        })),
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

#[derive(Deserialize)]
struct LoadReq {
    id: i64,
}

async fn load_save(
    State(s): State<AppState>,
    Json(p): Json<LoadReq>,
) -> Json<Value> {
    match s.engine.store.load(p.id) {
        Ok(st) => {
            match s.engine.replace(st,"load").await {
                Ok(()) => { let st=s.engine.state.read().await; Json(json!({"ok":true,"id":p.id,"state":state_summary(&st)})) },
                Err(e)=>Json(json!({"ok":false,"error":e})),
            }
        }
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

async fn ai_config_get(State(s): State<AppState>) -> Json<Value> {
    Json(json!({
        "ok":true,
        "config":s.engine.ai_config.read().await.clone()
    }))
}

async fn ai_local_status() -> Json<Value> {
    Json(json!({"ok":true,"local_ai":LocalAiRuntime::status_json()}))
}

async fn ai_local_start(State(s): State<AppState>) -> Json<Value> {
    match s.engine.local_ai.ensure_started().await {
        Ok(started) => Json(json!({"ok":true,"started":started,"local_ai":LocalAiRuntime::status_json()})),
        Err(e) => Json(json!({"ok":false,"error":e,"local_ai":LocalAiRuntime::status_json()})),
    }
}

async fn ai_config_set(
    State(s): State<AppState>,
    Json(cfg): Json<AiConfig>,
) -> Json<Value> {
    if let Err(e) = s.engine.store.set_ai_config(&cfg) {
        return Json(json!({"ok":false,"error":e}));
    }
    *s.engine.ai_config.write().await = cfg.clone();
    Json(json!({"ok":true,"config":cfg}))
}


async fn character_create(
    State(s): State<AppState>,
    Json(req): Json<CreateCharacterRequest>,
) -> Json<Value> {
    let character = creator::create(&req);
    match s.engine.mutate("character", |st| {st.character=character.clone();Ok(())}).await {
        Ok(())=> {let st=s.engine.state.read().await;Json(json!({"ok":true,"character":character,"state":state_summary(&st)}))},
        Err(e)=>Json(json!({"ok":false,"error":e})),
    }
}

async fn lore_search(State(s): State<AppState>, Json(mut q): Json<LoreQuery>) -> Json<Value> {
    let st=s.engine.state.read().await;
    q.character_only=true;
    q.known_fact_ids=st.character.knowledge.iter().map(|k|k.id.clone()).collect();
    q.year=st.world.clock.year; q.month=st.world.clock.month; q.day=st.world.clock.day;
    if q.limit == 0 { q.limit = 6; }
    let facts = lore::starter_facts();
    let found = lore::search(&facts, &q);
    Json(json!({"ok":true,"facts":found}))
}

#[derive(Deserialize)]
struct TextReq {
    text: String,
}

async fn magic_parse(Json(p): Json<TextReq>) -> Json<Value> {
    Json(json!({
        "ok":true,
        "semantics":systems::magic_semantics(&p.text)
    }))
}

#[derive(Deserialize)]
struct RecipeReq {
    recipe: String,
}

async fn craft(
    State(s): State<AppState>,
    Json(p): Json<RecipeReq>,
) -> Json<Value> {
    match s.engine.mutate("craft", |st| systems::craft(st, &p.recipe)).await {
        Ok(i) => Json(json!({"ok":true,"item":i})),
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

async fn brew(
    State(s): State<AppState>,
    Json(p): Json<RecipeReq>,
) -> Json<Value> {
    match s.engine.mutate("brew", |st| systems::brew(st, &p.recipe)).await {
        Ok(i) => Json(json!({"ok":true,"item":i})),
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

#[derive(Deserialize)]
struct EvidenceReq {
    clue: String,
    reliability: Option<i32>,
}

async fn hunt_evidence(
    State(s): State<AppState>,
    Json(p): Json<EvidenceReq>,
) -> Json<Value> {
    match s.engine.mutate("hunt_evidence", |st| {
        systems::add_hunt_evidence(st,&p.clue,p.reliability.unwrap_or(60));
        Ok(state_summary(st)["hunt"].clone())
    }).await {
        Ok(hunt)=>Json(json!({"ok":true,"hunt":hunt})),
        Err(e)=>Json(json!({"ok":false,"error":e})),
    }
}

async fn shutdown(State(s): State<AppState>) -> Json<Value> {
    let rt = s.engine.local_ai.clone();
    tokio::spawn(async move {
        rt.stop().await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Json(json!({"ok":true}))
}

