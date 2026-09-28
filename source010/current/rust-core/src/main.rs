mod ai;
mod director;
mod systems;
mod vertical_slice;

use ai::{call_gm_routed, local_status, AiRouteConfig, AiRuntime};
use axum::{extract::State, http::{Method,StatusCode}, routing::{get,post}, Json, Router};
use director::{apply_director_patch, deterministic_interpretation, director_prepass, ensure_director_state, fallback_turn};
use rusqlite::{params, Connection};
use serde_json::{json,Value};
use std::{path::PathBuf, sync::{Arc,Mutex}};
use systems::*;
use tower_http::cors::{Any,CorsLayer};
use vertical_slice::{apply_slice_progress, new_vertical_slice};

const GAME_VERSION:&str="0.10.0-preview.1";
const SAVE_SCHEMA:i64=3;

#[derive(Clone)]
struct AppState{db:Arc<Mutex<Connection>>,lore:Arc<Value>,ai:AiRuntime}

fn data_dir()->PathBuf{
    if let Ok(p)=std::env::var("LOCALAPPDATA"){return PathBuf::from(p).join("KronikiRPG");}
    std::env::current_dir().unwrap_or_else(|_|PathBuf::from(".")).join("data")
}
fn core_dir()->PathBuf{std::env::current_exe().ok().and_then(|p|p.parent().map(|x|x.to_path_buf())).unwrap_or_else(||PathBuf::from("."))}
fn default_server_path()->String{core_dir().join("ai").join("llama-server.exe").to_string_lossy().to_string()}
fn default_model_path()->String{data_dir().join("models").join("Qwen3-8B-Q5_K_M.gguf").to_string_lossy().to_string()}
fn init_db()->Connection{
    let d=data_dir();let _=std::fs::create_dir_all(&d);let _=std::fs::create_dir_all(d.join("models"));
    let c=Connection::open(d.join("kroniki.sqlite3")).expect("sqlite");
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS saves(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,state_json TEXT NOT NULL,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP);
      CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS migrations(version INTEGER PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP);").unwrap();
    let _=c.execute("INSERT OR IGNORE INTO migrations(version) VALUES(?1)",params![SAVE_SCHEMA]);c
}
fn load_lore()->Value{serde_json::from_str(include_str!("../data/lore-corpus.json")).unwrap_or_else(|_|json!({"entries":[]}))}
fn get_setting(db:&Connection,key:&str)->Option<String>{db.query_row("SELECT value FROM settings WHERE key=?1",params![key],|r|r.get::<_,String>(0)).ok()}
fn set_setting(db:&Connection,key:&str,value:&str)->Result<(),String>{db.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map(|_|()).map_err(|e|e.to_string())}

fn route_config(db:&Connection)->AiRouteConfig{
    let mode=get_setting(db,"ai_mode").unwrap_or_else(||"local".into());
    AiRouteConfig{
        mode,
        remote_api_key:get_setting(db,"openai_api_key").filter(|x|!x.trim().is_empty()).or_else(||std::env::var("OPENAI_API_KEY").ok()),
        remote_model:get_setting(db,"ai_model").unwrap_or_else(||"gpt-5.6-luna".into()),
        local_endpoint:get_setting(db,"local_ai_endpoint").unwrap_or_else(||"http://127.0.0.1:17477".into()),
        local_model_path:get_setting(db,"local_model_path").unwrap_or_else(default_model_path),
        local_server_path:get_setting(db,"local_server_path").unwrap_or_else(default_server_path),
        local_context:get_setting(db,"local_context").and_then(|x|x.parse().ok()).unwrap_or(12288),
        local_gpu_layers:get_setting(db,"local_gpu_layers").and_then(|x|x.parse().ok()).unwrap_or(99),
        local_threads:get_setting(db,"local_threads").and_then(|x|x.parse().ok()).unwrap_or_else(||std::thread::available_parallelism().map(|n|n.get() as u32).unwrap_or(6).saturating_sub(2).max(2)),
        timeout_ms:get_setting(db,"ai_timeout_ms").and_then(|x|x.parse().ok()).unwrap_or(120000),
        retries:get_setting(db,"ai_retries").and_then(|x|x.parse().ok()).unwrap_or(1),
    }
}

