use serde_json::{json, Value};

pub fn new_vertical_slice(character:Option<Value>)->Value{
    let char0=character.unwrap_or_else(||json!({
        "name":"Zębie","profession":"Czarodziej","level":1,"hp":24,"max_hp":24,"gold":68,
        "stats":{"STR":2,"DEX":3,"CON":3,"INT":5,"PER":4,"CHA":3},
        "skills":["Percepcja","Wiedza magiczna","Perswazja"],
        "items":["Płaszcz podróżny","Sztylet","Notatnik","Amulet-focus"],
        "known_facts":[],
        "magic":{"control":5,"vigor":8,"stamina":10,"chaos_saturation":0,"concentration":100,"formulas":["Quen","Aard"]},
        "alchemy":{"skill":2,"prepared":[]},"crafting":{"skill":2}
    }));
    json!({
        "_meta":{"save_schema":3,"game_version":"0.10.0-preview.1","campaign_template":"vertical_slice_010"},
        "character":char0,
        "world":{"year":1272,"month":6,"day":17,"hour":16,"minute":20,"weather":"ulewa po burzy","location":"Brzezina nad Pontarem","mode":"narrative","quest":"Ślady po zmroku"},
        "inventory":{"ingredients":{"alkohol":2,"glistnik":3,"mózg utopca":1,"berberka":2,"tłuszcz":1,"jaskółcze ziele":1,"pył kostny":1,"saletra":1,"fosfor":1},"items":[]},
        "combat":{"active":false,"enemy":"Nieznane stworzenie","enemy_guard":6,"guard":6,"pressure":0,"tempo":0,"distance":5.0,"morale":70},
        "npcs":{
            "marta":{"id":"marta","name":"Marta","canon":false,"role":"karczmarka","personality":["ostrożna","praktyczna"],"goal":"ochronić młodszego brata","hidden_goal":"ukryć, że brat poszedł nocą do ruin","fear":"utrata rodziny","secret":"widziała pod ruinami błękitny błysk","memories":[],"knowledge":["trzech ludzi zaginęło przy starym młynie","brat wymknął się nocą"],"autonomy":{"plan":"jeśli gracz zacznie podejrzewać brata, uprzedzić go przed zmrokiem","progress":35,"resources":25,"risk_tolerance":35}},
            "otwin":{"id":"otwin","name":"Otwin","canon":false,"role":"młynarz","personality":["milczący","uparty"],"goal":"ocalić młyn","hidden_goal":"zniszczyć ślady własnego eksperymentu z runami","fear":"straż i magowie","secret":"aktywował stary znak ochronny pod młynem","memories":[],"knowledge":["pod fundamentem jest starsza komora"],"autonomy":{"plan":"usunąć pozostałe runy przed nocą","progress":55,"resources":40,"risk_tolerance":60}}
        },
        "relations":{"player::marta":{"trust":12,"respect":2,"fear":0,"liking":5,"debt":0,"hostility":0,"leverage":0},"npc::marta::otwin":{"trust":-5,"respect":0,"fear":8,"liking":-4,"debt":0,"hostility":10,"leverage":0}},
        "factions":{"local_guard":{"id":"local_guard","name":"Straż rzeczna","canon":false,"active":true,"resources":45,"influence":40,"clock":10,"public_goal":"utrzymać przejezdny trakt","private_goal":"zamknąć sprawę zaginięć bez rozgłosu","history":[]}},
        "quests":[{"id":"missing_trail","title":"Ślady po zmroku","type":"side","status":"active","stage":0,"summary":"Ustal, co dzieje się przy starym młynie i dlaczego znikają ludzie.","urgency":45,"objectives":[{"id":"talk_marta","text":"Porozmawiaj z Martą","status":"active"},{"id":"collect_clues","text":"Zdobądź dwa niezależne tropy","status":"locked"},{"id":"reach_mill","text":"Dotrzyj do starego młyna","status":"locked"},{"id":"resolve_source","text":"Rozstrzygnij źródło zagrożenia","status":"locked"}],"rewards":{"gold":55,"reputation":4},"consequences":["Jeśli zwlekasz, ktoś jeszcze może wejść do ruin."]}],
        "clues":[],"hypotheses":[],"hunting":{"evidence":[],"hypotheses":[],"confidence":0,"preparation":0},
        "director":{"scene_goal":"Pierwsza scena ma dać graczowi kontrakt, osobisty haczyk NPC i co najmniej dwa możliwe kierunki śledztwa.","tension":32,"scene_turn":0,"last_initiative":"","threads":[
            {"id":"ruin_resonance","title":"Nieregularny rezonans pod ruinami","pressure":38,"status":"active","truth":"Stary znak ochronny został niestabilnie pobudzony; nie jest to świadoma bestia."},
            {"id":"martas_brother","title":"Brat Marty nie wrócił","pressure":52,"status":"active","truth":"Ukrywa się w pobliżu młyna po tym, jak zobaczył Otwina przy runach."},
            {"id":"guard_clock","title":"Straż zamknie trakt","pressure":18,"status":"active","truth":"Przy większej liczbie ofiar straż odetnie okolicę i utrudni śledztwo."}
        ],"principles":["Nie potwierdzaj teorii gracza bez testu lub wiarygodnego źródła.","Wydarzenia muszą wynikać z istniejących wątków.","Nie zdradzaj truth pola director.threads graczowi bez zdobycia wiedzy."]},
        "campaign_flags":{"contract_accepted":false,"visited_mill":false,"camp_prepared":false},
        "news_queue":[],
        "chronicle":[{"date":"17.06.1272 16:20","title":"Brzezina nad Pontarem","text":"Ulewa słabnie. W karczmie Marta czeka z wiadomością o trzecim zaginięciu przy starym młynie. Nie prosi jeszcze o bohaterstwo — chce wiedzieć, czy ktokolwiek odważy się sprawdzić ślady przed nocą."}],
        "suggested_actions":["Zapytaj Martę, co dokładnie łączy zaginionych","Obejrzyj przemoczoną mapę i ślady przyniesione z traktu","Zapytaj o stare ruiny pod młynem"]
    })
}

pub fn apply_slice_progress(mut state:Value,action:&str)->Value{
    let a=action.to_lowercase();
    if a.contains("marta")||a.contains("kontrakt")||a.contains("zagin"){
        state["campaign_flags"]["contract_accepted"]=json!(true);
        if let Some(q)=state.get_mut("quests").and_then(Value::as_array_mut).and_then(|qs|qs.iter_mut().find(|q|q.get("id").and_then(Value::as_str)==Some("missing_trail"))){
            q["stage"]=json!(1); if let Some(os)=q.get_mut("objectives").and_then(Value::as_array_mut){for o in os{if o.get("id").and_then(Value::as_str)==Some("talk_marta"){o["status"]=json!("done");}if o.get("id").and_then(Value::as_str)==Some("collect_clues"){o["status"]=json!("active");}}}
        }
    }
    if a.contains("młyn")||a.contains("mlyn"){state["campaign_flags"]["visited_mill"]=json!(true);}
    state
}

#[cfg(test)]mod tests{use super::*;#[test]fn seed_has_full_loop(){let s=new_vertical_slice(None);assert!(s.get("director").is_some());assert!(s.get("npcs").unwrap().as_object().unwrap().len()>=2);assert_eq!(s.pointer("/quests/0/status"),Some(&json!("active")));}}
