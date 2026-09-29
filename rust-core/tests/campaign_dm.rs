use kroniki_core::{ai, dm::{self, *}, domain::*, engine::Engine, store::Store};
fn action(text:&str)->PlayerAction { PlayerAction{text:text.into(),mode:"freeform".into()} }
fn seeded()->GameState { new_campaign(NewCampaign::default()) }
fn offline()->Engine {
    let store=Store::open(":memory:".into()).unwrap();
    store.set_ai_config(&ai::AiConfig{mode:"off".into(),..Default::default()}).unwrap();
    Engine::new(store)
}
fn travel(s:&GameState,place:&str)->GameState {
    let p=TurnPlan{intent_kind:"travel".into(),target:place.into(),..Default::default()};
    Engine::resolve_turn(s,&action(&format!("Idę do miejsca {place}")),&p).unwrap().0
}
fn investigate(s:&GameState)->GameState {
    let p=TurnPlan{intent_kind:"investigation".into(),..Default::default()};
    for i in 0..100 {
        let (next,_,r)=Engine::resolve_turn(s,&action(&format!("Badam ślady {i}")),&p).unwrap();
        if r.ok {return next;}
    }
    panic!("Expected at least one successful deterministic roll")
}
#[test]
fn clues_require_discovery_and_campaign_can_be_completed() {
    let s=seeded();
    let mill=investigate(&travel(&s,"Młyn"));
    assert!(!mill.campaign.clues["courier"].discovered);
    let bridge=investigate(&travel(&mill,"Stary most"));
    assert!(bridge.campaign.clues["tracks"].discovered);
    let end=investigate(&travel(&bridge,"Młyn"));
    assert!(end.campaign.clues["courier"].discovered);
    assert!(end.campaign.threads["missing-courier"].resolved);
    assert_eq!(end.world.quests["missing-courier"].status,"completed");
}
#[test]
fn model_and_player_views_do_not_contain_hidden_state() {
    let s=seeded();
    let context=ai::build_context(&s,&action("Rozglądam się"),&Resolution::default()).to_string();
    let public=state_summary(&s).to_string();
    for npc in s.world.npcs.values() {assert!(!context.contains(&npc.secret));assert!(!public.contains(&npc.secret));}
    for clue in s.campaign.clues.values() {assert!(!context.contains(&clue.text));assert!(!public.contains(&clue.text));}
    assert!(!public.contains("required_clues"));
}
#[test]
fn plan_cannot_inject_resources_unknown_npcs_or_destinations() {
    assert!(serde_json::from_str::<TurnPlan>(r#"{"intent_kind":"observe","hp":999}"#).is_err());
    let s=seeded();
    let mut p=TurnPlan{intent_kind:"travel".into(),target:"Pałac króla".into(),..Default::default()};
    assert!(validate_plan(&s,&p).is_err());
    p.intent_kind="social".into();p.target.clear();
    p.npc_moves=vec![NpcMove{npc_id:"ela".into(),kind:NpcMoveKind::Help}];
    assert!(validate_plan(&s,&p).is_err());
    p.npc_moves=vec![NpcMove{npc_id:"marta".into(),kind:NpcMoveKind::Help}];
    let (next,_,_)=Engine::resolve_turn(&s,&action("Pytam Martę o pomoc"),&p).unwrap();
    assert_eq!(next.world.npcs["marta"].relations["player"].trust,1);
    assert_eq!(next.world.npcs["marta"].memories.len(),1);
    let p=TurnPlan{intent_kind:"observe".into(),..Default::default()};
    let (_,intent,r)=Engine::resolve_turn(&s,&action("Atakuję mieczem"),&p).unwrap();
    assert_eq!(intent.kind,"combat_action");assert_eq!(r.cost_stamina,2);
}
#[test]
fn travel_advances_clock_once_and_background_does_not_expose_private_plans() {
    let mut s=seeded();let before=s.world.clock.hour as i64*60+s.world.clock.minute as i64;
    let next=travel(&s,"Stary most");
    let after=next.world.clock.hour as i64*60+next.world.clock.minute as i64;
    assert_eq!(after-before,10);
    s.world.npcs.get_mut("marta").unwrap().plan=vec!["SECRET_PRIVATE_PLAN".into()];
    let events=kroniki_core::systems::simulate_background(&mut s,180);
    assert!(!events.join(" ").contains("SECRET_PRIVATE_PLAN"));
}
#[test]
fn old_schema10_saves_load_and_memory_compacts_with_recall() {
    let mut v=serde_json::to_value(seeded()).unwrap();
    v.as_object_mut().unwrap().remove("memory");v.as_object_mut().unwrap().remove("campaign");
    let old:GameState=serde_json::from_value(v).unwrap();assert_eq!(old.memory.turns,0);
    let mut m=CampaignMemory::default();
    for revision in 0..350 {m.record(TurnMemory{revision,action:format!("Pytam Martę o most {revision}"),events:vec!["Marta odmawia".into()],..Default::default()});}
    assert_eq!(m.recent.len(),12);assert!(m.archive.len()<=256);assert!(!m.summaries.is_empty());
    assert!(!m.context("Marta most")["relevant_older_turns"].as_array().unwrap().is_empty());
}
#[tokio::test]
async fn autosave_restores_turns_and_loading_past_does_not_recall_future() {
    let e=offline();e.replace(seeded(),"new").await.unwrap();
    e.act(action("Rozglądam się")).await.unwrap();
    let checkpoint=e.state.read().await.clone();
    let id=e.store.save("before",&checkpoint,None).unwrap();
    for _ in 0..40 {e.act(action("Rozglądam się")).await.unwrap();}
    let restarted=Engine::new(e.store.clone());assert_eq!(restarted.state.read().await.memory.turns,41);
    restarted.replace(e.store.load(id).unwrap(),"load").await.unwrap();
    assert_eq!(restarted.state.read().await.memory.turns,1);
    restarted.replace(seeded(),"new").await.unwrap();
    assert_eq!(Engine::new(e.store.clone()).state.read().await.memory.turns,0);
}
#[tokio::test]
async fn cancellation_and_database_failure_do_not_commit_a_turn() {
    let e=offline();e.replace(seeded(),"new").await.unwrap();let rev=e.state.read().await.revision;
    e.cancel("cancelled");
    assert!(e.act_with_id(action("Rozglądam się"),"cancelled".into()).await.is_err());
    assert_eq!(e.state.read().await.revision,rev);
    e.store.conn.lock().unwrap().execute_batch("DROP TABLE campaigns;").unwrap();
    assert!(e.act(action("Rozglądam się")).await.is_err());
    assert_eq!(e.state.read().await.revision,rev);
}