async fn health(State(s):State<AppState>)->Json<Value>{
    let cfg={let c=s.db.lock().unwrap();route_config(&c)};
    Json(json!({"ok":true,"engine":"Rust Core","version":GAME_VERSION,"save_schema":SAVE_SCHEMA,"transport":"localhost","authoritative_mechanics":true,"ai":{"mode":cfg.mode,"profile":"Qwen3-8B Q5_K_M"}}))
}

fn lore_rank(lore:&Value,q:&str,year:i64,location:&str,limit:usize)->Vec<Value>{
    let q=q.to_lowercase();let loc=location.to_lowercase();let mut scored:Vec<(i64,Value)>=Vec::new();
    for e in lore.get("entries").and_then(Value::as_array).cloned().unwrap_or_default(){
        if let Some(v)=e.get("valid"){let a=v.get("fromYear").and_then(Value::as_i64).unwrap_or(-9999);let b=v.get("toYear").and_then(Value::as_i64).unwrap_or(9999);if year<a||year>b{continue;}}
        let mut hay=String::new();for k in ["title","summary","type"]{if let Some(x)=e.get(k).and_then(Value::as_str){hay.push_str(x);hay.push(' ');}}
        for k in ["tags","regions","aliases"]{if let Some(a)=e.get(k).and_then(Value::as_array){for x in a{if let Some(t)=x.as_str(){hay.push_str(t);hay.push(' ');}}}}
        let h=hay.to_lowercase();let mut score=0;for term in q.split_whitespace(){if term.len()>2&&h.contains(term){score+=4;}}if !loc.is_empty()&&h.contains(&loc){score+=3;}if score==0&&q.trim().is_empty(){score=1;}if score>0{scored.push((score,e));}
    }
    scored.sort_by(|a,b|b.0.cmp(&a.0));scored.into_iter().take(limit).map(|x|x.1).collect()
}
async fn lore_search(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let q=p.get("q").and_then(Value::as_str).unwrap_or("");let year=p.get("year").and_then(Value::as_i64).unwrap_or(1272);let loc=p.get("location").and_then(Value::as_str).unwrap_or("");Json(json!({"results":lore_rank(&s.lore,q,year,loc,30)}))}

fn migrate_state(mut st:Value)->Value{
    if !st.is_object(){st=json!({});} if !st.get("_meta").map(Value::is_object).unwrap_or(false){st["_meta"]=json!({});}
    let old=st.pointer("/_meta/save_schema").and_then(Value::as_i64).unwrap_or(1);
    if old<2{if !st.get("relations").map(Value::is_object).unwrap_or(false){st["relations"]=json!({});}if !st.get("npcs").map(Value::is_object).unwrap_or(false){st["npcs"]=json!({});}if !st.get("factions").map(Value::is_object).unwrap_or(false){st["factions"]=json!({});}if !st.get("quests").map(Value::is_array).unwrap_or(false){st["quests"]=json!([]);}if !st.get("news_queue").map(Value::is_array).unwrap_or(false){st["news_queue"]=json!([]);}if !st.get("hunting").map(Value::is_object).unwrap_or(false){st["hunting"]=json!({"evidence":[],"hypotheses":[],"confidence":0,"preparation":0});}}
    if old<3{
        ensure_director_state(&mut st);
        if !st.get("campaign_flags").map(Value::is_object).unwrap_or(false){st["campaign_flags"]=json!({});}
        if !st.pointer("/character/known_facts").map(Value::is_array).unwrap_or(false){if !st.get("character").map(Value::is_object).unwrap_or(false){st["character"]=json!({});}st["character"]["known_facts"]=json!([]);}
    }
    st["_meta"]["game_version"]=json!(GAME_VERSION);st["_meta"]["save_schema"]=json!(SAVE_SCHEMA);st
}

