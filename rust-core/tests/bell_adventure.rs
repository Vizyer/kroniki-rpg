use kroniki_core::{ai::{self,AiConfig},bell,dm::{self,NewCampaign},domain::*,engine::Engine,store::Store};
fn action(text:&str)->PlayerAction {PlayerAction{text:text.into(),mode:"freeform".into()}}
fn play(s:&GameState,text:&str)->GameState {
    let a=action(text);Engine::resolve_turn(s,&a,&dm::fallback_plan(s,&a)).unwrap().0
}
fn choice(s:&GameState,id:&str)->GameState {
    let text=bell::choices(s).into_iter().find(|c|c.id==id).unwrap().text;
    play(s,text)
}
fn start()->GameState {dm::new_campaign(NewCampaign::default())}
fn outcome(s:&GameState)->Option<&str>{s.campaign.bell.as_ref().unwrap().outcome.as_deref()}
#[test]
fn rescue_has_an_epilogue_and_persistent_witness_relationships() {
    let s=choice(&start(),"marta");
    let s=play(&s,"Idę do miejsca Młyn");
    let s=choice(&s,"ela");
    assert!(s.campaign.clues["courier"].discovered);
    let s=choice(&s,"rescue");assert_eq!(outcome(&s),Some("rescued"));
    assert_eq!(s.world.npcs["jan"].location,"Leśne schronienie");
    assert_eq!(s.world.npcs["jan"].relations["player"].trust,15);
    assert!(s.world.npcs["bor"].memories.is_empty()); // not a witness
    assert!(bell::choices(&s).is_empty());
}
#[test]
fn negotiation_requires_evidence_and_records_the_promise() {
    let s=choice(&start(),"marta");let mut s=play(&s,"Idę do miejsca Stary most");
    let remain=60-s.campaign.bell.as_ref().unwrap().elapsed_minutes;
    bell::advance(&mut s,remain);
    let s=choice(&s,"bargain");assert_eq!(outcome(&s),Some("guaranteed"));
    assert!(s.character.knowledge.iter().any(|k|k.id=="bell:promise"));
    let mut no_letter=play(&start(),"Idę do miejsca Stary most");bell::advance(&mut no_letter,60);
    assert!(!bell::choices(&no_letter).iter().any(|c|c.id=="bargain"));
    assert_eq!(outcome(&play(&no_letter,"Pokazuję poborcy list i proponuję poręczenie za kuriera")),None);
}
#[test]
fn choosing_betrayal_differs_from_abandonment_and_waiting() {
    let s=choice(&play(&start(),"Idę do miejsca Młyn"),"ela");let s=choice(&s,"surrender");
    assert_eq!(outcome(&s),Some("surrendered"));assert!(s.world.npcs["ela"].relations["player"].trust<0);
    assert_eq!(outcome(&choice(&start(),"leave")),Some("abandoned"));
    let mut s=start();let events=bell::advance(&mut s,120);
    assert_eq!(outcome(&s),Some("too_late"));assert_eq!(events.len(),4);
    assert!(bell::advance(&mut s,120).is_empty());
}
#[test]
fn rescue_expiring_during_action_cannot_teleport_captured_courier() {
    let mut s=choice(&play(&start(),"Idę do miejsca Młyn"),"ela");
    let remain=88-s.campaign.bell.as_ref().unwrap().elapsed_minutes;bell::advance(&mut s,remain);
    let a=action("Pomagam kurierowi uciec z Elą leśną drogą");
    let (s,_,r)=Engine::resolve_turn(&s,&a,&dm::fallback_plan(&s,&a)).unwrap();
    assert!(!r.ok);assert_eq!(outcome(&s),None);assert_eq!(s.world.npcs["jan"].location,"Stary most");
    assert!(!bell::choices(&s).iter().any(|c|c.id=="rescue"));
}
#[test]
fn combat_failure_keeps_courier_captured_and_success_frees_him() {
    let mut s=play(&start(),"Idę do miejsca Stary most");bell::advance(&mut s,80);
    let mut outcomes=std::collections::BTreeSet::new();
    for revision in 0..100 {
        s.revision=revision;let next=choice(&s,"force");
        outcomes.insert(outcome(&next).unwrap_or("failure").to_string());
        if outcome(&next).is_none(){assert_eq!(next.world.npcs["jan"].location,"Stary most");}
    }
    assert!(outcomes.contains("failure"));assert!(outcomes.contains("freed"));
}
#[test]
fn milestones_survive_serialization_and_do_not_leak_future_events() {
    let mut s=start();assert_eq!(bell::advance(&mut s,30).len(),1);
    let saved=serde_json::to_string(&s).unwrap();
    let mut loaded:GameState=serde_json::from_str(&saved).unwrap();
    assert!(bell::advance(&mut loaded,0).is_empty());
    assert!(bell::advance(&mut loaded,29).is_empty());assert_eq!(bell::advance(&mut loaded,1).len(),1);
    let ctx=ai::build_context(&start(),&action("Rozglądam się"),&Resolution::default()).to_string();
    assert!(!ctx.contains("capture"));assert!(!ctx.contains("departure"));assert!(!ctx.contains("elapsed_minutes"));
    let mut old=serde_json::to_value(start()).unwrap();old["campaign"].as_object_mut().unwrap().remove("bell");
    let legacy:GameState=serde_json::from_value(old).unwrap();assert!(legacy.campaign.bell.is_none());
}
#[tokio::test]
async fn offline_session_finishes_and_autosave_restores_epilogue() {
    let store=Store::open(":memory:".into()).unwrap();store.set_ai_config(&AiConfig{mode:"off".into(),..Default::default()}).unwrap();
    let e=Engine::new(store.clone());e.replace(start(),"new").await.unwrap();
    for text in ["Pytam Martę, co mogę zrobić dla kuriera","Idę do miejsca Młyn","Proszę Elę o pomoc w odnalezieniu kuriera","Pomagam kurierowi uciec z Elą leśną drogą"] {
        e.act(action(text)).await.unwrap();
    }
    let restored=Engine::new(store);assert_eq!(outcome(&*restored.state.read().await),Some("rescued"));
    assert!(restored.state.read().await.last_narration.starts_with("Epilog"));
    assert!(restored.act(action("Atakuję Jana")).await.is_err());
}
