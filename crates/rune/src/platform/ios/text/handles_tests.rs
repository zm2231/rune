use super::*;

fn ends() -> Ends {
    Ends { selection: 4..9, start: [100.0, 50.0, 0.0, 20.0], end: [150.0, 50.0, 0.0, 20.0] }
}

fn grabbed(at: (f32, f32)) -> State {
    let mut state = State::default();
    let effects = step(&mut state, Event::Down { touch: 1, at, ends: Some(ends()) });
    assert_eq!(effects, Effects::one(Effect::LoupeBegin(at)));
    state
}

#[test]
fn a_finger_on_a_handle_grabs_that_end_and_the_other_stays() {
    let mut state = grabbed((152.0, 75.0));
    assert!(state.dragging());
    assert_eq!(state.aim((162.0, 75.0)), Some((160.0, 60.0)));
    assert_eq!(step(&mut state, Event::Move { touch: 1, at: (162.0, 75.0), nearest: 10 }), Effects::two(Effect::LoupeMove((162.0, 75.0)), Effect::Select(4..10)));

    let mut state = grabbed((98.0, 40.0));
    assert_eq!(step(&mut state, Event::Move { touch: 1, at: (88.0, 40.0), nearest: 3 }), Effects::two(Effect::LoupeMove((88.0, 40.0)), Effect::Select(3..9)));
}

#[test]
fn a_finger_away_from_the_handles_or_on_a_caret_is_left_to_the_view() {
    let mut state = State::default();
    assert_eq!(step(&mut state, Event::Down { touch: 1, at: (125.0, 60.0), ends: Some(ends()) }), Effects::none());
    assert!(!state.dragging());
    let caret = Ends { selection: 4..4, ..ends() };
    assert_eq!(step(&mut state, Event::Down { touch: 1, at: (100.0, 60.0), ends: Some(caret) }), Effects::none());
    assert_eq!(step(&mut state, Event::Down { touch: 1, at: (100.0, 60.0), ends: None }), Effects::none());
    assert!(!state.dragging());
}

#[test]
fn a_handle_dragged_past_the_other_end_selects_from_it_and_never_collapses() {
    let mut state = grabbed((150.0, 70.0));
    assert_eq!(step(&mut state, Event::Move { touch: 1, at: (80.0, 60.0), nearest: 2 }), Effects::two(Effect::LoupeMove((80.0, 60.0)), Effect::Select(2..4)));
    assert_eq!(step(&mut state, Event::Move { touch: 1, at: (100.0, 60.0), nearest: 4 }), Effects::one(Effect::LoupeMove((100.0, 60.0))));
    assert_eq!(step(&mut state, Event::Move { touch: 1, at: (81.0, 60.0), nearest: 2 }), Effects::one(Effect::LoupeMove((81.0, 60.0))));
}

#[test]
fn lifting_ends_the_loupe_and_opens_the_menu_once_the_next_frame_has_the_selection() {
    let mut state = grabbed((150.0, 70.0));
    assert_eq!(step(&mut state, Event::Frame), Effects::none());
    assert_eq!(step(&mut state, Event::Up { touch: 2, cancelled: false }), Effects::none());
    assert!(state.dragging(), "another finger lifting leaves the drag");
    assert_eq!(step(&mut state, Event::Up { touch: 1, cancelled: false }), Effects::one(Effect::LoupeEnd));
    assert_eq!(state, State::MenuOwed);
    assert_eq!(step(&mut state, Event::Frame), Effects::one(Effect::Menu));
    assert_eq!(state, State::Idle);
    assert_eq!(step(&mut state, Event::Frame), Effects::none());
}

#[test]
fn a_cancelled_drag_only_ends_the_loupe_and_a_new_grab_drops_an_owed_menu() {
    let mut state = grabbed((150.0, 70.0));
    assert_eq!(step(&mut state, Event::Up { touch: 1, cancelled: true }), Effects::one(Effect::LoupeEnd));
    assert_eq!(state, State::Idle);

    let mut state = grabbed((150.0, 70.0));
    step(&mut state, Event::Up { touch: 1, cancelled: false });
    assert_eq!(step(&mut state, Event::Down { touch: 2, at: (150.0, 70.0), ends: Some(ends()) }), Effects::one(Effect::LoupeBegin((150.0, 70.0))));
    assert!(state.dragging());
}

#[test]
fn a_second_finger_during_a_drag_changes_nothing() {
    let mut state = grabbed((150.0, 70.0));
    assert_eq!(step(&mut state, Event::Down { touch: 2, at: (10.0, 10.0), ends: Some(ends()) }), Effects::none());
    assert_eq!(step(&mut state, Event::Move { touch: 2, at: (20.0, 10.0), nearest: 0 }), Effects::none());
    assert!(state.dragging());
}
