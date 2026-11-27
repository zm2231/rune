use super::*;

fn run(seq: &mut Seq, state: &mut State, event: Event) -> Vec<Effect> {
    step(seq, state, event).into_iter().collect()
}

fn asked(seq: &mut Seq, state: &mut State) -> Attempt {
    let effects = run(seq, state, Event::Grab);
    let State::Asking(attempt) = *state else { panic!("a grab asks UIKit: {state:?}") };
    assert_eq!(effects, [Effect::Prefer(true), Effect::AnswerAfter(attempt, ANSWER_SECS)]);
    attempt
}

#[test]
fn the_scene_locking_the_pointer_grants_the_lock_and_starts_motion() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    let attempt = asked(&mut seq, &mut state);
    assert_eq!(run(&mut seq, &mut state, Event::Changed { locked: true }), [Effect::Motion(true), Effect::Granted]);
    assert_eq!(state, State::Held);
    assert!(run(&mut seq, &mut state, Event::Due { attempt, locked: true }).is_empty());
}

#[test]
fn a_scene_that_never_locks_refuses_and_stops_asking() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    let attempt = asked(&mut seq, &mut state);
    assert!(run(&mut seq, &mut state, Event::Changed { locked: false }).is_empty());
    assert_eq!(run(&mut seq, &mut state, Event::Due { attempt, locked: false }), [Effect::Prefer(false), Effect::Lost]);
    assert_eq!(state, State::Free);
    assert!(run(&mut seq, &mut state, Event::Changed { locked: true }).is_empty());
}

#[test]
fn a_lock_already_in_place_when_the_answer_is_due_is_granted() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    let attempt = asked(&mut seq, &mut state);
    assert_eq!(run(&mut seq, &mut state, Event::Due { attempt, locked: true }), [Effect::Motion(true), Effect::Granted]);
    assert_eq!(state, State::Held);
}

#[test]
fn the_scene_dropping_the_lock_reports_it_lost_and_stops_asking() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    asked(&mut seq, &mut state);
    run(&mut seq, &mut state, Event::Changed { locked: true });
    assert_eq!(run(&mut seq, &mut state, Event::Changed { locked: false }), [Effect::Motion(false), Effect::Prefer(false), Effect::Lost]);
    assert_eq!(state, State::Free);
}

#[test]
fn releasing_the_lock_stops_motion_and_asks_uikit_to_let_go_without_reporting_a_loss() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    asked(&mut seq, &mut state);
    run(&mut seq, &mut state, Event::Changed { locked: true });
    assert_eq!(run(&mut seq, &mut state, Event::Ungrab), [Effect::Motion(false), Effect::Prefer(false)]);
    assert!(run(&mut seq, &mut state, Event::Changed { locked: false }).is_empty());
}

#[test]
fn an_answer_due_for_an_earlier_ask_changes_nothing() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    let first = asked(&mut seq, &mut state);
    assert_eq!(run(&mut seq, &mut state, Event::Ungrab), [Effect::Prefer(false)]);
    assert!(run(&mut seq, &mut state, Event::Due { attempt: first, locked: false }).is_empty());
    let second = asked(&mut seq, &mut state);
    assert_ne!(first, second);
    assert!(run(&mut seq, &mut state, Event::Due { attempt: first, locked: false }).is_empty());
    assert_eq!(state, State::Asking(second));
}
