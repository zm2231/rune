use super::*;

fn two(mode: Mode, details_visible: bool) -> Windows {
    Windows::new(mode, vec![
        Window { key: "main".into(), starts_visible: true, state: State::Hidden },
        Window { key: "details".into(), starts_visible: details_visible, state: State::Hidden },
    ])
}

fn effects(windows: &mut Windows, event: Event<'_>) -> Vec<Effect> {
    step(windows, event).into_iter().collect()
}

#[test]
fn ipad_fresh_launch_opens_a_visible_second_window_as_its_own_scene_after_the_first_frame() {
    let mut w = two(Mode::Scenes, true);
    assert_eq!(effects(&mut w, Event::Connected { scene: 1, key: None }), [Effect::Attach { window: 0, scene: 1 }]);
    assert_eq!(effects(&mut w, Event::Drawn), [Effect::Activate(1)]);
    assert_eq!(effects(&mut w, Event::Connected { scene: 2, key: Some("details") }), [Effect::Attach { window: 1, scene: 2 }]);
    assert_eq!((w.state(0), w.state(1)), (State::Scene(1), State::Scene(2)));
    assert!(effects(&mut w, Event::Drawn).is_empty());
}

#[test]
fn ipad_restored_launch_leaves_closed_windows_closed() {
    let mut w = two(Mode::Scenes, true);
    assert_eq!(effects(&mut w, Event::Connected { scene: 1, key: Some("main") }), [Effect::Attach { window: 0, scene: 1 }]);
    assert!(effects(&mut w, Event::Drawn).is_empty());
    assert_eq!(w.state(1), State::Hidden);
}

#[test]
fn ipad_restores_the_second_window_into_the_scene_that_carries_its_key() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: Some("main") });
    assert_eq!(effects(&mut w, Event::Connected { scene: 2, key: Some("details") }), [Effect::Attach { window: 1, scene: 2 }]);
}

#[test]
fn ipad_refuses_a_session_whose_key_is_unknown_or_already_showing() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    assert_eq!(effects(&mut w, Event::Connected { scene: 2, key: Some("gone") }), [Effect::Refuse(2)]);
    assert_eq!(effects(&mut w, Event::Connected { scene: 3, key: Some("main") }), [Effect::Refuse(3)]);
    assert_eq!(effects(&mut w, Event::Connected { scene: 4, key: None }), [Effect::Refuse(4)]);
}

#[test]
fn ipad_user_closing_a_window_runs_on_close_and_a_show_brings_the_same_window_back() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Show }), [Effect::Activate(1)]);
    effects(&mut w, Event::Connected { scene: 2, key: Some("details") });
    assert!(effects(&mut w, Event::Disconnected(2)).is_empty());
    assert_eq!(w.state(1), State::Away);
    assert_eq!(effects(&mut w, Event::Discarded("details")), [Effect::Closed(1)]);
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Show }), [Effect::Activate(1)]);
    assert_eq!(effects(&mut w, Event::Connected { scene: 3, key: Some("details") }), [Effect::Attach { window: 1, scene: 3 }]);
}

#[test]
fn ipad_a_scene_the_system_disconnects_comes_back_without_on_close() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    effects(&mut w, Event::Ask { window: 1, ask: Ask::Show });
    effects(&mut w, Event::Connected { scene: 2, key: Some("details") });
    effects(&mut w, Event::Disconnected(2));
    assert_eq!(effects(&mut w, Event::Connected { scene: 5, key: Some("details") }), [Effect::Attach { window: 1, scene: 5 }]);
}

#[test]
fn ipad_hide_and_close_end_the_scene_and_only_close_runs_on_close() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    effects(&mut w, Event::Ask { window: 1, ask: Ask::Show });
    effects(&mut w, Event::Connected { scene: 2, key: Some("details") });
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Hide }), [Effect::Destroy(1)]);
    assert!(effects(&mut w, Event::Disconnected(2)).is_empty());
    assert!(effects(&mut w, Event::Discarded("details")).is_empty());
    effects(&mut w, Event::Ask { window: 1, ask: Ask::Show });
    effects(&mut w, Event::Connected { scene: 3, key: Some("details") });
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Close }), [Effect::Destroy(1), Effect::Closed(1)]);
}

#[test]
fn ipad_hide_while_the_scene_is_asked_for_refuses_it_when_it_connects() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    effects(&mut w, Event::Ask { window: 1, ask: Ask::Show });
    assert!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Hide }).is_empty());
    assert_eq!(effects(&mut w, Event::Connected { scene: 2, key: Some("details") }), [Effect::Refuse(2)]);
    assert_eq!(w.state(1), State::Hidden);
}

#[test]
fn ipad_focus_brings_a_shown_window_forward() {
    let mut w = two(Mode::Scenes, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    assert_eq!(effects(&mut w, Event::Ask { window: 0, ask: Ask::Focus }), [Effect::Activate(0)]);
    assert!(effects(&mut w, Event::Ask { window: 0, ask: Ask::Show }).is_empty());
}

#[test]
fn iphone_shows_a_second_window_as_a_sheet_after_the_first_frame_on_every_launch() {
    let mut w = two(Mode::Sheets, true);
    assert_eq!(effects(&mut w, Event::Connected { scene: 1, key: Some("main") }), [Effect::Attach { window: 0, scene: 1 }]);
    assert_eq!(effects(&mut w, Event::Drawn), [Effect::Present(1)]);
    assert_eq!(w.state(1), State::Sheet);
}

#[test]
fn iphone_swiping_a_sheet_down_runs_on_close_and_a_show_presents_the_same_window_again() {
    let mut w = two(Mode::Sheets, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Show }), [Effect::Present(1)]);
    assert_eq!(effects(&mut w, Event::Dismissed(1)), [Effect::Closed(1)]);
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Show }), [Effect::Present(1)]);
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Close }), [Effect::Dismiss(1), Effect::Closed(1)]);
}

#[test]
fn iphone_hiding_a_lower_sheet_presents_the_ones_above_again() {
    let mut w = two(Mode::Sheets, false);
    let third = w.push("third".into());
    effects(&mut w, Event::Connected { scene: 1, key: None });
    effects(&mut w, Event::Ask { window: 1, ask: Ask::Show });
    effects(&mut w, Event::Ask { window: third, ask: Ask::Show });
    assert_eq!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Hide }), [Effect::Dismiss(1), Effect::Present(third)]);
    assert_eq!(w.state(third), State::Sheet);
}

#[test]
fn iphone_the_primary_cannot_hide() {
    let mut w = two(Mode::Sheets, false);
    effects(&mut w, Event::Connected { scene: 1, key: None });
    assert_eq!(effects(&mut w, Event::Ask { window: 0, ask: Ask::Close }), [Effect::Refused(0)]);
    assert_eq!(w.state(0), State::Scene(1));
}

#[test]
fn iphone_a_sheet_asked_for_before_the_scene_connects_presents_with_it() {
    let mut w = two(Mode::Sheets, false);
    assert!(effects(&mut w, Event::Ask { window: 1, ask: Ask::Show }).is_empty());
    assert_eq!(effects(&mut w, Event::Connected { scene: 1, key: None }), [Effect::Attach { window: 0, scene: 1 }, Effect::Present(1)]);
    effects(&mut w, Event::Disconnected(1));
    assert_eq!(effects(&mut w, Event::Connected { scene: 2, key: None }), [Effect::Attach { window: 0, scene: 2 }, Effect::Present(1)]);
}
