use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn clamp_i(v: i64, lo: i64, hi: i64) -> i64 { v.max(lo).min(hi) }
pub fn clamp_f(v: f64, lo: f64, hi: f64) -> f64 { v.max(lo).min(hi) }

fn ensure_object<'a>(root: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    if !root.get(key).map(Value::is_object).unwrap_or(false) {
        root[key] = json!({});
    }
    root.get_mut(key).and_then(Value::as_object_mut).expect("object")
}

fn ensure_array<'a>(root: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    if !root.get(key).map(Value::is_array).unwrap_or(false) {
        root[key] = json!([]);
    }
    root.get_mut(key).and_then(Value::as_array_mut).expect("array")
}

fn world_stamp(state: &Value) -> String {
    let w = state.get("world").unwrap_or(&Value::Null);
    format!("{:02}.{:02}.{} {:02}:{:02}",
        w.get("day").and_then(Value::as_i64).unwrap_or(1),
        w.get("month").and_then(Value::as_i64).unwrap_or(1),
        w.get("year").and_then(Value::as_i64).unwrap_or(1272),
        w.get("hour").and_then(Value::as_i64).unwrap_or(12),
        w.get("minute").and_then(Value::as_i64).unwrap_or(0))
}

pub fn absolute_minutes(state: &Value) -> i64 {
    let w = state.get("world").unwrap_or(&Value::Null);
    let year = w.get("year").and_then(Value::as_i64).unwrap_or(1272);
    let month = w.get("month").and_then(Value::as_i64).unwrap_or(1);
    let day = w.get("day").and_then(Value::as_i64).unwrap_or(1);
    let hour = w.get("hour").and_then(Value::as_i64).unwrap_or(0);
    let minute = w.get("minute").and_then(Value::as_i64).unwrap_or(0);
    ((((year * 12 + (month - 1)) * 30 + (day - 1)) * 24 + hour) * 60) + minute
}

pub fn advance_time(state: &mut Value, minutes: i64) -> Vec<Value> {
    let minutes = minutes.max(0);
    let old_year = state.pointer("/world/year").and_then(Value::as_i64).unwrap_or(1272);
    if !state.get("world").map(Value::is_object).unwrap_or(false) { state["world"] = json!({}); }
    let w = state.get_mut("world").and_then(Value::as_object_mut).unwrap();
    let mut minute = w.get("minute").and_then(Value::as_i64).unwrap_or(0) + minutes;
    let mut hour = w.get("hour").and_then(Value::as_i64).unwrap_or(0) + minute / 60;
    minute %= 60;
    let mut day = w.get("day").and_then(Value::as_i64).unwrap_or(1) + hour / 24;
    hour %= 24;
    let mut month = w.get("month").and_then(Value::as_i64).unwrap_or(1);
    let mut year = old_year;
    while day > 30 { day -= 30; month += 1; if month > 12 { month = 1; year += 1; } }
    w.insert("minute".into(), json!(minute)); w.insert("hour".into(), json!(hour));
    w.insert("day".into(), json!(day)); w.insert("month".into(), json!(month)); w.insert("year".into(), json!(year));
    milestone_events(old_year, year)
}

fn milestone_events(from_year: i64, to_year: i64) -> Vec<Value> {
    let milestones = [
        (1263, "Upadek Cintry"),
        (1267, "Przewrót na Thanedd"),
        (1268, "Brenna i Rivia"),
        (1271, "Wydarzenia pierwszej gry"),
        (1272, "Wojna i Dziki Gon"),
        (1275, "Krew i Wino"),
    ];
    milestones.into_iter().filter(|(y, _)| *y > from_year && *y <= to_year)
        .map(|(y,t)| json!({"year":y,"title":t,"kind":"historical_milestone","knowledge":"world_truth"})).collect()
}

