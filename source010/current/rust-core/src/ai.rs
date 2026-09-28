use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct AiRuntime {
    pub client: Client,
    pub cancellations: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    pub seq: Arc<AtomicU64>,
    local_process: Arc<Mutex<Option<Child>>>,
}

#[derive(Debug, Clone)]
pub struct AiRouteConfig {
    pub mode: String,
    pub remote_api_key: Option<String>,
    pub remote_model: String,
    pub local_endpoint: String,
    pub local_model_path: String,
    pub local_server_path: String,
    pub local_context: u32,
    pub local_gpu_layers: i32,
    pub local_threads: u32,
    pub timeout_ms: u64,
    pub retries: u32,
}

impl AiRuntime {
    pub fn new() -> Self {
        Self {
            client: Client::builder().build().expect("http client"),
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            seq: Arc::new(AtomicU64::new(1)),
            local_process: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_request_id(&self) -> String {
        format!("gm-{}", self.seq.fetch_add(1, Ordering::Relaxed))
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Some(n) = self.cancellations.lock().unwrap().remove(id) {
            n.notify_waiters();
            true
        } else {
            false
        }
    }

    pub fn stop_local_server(&self) {
        if let Some(mut child) = self.local_process.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    async fn local_ready(&self, endpoint: &str) -> bool {
        let url = format!("{}/health", endpoint.trim_end_matches('/'));
        match tokio::time::timeout(
            Duration::from_millis(900),
            self.client.get(url).send(),
        )
        .await
        {
            Ok(Ok(r)) => r.status().is_success(),
            _ => false,
        }
    }

    pub async fn ensure_local_server(&self, cfg: &AiRouteConfig) -> Result<(), String> {
        if self.local_ready(&cfg.local_endpoint).await {
            return Ok(());
        }
        if cfg.local_server_path.trim().is_empty() || !Path::new(&cfg.local_server_path).exists() {
            return Err(format!(
                "Brak llama-server: {}",
                if cfg.local_server_path.is_empty() { "nie skonfigurowano ścieżki" } else { &cfg.local_server_path }
            ));
        }
        if cfg.local_model_path.trim().is_empty() || !Path::new(&cfg.local_model_path).exists() {
            return Err(format!(
                "Brak lokalnego modelu GGUF: {}",
                if cfg.local_model_path.is_empty() { "nie skonfigurowano ścieżki" } else { &cfg.local_model_path }
            ));
        }

        {
            let mut guard = self.local_process.lock().unwrap();
            let needs_spawn = match guard.as_mut() {
                Some(child) => child.try_wait().ok().flatten().is_some(),
                None => true,
            };
            if needs_spawn {
                *guard = None;
                let endpoint = cfg.local_endpoint.trim_end_matches('/');
                let port = endpoint.rsplit(':').next().and_then(|x| x.parse::<u16>().ok()).unwrap_or(17477);
                let mut cmd = Command::new(&cfg.local_server_path);
                cmd.arg("-m")
                    .arg(&cfg.local_model_path)
                    .arg("--host")
                    .arg("127.0.0.1")
                    .arg("--port")
                    .arg(port.to_string())
                    .arg("-c")
                    .arg(cfg.local_context.max(4096).to_string())
                    .arg("-ngl")
                    .arg(cfg.local_gpu_layers.to_string())
                    .arg("-t")
                    .arg(cfg.local_threads.max(2).to_string())
                    .arg("--jinja")
                    .arg("--no-webui")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let child = cmd.spawn().map_err(|e| format!("Nie można uruchomić llama-server: {e}"))?;
                *guard = Some(child);
            }
        }

        for _ in 0..120 {
            if self.local_ready(&cfg.local_endpoint).await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err("Lokalny model nie uruchomił się w ciągu 60 sekund.".into())
    }

    pub async fn is_local_ready(&self, endpoint:&str)->bool { self.local_ready(endpoint).await }
}

pub fn local_status(cfg: &AiRouteConfig) -> Value {
    json!({
        "mode": cfg.mode,
        "endpoint": cfg.local_endpoint,
        "server_path": cfg.local_server_path,
        "server_exists": !cfg.local_server_path.is_empty() && Path::new(&cfg.local_server_path).exists(),
        "model_path": cfg.local_model_path,
        "model_exists": !cfg.local_model_path.is_empty() && Path::new(&cfg.local_model_path).exists(),
        "profile": "Qwen3-8B Q5_K_M",
        "context": cfg.local_context,
        "gpu_layers": cfg.local_gpu_layers,
        "threads": cfg.local_threads,
        "recommended_for": "16 GB RAM / 8 GB VRAM"
    })
}

fn gm_schema() -> Value {
    json!({
        "type":"object","additionalProperties":false,
        "properties":{
            "interpretation":{"type":"object","additionalProperties":false,"properties":{
                "action_type":{"type":"string"},"intent":{"type":"string"},
                "assumptions":{"type":"array","maxItems":6,"items":{"type":"string"}}
            },"required":["action_type","intent","assumptions"]},
            "director":{"type":"object","additionalProperties":false,"properties":{
                "scene_goal":{"type":"string"},"tension":{"type":"integer","minimum":0,"maximum":100},
                "initiative":{"type":"string"},"reason":{"type":"string"}
            },"required":["scene_goal","tension","initiative","reason"]},
            "narration":{"type":"string"},
            "suggestions":{"type":"array","maxItems":5,"items":{"type":"string"}},
            "patch":{"type":"object","additionalProperties":false,
                "properties":{
                    "npc_memories":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"npc_id":{"type":"string"},"name":{"type":"string"},"text":{"type":"string"},"importance":{"type":"integer","minimum":0,"maximum":5},"private":{"type":"boolean"}},"required":["npc_id","name","text","importance","private"]}},
                    "relations":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"npc_id":{"type":"string"},"delta":{"type":"object","additionalProperties":false,"properties":{"trust":{"type":"integer"},"respect":{"type":"integer"},"fear":{"type":"integer"},"liking":{"type":"integer"},"debt":{"type":"integer"},"hostility":{"type":"integer"},"leverage":{"type":"integer"}},"required":["trust","respect","fear","liking","debt","hostility","leverage"]}},"required":["npc_id","delta"]}},
                    "npc_relations":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"from_id":{"type":"string"},"to_id":{"type":"string"},"trust":{"type":"integer"},"respect":{"type":"integer"},"fear":{"type":"integer"},"liking":{"type":"integer"},"hostility":{"type":"integer"}},"required":["from_id","to_id","trust","respect","fear","liking","hostility"]}},
                    "quest_events":{"type":"array","maxItems":6,"items":{"type":"object","additionalProperties":false,"properties":{"quest_id":{"type":"string"},"title":{"type":"string"},"status":{"type":"string"},"summary":{"type":"string"},"urgency":{"type":"integer","minimum":0,"maximum":100},"stage":{"type":"integer","minimum":0}},"required":["quest_id","title","status","summary","urgency","stage"]}},
                    "clues":{"type":"array","maxItems":6,"items":{"type":"object","additionalProperties":false,"properties":{"text":{"type":"string"},"reliability":{"type":"number"},"quality":{"type":"integer"},"evidence_type":{"type":"string"},"source":{"type":"string"}},"required":["text","reliability","quality","evidence_type","source"]}},
                    "knowledge_reveals":{"type":"array","maxItems":6,"items":{"type":"object","additionalProperties":false,"properties":{"fact":{"type":"string"},"source":{"type":"string"},"confidence":{"type":"integer","minimum":0,"maximum":100}},"required":["fact","source","confidence"]}},
                    "faction_events":{"type":"array","maxItems":4,"items":{"type":"object","additionalProperties":false,"properties":{"faction_id":{"type":"string"},"summary":{"type":"string"},"clock_delta":{"type":"integer","minimum":-10,"maximum":10}},"required":["faction_id","summary","clock_delta"]}},
                    "director_threads":{"type":"array","maxItems":5,"items":{"type":"object","additionalProperties":false,"properties":{"id":{"type":"string"},"title":{"type":"string"},"pressure_delta":{"type":"integer","minimum":-20,"maximum":20},"status":{"type":"string"}},"required":["id","title","pressure_delta","status"]}},
                    "world_flags":{"type":"array","maxItems":12,"items":{"type":"object","additionalProperties":false,"properties":{"key":{"type":"string"},"value":{"type":"string"}},"required":["key","value"]}}
                },
                "required":["npc_memories","relations","npc_relations","quest_events","clues","knowledge_reveals","faction_events","director_threads","world_flags"]
            }
        },
        "required":["interpretation","director","narration","suggestions","patch"]
    })
}

