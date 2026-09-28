use crate::domain::GameState;

pub fn advance(state:&mut GameState) -> Vec<String> {
    let mut events=Vec::new();
    state.director.tension = state.director.tension.clamp(0,100);

    for (name,clock) in state.director.clocks.iter_mut() {
        if *clock >= 10 {
            events.push(format!("Wątek '{}' osiąga punkt przełomowy.",name));
            *clock=0;
        }
    }

    if state.director.tension >= 70 {
        if let Some(thread)=state.director.open_threads.first().cloned() {
            events.push(format!("Presja wątku '{}' staje się widoczna w scenie.",thread));
            state.director.tension=(state.director.tension-15).max(0);
        }
    }

    if events.is_empty() {
        for npc in state.world.npcs.values().filter(|n|n.active) {
            if let Some(step)=npc.plan.first() {
                events.push(format!("{} przygotowuje ruch wynikający z planu: {}",npc.name,step));
                break;
            }
        }
    }

    if !events.is_empty() {
        state.director.recent_events.extend(events.clone());
        if state.director.recent_events.len()>12 {
            let drain=state.director.recent_events.len()-12;
            state.director.recent_events.drain(0..drain);
        }
    }
    events
}