pub fn process_world_tick(mut state: Value, minutes: i64, reason: &str) -> Value {
    let before_abs = absolute_minutes(&state);
    let mut world_events = advance_time(&mut state, minutes);
    let now_abs = absolute_minutes(&state);

    // Quest deadlines are authoritative and never delegated to the narrator.
    if let Some(quests) = state.get_mut("quests").and_then(Value::as_array_mut) {
        for q in quests.iter_mut() {
            if q.get("status").and_then(Value::as_str).unwrap_or("active") != "active" { continue; }
            let deadline = q.get("deadline_abs_minutes").and_then(Value::as_i64);
            if let Some(d) = deadline {
                if before_abs < d && now_abs >= d {
                    q["status"] = json!("failed");
                    q["failed_reason"] = json!("deadline");
                    world_events.push(json!({"kind":"quest_deadline","quest_id":q.get("id").cloned().unwrap_or(json!("unknown")),"title":q.get("title").cloned().unwrap_or(json!("Zadanie"))}));
                }
            }
        }
    }

    // Faction clocks: slow, resource-bound background autonomy.
    if let Some(factions) = state.get_mut("factions").and_then(Value::as_object_mut) {
        let ticks = (minutes / 60).max(0);
        if ticks > 0 {
            for (id, f) in factions.iter_mut() {
                let active = f.get("active").and_then(Value::as_bool).unwrap_or(true);
                if !active { continue; }
                let resources = f.get("resources").and_then(Value::as_i64).unwrap_or(50);
                let influence = f.get("influence").and_then(Value::as_i64).unwrap_or(50);
                let old = f.get("clock").and_then(Value::as_i64).unwrap_or(0);
                let speed = if resources > 70 && influence > 70 { 2 } else { 1 };
                let new_clock = clamp_i(old + ticks * speed, 0, 100);
                f["clock"] = json!(new_clock);
                for threshold in [25,50,75,100] {
                    if old < threshold && new_clock >= threshold {
                        world_events.push(json!({"kind":"faction_move","faction_id":id,"clock":threshold,"public":threshold>=75,"goal":f.get("public_goal").cloned().unwrap_or(json!("nieznany"))}));
                    }
                }
            }
        }
    }

    // NPC plans progress only when enough in-world time passes.
    if let Some(npcs) = state.get_mut("npcs").and_then(Value::as_object_mut) {
        if minutes >= 10 {
            for (id,npc) in npcs.iter_mut() {
                let plan = npc.pointer("/autonomy/plan").and_then(Value::as_str).unwrap_or("").to_string();
                if plan.is_empty() { continue; }
                let risk = npc.pointer("/autonomy/risk_tolerance").and_then(Value::as_i64).unwrap_or(50);
                let old = npc.pointer("/autonomy/progress").and_then(Value::as_i64).unwrap_or(0);
                let delta = ((minutes / 10).max(1) * (40 + risk) / 90).max(1);
                if !npc.get("autonomy").map(Value::is_object).unwrap_or(false) { npc["autonomy"] = json!({}); }
                npc["autonomy"]["progress"] = json!(clamp_i(old+delta,0,100));
                npc["autonomy"]["last_tick_reason"] = json!(reason);
                if old < 100 && old+delta >= 100 {
                    world_events.push(json!({"kind":"npc_plan_completed","npc_id":id,"plan":plan,"public":false}));
                }
            }
        }
    }

    // Information propagation: world truth enters character knowledge only when its due time arrives.
    let mut delivered = Vec::new();
    if let Some(queue) = state.get_mut("news_queue").and_then(Value::as_array_mut) {
        let mut keep = Vec::new();
        for item in queue.drain(..) {
            if item.get("deliver_at_abs_minutes").and_then(Value::as_i64).unwrap_or(i64::MAX) <= now_abs {
                delivered.push(item);
            } else { keep.push(item); }
        }
        *queue = keep;
    }
    if !delivered.is_empty() {
        if !state.get("character").map(Value::is_object).unwrap_or(false) { state["character"] = json!({}); }
        let c = state.get_mut("character").and_then(Value::as_object_mut).unwrap();
        let knowledge = c.entry("known_events").or_insert_with(||json!([]));
        if let Some(a)=knowledge.as_array_mut(){ for e in &delivered { a.push(e.clone()); } }
        world_events.push(json!({"kind":"news_delivered","count":delivered.len()}));
    }

    json!({"ok":true,"state":state,"advanced_minutes":minutes,"world_events":world_events,"delivered_news":delivered,"rule":"world_truth_not_character_knowledge"})
}

