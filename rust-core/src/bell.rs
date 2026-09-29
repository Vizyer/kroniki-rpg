//! Authored, finite adventure. Prose cannot choose a branch or change its truth.
use crate::{domain::*, dm::short};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BellAdventure {
    pub elapsed_minutes: i64,
    pub milestones: BTreeSet<String>,
    pub outcome: Option<String>,
    pub epilogue: String,
    pub public_events: Vec<String>,
}
#[derive(Clone)]
pub struct Choice { pub id: &'static str, pub text: &'static str }
fn found(s:&GameState,id:&str)->bool {s.campaign.clues.get(id).map(|c|c.discovered).unwrap_or(false)}
fn choice(id:&'static str,text:&'static str)->Choice {Choice{id,text}}

pub fn choices(s:&GameState)->Vec<Choice> {
    let Some(b)=&s.campaign.bell else{return vec![]};
    if b.outcome.is_some(){return vec![];}
    let mut v=Vec::new();
    match s.world.location.as_str() {
        "Gospoda nad brodem"=>v.push(choice("marta","Pytam Martę, co mogę zrobić dla kuriera")),
        "Stary most"=>{
            v.push(choice("bor","Pytam Bora o drogę i poborcę"));
            if b.elapsed_minutes>=60 {
                if found(s,"letter") {v.push(choice("bargain","Pokazuję poborcy list i proponuję poręczenie za kuriera"));}
                if b.elapsed_minutes>=90 {v.push(choice("force","Atakuję eskortę, żeby uwolnić kuriera"));}
            }
        },
        "Młyn"=>{
            if b.elapsed_minutes<90 {
                if !found(s,"courier") {v.push(choice("ela","Proszę Elę o pomoc w odnalezieniu kuriera"));}
                else {
                    v.push(choice("rescue","Pomagam kurierowi uciec z Elą leśną drogą"));
                    v.push(choice("surrender","Wydaję kuriera poborcy, wysyłając sygnał z młyna"));
                }
            } else {v.push(choice("empty_mill","Pytam Elę, co stało się w młynie"));}
        },_=>{}
    }
    v.push(choice("leave","Opuszczam okolicę i kończę sprawę kuriera"));
    v
}
fn selected(s:&GameState,action:&PlayerAction)->Option<Choice> {
    let text=action.text.trim().trim_end_matches('.').to_lowercase();
    choices(s).into_iter().find(|c|c.text.to_lowercase()==text)
}

/// Exact, visible choices are explicit player decisions, never inferred from narrator prose.
pub fn resolution(s:&GameState,a:&PlayerAction)->Option<Resolution> {
    let c=selected(s,a)?;
    if c.id=="force" {
        let mut r=crate::systems::resolve_action(s,a,&crate::systems::infer_intent(&a.text));
        r.duration_seconds=300;
        return Some(r);
    }
    Some(Resolution{ok:true,degree:"no_check".into(),duration_seconds:if matches!(c.id,"rescue"|"bargain"|"surrender"){300}else{60},..Default::default()})
}
fn reveal(s:&mut GameState,id:&str,events:&mut Vec<String>) {
    if let Some(c)=s.campaign.clues.get_mut(id) {
        if !c.discovered {
            c.discovered=true;
            let text=c.text.clone();
            s.character.knowledge.push(KnowledgeFact{id:format!("clue:{id}"),statement:text.clone(),source:"witness".into(),confidence:100,canon:false});
            events.push(text);
        }
    }
    if id=="courier" {if let Some(n)=s.world.npcs.get_mut("jan"){n.active=true;}}
}
fn remember(s:&mut GameState,npc_id:&str,event:&str,trust:i32) {
    if let Some(n)=s.world.npcs.get_mut(npc_id) {
        let r=n.relations.entry("player".into()).or_default();r.trust=(r.trust+trust).clamp(-100,100);
        n.memories.push(NpcMemory{id:format!("bell:{}",s.revision),summary:short(event,500),importance:5,valence:trust,at:s.world.clock.clone()});
        if n.memories.len()>24 {n.memories.remove(0);}
    }
}
fn finish(s:&mut GameState,id:&str,text:&str,events:&mut Vec<String>) {
    let Some(b)=s.campaign.bell.as_mut() else{return};
    if b.outcome.is_some(){return;}
    b.outcome=Some(id.into());b.epilogue=text.into();
    events.push(text.into());
    if let Some(t)=s.campaign.threads.get_mut("missing-courier"){t.resolved=true;}
    if let Some(q)=s.world.quests.get_mut("missing-courier") {
        q.status=if matches!(id,"rescued"|"guaranteed"|"freed"){"completed"}else{"failed"}.into();
        q.summary=text.into();q.stage+=1;
    }
    s.director.scene_goal="Przygoda zakończona. Przedstaw zapisany epilog; nie otwieraj ponownie tej samej sprawy.".into();
    s.campaign.scene="epilogue".into();
}

