use super::NativeViewId;
use super::handoff::{Effect, Event, Keyboard, step};

const WEB: NativeViewId = NativeViewId(1);
const MAP: NativeViewId = NativeViewId(2);

fn run(state: &mut Keyboard, event: Event) -> Vec<Effect> { step(state, event).into_iter().collect() }

#[test]
fn tab_onto_a_host_enters_it_and_blurs_rune_once_it_has_the_keyboard() {
    let mut state = Keyboard::Rune;
    assert_eq!(run(&mut state, Event::TabInto { view: WEB, back: false }), [Effect::Enter { view: WEB, back: false }]);
    assert_eq!(state, Keyboard::Hosted(WEB));
    assert_eq!(run(&mut state, Event::Pressed(WEB)), [Effect::Blur]);
    assert_eq!(run(&mut state, Event::TabInto { view: MAP, back: true }), [Effect::Enter { view: MAP, back: true }]);
    assert_eq!(state, Keyboard::Hosted(MAP));
}

#[test]
fn a_press_inside_a_host_blurs_rune() {
    let mut state = Keyboard::Rune;
    assert_eq!(run(&mut state, Event::Pressed(WEB)), [Effect::Blur]);
    assert_eq!(state, Keyboard::Hosted(WEB));
}

#[test]
fn tabbing_out_continues_past_the_host_in_its_direction() {
    for back in [false, true] {
        let mut state = Keyboard::Hosted(WEB);
        assert_eq!(run(&mut state, Event::TabbedOut { view: WEB, back }), [Effect::Continue { from: WEB, back }]);
        assert_eq!(run(&mut state, Event::RuneFocused), [Effect::Reclaim(WEB)]);
        assert_eq!(state, Keyboard::Rune);
    }
}

#[test]
fn a_tab_out_reported_without_a_known_host_still_continues() {
    let mut state = Keyboard::Rune;
    assert_eq!(run(&mut state, Event::TabbedOut { view: MAP, back: true }), [Effect::Continue { from: MAP, back: true }]);
    assert_eq!(run(&mut state, Event::RuneFocused), [Effect::Reclaim(MAP)]);
}

#[test]
fn rune_focusing_reclaims_the_keyboard_only_from_a_host() {
    let mut state = Keyboard::Rune;
    assert!(run(&mut state, Event::RuneFocused).is_empty());
    let mut state = Keyboard::Hosted(WEB);
    assert_eq!(run(&mut state, Event::RuneFocused), [Effect::Reclaim(WEB)]);
    assert!(run(&mut state, Event::RuneFocused).is_empty(), "once back, focus moves inside Rune alone");
}

#[test]
fn a_host_that_refuses_the_keyboard_hands_it_back() {
    let mut state = Keyboard::Rune;
    run(&mut state, Event::TabInto { view: WEB, back: false });
    assert_eq!(run(&mut state, Event::Refused(WEB)), [Effect::Reclaim(WEB)]);
    assert_eq!(state, Keyboard::Rune);
}

#[test]
fn a_hardware_tab_with_only_shift_leaves_a_host() {
    let (tab, shift, command, caps) = (0x2B, 1 << 17, 1 << 20, 1 << 16);
    assert_eq!(super::tab_press(tab, 0), Some(false));
    assert_eq!(super::tab_press(tab, shift), Some(true));
    assert_eq!(super::tab_press(tab, shift | caps), Some(true));
    assert_eq!(super::tab_press(tab, command), None, "Command-Tab is the system's");
    assert_eq!(super::tab_press(0x28, 0), None, "Return stays with the view");
}