pub fn npc_event(mut state: Value, p: &Value) -> Value {
    let npc_id = p.get("npc_id").and_then(Value::as_str).unwrap_or("npc").trim();
    let npc_id = if npc_id.is_empty(){"npc"}else{npc_id};
    let name = p.get("name").and_then(Value::as_str).unwrap_or(npc_id);
    let text = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
    let importance = clamp_i(p.get("importance").and_then(Value::as_i64).unwrap_or(2),0,5);
    let stamp = world_stamp(&state);

    let npcs = ensure_object(&mut state,"npcs");
    if !npcs.contains_key(npc_id) {
        npcs.insert(npc_id.to_string(), json!({
            "id":npc_id,"name":name,"canon":false,"role":p.get("role").cloned().unwrap_or(json!("postać kampanii")),
            "personality":p.get("personality").cloned().unwrap_or(json!([])),
            "goal":p.get("goal").cloned().unwrap_or(json!("")),"hidden_goal":"","fear":"","secret":"",
            "memories":[],"knowledge":[],"autonomy":{"plan":"","progress":0,"resources":50,"risk_tolerance":50}
        }));
    }
    if let Some(npc)=npcs.get_mut(npc_id) {
        npc["last_event"] = json!(text);
        if importance >= 2 && !text.is_empty() {
            if !npc.get("memories").map(Value::is_array).unwrap_or(false) { npc["memories"] = json!([]); }
            let a=npc.get_mut("memories").and_then(Value::as_array_mut).unwrap();
            a.push(json!({"at":stamp,"text":text,"importance":importance,"private":p.get("private").and_then(Value::as_bool).unwrap_or(false)}));
            if a.len()>40 { let n=a.len()-40; a.drain(0..n); }
        }
        if let Some(plan)=p.get("plan").and_then(Value::as_str) {
            if !npc.get("autonomy").map(Value::is_object).unwrap_or(false) { npc["autonomy"] = json!({}); }
            npc["autonomy"]["plan"] = json!(plan); npc["autonomy"]["progress"] = json!(0);
        }
    }

    let relations = ensure_object(&mut state,"relations");
    let rel_key=format!("player::{npc_id}");
    let rel=relations.entry(rel_key.clone()).or_insert_with(||json!({"trust":0,"respect":0,"fear":0,"liking":0,"debt":0,"hostility":0,"leverage":0,"public_status":"neutral","private_notes":[]}));
    for k in ["trust","respect","fear","liking","debt","hostility","leverage"] {
        let delta=p.pointer(&format!("/relation_delta/{k}")).and_then(Value::as_i64).unwrap_or(0);
        if delta!=0 { rel[k]=json!(clamp_i(rel.get(k).and_then(Value::as_i64).unwrap_or(0)+delta,-100,100)); }
    }
    json!({"ok":true,"state":state,"npc_id":npc_id,"relation_key":rel_key})
}

pub fn quest_update(mut state: Value, p: &Value) -> Value {
    let quest_id=p.get("quest_id").and_then(Value::as_str).unwrap_or("quest");
    let title=p.get("title").and_then(Value::as_str).unwrap_or("Zadanie");
    let quests=ensure_array(&mut state,"quests");
    let idx=quests.iter().position(|q|q.get("id").and_then(Value::as_str)==Some(quest_id));
    if idx.is_none(){ quests.push(json!({"id":quest_id,"title":title,"type":p.get("type").cloned().unwrap_or(json!("side")),"status":"active","stage":0,"summary":"","urgency":0,"objectives":[],"rewards":{},"consequences":[]})); }
    let q=quests.iter_mut().find(|q|q.get("id").and_then(Value::as_str)==Some(quest_id)).unwrap();
    for k in ["status","summary"] { if let Some(v)=p.get(k){ q[k]=v.clone(); } }
    if let Some(v)=p.get("urgency").and_then(Value::as_i64){q["urgency"]=json!(clamp_i(v,0,100));}
    if let Some(v)=p.get("stage").and_then(Value::as_i64){q["stage"]=json!(v.max(0));}
    if let Some(deadline)=p.get("deadline_abs_minutes").and_then(Value::as_i64){q["deadline_abs_minutes"]=json!(deadline);}
    if let Some(obj)=p.get("objective") {
        if !q.get("objectives").map(Value::is_array).unwrap_or(false){q["objectives"]=json!([]);}
        let a=q.get_mut("objectives").and_then(Value::as_array_mut).unwrap();
        let oid=obj.get("id").and_then(Value::as_str).unwrap_or("objective");
        if let Some(existing)=a.iter_mut().find(|x|x.get("id").and_then(Value::as_str)==Some(oid)){*existing=obj.clone();} else {a.push(obj.clone());}
    }
    let q_out=q.clone();
    json!({"ok":true,"state":state,"quest":q_out})
}