pub fn apply_choice(s:&mut GameState,before:&GameState,a:&PlayerAction,r:&mut Resolution)->Vec<String> {
    let mut events=Vec::new();
    if s.campaign.bell.as_ref().and_then(|b|b.outcome.as_ref()).is_some(){return events;}
    if found(s,"courier") {if let Some(n)=s.world.npcs.get_mut("jan"){n.active=true;}}
    let Some(c)=selected(before,a) else{return events};
    // Recheck after time advancement: a plan can become impossible while travelling/acting.
    if !choices(s).iter().any(|now|now.id==c.id) {
        r.ok=false;r.degree="failure".into();
        events.push("Sytuacja zmieniła się przed zakończeniem działania. Wybierz dalszy krok na podstawie obecnej sceny.".into());
        return events;
    }
    match c.id {
        "marta"=>{
            events.push("Marta odsłania list spod rachunków. «Jan niósł lekarstwo Eli. Poborca chce go zatrzymać za zaległe myto. Spróbuj przekonać strażnika albo szukaj przy młynie».".into());
            reveal(s,"letter",&mut events);remember(s,"marta","Przekazała bohaterowi list i prośbę o pomoc Janowi.",1);
        },
        "bor"=>{
            events.push("Bor wskazuje ślady przy poręczy. «Wóz skręcił do młyna. Poborca zbiera ludzi. List lub poręczenie mogą go powstrzymać; sam rozkaz nie wystarczy».".into());
            reveal(s,"tracks",&mut events);remember(s,"bor","Wyjaśnił bohaterowi drogę do młyna i możliwość poręczenia.",0);
        },
        "ela"=>{
            events.push("Ela długo przygląda się twoim dłoniom, po czym odsuwa worki. «Jan oddał mi lekarstwo. Nie pozwolę zabrać go bez słowa. Mogę przeprowadzić go przez las, jeśli pomożesz mu iść».".into());
            reveal(s,"courier",&mut events);remember(s,"ela","Pokazała bohaterowi rannego Jana i zaoferowała pomoc w ucieczce.",1);
        },
        "empty_mill"=>{
            events.push("Ela pokazuje przewróconą ławę. «Zabrali Jana na most. Jeśli zdążysz, zastaniesz jeszcze eskortę».".into());
            remember(s,"ela","Powiedziała bohaterowi o zabraniu Jana na most.",0);
        },
        "rescue"=>{
            remember(s,"ela","Bohater pomógł wyprowadzić Jana leśną drogą.",10);
            remember(s,"jan","Bohater pomógł mu uciec przed poborcą.",15);
            if let Some(n)=s.world.npcs.get_mut("jan"){n.location="Leśne schronienie".into();}
            if let Some(n)=s.world.npcs.get_mut("ela"){n.location="Leśne schronienie".into();}
            finish(s,"rescued","Epilog — leśna droga. Pomagasz Janowi wyjść z młyna. Ela prowadzi go do schronienia, a lekarstwo zostaje u jej rodziny. Kurier jest bezpieczny; spór z poborcą pozostaje nierozwiązany. Ela i Jan pamiętają twoją pomoc.",&mut events);
        },
        "bargain"=>{
            remember(s,"poborca","Bohater przedstawił list i poręczył za Jana do czasu rozpatrzenia długu.",5);
            s.character.knowledge.push(KnowledgeFact{id:"bell:promise".into(),statement:"Poręczono za Jana: ma stawić się przed miejscowym sądem w sprawie zaległego myta.".into(),source:"agreement".into(),confidence:100,canon:false});
            if let Some(n)=s.world.npcs.get_mut("jan"){n.location="Młyn".into();}
            finish(s,"guaranteed","Epilog — poręczenie. Poborca uznaje list i twoje poręczenie: Jan pozostaje wolny do rozpatrzenia długu przez sąd. Posyła strażnika z odwołaniem zatrzymania. Lekarstwo pozostaje u rodziny Eli. Zobowiązanie zostaje zapisane.",&mut events);
        },
        "surrender"=>{
            remember(s,"ela","Bohater wydał Jana poborcy mimo udzielonego mu schronienia.",-20);
            remember(s,"jan","Bohater sprowadził poborcę sygnałem z młyna.",-25);
            if let Some(n)=s.world.npcs.get_mut("jan"){n.location="Areszt".into();}
            finish(s,"surrendered","Epilog — wydanie. Sygnał sprowadza strażników, którzy zabierają Jana do aresztu. Ela zachowuje lekarstwo, ale odmawia dalszej rozmowy z tobą. Sprawa kończy się zatrzymaniem kuriera.",&mut events);
        },
        "force"=>{
            if r.ok {
                remember(s,"jan","Bohater uwolnił go z eskorty na moście.",15);
                remember(s,"poborca","Bohater zaatakował eskortę i uwolnił zatrzymanego.",-30);
                if let Some(n)=s.world.npcs.get_mut("jan"){n.location="Leśne schronienie".into();}
                finish(s,"freed","Epilog — zerwana eskorta. Po starciu Jan wymyka się strażnikom i ucieka do lasu. Poborca zapamiętuje twój udział. Kurier odzyskuje wolność, ale konflikt ze strażą pozostaje.",&mut events);
            } else {events.push("Eskorta odpiera próbę uwolnienia Jana. Kurier pozostaje pod strażą; możesz poszukać innej drogi rozwiązania sprawy.".into());}
        },
        "leave"=>{
            s.world.location="Trakt za brodem".into();
            finish(s,"abandoned","Epilog — dalsza droga. Opuszczasz okolicę. Sprawa Jana pozostaje poza twoją wiedzą; nie wiesz, jak zakończyła się dla mieszkańców. To świadome zakończenie twojego udziału w tej przygodzie.",&mut events);
        },_=>{}
    }
    record_public(s,&events);
    events
}
fn record_public(s:&mut GameState,events:&[String]) {
    if let Some(b)=s.campaign.bell.as_mut(){
        b.public_events.extend(events.iter().cloned());
        if b.public_events.len()>16 {let n=b.public_events.len()-16;b.public_events.drain(..n);}
    }
}

