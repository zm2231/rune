use super::*;

fn run(state: &mut State, event: Event) -> Vec<Effect> {
    step(state, event).into_iter().collect()
}

#[test]
fn a_hidden_view_parks_on_a_memory_warning_and_remakes_its_gpu_side_when_it_draws_again() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::MemoryWarning { on_screen: false }), [Effect::Trim, Effect::Release]);
    assert!(run(&mut state, Event::MemoryWarning { on_screen: false }).is_empty());
    assert_eq!(run(&mut state, Event::Drawing), [Effect::Remake]);
    assert!(run(&mut state, Event::Drawing).is_empty());
}

#[test]
fn a_view_on_screen_only_trims() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::MemoryWarning { on_screen: true }), [Effect::Trim]);
    assert_eq!(state, State::Live);
}
