use serde_json::{json, Map, Value};

fn clamp(v:i64,lo:i64,hi:i64)->i64{v.max(lo).min(hi)}

fn ensure_obj<'a>(root:&'a mut Value,key:&str)->&'a mut Map<String,Value>{
    if !root.get(key).map(Value::is_object).unwrap_or(false){root[key]=json!({});}
    root.get_mut(key).and_then(Value::as_object_mut).expect("object")
}
fn ensure_arr<'a>(root:&'a mut Value,key:&str)->&'a mut Vec<Value>{
    if !root.get(key).map(Value::is_array).unwrap_or(false){root[key]=json!([]);}
    root.get_mut(key).and_then(Value::as_array_mut).expect("array")
}

pub fn ensure_director_state(state:&mut Value){
    if !state.get("director").map(Value::is_object).unwrap_or(false){
        state["director"]=json!({
            "scene_goal":"Pozwól graczowi działać swobodnie, ale rozwijaj istniejące konsekwencje.",
            "tension":25,
            "threads":[],
            "scene_turn":0,
            "last_initiative":"",
            "principles":[
                "Nie potwierdzaj założeń gracza jako faktów bez dowodu.",
                "Rozwijaj istniejące wątki zamiast generować losowe twisty.",
                "NPC działają zgodnie z wiedzą, celem, zasobami i ryzykiem.",
                "Nie ujawniaj ukrytej prawdy świata bez wiarygodnego kanału wiedzy."
            ]
        });
    }
    if !state["director"].get("threads").map(Value::is_array).unwrap_or(false){state["director"]["threads"]=json!([]);}
}

fn active_quests(state:&Value)->Vec<Value>{
    state.get("quests").and_then(Value::as_array).map(|a|a.iter().filter(|q|q.get("status").and_then(Value::as_str).unwrap_or("active")=="active").take(8).cloned().collect()).unwrap_or_default()
}
fn active_threads(state:&Value)->Vec<Value>{
    state.pointer("/director/threads").and_then(Value::as_array).map(|a|a.iter().filter(|t|t.get("status").and_then(Value::as_str).unwrap_or("active")!="resolved").take(8).cloned().collect()).unwrap_or_default()
}
fn due_npcs(state:&Value)->Vec<Value>{
    state.get("npcs").and_then(Value::as_object).map(|m|m.iter().filter_map(|(id,n)|{
        let progress=n.pointer("/autonomy/progress").and_then(Value::as_i64).unwrap_or(0);
        let plan=n.pointer("/autonomy/plan").and_then(Value::as_str).unwrap_or("");
        if !plan.is_empty() && progress>=70 {Some(json!({"id":id,"name":n.get("name"),"plan":plan,"progress":progress,"goal":n.get("goal"),"hidden_goal":n.get("hidden_goal")}))} else {None}
    }).take(8).collect()).unwrap_or_default()
}
fn due_factions(state:&Value)->Vec<Value>{
    state.get("factions").and_then(Value::as_object).map(|m|m.iter().filter_map(|(id,f)|{
        let clock=f.get("clock").and_then(Value::as_i64).unwrap_or(0);
        if clock>=25 {Some(json!({"id":id,"name":f.get("name"),"clock":clock,"goal":f.get("public_goal"),"private_goal":f.get("private_goal"),"resources":f.get("resources")}))} else {None}
    }).take(6).collect()).unwrap_or_default()
}