async fn save(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{
    let name=p.get("name").and_then(Value::as_str).map(str::trim).filter(|x|!x.is_empty()).unwrap_or("Zapis");let st=migrate_state(p.get("state").cloned().unwrap_or(json!({})));
    let txt=match serde_json::to_string(&st){Ok(x)=>x,Err(e)=>return Json(json!({"ok":false,"error":format!("Nie można zakodować stanu zapisu: {e}")}))};let c=s.db.lock().unwrap();
    if let Some(id)=p.get("id").and_then(Value::as_i64){return match c.execute("UPDATE saves SET name=?1,state_json=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",params![name,txt,id]){Ok(n) if n>0=>Json(json!({"ok":true,"id":id,"schema":SAVE_SCHEMA})),Ok(_)=>Json(json!({"ok":false,"id":id,"error":"Nie znaleziono zapisu do nadpisania."})),Err(e)=>Json(json!({"ok":false,"id":id,"error":e.to_string()}))};}
    match c.execute("INSERT INTO saves(name,state_json) VALUES(?1,?2)",params![name,txt]){Ok(1)=>Json(json!({"ok":true,"id":c.last_insert_rowid(),"schema":SAVE_SCHEMA})),Ok(_)=>Json(json!({"ok":false,"error":"SQLite nie utworzył rekordu zapisu."})),Err(e)=>Json(json!({"ok":false,"error":e.to_string()}))}
}
async fn list_saves(State(s):State<AppState>)->Json<Value>{let c=s.db.lock().unwrap();let mut stmt=match c.prepare("SELECT id,name,updated_at FROM saves ORDER BY updated_at DESC"){Ok(x)=>x,Err(e)=>return Json(json!({"error":e.to_string(),"results":[]}))};let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"updated_at":r.get::<_,String>(2)?}))).ok();Json(json!({"results":rows.map(|xs|xs.filter_map(Result::ok).collect::<Vec<_>>()).unwrap_or_default()}))}
async fn load_save(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let id=p.get("id").and_then(Value::as_i64).unwrap_or(-1);let c=s.db.lock().unwrap();match c.query_row("SELECT state_json FROM saves WHERE id=?1",params![id],|r|r.get::<_,String>(0)){Ok(t)=>Json(json!({"state":migrate_state(serde_json::from_str::<Value>(&t).unwrap_or(json!({}))),"id":id})),Err(e)=>Json(json!({"error":e.to_string()}))}}
async fn delete_save(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let id=p.get("id").and_then(Value::as_i64).unwrap_or(-1);let c=s.db.lock().unwrap();let n=c.execute("DELETE FROM saves WHERE id=?1",params![id]).unwrap_or(0);Json(json!({"ok":n>0}))}

async fn world_tick(Json(p):Json<Value>)->Json<Value>{Json(process_world_tick(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),p.get("minutes").and_then(Value::as_i64).unwrap_or(0),p.get("reason").and_then(Value::as_str).unwrap_or("world_tick")))}
async fn combat(Json(p):Json<Value>)->Json<Value>{Json(combat_pulse(p.get("combat").cloned().unwrap_or(json!({})),p.get("action").and_then(Value::as_str).unwrap_or("")))}
async fn magic(Json(p):Json<Value>)->Json<Value>{Json(magic_cast(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),&p))}
async fn hunt(Json(p):Json<Value>)->Json<Value>{Json(hunt_action(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),&p))}
async fn alchemy(Json(p):Json<Value>)->Json<Value>{Json(alchemy_craft(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),&p))}
async fn crafting(Json(p):Json<Value>)->Json<Value>{Json(crafting_craft(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),&p))}
async fn npc(Json(p):Json<Value>)->Json<Value>{Json(npc_event(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),&p))}
async fn quest(Json(p):Json<Value>)->Json<Value>{Json(quest_update(migrate_state(p.get("state").cloned().unwrap_or(json!({}))),&p))}
async fn director_tick(Json(p):Json<Value>)->Json<Value>{let st=migrate_state(p.get("state").cloned().unwrap_or(json!({})));Json(director_prepass(st,p.get("reason").and_then(Value::as_str).unwrap_or("director tick")))}
async fn campaign_new(Json(p):Json<Value>)->Json<Value>{let character=p.get("character").cloned();Json(json!({"ok":true,"state":new_vertical_slice(character),"version":GAME_VERSION,"template":"vertical_slice_010"}))}