fn ingredient_counts(state:&Value)->Map<String,Value>{
    state.pointer("/inventory/ingredients").and_then(Value::as_object).cloned().unwrap_or_default()
}
fn set_ingredient_counts(state:&mut Value, counts:Map<String,Value>){
    if !state.get("inventory").map(Value::is_object).unwrap_or(false){state["inventory"]=json!({});}
    state["inventory"]["ingredients"]=Value::Object(counts);
}

pub fn alchemy_craft(mut state:Value,p:&Value)->Value{
    let recipe=p.get("recipe").and_then(Value::as_str).unwrap_or("Jaskółka");
    let catalog:Vec<(&str,i64,Vec<(&str,i64)>,i64)>=vec![
        ("Jaskółka",3,vec![("alkohol",1),("glistnik",2),("mózg utopca",1)],25),
        ("Kot",2,vec![("alkohol",1),("berberka",2)],20),
        ("Grom",4,vec![("alkohol",1),("werbena",2),("eter",1)],35),
        ("Samum",3,vec![("saletra",1),("fosfor",1)],30),
        ("Kartacz",4,vec![("saletra",2),("wapno",1)],40),
        ("Olej na nekrofagi",3,vec![("tłuszcz",1),("jaskółcze ziele",1),("pył kostny",1)],30),
        ("Antidotum",2,vec![("alkohol",1),("węgiel",1),("zioła",1)],20),
    ];
    let Some((_,difficulty,needs,toxicity))=catalog.iter().find(|x|x.0.eq_ignore_ascii_case(recipe)).cloned() else{return json!({"ok":false,"error":"Nieznana receptura","state":state});};
    let skill=p.get("skill").and_then(Value::as_i64).or_else(||state.pointer("/character/alchemy/skill").and_then(Value::as_i64)).unwrap_or(2);
    let tools=p.get("tool_quality").and_then(Value::as_i64).unwrap_or(0);
    let mut counts=ingredient_counts(&state);
    let missing:Vec<String>=needs.iter().filter_map(|(n,q)|{let have=counts.get(*n).and_then(Value::as_i64).unwrap_or(0);if have<*q{Some(format!("{} ({}/{})",n,have,q))}else{None}}).collect();
    if !missing.is_empty(){return json!({"ok":false,"error":"Brak składników","missing":missing,"state":state});}
    let roll=p.get("roll").and_then(Value::as_i64).unwrap_or_else(||stable_d20(&format!("alchemy:{recipe}:{}",world_stamp(&state))));
    let total=roll+skill+tools; let target=10+difficulty; let margin=total-target; let success=margin>=0;
    for (n,q) in &needs {
        let have=counts.get(*n).and_then(Value::as_i64).unwrap_or(0);
        let loss=if success{*q}else{((*q+1)/2).max(1)};
        counts.insert(n.to_string(),json!((have-loss).max(0)));
    }
    set_ingredient_counts(&mut state,counts);
    if success {
        if !state.pointer("/character/alchemy/prepared").map(Value::is_array).unwrap_or(false){
            if !state.get("character").map(Value::is_object).unwrap_or(false){state["character"]=json!({});}
            if !state["character"].get("alchemy").map(Value::is_object).unwrap_or(false){state["character"]["alchemy"]=json!({});}
            state["character"]["alchemy"]["prepared"]=json!([]);
        }
        let quality=clamp_i(50+margin*5,20,100);
        state["character"]["alchemy"]["prepared"].as_array_mut().unwrap().push(json!({"name":recipe,"quality":quality,"freshness":100,"toxicity":toxicity,"uses":if recipe.contains("Olej"){3}else{1}}));
    }
    json!({"ok":success,"state":state,"resolution":{"recipe":recipe,"roll":roll,"skill":skill,"difficulty":target,"margin":margin,"quality":clamp_i(50+margin*5,0,100),"toxicity":toxicity,"partial_loss":!success}})
}