/// Scheduled once by game time; no wall-clock progression or model calls.
pub fn advance(s:&mut GameState,minutes:i64)->Vec<String> {
    let mut events=Vec::new();
    let Some(b)=s.campaign.bell.as_mut() else{return events};
    if b.outcome.is_some() || minutes<=0{return events;}
    b.elapsed_minutes=b.elapsed_minutes.saturating_add(minutes);
    let now=b.elapsed_minutes;
    for (at,id) in [(30,"warning"),(60,"collector"),(90,"capture"),(120,"departure")] {
        if now<at || !s.campaign.bell.as_mut().unwrap().milestones.insert(id.into()){continue;}
        match id {
            "warning"=>{
                events.push("Od mostu rozlega się pojedynczy dzwon. Woźny obchodzi okolicę: «Za pół godziny poborca rozpocznie poszukiwania kuriera!».".into());
            },
            "collector"=>{
                if let Some(n)=s.world.npcs.get_mut("poborca"){n.active=true;}
                events.push("Woźny ogłasza przy drodze: «Poborca czeka na moście. Przyjmuje świadków i poręczenia przed zatrzymaniem kuriera».".into());
            },
            "capture"=>{
                if let Some(n)=s.world.npcs.get_mut("jan"){n.active=true;n.location="Stary most".into();}
                if let Some(c)=s.campaign.clues.get_mut("courier"){if !c.discovered {c.location="Stary most".into();c.text="Widzisz Jana żywego pod strażą na moście. Strażnicy potwierdzają, że zabrali go z młyna za zaległe myto.".into();c.requires.clear();}}
                events.push("Dzwon bije trzy razy. Woźny ogłasza: «Jan został zatrzymany. Eskorta wyruszy z mostu za pół godziny».".into());
            },
            "departure"=>{
                if let Some(n)=s.world.npcs.get_mut("jan"){n.location="Areszt".into();}
                if let Some(n)=s.world.npcs.get_mut("poborca"){n.location="Areszt".into();}
                finish(s,"too_late","Epilog — zamknięty most. Woźny ogłasza odjazd eskorty z Janem do aresztu. Okazja do interwencji w tej przygodzie minęła. Lekarstwo pozostało u Eli, lecz kurier stracił wolność.",&mut events);
            },_=>{}
        }
    }
    if let Some(t)=s.campaign.threads.get_mut("missing-courier"){t.urgency=if now>=90{5}else if now>=60{4}else if now>=30{2}else{1};}
    record_public(s,&events);
    events
}

pub fn public(s:&GameState)->Value {
    match &s.campaign.bell {
        None=>Value::Null,
        Some(b)=>json!({"outcome":b.outcome,"epilogue":b.epilogue,"announcements":b.public_events.iter().rev().take(4).collect::<Vec<_>>(),
            "choices":choices(s).iter().map(|c|c.text).collect::<Vec<_>>()})
    }
}