async fn character_generate(Json(p):Json<Value>)->Json<Value>{let prof=p.get("profession").and_then(Value::as_str).unwrap_or("Łowca");let concept=p.get("concept").and_then(Value::as_str).unwrap_or("");let origin=p.get("origin").and_then(Value::as_str).unwrap_or("Własna droga");Json(json!({"name":"Aldren","profession":prof,"origin_story":origin,"concept":concept,"stats":{"STR":3,"DEX":4,"CON":3,"INT":4,"PER":5,"CHA":3},"knowledge":{"history":1,"politics":1,"geography":2,"monsters":if prof.to_lowercase().contains("wiedź") {4}else{2},"magic":if prof.to_lowercase().contains("czarod") {4}else{1}},"magic":{"control":if prof.to_lowercase().contains("czarod") {5}else{2},"vigor":if prof.to_lowercase().contains("czarod") {8}else{4},"stamina":10,"chaos_saturation":0,"concentration":100},"alchemy":{"skill":if prof.to_lowercase().contains("wiedź") {4}else{1},"prepared":[]},"crafting":{"skill":2},"generated":"local_proposal"}))}

async fn ai_config_get(State(s):State<AppState>)->Json<Value>{let c=s.db.lock().unwrap();let cfg=route_config(&c);let mut status=local_status(&cfg);status["remote_configured"]=json!(cfg.remote_api_key.is_some());status["remote_model"]=json!(cfg.remote_model);status["timeout_ms"]=json!(cfg.timeout_ms);status["retries"]=json!(cfg.retries);Json(status)}
async fn ai_config_set(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{{let c=s.db.lock().unwrap();
    if let Some(mode)=p.get("mode").and_then(Value::as_str){if ["local","hybrid","online"].contains(&mode){let _=set_setting(&c,"ai_mode",mode);}}
    if p.get("clear_key").and_then(Value::as_bool).unwrap_or(false){let _=set_setting(&c,"openai_api_key","");}else if let Some(k)=p.get("api_key").and_then(Value::as_str){if !k.trim().is_empty(){let _=set_setting(&c,"openai_api_key",k.trim());}}
    for (json_key,setting) in [("model","ai_model"),("local_model_path","local_model_path"),("local_server_path","local_server_path"),("local_endpoint","local_ai_endpoint")] {if let Some(v)=p.get(json_key).and_then(Value::as_str){if !v.trim().is_empty(){let _=set_setting(&c,setting,v.trim());}}}
    if let Some(v)=p.get("local_context").and_then(Value::as_u64){let _=set_setting(&c,"local_context",&v.clamp(4096,32768).to_string());}
    if let Some(v)=p.get("local_gpu_layers").and_then(Value::as_i64){let _=set_setting(&c,"local_gpu_layers",&v.clamp(0,999).to_string());}
    if let Some(v)=p.get("local_threads").and_then(Value::as_u64){let _=set_setting(&c,"local_threads",&v.clamp(2,64).to_string());}
    if let Some(v)=p.get("timeout_ms").and_then(Value::as_u64){let _=set_setting(&c,"ai_timeout_ms",&v.clamp(5000,300000).to_string());}
    if let Some(v)=p.get("retries").and_then(Value::as_u64){let _=set_setting(&c,"ai_retries",&v.min(3).to_string());}
}ai_config_get(State(s)).await}
async fn ai_status(State(s):State<AppState>)->Json<Value>{let c=s.db.lock().unwrap();let cfg=route_config(&c);drop(c);let mut v=local_status(&cfg);v["ready"]=json!(s.ai.is_local_ready(&cfg.local_endpoint).await);v["remote_configured"]=json!(cfg.remote_api_key.is_some());Json(v)}
async fn ai_start_local(State(s):State<AppState>)->Json<Value>{let cfg={let c=s.db.lock().unwrap();route_config(&c)};match s.ai.ensure_local_server(&cfg).await{Ok(_)=>Json(json!({"ok":true,"backend":"local","status":local_status(&cfg)})),Err(e)=>Json(json!({"ok":false,"error":e,"status":local_status(&cfg)}))}}
async fn ai_cancel(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let id=p.get("request_id").and_then(Value::as_str).unwrap_or("");Json(json!({"ok":s.ai.cancel(id),"request_id":id}))}

fn state_for_prompt(state:&Value)->Value{json!({
    "character":state.get("character").cloned().unwrap_or(json!({})),"world":state.get("world").cloned().unwrap_or(json!({})),"combat":state.get("combat").cloned().unwrap_or(json!({})),
    "npcs":state.get("npcs").cloned().unwrap_or(json!({})),"relations":state.get("relations").cloned().unwrap_or(json!({})),"quests":state.get("quests").cloned().unwrap_or(json!([])),
    "hunting":state.get("hunting").cloned().unwrap_or(json!({})),"factions":state.get("factions").cloned().unwrap_or(json!({})),"campaign_flags":state.get("campaign_flags").cloned().unwrap_or(json!({})),
    "known_facts":state.pointer("/character/known_facts").cloned().unwrap_or(json!([])),"recent_chronicle":state.get("chronicle").and_then(Value::as_array).map(|a|a.iter().rev().take(8).cloned().collect::<Vec<_>>()).unwrap_or_default()
})}

fn mechanics_for_action(state:Value, action:&str, p:&Value, interpretation:&Value)->Value{
    let kind=interpretation.get("action_type").and_then(Value::as_str).unwrap_or("general_action");
    if kind=="magic_attempt"{
        let mut mp=json!({"intent":action,"state":state,"hasty":p.get("hasty").cloned().unwrap_or(json!(false)),"channel_seconds":p.get("channel_seconds").cloned().unwrap_or(json!(0)),"learned":p.get("learned").cloned().unwrap_or(json!(false))});
        if let Some(r)=p.get("roll"){mp["roll"]=r.clone();}
        let r=magic_cast(mp["state"].clone(),&mp);json!({"state":r.get("state").cloned().unwrap_or(json!({})),"mechanics":{"action":action,"magic":r.get("resolution").cloned().unwrap_or(json!({})),"authoritative":true}})
    }else{resolve_player_action(state,action,p)}
}

async fn action(State(s):State<AppState>,Json(p):Json<Value>)->(StatusCode,Json<Value>){
    let action=p.get("action").and_then(Value::as_str).unwrap_or("działanie").trim().to_string();if action.is_empty(){return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"Pusta deklaracja działania."})));}
    let state0=migrate_state(p.get("state").cloned().unwrap_or_else(||new_vertical_slice(None)));
    let interpretation=deterministic_interpretation(&action,&state0);
    let mechanics_result=mechanics_for_action(state0,&action,&p,&interpretation);let mut state_after=mechanics_result.get("state").cloned().unwrap_or(json!({}));let mechanics=mechanics_result.get("mechanics").cloned().unwrap_or(json!({}));
    state_after=apply_slice_progress(state_after,&action);
    let director_result=director_prepass(state_after,&action);let state_directed=director_result.get("state").cloned().unwrap_or(json!({}));
    let year=state_directed.pointer("/world/year").and_then(Value::as_i64).unwrap_or(1272);let loc=state_directed.pointer("/world/location").and_then(Value::as_str).unwrap_or("");let lore=lore_rank(&s.lore,&format!("{} {}",action,loc),year,loc,8);
    let request_id=p.get("request_id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(||s.ai.new_request_id());
    let cfg={let c=s.db.lock().unwrap();route_config(&c)};
    let system=r#"Jesteś autonomicznym Mistrzem Gry w mrocznym RPG osadzonym w kontinuitetcie sagi wiedźmińskiej i gier CDPR do końca Blood and Wine. Pisz naturalnie po polsku.

ZASADY AUTORYTETU:
- Gracz kontroluje słowa, myśli, zamiary i próby swojej postaci. Nie kontroluje prawdy świata, NPC ani wyniku akcji.
- PLAYER_ACTION może zawierać założenia (np. „energia miejsca jest wzburzona”). Nie uznawaj ich za prawdę, dopóki WORLD TRUTH / mechanika / wiarygodna wiedza ich nie potwierdzi.
- MECHANICAL_RESOLUTION jest nadrzędne. Nie zmieniaj wyniku, kosztu, obrażeń, zasobów ani skali efektu.
- DIRECTOR_CONTEXT opisuje aktywne wątki, presję i istniejące źródła inicjatywy. Inicjuj zdarzenia samodzielnie, gdy wynikają z NPC, wątku, zegara, miejsca, frakcji, zagrożenia lub wcześniejszej decyzji. Nie twórz losowych twistów bez przyczyny.
- NPC mają własne cele, pamięć, relacje, sekrety, ograniczoną wiedzę i mogą kłamać, odmawiać, uciekać, działać poza ekranem.
- LORE_CONTEXT to fakty dla MG. Nie jest automatycznie wiedzą bohatera. Nie ujawniaj przyszłego lore ani ukrytej prawdy bez kanału wiedzy.
- Nie pytaj gracza o doprecyzowanie, jeśli rozsądnie możesz wywnioskować intencję z tekstu i kontekstu. Wybierz najbardziej prawdopodobną interpretację i prowadź scenę dalej.
- Kończ odpowiedź nową sytuacją, napięciem, reakcją lub naturalnym punktem decyzji, zamiast biernego „co robisz?”.
- Twórz oryginalną narrację; nie cytuj książek ani gier.

PATCH:
Możesz proponować wyłącznie zmiany społeczne/narracyjne zgodne ze schematem: pamięć NPC, relacje, NPC↔NPC, questy, tropy, wiedza zdobyta przez postać, ruchy istniejących frakcji, istniejące wątki Director i proste flagi. Nie wolno patchować HP, złota, ekwipunku, wyników walki, zasobów magii ani mechaniki."#;
    let user=json!({"PLAYER_ACTION":action,"INTERPRETATION":interpretation,"MECHANICAL_RESOLUTION":mechanics,"DIRECTOR_CONTEXT":director_result,"CAMPAIGN_STATE":state_for_prompt(&state_directed),"LORE_CONTEXT":lore.iter().map(|e|json!({"id":e.get("id"),"title":e.get("title"),"summary":e.get("summary"),"knowledgeTier":e.get("knowledgeTier"),"branch":e.get("branch"),"valid":e.get("valid")})).collect::<Vec<_>>()});
    let use_ai=p.get("use_ai").and_then(Value::as_bool).unwrap_or(true);
    let ai_result=if use_ai{call_gm_routed(&s.ai,&cfg,system,&user.to_string(),&request_id).await.ok()}else{None};
    let (turn,backend,ai_error)=match ai_result{Some((v,b))=>(v,b,Value::Null),None=>(fallback_turn(&action,&interpretation,&mechanics,&director_result),"deterministic-fallback".into(),if use_ai{json!("Lokalny/online MGAI był niedostępny; użyto bezpiecznego autonomicznego fallbacku.")}else{Value::Null})};
    let patch=turn.get("patch").cloned().unwrap_or(json!({}));let mut final_state=apply_ai_patch(state_directed,&patch);final_state=apply_director_patch(final_state,&patch);
    if let Some(d)=turn.get("director"){if let Some(t)=d.get("tension").and_then(Value::as_i64){ensure_director_state(&mut final_state);final_state["director"]["tension"]=json!(t.clamp(0,100));}}
    let narration=turn.get("narration").and_then(Value::as_str).unwrap_or("Świat reaguje.").to_string();append_chronicle(&mut final_state,"Scena",&narration);
    let suggestions=turn.get("suggestions").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter_map(|x|x.as_str().map(str::to_string)).take(5).collect::<Vec<_>>();
    (StatusCode::OK,Json(json!({"ok":true,"request_id":request_id,"narration":narration,"suggestions":suggestions,"patch":patch,"state":final_state,"mechanics":mechanics,"interpretation":turn.get("interpretation").cloned().unwrap_or(interpretation),"director":turn.get("director").cloned().unwrap_or_else(||director_result.clone()),"ai_backend":backend,"ai_error":ai_error,"lore_refs":lore.iter().filter_map(|e|e.get("id").and_then(Value::as_str)).collect::<Vec<_>>() })))
}