pub fn crafting_craft(mut state:Value,p:&Value)->Value{
    let item=p.get("item").and_then(Value::as_str).unwrap_or("Zestaw naprawczy");
    let difficulty=p.get("difficulty").and_then(Value::as_i64).unwrap_or(3);
    let skill=p.get("skill").and_then(Value::as_i64).or_else(||state.pointer("/character/crafting/skill").and_then(Value::as_i64)).unwrap_or(2);
    let material_quality=clamp_i(p.get("material_quality").and_then(Value::as_i64).unwrap_or(50),0,100);
    let tool_quality=clamp_i(p.get("tool_quality").and_then(Value::as_i64).unwrap_or(0),-3,5);
    let minutes=p.get("minutes").and_then(Value::as_i64).unwrap_or(60).max(5);
    let roll=p.get("roll").and_then(Value::as_i64).unwrap_or_else(||stable_d20(&format!("craft:{item}:{}",world_stamp(&state))));
    let target=10+difficulty; let margin=roll+skill+tool_quality-target; let success=margin>=0;
    let _=advance_time(&mut state,minutes);
    if success {
        if !state.pointer("/inventory/items").map(Value::is_array).unwrap_or(false){if !state.get("inventory").map(Value::is_object).unwrap_or(false){state["inventory"]=json!({});}state["inventory"]["items"]=json!([]);}
        let q=clamp_i(material_quality+margin*4,10,100); state["inventory"]["items"].as_array_mut().unwrap().push(json!({"name":item,"quality":q,"condition":100,"crafted":true}));
    }
    json!({"ok":success,"state":state,"resolution":{"item":item,"roll":roll,"difficulty":target,"margin":margin,"time_minutes":minutes,"quality":clamp_i(material_quality+margin*4,0,100)}})
}

fn magic_semantics(intent:&str)->(String,String,i64,i64){
    let s=intent.to_lowercase();
    let source=if s.contains("ogie")||s.contains("płomie")||s.contains("igni"){"fire"}else if s.contains("wod")||s.contains("lód")||s.contains("mróz"){"water"}else if s.contains("wiatr")||s.contains("powiet")||s.contains("aard"){"air"}else if s.contains("ziem")||s.contains("kamień")||s.contains("yrden"){"earth"}else{"chaos"};
    let op=if s.contains("wycz")||s.contains("bada")||s.contains("analiz")||s.contains("rezon")||s.contains("energia")||s.contains("pash iritor"){"sense"}else if s.contains("tarc")||s.contains("barier")||s.contains("quen"){"shield"}else if s.contains("odep")||s.contains("rzuc")||s.contains("pchn")||s.contains("aard"){"force"}else if s.contains("spal")||s.contains("podpal")||s.contains("igni"){"damage"}else if s.contains("spowol")||s.contains("uwię")||s.contains("yrden"){"control"}else if s.contains("wpły")||s.contains("uspok")||s.contains("axii"){"mind"}else{"shape"};
    let scale=if s.contains("cały")||s.contains("obszar")||s.contains("wielk")||s.contains("burz"){3}else if s.contains("kilka")||s.contains("grup")||s.contains("szerok"){2}else{1};
    let range=if s.contains("daleko")||s.contains("odleg")||s.contains("horyzont"){3}else if s.contains("dystans")||s.contains("kilkanaście")||s.contains("10 m"){2}else{1};
    (source.into(),op.into(),scale,range)
}

