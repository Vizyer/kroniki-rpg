mod ai;
mod systems;

use ai::{call_gm, AiRuntime};
use axum::{extract::State, http::{Method,StatusCode}, routing::{get,post}, Json, Router};
use rusqlite::{params, Connection};
use serde_json::{json,Value};
use std::{path::PathBuf, sync::{Arc,Mutex}};
use systems::*;
use tower_http::cors::{Any,CorsLayer};

const GAME_VERSION:&str="0.9.0-preview.1";
const SAVE_SCHEMA:i64=2;

#[derive(Clone)]
struct AppState{db:Arc<Mutex<Connection>>,lore:Arc<Value>,ai:AiRuntime}

fn data_dir()->PathBuf{
    if let Ok(p)=std::env::var("LOCALAPPDATA"){return PathBuf::from(p).join("KronikiRPG");}
    std::env::current_dir().unwrap_or_else(|_|PathBuf::from(".")).join("data")
}
fn init_db()->Connection{
    let d=data_dir();let _=std::fs::create_dir_all(&d);let c=Connection::open(d.join("kroniki.sqlite3")).expect("sqlite");
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS saves(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,state_json TEXT NOT NULL,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP);
      CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS migrations(version INTEGER PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP);").unwrap();
    let _=c.execute("INSERT OR IGNORE INTO migrations(version) VALUES(?1)",params![SAVE_SCHEMA]);
    c
}
fn load_lore()->Value{serde_json::from_str(include_str!("../data/lore-corpus.json")).unwrap_or_else(|_|json!({"entries":[]}))}
fn get_setting(db:&Connection,key:&str)->Option<String>{db.query_row("SELECT value FROM settings WHERE key=?1",params![key],|r|r.get::<_,String>(0)).ok()}
fn set_setting(db:&Connection,key:&str,value:&str){let _=db.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]);}

async fn health()->Json<Value>{Json(json!({"ok":true,"engine":"Rust Core","version":GAME_VERSION,"save_schema":SAVE_SCHEMA,"transport":"localhost","authoritative_mechanics":true}))}