async fn shutdown(State(s):State<AppState>)->Json<Value>{s.ai.stop_local_server();tokio::spawn(async{tokio::time::sleep(std::time::Duration::from_millis(150)).await;std::process::exit(0);});Json(json!({"ok":true,"shutting_down":true}))}

#[tokio::main]
async fn main(){
    let state=AppState{db:Arc::new(Mutex::new(init_db())),lore:Arc::new(load_lore()),ai:AiRuntime::new()};let cors=CorsLayer::new().allow_origin(Any).allow_methods([Method::GET,Method::POST]).allow_headers(Any);
    let app=Router::new()
      .route("/health",get(health)).route("/shutdown",post(shutdown)).route("/lore/search",post(lore_search))
      .route("/save",post(save)).route("/saves",get(list_saves)).route("/save/load",post(load_save)).route("/save/delete",post(delete_save))
      .route("/world/tick",post(world_tick)).route("/combat/pulse",post(combat)).route("/magic/cast",post(magic)).route("/hunt/action",post(hunt))
      .route("/alchemy/craft",post(alchemy)).route("/crafting/craft",post(crafting)).route("/npc/event",post(npc)).route("/quest/update",post(quest)).route("/director/tick",post(director_tick))
      .route("/campaign/new",post(campaign_new)).route("/character/generate",post(character_generate))
      .route("/ai/config",get(ai_config_get).post(ai_config_set)).route("/ai/status",get(ai_status)).route("/ai/start-local",post(ai_start_local)).route("/ai/cancel",post(ai_cancel)).route("/action",post(action))
      .layer(cors).with_state(state);
    let listener=tokio::net::TcpListener::bind("127.0.0.1:17377").await.expect("port 17377");println!("Kroniki Rust Core {}: http://127.0.0.1:17377",GAME_VERSION);axum::serve(listener,app).await.unwrap();
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn migrate_adds_director(){let s=migrate_state(json!({"_meta":{"save_schema":2},"character":{}}));assert!(s.get("director").is_some());assert_eq!(s.pointer("/_meta/save_schema"),Some(&json!(3)));}
    #[test]fn magic_action_is_detected(){let st=new_vertical_slice(None);let i=deterministic_interpretation("Pash iritor — badam energię miejsca",&st);let r=mechanics_for_action(st,"Pash iritor — badam energię miejsca",&json!({"roll":20}),&i);assert!(r.pointer("/mechanics/magic").is_some());}
}