pub fn magic_cast(mut state:Value,p:&Value)->Value{
    let intent=p.get("intent").and_then(Value::as_str).unwrap_or("zaklęcie"); let hasty=p.get("hasty").and_then(Value::as_bool).unwrap_or(false);
    let (source,operation,scale,range)=magic_semantics(intent);
    let control=state.pointer("/character/magic/control").and_then(Value::as_i64).unwrap_or(3);
    let vigor=state.pointer("/character/magic/vigor").and_then(Value::as_i64).unwrap_or(6);
    let stamina=state.pointer("/character/magic/stamina").and_then(Value::as_i64).unwrap_or(10);
    let saturation=state.pointer("/character/magic/chaos_saturation").and_then(Value::as_i64).unwrap_or(0);
    let concentration=state.pointer("/character/magic/concentration").and_then(Value::as_i64).unwrap_or(100);
    let pressure=state.pointer("/combat/pressure").and_then(Value::as_i64).unwrap_or(0);
    let pressure_penalty=if pressure>=9{3}else if pressure>=6{2}else if pressure>=3{1}else{0};
    let learned=p.get("learned").and_then(Value::as_bool).unwrap_or(false);
    let channel_seconds=p.get("channel_seconds").and_then(Value::as_i64).unwrap_or(0).max(0);
    let channel_bonus=(channel_seconds/4).min(3);
    let sensing_bonus=if operation=="sense"{2}else{0};
    let mut difficulty=5+scale*2+range+pressure_penalty+(if hasty{2}else{0})-(if learned{2}else{0})-channel_bonus-sensing_bonus;
    difficulty=difficulty.max(2);
    let mut cost=1+scale+range+(if operation=="shield"{1}else{0})-(if learned{1}else{0})-(channel_bonus/2); cost=cost.max(1);
    let mut instability=(scale-1)+(range-1)+(if source=="chaos"{1}else{0})+(if hasty{1}else{0})-(channel_bonus/2); instability=instability.max(0);
    if cost>vigor{instability+=cost-vigor;}
    if saturation>=61{instability+=2}else if saturation>=41{instability+=1}
    let roll=p.get("roll").and_then(Value::as_i64).unwrap_or_else(||stable_d20(&format!("magic:{intent}:{}",world_stamp(&state))));
    let margin=roll+control-difficulty; let success=margin>=0;
    let cast_seconds=(2+scale*2+range+channel_seconds-(if hasty{2}else{0})).max(1);
    if !state.get("character").map(Value::is_object).unwrap_or(false){state["character"]=json!({});}
    if !state["character"].get("magic").map(Value::is_object).unwrap_or(false){state["character"]["magic"]=json!({});}
    state["character"]["magic"]["stamina"]=json!((stamina-cost).max(0));
    state["character"]["magic"]["chaos_saturation"]=json!(clamp_i(saturation+cost*2+instability*3,0,100));
    state["character"]["magic"]["concentration"]=json!(clamp_i(concentration-(if success{instability*2}else{8+instability*3}),0,100));
    let effects=match operation.as_str(){
        "shield"=>json!({"magic_shield":2+scale,"guard":0,"tempo":0}),
        "force"=>json!({"enemy_guard":-scale,"pressure":1+scale/2,"distance":1.0+scale as f64*0.5}),
        "damage"=>json!({"damage":scale,"pressure":scale,"panic":scale>=2}),
        "control"=>json!({"zone_control":scale,"enemy_tempo":-1,"pressure":1}),
        "mind"=>json!({"enemy_tempo":-1,"interrupt":success,"deescalation":scale}),
        "sense"=>json!({"knowledge_probe":success,"depth":clamp_i(1+(margin.max(0)/5),1,4),"interference":instability,"rule":"reveals_only_validated_information"}),
        _=>json!({"fictional_effect":true,"scale":scale}),
    };
    let backlash=if !success && instability>=2 {Some(match source.as_str(){"fire"=>"oparzenie / niekontrolowany żar","air"=>"uderzenie zwrotne / utrata równowagi","water"=>"wychłodzenie / skurcz","earth"=>"uraz przeciążeniowy","chaos"=>"zaburzenie percepcji / rezonans",_=>"przeciążenie"})}else{None};
    json!({"ok":success,"state":state,"resolution":{"intent":intent,"source":source,"operation":operation,"scale":scale,"range":range,"cost":cost,"difficulty":difficulty,"roll":roll,"control":control,"margin":margin,"success":success,"instability":instability,"cast_seconds":cast_seconds,"effects":effects,"backlash":backlash,"contract":"mechanics_first_narration_second"}})
}