pub fn deterministic_interpretation(action:&str,state:&Value)->Value{
    let a=action.to_lowercase();
    let action_type=if a.contains("pash")||a.contains("inkant")||a.contains("zakl")||a.contains("mag")||a.contains("aard")||a.contains("igni")||a.contains("quen")||a.contains("yrden")||a.contains("axii"){"magic_attempt"}
        else if a.contains("mówię")||a.contains("mowie")||a.contains("pytam")||a.contains("rozmaw")||a.contains("krzyczę")||a.contains("krzycze"){"social"}
        else if a.contains("szuk")||a.contains("badam")||a.contains("ogląd")||a.contains("oglad")||a.contains("trop")||a.contains("sprawdz"){"investigation"}
        else if a.contains("atak")||a.contains("tnę")||a.contains("tne")||a.contains("cios")||a.contains("strzel"){"combat_attempt"}
        else if a.contains("idę")||a.contains("ide")||a.contains("ruszam")||a.contains("jadę")||a.contains("jade")||a.contains("podróż")||a.contains("podroz"){"movement"}
        else {"general_action"};
    let mut intent=action.trim().to_string();
    if action_type=="magic_attempt"{
        if a.contains("energia")||a.contains("rezon")||a.contains("wycz")||a.contains("pash iritor") {intent="zbadać strukturę i źródło lokalnego zaburzenia magicznego bez zakładania, że zaburzenie faktycznie istnieje".into();}
        else {intent="wykonać opisaną próbę magiczną zgodnie z deklarowanym efektem i kontekstem sceny".into();}
    }
    let assumptions=if action_type=="magic_attempt" && (a.contains("wydaje")||a.contains("energia tego miejsca")){
        vec!["Gracz opisuje przypuszczenie postaci, nie ustanawia faktu świata.".to_string()]
    }else{vec![]};
    json!({"action_type":action_type,"intent":intent,"assumptions":assumptions,"location":state.pointer("/world/location").cloned().unwrap_or(json!(""))})
}

pub fn director_snapshot(mut state:Value, action:&str)->Value{
    ensure_director_state(&mut state);
    let base=state.pointer("/director/tension").and_then(Value::as_i64).unwrap_or(25);
    let a=action.to_lowercase();
    let mut tension=base;
    if a.contains("atak")||a.contains("zakl")||a.contains("inkant")||a.contains("pash") {tension+=5;}
    if state.pointer("/combat/active").and_then(Value::as_bool).unwrap_or(false){tension+=15;}
    let due_n=due_npcs(&state); let due_f=due_factions(&state); let threads=active_threads(&state); let quests=active_quests(&state);
    if !due_n.is_empty(){tension+=5;} if !due_f.is_empty(){tension+=4;}
    json!({
        "scene_goal":state.pointer("/director/scene_goal").and_then(Value::as_str).unwrap_or("Rozwijaj konsekwencje sceny."),
        "tension":clamp(tension,0,100),
        "active_threads":threads,
        "active_quests":quests,
        "due_npc_plans":due_n,
        "due_factions":due_f,
        "rule":"Każda inicjatywa MG musi wynikać z istniejącego NPC, wątku, zegara, wcześniejszej decyzji, miejsca, frakcji albo zagrożenia."
    })
}

pub fn director_prepass(mut state:Value,action:&str)->Value{
    ensure_director_state(&mut state);
    let snap=director_snapshot(state.clone(),action);
    let mut initiative="none".to_string();
    let mut reason="Brak potrzeby wymuszania nowego wydarzenia; scena może odpowiedzieć bez dodatkowej inicjatywy.".to_string();
    if let Some(n)=snap.get("due_npc_plans").and_then(Value::as_array).and_then(|a|a.first()){
        initiative=format!("NPC {} może wykonać kolejny krok planu: {}",n.get("name").and_then(Value::as_str).unwrap_or("NPC"),n.get("plan").and_then(Value::as_str).unwrap_or("działanie"));
        reason="Istniejący plan NPC osiągnął wysoki postęp.".into();
    }else if let Some(t)=snap.get("active_threads").and_then(Value::as_array).and_then(|a|a.iter().max_by_key(|x|x.get("pressure").and_then(Value::as_i64).unwrap_or(0))){
        if t.get("pressure").and_then(Value::as_i64).unwrap_or(0)>=60{
            initiative=format!("Wątek '{}' powinien wywrzeć presję na scenę.",t.get("title").and_then(Value::as_str).unwrap_or("aktywny wątek"));
            reason="Ciśnienie istniejącego wątku przekroczyło próg reakcji.".into();
        }
    }else if let Some(f)=snap.get("due_factions").and_then(Value::as_array).and_then(|a|a.first()){
        if f.get("clock").and_then(Value::as_i64).unwrap_or(0)>=75{
            initiative=format!("Frakcja {} może wykonać widoczny ruch.",f.get("name").and_then(Value::as_str).unwrap_or("frakcja"));
            reason="Zegar istniejącej frakcji jest blisko realizacji celu.".into();
        }
    }
    state["director"]["scene_turn"]=json!(state.pointer("/director/scene_turn").and_then(Value::as_i64).unwrap_or(0)+1);
    state["director"]["tension"]=snap.get("tension").cloned().unwrap_or(json!(25));
    state["director"]["last_initiative"]=json!(initiative);
    json!({"state":state,"snapshot":snap,"initiative":initiative,"reason":reason})
}

