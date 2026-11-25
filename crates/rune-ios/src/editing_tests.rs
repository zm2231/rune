use super::*;
use rune::Keyboard;

use EditingEffect::*;

fn field(key: &str, text: &str, caret: usize) -> Field {
    Field {
        key: key.into(),
        text: text.into(),
        selection: (caret, caret),
        marked: None,
        traits: Traits { hints: TextHints::default(), secure: false, single_line: true },
        offset: [0.0, 0.0],
    }
}

fn run(editor: &mut Editor, event: EditingEvent) -> Vec<EditingEffect> {
    step(editor, event).into_iter().collect()
}

fn editing(f: Field) -> Editor {
    let mut e = Editor::default();
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(f))), [BecomeFirstResponder]);
    e
}

#[test]
fn focus_shows_the_keyboard_and_blur_hides_it() {
    let mut e = editing(field("name", "", 0));
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(field("name", "", 0)))), []);
    assert_eq!(run(&mut e, EditingEvent::Framed(None)), [ResignFirstResponder]);
    assert_eq!(e.state, Editing::Idle);
    assert_eq!(run(&mut e, EditingEvent::Framed(None)), []);
}

#[test]
fn an_edit_uikit_made_never_echoes_back() {
    let mut e = editing(field("name", "", 0));
    run(&mut e, EditingEvent::CallBegan);
    assert_eq!(run(&mut e, EditingEvent::Edited(Some(field("name", "a", 1)))), []);
    assert_eq!(run(&mut e, EditingEvent::CallEnded), [], "nothing was held, so no turn is asked for");
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(field("name", "a", 1)))), [], "the next frame finds what UIKit already saw");
}

#[test]
fn a_change_the_app_made_reaches_uikit() {
    let mut e = editing(field("name", "abc", 3));
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(field("name", "", 0)))), [TextChanged]);
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(field("name", "", 0)))), []);
    let mut moved = field("name", "", 0);
    moved.offset = [0.0, -12.0];
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(moved))), [SelectionChanged], "a scroll moves the handles");
}

#[test]
fn a_tap_that_moves_the_caret_tells_uikit() {
    let mut e = editing(field("name", "hello", 5));
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(field("name", "hello", 2)))), [SelectionChanged]);
}

#[test]
fn another_field_reloads_the_keyboard_when_its_traits_differ() {
    let mut e = editing(field("name", "x", 1));
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(field("other", "y", 1)))), [TextChanged]);
    let mut email = field("email", "", 0);
    email.traits.hints.keyboard = Keyboard::Email;
    assert_eq!(run(&mut e, EditingEvent::Framed(Some(email))), [ReloadInputViews, TextChanged]);
}

#[test]
fn uikit_is_called_back_only_after_its_own_call_returns() {
    let mut e = editing(field("name", "a", 1));
    run(&mut e, EditingEvent::CallBegan);
    // Return in "name" submitted, and the app moved focus to "next" inside the call's frame.
    let mut next = field("next", "", 0);
    next.traits.secure = true;
    assert_eq!(run(&mut e, EditingEvent::Edited(Some(next))), [], "held while the call is open");
    assert_eq!(run(&mut e, EditingEvent::CallEnded), [ScheduleFlush]);
    assert_eq!(run(&mut e, EditingEvent::Flush), [ReloadInputViews, TextChanged]);
    assert_eq!(run(&mut e, EditingEvent::Flush), [], "a second turn finds nothing left");
}

#[test]
fn a_blur_inside_a_call_resigns_after_it() {
    let mut e = editing(field("name", "a", 1));
    run(&mut e, EditingEvent::CallBegan);
    run(&mut e, EditingEvent::CallBegan);
    assert_eq!(run(&mut e, EditingEvent::Edited(None)), []);
    assert_eq!(run(&mut e, EditingEvent::CallEnded), [], "the outer call is still open");
    assert_eq!(run(&mut e, EditingEvent::CallEnded), [ScheduleFlush]);
    run(&mut e, EditingEvent::CallBegan);
    assert_eq!(run(&mut e, EditingEvent::Flush), [], "a turn that lands inside another call waits for it");
    assert_eq!(run(&mut e, EditingEvent::CallEnded), [ScheduleFlush]);
    assert_eq!(run(&mut e, EditingEvent::Flush), [ResignFirstResponder]);
}

#[test]
fn effects_after_a_call_wait_behind_the_ones_it_held() {
    let mut e = editing(field("name", "a", 1));
    run(&mut e, EditingEvent::CallBegan);
    run(&mut e, EditingEvent::Edited(Some(field("other", "", 0))));
    run(&mut e, EditingEvent::CallEnded);
    assert_eq!(run(&mut e, EditingEvent::Framed(None)), [], "a frame before the turn keeps the order");
    assert_eq!(run(&mut e, EditingEvent::Flush), [TextChanged, ResignFirstResponder]);
}

#[test]
fn dismissing_the_keyboard_blurs_the_field_once() {
    let mut e = editing(field("name", "a", 1));
    assert_eq!(run(&mut e, EditingEvent::Resigned), [Blur("name".into())]);
    assert_eq!(run(&mut e, EditingEvent::Resigned), [], "our own resign comes back as nothing");
}