pub fn hunt_action(mut state:Value,p:&Value)->Value{
    if !state.get("hunting").map(Value::is_object).unwrap_or(false){state["hunting"]=json!({"evidence":[],"hypotheses":[],"confidence":0,"preparation":0});}
    if !state["hunting"].get("evidence").map(Value::is_array).unwrap_or(false){state["hunting"]["evidence"]=json!([]);}
    if !state["hunting"].get("hypotheses").map(Value::is_array).unwrap_or(false){state["hunting"]["hypotheses"]=json!([]);}
    if state["hunting"].get("confidence").and_then(Value::as_i64).is_none(){state["hunting"]["confidence"]=json!(0);}
    if state["hunting"].get("preparation").and_then(Value::as_i64).is_none(){state["hunting"]["preparation"]=json!(0);}
    let kind=p.get("kind").and_then(Value::as_str).unwrap_or("clue");
    match kind {
        "clue"=>{
            let reliability=clamp_f(p.get("reliability").and_then(Value::as_f64).unwrap_or(0.6),0.0,1.0);
            let quality=clamp_i(p.get("quality").and_then(Value::as_i64).unwrap_or((reliability*100.0) as i64),0,100);
            let text=p.get("text").and_then(Value::as_str).unwrap_or("Nowy trop");
            state["hunting"]["evidence"].as_array_mut().unwrap().push(json!({"text":text,"type":p.get("evidence_type").cloned().unwrap_or(json!("observation")),"source":p.get("source").cloned().unwrap_or(json!("observation")),"reliability":reliability,"quality":quality,"confirmed":false}));
            let old=state["hunting"]["confidence"].as_i64().unwrap_or(0); state["hunting"]["confidence"]=json!(clamp_i(old+(reliability*12.0).round() as i64,0,100));
        },
        "hypothesis"=>{
            let label=p.get("label").and_then(Value::as_str).unwrap_or("Nieznany potwór"); let evidence_count=state["hunting"]["evidence"].as_array().map(|a|a.len()).unwrap_or(0) as i64;
            let confidence=clamp_i(15+evidence_count*12,0,85); state["hunting"]["hypotheses"].as_array_mut().unwrap().push(json!({"label":label,"confidence":confidence,"player_theory":true,"truth_revealed":false}));
        },
        "prepare"=>{let delta=clamp_i(p.get("delta").and_then(Value::as_i64).unwrap_or(15),1,40);let old=state["hunting"]["preparation"].as_i64().unwrap_or(0);state["hunting"]["preparation"]=json!(clamp_i(old+delta,0,100));},
        "contradiction"=>{let old=state["hunting"]["confidence"].as_i64().unwrap_or(0);state["hunting"]["confidence"]=json!(clamp_i(old-10,0,100));},
        _=>{}
    }
    let hunting_out=state.get("hunting").cloned().unwrap_or(json!({}));
    json!({"ok":true,"state":state,"hunting":hunting_out,"rule":"RAG_truth_must_not_leak_species"})
}

pub fn combat_pulse(mut combat:Value, action:&str)->Value{
    let g=combat.get("enemy_guard").and_then(Value::as_i64).unwrap_or(4);let guard=combat.get("guard").and_then(Value::as_i64).unwrap_or(6);let pressure=combat.get("pressure").and_then(Value::as_i64).unwrap_or(0);let tempo=combat.get("tempo").and_then(Value::as_i64).unwrap_or(0);let dist=combat.get("distance").and_then(Value::as_f64).unwrap_or(4.0);let morale=combat.get("morale").and_then(Value::as_i64).unwrap_or(70);
    let mut time=3.0;
    match action {
        "Natarcie"=>{combat["enemy_guard"]=json!((g-1).max(0));combat["pressure"]=json!(pressure+1);combat["morale"]=json!((morale-3).max(0));time=2.8},
        "Finta"=>{combat["enemy_guard"]=json!((g-2).max(0));combat["tempo"]=json!((tempo+1).min(3));combat["morale"]=json!((morale-2).max(0));time=2.4},
        "Obrona"=>{combat["guard"]=json!((guard+2).min(10));combat["pressure"]=json!((pressure-1).max(0));time=2.0},
        "Pozycja"=>{combat["tempo"]=json!((tempo+1).min(3));combat["distance"]=json!((dist-0.5).max(1.0));time=2.2},
        "Aard"=>{combat["enemy_guard"]=json!((g-1).max(0));combat["pressure"]=json!(pressure+2);combat["distance"]=json!(dist+1.5);combat["morale"]=json!((morale-5).max(0));time=1.8},
        "Odwrót"=>{combat["distance"]=json!(dist+2.0);combat["tempo"]=json!((tempo-1).max(-3));time=3.0},_=>{}
    }
    json!({"combat":combat,"mechanics":{"action":action,"time_seconds":time,"contract":"pulse_resolution"}})
}