fn extract_responses_text(v: &Value) -> Option<String> {
    if let Some(s) = v.get("output_text").and_then(Value::as_str) { return Some(s.to_string()); }
    for o in v.get("output")?.as_array()? {
        if let Some(content) = o.get("content").and_then(Value::as_array) {
            for c in content { if let Some(t) = c.get("text").and_then(Value::as_str) { return Some(t.to_string()); } }
        }
    }
    None
}
fn extract_chat_text(v: &Value) -> Option<String> {
    v.pointer("/choices/0/message/content").and_then(Value::as_str).map(str::to_string)
}
fn parse_json_loose(text: &str) -> Result<Value, String> {
    if let Ok(v) = serde_json::from_str::<Value>(text) { return Ok(v); }
    let start = text.find('{').ok_or_else(|| "AI nie zwróciło obiektu JSON".to_string())?;
    let end = text.rfind('}').ok_or_else(|| "AI nie domknęło obiektu JSON".to_string())?;
    if end <= start { return Err("AI zwróciło uszkodzony JSON".into()); }
    serde_json::from_str::<Value>(&text[start..=end]).map_err(|e| format!("AI zwróciło niepoprawny JSON: {e}"))
}

async fn call_remote_gm(runtime:&AiRuntime,cfg:&AiRouteConfig,system:&str,user:&str,notify:&Arc<Notify>)->Result<Value,String>{
    let key=cfg.remote_api_key.as_deref().filter(|x|!x.trim().is_empty()).ok_or_else(||"Brak klucza API dla trybu online".to_string())?;
    let body=json!({
        "model":cfg.remote_model,
        "input":[{"role":"system","content":[{"type":"input_text","text":system}]},{"role":"user","content":[{"type":"input_text","text":user}]}],
        "text":{"format":{"type":"json_schema","name":"kroniki_gm_turn","strict":true,"schema":gm_schema()}},
        "reasoning":{"effort":"low"},"max_output_tokens":2200
    });
    let mut last_err="AI request failed".to_string();
    for attempt in 0..=cfg.retries {
        let fut=runtime.client.post("https://api.openai.com/v1/responses").bearer_auth(key).json(&body).send();
        let outcome=tokio::select!{_=notify.notified()=>return Err("cancelled".into()),r=tokio::time::timeout(Duration::from_millis(cfg.timeout_ms),fut)=>r};
        match outcome {
            Ok(Ok(resp))=>{
                let status=resp.status();
                match resp.json::<Value>().await{
                    Ok(v) if status.is_success()=>{let txt=extract_responses_text(&v).ok_or_else(||"AI response had no output text".to_string())?;return parse_json_loose(&txt);}
                    Ok(v)=>last_err=format!("OpenAI HTTP {}: {}",status,v.pointer("/error/message").and_then(Value::as_str).unwrap_or("request failed")),
                    Err(e)=>last_err=format!("OpenAI response decode failed: {e}")
                }
                if status.as_u16()<500 && status.as_u16()!=429{break;}
            }
            Ok(Err(e))=>last_err=format!("network error: {e}"),
            Err(_)=>last_err=format!("timeout after {} ms",cfg.timeout_ms),
        }
        if attempt<cfg.retries{tokio::time::sleep(Duration::from_millis(450*(attempt as u64+1))).await;}
    }
    Err(last_err)
}

