use axum::{extract::State, http::Method, routing::{get, post}, Json, Router};
use kroniki_core::{
    ai::AiConfig,
    domain::{state_summary, GameState, PlayerAction},
    engine::Engine,
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
    let app_state = AppState { engine };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/state", get(get_state).post(set_state))
        .route("/action", post(action))
        .route("/tick", post(tick))
        .route("/save", post(save))
        .route("/saves", get(list_saves))
        .route("/save/load", post(load_save))
        .route("/ai/config", get(ai_config_get).post(ai_config_set))
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
        "ai": s.engine.ai_config.read().await.clone()
    }))
}

async fn get_state(State(s): State<AppState>) -> Json<Value> {
    Json(state_summary(&s.engine.state.read().await))
}

async fn set_state(
    State(s): State<AppState>,
    Json(v): Json<GameState>,
) -> Json<Value> {
    *s.engine.state.write().await = v.clone();
    Json(json!({"ok":true,"state":v}))
}

async fn action(
    State(s): State<AppState>,
    Json(p): Json<PlayerAction>,
) -> Json<Value> {
    match s.engine.act(p).await {
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
    let events = s.engine.tick(p.minutes.max(0)).await;
    Json(json!({
        "ok":true,
        "events":events,
        "state":state_summary(&s.engine.state.read().await)
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
            *s.engine.state.write().await = st.clone();
            Json(json!({"ok":true,"id":p.id,"state":st}))
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
    let mut st = s.engine.state.write().await;
    match systems::craft(&mut st, &p.recipe) {
        Ok(i) => Json(json!({"ok":true,"item":i})),
        Err(e) => Json(json!({"ok":false,"error":e})),
    }
}

async fn brew(
    State(s): State<AppState>,
    Json(p): Json<RecipeReq>,
) -> Json<Value> {
    let mut st = s.engine.state.write().await;
    match systems::brew(&mut st, &p.recipe) {
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
    let mut st = s.engine.state.write().await;
    systems::add_hunt_evidence(
        &mut st,
        &p.clue,
        p.reliability.unwrap_or(60),
    );
    Json(json!({"ok":true,"hunt":st.world.hunt}))
}

async fn shutdown() -> Json<Value> {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Json(json!({"ok":true}))
}