fn lore_rank(lore:&Value,q:&str,year:i64,location:&str,limit:usize)->Vec<Value>{
    let q=q.to_lowercase();let loc=location.to_lowercase();let mut scored:Vec<(i64,Value)>=Vec::new();
    for e in lore.get("entries").and_then(Value::as_array).cloned().unwrap_or_default(){
        if let Some(v)=e.get("valid"){
            let a=v.get("fromYear").and_then(Value::as_i64).unwrap_or(-9999);let b=v.get("toYear").and_then(Value::as_i64).unwrap_or(9999);if year<a||year>b{continue;}
        }
        let mut hay=String::new();for k in ["title","summary","type"]{if let Some(x)=e.get(k).and_then(Value::as_str){hay.push_str(x);hay.push(' ');}}
        for k in ["tags","regions","aliases"]{if let Some(a)=e.get(k).and_then(Value::as_array){for x in a{if let Some(t)=x.as_str(){hay.push_str(t);hay.push(' ');}}}}
        let h=hay.to_lowercase();let mut score=0;for term in q.split_whitespace(){if term.len()>2&&h.contains(term){score+=4;}}if !loc.is_empty()&&h.contains(&loc){score+=3;}if score==0&&q.trim().is_empty(){score=1;}if score>0{scored.push((score,e));}
    }
    scored.sort_by(|a,b|b.0.cmp(&a.0));scored.into_iter().take(limit).map(|x|x.1).collect()
}
async fn lore_search(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{
    let q=p.get("q").and_then(Value::as_str).unwrap_or("");let year=p.get("year").and_then(Value::as_i64).unwrap_or(1272);let loc=p.get("location").and_then(Value::as_str).unwrap_or("");
    Json(json!({"results":lore_rank(&s.lore,q,year,loc,30)}))
}

fn migrate_state(mut st:Value)->Value{
    if !st.is_object(){st=json!({});}
    if !st.get("_meta").map(Value::is_object).unwrap_or(false){st["_meta"]=json!({});}
    let old=st.pointer("/_meta/save_schema").and_then(Value::as_i64).unwrap_or(1);
    if old<2{
        if !st.get("relations").map(Value::is_object).unwrap_or(false){st["relations"]=json!({});}
        if !st.get("npcs").map(Value::is_object).unwrap_or(false){st["npcs"]=json!({});}
        if !st.get("factions").map(Value::is_object).unwrap_or(false){st["factions"]=json!({});}
        if !st.get("quests").map(Value::is_array).unwrap_or(false){st["quests"]=json!([]);}
        if !st.get("news_queue").map(Value::is_array).unwrap_or(false){st["news_queue"]=json!([]);}
        if !st.get("hunting").map(Value::is_object).unwrap_or(false){st["hunting"]=json!({"evidence":[],"hypotheses":[],"confidence":0,"preparation":0});}
    }
    st["_meta"]["game_version"]=json!(GAME_VERSION);st["_meta"]["save_schema"]=json!(SAVE_SCHEMA);st
}
async fn save(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{
    let name=p.get("name").and_then(Value::as_str).map(str::trim).filter(|x|!x.is_empty()).unwrap_or("Zapis");
    let st=migrate_state(p.get("state").cloned().unwrap_or(json!({})));
    let txt=match serde_json::to_string(&st){
        Ok(x)=>x,
        Err(e)=>return Json(json!({"ok":false,"error":format!("Nie można zakodować stanu zapisu: {e}")})),
    };
    let c=s.db.lock().unwrap();
    if let Some(id)=p.get("id").and_then(Value::as_i64){
        return match c.execute("UPDATE saves SET name=?1,state_json=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",params![name,txt,id]){
            Ok(n) if n>0=>Json(json!({"ok":true,"id":id,"schema":SAVE_SCHEMA})),
            Ok(_)=>Json(json!({"ok":false,"id":id,"error":"Nie znaleziono zapisu do nadpisania."})),
            Err(e)=>Json(json!({"ok":false,"id":id,"error":e.to_string()})),
        };
    }
    match c.execute("INSERT INTO saves(name,state_json) VALUES(?1,?2)",params![name,txt]){
        Ok(1)=>Json(json!({"ok":true,"id":c.last_insert_rowid(),"schema":SAVE_SCHEMA})),
        Ok(_)=>Json(json!({"ok":false,"error":"SQLite nie utworzył rekordu zapisu."})),
        Err(e)=>Json(json!({"ok":false,"error":e.to_string()})),
    }
}

async fn list_saves(State(s):State<AppState>)->Json<Value>{let c=s.db.lock().unwrap();let mut stmt=match c.prepare("SELECT id,name,updated_at FROM saves ORDER BY updated_at DESC"){Ok(x)=>x,Err(e)=>return Json(json!({"error":e.to_string(),"results":[]}))};let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"updated_at":r.get::<_,String>(2)?}))).ok();Json(json!({"results":rows.map(|xs|xs.filter_map(Result::ok).collect::<Vec<_>>()).unwrap_or_default()}))}
async fn load_save(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let id=p.get("id").and_then(Value::as_i64).unwrap_or(-1);let c=s.db.lock().unwrap();match c.query_row("SELECT state_json FROM saves WHERE id=?1",params![id],|r|r.get::<_,String>(0)){Ok(t)=>Json(json!({"state":migrate_state(serde_json::from_str::<Value>(&t).unwrap_or(json!({}))),"id":id})),Err(e)=>Json(json!({"error":e.to_string()}))}}
async fn delete_save(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let id=p.get("id").and_then(Value::as_i64).unwrap_or(-1);let c=s.db.lock().unwrap();let n=c.execute("DELETE FROM saves WHERE id=?1",params![id]).unwrap_or(0);Json(json!({"ok":n>0}))}

