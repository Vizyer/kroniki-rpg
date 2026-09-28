use reqwest::Client;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::{Arc, Mutex, atomic::{AtomicU64,Ordering}}, time::Duration};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct AiRuntime {
    pub client: Client,
    pub cancellations: Arc<Mutex<HashMap<String,Arc<Notify>>>>,
    pub seq: Arc<AtomicU64>,
}

impl AiRuntime {
    pub fn new()->Self{Self{client:Client::builder().build().expect("http client"),cancellations:Arc::new(Mutex::new(HashMap::new())),seq:Arc::new(AtomicU64::new(1))}}
    pub fn new_request_id(&self)->String{format!("gm-{}",self.seq.fetch_add(1,Ordering::Relaxed))}
    pub fn cancel(&self,id:&str)->bool{if let Some(n)=self.cancellations.lock().unwrap().remove(id){n.notify_waiters();true}else{false}}
}

fn gm_schema()->Value{json!({
    "type":"object","additionalProperties":false,
    "properties":{
        "narration":{"type":"string"},
        "suggestions":{"type":"array","maxItems":5,"items":{"type":"string"}},
        "patch":{"type":"object","additionalProperties":false,
            "properties":{
                "npc_memories":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"npc_id":{"type":"string"},"name":{"type":"string"},"text":{"type":"string"},"importance":{"type":"integer","minimum":0,"maximum":5},"private":{"type":"boolean"}},"required":["npc_id","name","text","importance","private"]}},
                "relations":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"npc_id":{"type":"string"},"delta":{"type":"object","additionalProperties":false,"properties":{"trust":{"type":"integer"},"respect":{"type":"integer"},"fear":{"type":"integer"},"liking":{"type":"integer"},"debt":{"type":"integer"},"hostility":{"type":"integer"},"leverage":{"type":"integer"}},"required":["trust","respect","fear","liking","debt","hostility","leverage"]}},"required":["npc_id","delta"]}},
                "quest_events":{"type":"array","maxItems":6,"items":{"type":"object","additionalProperties":false,"properties":{"quest_id":{"type":"string"},"title":{"type":"string"},"status":{"type":"string"},"summary":{"type":"string"},"urgency":{"type":"integer","minimum":0,"maximum":100},"stage":{"type":"integer","minimum":0}},"required":["quest_id","title","status","summary","urgency","stage"]}},
                "clues":{"type":"array","maxItems":6,"items":{"type":"object","additionalProperties":false,"properties":{"text":{"type":"string"},"reliability":{"type":"number"},"quality":{"type":"integer"},"evidence_type":{"type":"string"},"source":{"type":"string"}},"required":["text","reliability","quality","evidence_type","source"]}},
                "world_flags":{"type":"array","maxItems":12,"items":{"type":"object","additionalProperties":false,"properties":{"key":{"type":"string"},"value":{"type":"string"}},"required":["key","value"]}}
            },"required":["npc_memories","relations","quest_events","clues","world_flags"]}
    },"required":["narration","suggestions","patch"]
})}

fn extract_text(v:&Value)->Option<String>{
    if let Some(s)=v.get("output_text").and_then(Value::as_str){return Some(s.to_string());}
    for o in v.get("output")?.as_array()?{
        if let Some(content)=o.get("content").and_then(Value::as_array){for c in content{if let Some(t)=c.get("text").and_then(Value::as_str){return Some(t.to_string());}}}
    }
    None
}

pub async fn call_gm(runtime:&AiRuntime,api_key:&str,model:&str,system:&str,user:&str,request_id:&str,timeout_ms:u64,retries:u32)->Result<Value,String>{
    let notify=Arc::new(Notify::new()); runtime.cancellations.lock().unwrap().insert(request_id.to_string(),notify.clone());
    let body=json!({
        "model":model,
        "input":[
            {"role":"system","content":[{"type":"input_text","text":system}]},
            {"role":"user","content":[{"type":"input_text","text":user}]}
        ],
        "text":{"format":{"type":"json_schema","name":"kroniki_gm_turn","strict":true,"schema":gm_schema()}},
        "reasoning":{"effort":"low"},
        "max_output_tokens":1800
    });
    let mut last_err="AI request failed".to_string();
    for attempt in 0..=retries {
        let fut=runtime.client.post("https://api.openai.com/v1/responses").bearer_auth(api_key).json(&body).send();
        let outcome=tokio::select!{
            _=notify.notified()=>{runtime.cancellations.lock().unwrap().remove(request_id);return Err("cancelled".into());},
            r=tokio::time::timeout(Duration::from_millis(timeout_ms),fut)=>r,
        };
        match outcome {
            Ok(Ok(resp))=>{
                let status=resp.status();
                match resp.json::<Value>().await{
                    Ok(v) if status.is_success()=>{
                        runtime.cancellations.lock().unwrap().remove(request_id);
                        let txt=extract_text(&v).ok_or_else(||"AI response had no output text".to_string())?;
                        return serde_json::from_str::<Value>(&txt).map_err(|e|format!("AI returned invalid structured JSON: {e}"));
                    },
                    Ok(v)=>last_err=format!("OpenAI HTTP {}: {}",status,v.get("error").and_then(|x|x.get("message")).and_then(Value::as_str).unwrap_or("request failed")),
                    Err(e)=>last_err=format!("OpenAI response decode failed: {e}"),
                }
                if status.as_u16()<500 && status.as_u16()!=429 {break;}
            },
            Ok(Err(e))=>last_err=format!("network error: {e}"),
            Err(_)=>last_err=format!("timeout after {timeout_ms} ms"),
        }
        if attempt<retries{tokio::time::sleep(Duration::from_millis(450*(attempt as u64+1))).await;}
    }
    runtime.cancellations.lock().unwrap().remove(request_id);
    Err(last_err)
}