fn relation_delta(root:&mut Value,key:&str,delta:&Value){
    let rels=ensure_obj(root,"relations");
    let rel=rels.entry(key.to_string()).or_insert_with(||json!({"trust":0,"respect":0,"fear":0,"liking":0,"debt":0,"hostility":0,"leverage":0}));
    for k in ["trust","respect","fear","liking","debt","hostility","leverage"]{
        let d=delta.get(k).and_then(Value::as_i64).unwrap_or(0);
        if d!=0{rel[k]=json!(clamp(rel.get(k).and_then(Value::as_i64).unwrap_or(0)+d,-100,100));}
    }
}

pub fn apply_director_patch(mut state:Value,patch:&Value)->Value{
    ensure_director_state(&mut state);
    if let Some(rels)=patch.get("npc_relations").and_then(Value::as_array){
        for r in rels.iter().take(8){
            let from=r.get("from_id").and_then(Value::as_str).unwrap_or("").trim();
            let to=r.get("to_id").and_then(Value::as_str).unwrap_or("").trim();
            if from.is_empty()||to.is_empty()||from==to{continue;}
            relation_delta(&mut state,&format!("npc::{from}::{to}"),&json!({
                "trust":clamp(r.get("trust").and_then(Value::as_i64).unwrap_or(0),-15,15),
                "respect":clamp(r.get("respect").and_then(Value::as_i64).unwrap_or(0),-15,15),
                "fear":clamp(r.get("fear").and_then(Value::as_i64).unwrap_or(0),-15,15),
                "liking":clamp(r.get("liking").and_then(Value::as_i64).unwrap_or(0),-15,15),
                "hostility":clamp(r.get("hostility").and_then(Value::as_i64).unwrap_or(0),-15,15)
            }));
        }
    }
    if let Some(items)=patch.get("knowledge_reveals").and_then(Value::as_array){
        if !state.get("character").map(Value::is_object).unwrap_or(false){state["character"]=json!({});}
        if !state["character"].get("known_facts").map(Value::is_array).unwrap_or(false){state["character"]["known_facts"]=json!([]);}
        let arr=state["character"]["known_facts"].as_array_mut().unwrap();
        for k in items.iter().take(6){
            let fact=k.get("fact").and_then(Value::as_str).unwrap_or("").trim();
            let source=k.get("source").and_then(Value::as_str).unwrap_or("").trim();
            let confidence=clamp(k.get("confidence").and_then(Value::as_i64).unwrap_or(50),0,100);
            if fact.is_empty()||source.is_empty(){continue;}
            if !arr.iter().any(|x|x.get("fact").and_then(Value::as_str)==Some(fact)){arr.push(json!({"fact":fact,"source":source,"confidence":confidence}));}
        }
        if arr.len()>120{let n=arr.len()-120;arr.drain(0..n);}
    }
    if let Some(items)=patch.get("faction_events").and_then(Value::as_array){
        let factions=ensure_obj(&mut state,"factions");
        for e in items.iter().take(4){
            let id=e.get("faction_id").and_then(Value::as_str).unwrap_or("").trim(); if id.is_empty(){continue;}
            if let Some(f)=factions.get_mut(id){
                let old=f.get("clock").and_then(Value::as_i64).unwrap_or(0);
                let d=clamp(e.get("clock_delta").and_then(Value::as_i64).unwrap_or(0),-10,10);
                f["clock"]=json!(clamp(old+d,0,100));
                if !f.get("history").map(Value::is_array).unwrap_or(false){f["history"]=json!([]);}
                if let Some(summary)=e.get("summary").and_then(Value::as_str){if !summary.trim().is_empty(){f["history"].as_array_mut().unwrap().push(json!(summary));}}
            }
        }
    }
    if let Some(items)=patch.get("director_threads").and_then(Value::as_array){
        let threads=state["director"]["threads"].as_array_mut().unwrap();
        for e in items.iter().take(5){
            let id=e.get("id").and_then(Value::as_str).unwrap_or("").trim();if id.is_empty(){continue;}
            if let Some(t)=threads.iter_mut().find(|x|x.get("id").and_then(Value::as_str)==Some(id)){
                let old=t.get("pressure").and_then(Value::as_i64).unwrap_or(0);
                t["pressure"]=json!(clamp(old+clamp(e.get("pressure_delta").and_then(Value::as_i64).unwrap_or(0),-20,20),0,100));
                if let Some(s)=e.get("status").and_then(Value::as_str){if ["active","dormant","resolved","failed"].contains(&s){t["status"]=json!(s);}}
            }else{
                threads.push(json!({"id":id,"title":e.get("title").and_then(Value::as_str).unwrap_or(id),"pressure":clamp(e.get("pressure_delta").and_then(Value::as_i64).unwrap_or(10),0,100),"status":"active"}));
            }
        }
    }
    state
}