async fn world_tick(Json(p):Json<Value>)->Json<Value>{Json(process_world_tick(p.get("state").cloned().unwrap_or(json!({})),p.get("minutes").and_then(Value::as_i64).unwrap_or(0),p.get("reason").and_then(Value::as_str).unwrap_or("world_tick")))}
async fn combat(Json(p):Json<Value>)->Json<Value>{Json(combat_pulse(p.get("combat").cloned().unwrap_or(json!({})),p.get("action").and_then(Value::as_str).unwrap_or("")))}
async fn magic(Json(p):Json<Value>)->Json<Value>{Json(magic_cast(p.get("state").cloned().unwrap_or(json!({})),&p))}
async fn hunt(Json(p):Json<Value>)->Json<Value>{Json(hunt_action(p.get("state").cloned().unwrap_or(json!({})),&p))}
async fn alchemy(Json(p):Json<Value>)->Json<Value>{Json(alchemy_craft(p.get("state").cloned().unwrap_or(json!({})),&p))}
async fn crafting(Json(p):Json<Value>)->Json<Value>{Json(crafting_craft(p.get("state").cloned().unwrap_or(json!({})),&p))}
async fn npc(State(_s):State<AppState>,Json(p):Json<Value>)->Json<Value>{Json(npc_event(p.get("state").cloned().unwrap_or(json!({})),&p))}
async fn quest(Json(p):Json<Value>)->Json<Value>{Json(quest_update(p.get("state").cloned().unwrap_or(json!({})),&p))}

async fn character_generate(Json(p):Json<Value>)->Json<Value>{let prof=p.get("profession").and_then(Value::as_str).unwrap_or("Łowca");let concept=p.get("concept").and_then(Value::as_str).unwrap_or("");let origin=p.get("origin").and_then(Value::as_str).unwrap_or("Własna droga");Json(json!({"name":"Aldren","profession":prof,"origin_story":origin,"concept":concept,"stats":{"STR":3,"DEX":4,"CON":3,"INT":4,"PER":5,"CHA":3},"knowledge":{"history":1,"politics":1,"geography":2,"monsters":if prof.to_lowercase().contains("wiedź") {4}else{2},"magic":if prof.to_lowercase().contains("czarod") {4}else{1}},"magic":{"control":if prof.to_lowercase().contains("czarod") {5}else{2},"vigor":if prof.to_lowercase().contains("czarod") {8}else{4},"stamina":10,"chaos_saturation":0,"concentration":100},"alchemy":{"skill":if prof.to_lowercase().contains("wiedź") {4}else{1},"prepared":[]},"crafting":{"skill":2},"generated":"local_proposal"}))}