async fn call_local_gm(runtime:&AiRuntime,cfg:&AiRouteConfig,system:&str,user:&str,notify:&Arc<Notify>)->Result<Value,String>{
    runtime.ensure_local_server(cfg).await?;
    let url=format!("{}/v1/chat/completions",cfg.local_endpoint.trim_end_matches('/'));
    let body=json!({
        "model":"local",
        "messages":[{"role":"system","content":system},{"role":"user","content":user}],
        "temperature":0.72,"top_p":0.92,"max_tokens":2200,
        "response_format":{"type":"json_object"}
    });
    let fut=runtime.client.post(url).json(&body).send();
    let response=tokio::select!{_=notify.notified()=>return Err("cancelled".into()),r=tokio::time::timeout(Duration::from_millis(cfg.timeout_ms.max(120_000)),fut)=>r};
    match response{
        Ok(Ok(resp))=>{let status=resp.status();let v=resp.json::<Value>().await.map_err(|e|format!("Lokalny AI: zła odpowiedź HTTP: {e}"))?;if !status.is_success(){return Err(format!("Lokalny AI HTTP {status}: {v}"));}let txt=extract_chat_text(&v).ok_or_else(||"Lokalny AI nie zwrócił treści".to_string())?;parse_json_loose(&txt)}
        Ok(Err(e))=>Err(format!("Lokalny AI: błąd sieci: {e}")),
        Err(_)=>Err("Lokalny AI: timeout".into()),
    }
}

pub async fn call_gm_routed(runtime:&AiRuntime,cfg:&AiRouteConfig,system:&str,user:&str,request_id:&str)->Result<(Value,String),String>{
    let notify=Arc::new(Notify::new());runtime.cancellations.lock().unwrap().insert(request_id.to_string(),notify.clone());
    let mode=cfg.mode.to_lowercase();
    let result=match mode.as_str(){
        "online"=>call_remote_gm(runtime,cfg,system,user,&notify).await.map(|v|(v,"online".into())),
        "hybrid"=>match call_local_gm(runtime,cfg,system,user,&notify).await{Ok(v)=>Ok((v,"local".into())),Err(local_err)=>match call_remote_gm(runtime,cfg,system,user,&notify).await{Ok(v)=>Ok((v,"online-fallback".into())),Err(remote_err)=>Err(format!("local: {local_err}; online: {remote_err}"))}},
        _=>call_local_gm(runtime,cfg,system,user,&notify).await.map(|v|(v,"local".into())),
    };
    runtime.cancellations.lock().unwrap().remove(request_id);result
}