pub fn fallback_turn(action:&str,interpretation:&Value,mechanics:&Value,director:&Value)->Value{
    let kind=interpretation.get("action_type").and_then(Value::as_str).unwrap_or("general_action");
    let success=mechanics.pointer("/check/success").and_then(Value::as_bool);
    let initiative=director.get("initiative").and_then(Value::as_str).unwrap_or("none");
    let narration=match (kind,success){
        ("magic_attempt",Some(true))=>"Inkantacja splata się z Chaosem. Odpowiedź miejsca nie jest prostym potwierdzeniem twojej hipotezy: zyskujesz jedynie tyle, ile naprawdę pozwolił ustalić wynik próby.",
        ("magic_attempt",Some(false))=>"Formuła napina Chaos, ale wzór wymyka się spod kontroli. Nie otrzymujesz pewnego potwierdzenia własnego przypuszczenia; pozostaje ryzyko, że źródło odczucia było inne.",
        ("investigation",Some(true))=>"Metodyczne badanie przynosi konkretny szczegół, którego wcześniej nie dało się pewnie oddzielić od tła.",
        ("investigation",Some(false))=>"Badanie nie daje wiarygodnego rozstrzygnięcia. Brak dowodu nie staje się dowodem braku.",
        ("social",_)=>"Rozmówca reaguje zgodnie z własnym interesem i tym, co naprawdę wie. Nie przyjmuje twoich założeń za prawdę tylko dlatego, że zostały wypowiedziane.",
        _=>"Świat odpowiada na twoje działanie zgodnie z istniejącym stanem, mechaniką i upływem czasu."
    };
    let extra=if initiative!="none"{format!(" W tle narasta również konsekwencja istniejącego wątku: {initiative}")}else{String::new()};
    json!({
        "interpretation":interpretation,
        "director":{"scene_goal":director.pointer("/snapshot/scene_goal").cloned().unwrap_or(json!("")),"tension":director.pointer("/snapshot/tension").cloned().unwrap_or(json!(25)),"initiative":initiative,"reason":director.get("reason").cloned().unwrap_or(json!(""))},
        "narration":format!("{narration}{extra}"),
        "suggestions":["Dopytaj lub sprawdź szczegół","Zmień podejście","Pozwól wydarzeniom chwilę się rozwinąć"],
        "patch":{"npc_memories":[],"relations":[],"npc_relations":[],"quest_events":[],"clues":[],"knowledge_reveals":[],"faction_events":[],"director_threads":[],"world_flags":[]}
    })
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn player_assumption_is_not_truth(){let s=json!({"world":{"location":"Ruiny"}});let i=deterministic_interpretation("energia tego miejsca wydaje się wzburzona, pash iritor",&s);assert_eq!(i["action_type"],json!("magic_attempt"));assert!(i["assumptions"].as_array().unwrap().len()>0);}
    #[test]fn npc_relation_is_bounded(){let s=json!({});let p=json!({"npc_relations":[{"from_id":"a","to_id":"b","trust":999,"respect":0,"fear":0,"liking":0,"hostility":0}]});let o=apply_director_patch(s,&p);assert_eq!(o.pointer("/relations/npc::a::b/trust"),Some(&json!(15)));}
    #[test]fn director_patch_cannot_touch_hp(){let s=json!({"character":{"hp":9}});let p=json!({"character":{"hp":999},"knowledge_reveals":[]});let o=apply_director_patch(s,&p);assert_eq!(o.pointer("/character/hp"),Some(&json!(9)));}
}
