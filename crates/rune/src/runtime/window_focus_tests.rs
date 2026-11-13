use super::*;
use crate::{Ime, TextEdit, TextStyle, text_input};

/// A field, and what `App::on_focus` heard.
struct Form {
    field: TextEdit,
    heard: Vec<bool>,
}

fn view(s: &Form, _: &mut Cx<Form>) -> El {
    text_input(&s.field, TextStyle::new(14.0, 400.0)).key("field").size(200.0, 40.0).on_edit(|s: &mut Form| &mut s.field)
}

fn preedit(rt: &Runtime<Form>) -> Option<String> {
    rt.state.field.preedit().map(|(text, _)| text.to_string())
}

/// A focused field in a focused window, composing "pending" after "committed".
fn composing() -> Runtime<Form> {
    let mut edit = TextEdit::new("committed");
    edit.focused = true;
    let mut rt = Runtime::new(Form { field: edit, heard: Vec::new() }, view, (200.0, 40.0), Env::new(true, 1.0));
    rt.hooks.on_focus = Some(|s, focused, _| s.heard.push(focused));
    rt.frame(0.0);
    rt.window_focus(true);
    rt.ime(Ime::Preedit("pending".into(), None, Vec::new()));
    rt.frame(0.0);
    assert_eq!(preedit(&rt), Some("pending".into()));
    rt.edit.ime_reset = false;
    rt
}

#[test]
fn a_window_that_loses_the_focus_ends_the_composition_as_text() {
    let mut rt = composing();
    rt.window_focus(false);
    rt.frame(0.0);
    assert!(preedit(&rt).is_none() && rt.edit.ime_reset, "the input method drops its copy");
    assert_eq!(rt.state.field.text, "committedpending");
}

#[test]
fn a_scene_that_loses_the_focus_and_gets_it_back_keeps_the_composition_open() {
    let mut rt = composing();
    rt.scene_focus(false);
    rt.frame(0.0);
    assert_eq!(rt.host.window_focused, Some(false));
    assert_eq!(preedit(&rt), Some("pending".into()));
    assert_eq!(rt.state.field.text, "committed");
    assert!(!rt.edit.ime_reset, "the keyboard is not told to drop what it composes");
    rt.scene_focus(true);
    rt.frame(0.0);
    assert!(rt.host.window_focused == Some(true) && rt.state.field.focused);
    assert_eq!(preedit(&rt), Some("pending".into()));
    // The composition goes on where it was.
    rt.ime(Ime::Commit("done".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.field.text, "committeddone");
}

#[test]
fn a_scene_that_loses_the_focus_still_tells_the_app_and_releases_held_keys_once() {
    let mut rt = composing();
    rt.scene_focus(false);
    assert!(rt.keys_queued(), "the focused handler hears `PhysicalInput::FocusLost`");
    rt.frame(0.0);
    rt.scene_focus(false);
    assert!(!rt.keys_queued(), "a repeat changes nothing");
    rt.scene_focus(true);
    rt.frame(0.0);
    assert_eq!(rt.state.heard, [true, false, true], "each change once");
}