pub fn resolve_player_action(mut state:Value,action:&str,p:&Value)->Value{
    let a=action.to_lowercase();
    let mut seconds=30i64;
    if a.contains("przeszuk")||a.contains("badam")||a.contains("zbada") {seconds=600;}
    else if a.contains("rozmaw")||a.contains("pytam")||a.contains("mówię") {seconds=90;}
    else if a.contains("biegn")||a.contains("idę")||a.contains("ruszam") {seconds=180;}
    else if a.contains("czekam") {seconds=3600;}
    let minutes=(seconds+59)/60; let events=advance_time(&mut state,minutes);
    let mut check=Value::Null;
    if a.contains("badam")||a.contains("szuk")||a.contains("trop") {
        let stat=state.pointer("/character/stats/PER").and_then(Value::as_i64).unwrap_or(3);
        let roll=p.get("roll").and_then(Value::as_i64).unwrap_or_else(||stable_d20(&format!("action:{action}:{}",world_stamp(&state))));
        let difficulty=p.get("difficulty").and_then(Value::as_i64).unwrap_or(12); let total=roll+stat; check=json!({"skill":"Percepcja","roll":roll,"modifier":stat,"difficulty":difficulty,"margin":total-difficulty,"success":total>=difficulty});
    }
    json!({"state":state,"mechanics":{"action":action,"time_seconds":seconds,"check":check,"world_events":events,"authoritative":true}})
}

pub fn apply_ai_patch(mut state:Value,patch:&Value)->Value{
    // AI can only alter narrative/social structures. HP, gold, inventory quantities, combat and magic resources are excluded.
    if let Some(memories)=patch.get("npc_memories").and_then(Value::as_array){
        for m in memories.iter().take(8){ state=npc_event(state,&json!({"npc_id":m.get("npc_id").cloned().unwrap_or(json!("npc")),"name":m.get("name").cloned().unwrap_or(json!("NPC")),"text":m.get("text").cloned().unwrap_or(json!("")),"importance":m.get("importance").cloned().unwrap_or(json!(2)),"private":m.get("private").cloned().unwrap_or(json!(false))}))["state"].clone(); }
    }
    if let Some(rels)=patch.get("relations").and_then(Value::as_array){
        for r in rels.iter().take(8){let id=r.get("npc_id").and_then(Value::as_str).unwrap_or("npc");state=npc_event(state,&json!({"npc_id":id,"text":"","importance":0,"relation_delta":r.get("delta").cloned().unwrap_or(json!({}))}))["state"].clone();}
    }
    if let Some(qs)=patch.get("quest_events").and_then(Value::as_array){
        for q in qs.iter().take(6){state=quest_update(state,q)["state"].clone();}
    }
    if let Some(clues)=patch.get("clues").and_then(Value::as_array){
        for c in clues.iter().take(6){let mut cp=c.clone();if let Some(o)=cp.as_object_mut(){o.insert("kind".into(),json!("clue"));}state=hunt_action(state,&cp)["state"].clone();}
    }
    if let Some(flags)=patch.get("world_flags").and_then(Value::as_array){
        let map=ensure_object(&mut state,"campaign_flags");
        for f in flags.iter().take(12){if let Some(k)=f.get("key").and_then(Value::as_str){if let Some(v)=f.get("value"){if v.is_boolean()||v.is_number()||v.is_string(){map.insert(k.chars().take(80).collect(),v.clone());}}}}
    }
    state
}

pub fn append_chronicle(state:&mut Value,title:&str,text:&str){
    let stamp=world_stamp(state); let arr=ensure_array(state,"chronicle"); arr.push(json!({"date":stamp,"title":title,"text":text})); if arr.len()>250{let n=arr.len()-250;arr.drain(0..n);}
}

pub fn stable_d20(seed:&str)->i64{let mut h=DefaultHasher::new();seed.hash(&mut h);(h.finish()%20) as i64+1}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn time_advances(){let s=json!({"world":{"year":1272,"month":1,"day":30,"hour":23,"minute":30}});let r=process_world_tick(s,90,"test");assert_eq!(r.pointer("/state/world/month").unwrap(),&json!(2));assert_eq!(r.pointer("/state/world/day").unwrap(),&json!(1));}
    #[test]fn npc_memory_is_bounded(){let mut s=json!({});for i in 0..50{s=npc_event(s,&json!({"npc_id":"marta","text":format!("e{i}"),"importance":3}))["state"].clone();}assert_eq!(s.pointer("/npcs/marta/memories").unwrap().as_array().unwrap().len(),40);}
    #[test]fn ai_patch_cannot_touch_hp(){let s=json!({"character":{"hp":12}});let p=json!({"world_flags":[{"key":"x","value":true}],"hp":999});let out=apply_ai_patch(s,&p);assert_eq!(out.pointer("/character/hp"),Some(&json!(12)));}
}