async fn ai_config_get(State(s):State<AppState>)->Json<Value>{let c=s.db.lock().unwrap();let configured=get_setting(&c,"openai_api_key").map(|x|!x.trim().is_empty()).unwrap_or_else(||std::env::var("OPENAI_API_KEY").map(|x|!x.trim().is_empty()).unwrap_or(false));Json(json!({"configured":configured,"model":get_setting(&c,"ai_model").unwrap_or_else(||"gpt-5.6-luna".into()),"timeout_ms":get_setting(&c,"ai_timeout_ms").and_then(|x|x.parse::<u64>().ok()).unwrap_or(45000),"retries":get_setting(&c,"ai_retries").and_then(|x|x.parse::<u32>().ok()).unwrap_or(1)}))}
async fn ai_config_set(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{{let c=s.db.lock().unwrap();if p.get("clear_key").and_then(Value::as_bool).unwrap_or(false){set_setting(&c,"openai_api_key","");}else if let Some(k)=p.get("api_key").and_then(Value::as_str){if !k.trim().is_empty(){set_setting(&c,"openai_api_key",k.trim());}}if let Some(m)=p.get("model").and_then(Value::as_str){if !m.trim().is_empty(){set_setting(&c,"ai_model",m.trim());}}if let Some(t)=p.get("timeout_ms").and_then(Value::as_u64){set_setting(&c,"ai_timeout_ms",&t.clamp(5000,120000).to_string());}if let Some(r)=p.get("retries").and_then(Value::as_u64){set_setting(&c,"ai_retries",&r.min(3).to_string());}}ai_config_get(State(s)).await}
async fn ai_cancel(State(s):State<AppState>,Json(p):Json<Value>)->Json<Value>{let id=p.get("request_id").and_then(Value::as_str).unwrap_or("");Json(json!({"ok":s.ai.cancel(id),"request_id":id}))}

fn state_for_prompt(state:&Value)->Value{json!({"character":state.get("character").cloned().unwrap_or(json!({})),"world":state.get("world").cloned().unwrap_or(json!({})),"combat":state.get("combat").cloned().unwrap_or(json!({})),"npcs":state.get("npcs").cloned().unwrap_or(json!({})),"relations":state.get("relations").cloned().unwrap_or(json!({})),"quests":state.get("quests").cloned().unwrap_or(json!([])),"hunting":state.get("hunting").cloned().unwrap_or(json!({})),"campaign_flags":state.get("campaign_flags").cloned().unwrap_or(json!({})),"recent_chronicle":state.get("chronicle").and_then(Value::as_array).map(|a|a.iter().rev().take(6).cloned().collect::<Vec<_>>()).unwrap_or_default()})}
fn local_fallback(action:&str,mechanics:&Value)->(String,Vec<String>){let check=mechanics.pointer("/check/success").and_then(Value::as_bool);let extra=match check{Some(true)=>" Dostrzegasz coś, co wcześniej ginęło w tle sceny.",Some(false)=>" Próba nie daje pewnej odpowiedzi; świat nie zdradza więcej, niż rzeczywiście udało się ustalić.",None=>" Świat reaguje na tę decyzję, a czas płynie dalej."};(format!("Podejmujesz działanie: {action}.{extra}"),vec!["Rozejrzyj się uważniej".into(),"Porozmawiaj z obecną osobą".into(),"Zmień pozycję lub miejsce".into()])}

async fn action(State(s):State<AppState>,Json(p):Json<Value>)->(StatusCode,Json<Value>){
    let action=p.get("action").and_then(Value::as_str).unwrap_or("działanie").trim().to_string();
    let state0=migrate_state(p.get("state").cloned().unwrap_or(json!({})));
    let mechanics_result=resolve_player_action(state0,&action,&p);let state_after=mechanics_result.get("state").cloned().unwrap_or(json!({}));let mechanics=mechanics_result.get("mechanics").cloned().unwrap_or(json!({}));
    let year=state_after.pointer("/world/year").and_then(Value::as_i64).unwrap_or(1272);let loc=state_after.pointer("/world/location").and_then(Value::as_str).unwrap_or("");let lore=lore_rank(&s.lore,&format!("{} {}",action,loc),year,loc,8);
    let request_id=p.get("request_id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(||s.ai.new_request_id());
    let (api_key,model,timeout_ms,retries)={let c=s.db.lock().unwrap();(get_setting(&c,"openai_api_key").filter(|x|!x.trim().is_empty()).or_else(||std::env::var("OPENAI_API_KEY").ok()),get_setting(&c,"ai_model").unwrap_or_else(||"gpt-5.6-luna".into()),get_setting(&c,"ai_timeout_ms").and_then(|x|x.parse::<u64>().ok()).unwrap_or(45000),get_setting(&c,"ai_retries").and_then(|x|x.parse::<u32>().ok()).unwrap_or(1))};
    let system="Jesteś Mistrzem Gry w mrocznym RPG osadzonym w kontinuitetcie saga + gry CDPR do końca Blood and Wine. Pisz po polsku. Mechanika przekazana w MECHANICAL_RESOLUTION jest nadrzędna: nie zmieniaj wyniku, obrażeń, zasobów ani skali efektu. Lore context to fakty dla MG, nie automatyczna wiedza postaci. Nie ujawniaj przyszłych wydarzeń ani tajemnic, jeśli bohater nie ma wiarygodnego kanału wiedzy. Nie cytuj źródeł ani tekstów książek/gier; twórz oryginalną narrację na podstawie faktów. NPC mają własne cele, pamięć i ograniczoną wiedzę. Zwróć wyłącznie obiekt zgodny ze schematem. Patch może zmieniać tylko pamięć/relacje/questy/tropy/flagi narracyjne; nie wolno nim zmieniać HP, pieniędzy, ekwipunku, walki, magii ani wyników testów.";
    let user=json!({"PLAYER_ACTION":action,"MECHANICAL_RESOLUTION":mechanics,"CAMPAIGN_STATE":state_for_prompt(&state_after),"LORE_CONTEXT":lore.iter().map(|e|json!({"id":e.get("id"),"title":e.get("title"),"summary":e.get("summary"),"knowledgeTier":e.get("knowledgeTier"),"branch":e.get("branch"),"valid":e.get("valid")})).collect::<Vec<_>>()});
    let use_ai=p.get("use_ai").and_then(Value::as_bool).unwrap_or(true);
    let ai_result=if use_ai{if let Some(key)=api_key{call_gm(&s.ai,&key,&model,system,&user.to_string(),&request_id,timeout_ms,retries).await.ok()}else{None}}else{None};
    let (narration,suggestions,patch,ai_used,ai_error)=if let Some(v)=ai_result{(v.get("narration").and_then(Value::as_str).unwrap_or("Swiat reaguje.").to_string(),v.get("suggestions").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter_map(|x|x.as_str().map(str::to_string)).take(5).collect::<Vec<_>>(),v.get("patch").cloned().unwrap_or(json!({})),true,Value::Null)}else{let(f,sug)=local_fallback(&action,&mechanics);(f,sug,json!({"npc_memories":[],"relations":[],"quest_events":[],"clues":[],"world_flags":[]}),false,if use_ai{json!("AI niedostępne, timeout/anulowanie lub brak klucza — użyto lokalnego fallbacku.")}else{Value::Null})};
    let mut final_state=apply_ai_patch(state_after,&patch);append_chronicle(&mut final_state,"Scena",&narration);
    (StatusCode::OK,Json(json!({"ok":true,"request_id":request_id,"narration":narration,"suggestions":suggestions,"patch":patch,"state":final_state,"mechanics":mechanics,"ai_used":ai_used,"ai_error":ai_error,"lore_refs":lore.iter().filter_map(|e|e.get("id").and_then(Value::as_str)).collect::<Vec<_>>() })))
}

async fn shutdown()->Json<Value>{tokio::spawn(async{tokio::time::sleep(std::time::Duration::from_millis(150)).await;std::process::exit(0);});Json(json!({"ok":true,"shutting_down":true}))}

#[tokio::main]
async fn main(){
    let state=AppState{db:Arc::new(Mutex::new(init_db())),lore:Arc::new(load_lore()),ai:AiRuntime::new()};
    let cors=CorsLayer::new().allow_origin(Any).allow_methods([Method::GET,Method::POST]).allow_headers(Any);
    let app=Router::new()
      .route("/health",get(health)).route("/shutdown",post(shutdown)).route("/lore/search",post(lore_search))
      .route("/save",post(save)).route("/saves",get(list_saves)).route("/save/load",post(load_save)).route("/save/delete",post(delete_save))
      .route("/world/tick",post(world_tick)).route("/combat/pulse",post(combat)).route("/magic/cast",post(magic)).route("/hunt/action",post(hunt))
      .route("/alchemy/craft",post(alchemy)).route("/crafting/craft",post(crafting)).route("/npc/event",post(npc)).route("/quest/update",post(quest))
      .route("/character/generate",post(character_generate)).route("/ai/config",get(ai_config_get).post(ai_config_set)).route("/ai/cancel",post(ai_cancel)).route("/action",post(action))
      .layer(cors).with_state(state);
    let listener=tokio::net::TcpListener::bind("127.0.0.1:17377").await.expect("port 17377");println!("Kroniki Rust Core {}: http://127.0.0.1:17377",GAME_VERSION);axum::serve(listener,app).await.unwrap();
}

