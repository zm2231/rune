use rune::{Autocapitalize, Autocomplete, EnterKeyHint, InputMode};

use super::*;
use crate::Kit;

/// Each keyboard builder leaves one change for the editor that `build` makes, as `spellcheck`
/// does, on each of the three fields.
#[test]
fn keyboard_builders_reach_the_editor_as_spellcheck_does() {
    let edit = TextEdit::new("");
    let queued = [
        TextInput::new(&edit).edits.visual.len(),
        TextInput::new(&edit).spellcheck(true).edits.visual.len(),
        TextInput::new(&edit).input_mode(InputMode::Email).edits.visual.len(),
        TextInput::new(&edit).autocomplete(Autocomplete::OneTimeCode).edits.visual.len(),
        TextInput::new(&edit).autocapitalize(Autocapitalize::Words).edits.visual.len(),
        TextInput::new(&edit).autocorrect(false).edits.visual.len(),
        TextInput::new(&edit).enter_key_hint(EnterKeyHint::Send).edits.visual.len(),
    ];
    assert_eq!(queued, [0, 1, 1, 1, 1, 1, 1]);
    let queued = [
        TextArea::new(&edit).edits.visual.len(),
        TextArea::new(&edit).spellcheck(true).edits.visual.len(),
        TextArea::new(&edit).input_mode(InputMode::Url).edits.visual.len(),
        TextArea::new(&edit).autocomplete(Autocomplete::StreetAddress).edits.visual.len(),
        TextArea::new(&edit).autocapitalize(Autocapitalize::None).edits.visual.len(),
        TextArea::new(&edit).autocorrect(true).edits.visual.len(),
        TextArea::new(&edit).enter_key_hint(EnterKeyHint::Done).edits.visual.len(),
    ];
    assert_eq!(queued, [0, 1, 1, 1, 1, 1, 1]);
    let queued = [
        TextField::new("Name", &edit).edits.visual.len(),
        TextField::new("Name", &edit).input_mode(InputMode::Numeric).edits.visual.len(),
        TextField::new("Name", &edit).autocomplete(Autocomplete::Name).edits.visual.len(),
        TextField::new("Name", &edit).autocapitalize(Autocapitalize::Characters).edits.visual.len(),
        TextField::new("Name", &edit).autocorrect(false).edits.visual.len(),
        TextField::new("Name", &edit).enter_key_hint(EnterKeyHint::Next).edits.visual.len(),
    ];
    assert_eq!(queued, [0, 1, 1, 1, 1, 1]);
}

/// A field with every keyboard builder set, and a disabled one, still builds: the builders
/// apply to the editor whether or not it takes edits.
#[test]
fn a_field_with_keyboard_traits_builds_enabled_and_disabled() {
    let k = Kit::neutral(true);
    let edit = TextEdit::new("");
    for disabled in [false, true] {
        let _ = TextInput::new(&edit).input_mode(InputMode::Email).autocomplete(Autocomplete::Email)
            .autocapitalize(Autocapitalize::None).autocorrect(false).enter_key_hint(EnterKeyHint::Next)
            .disabled(disabled).build(&k);
        let _ = TextArea::new(&edit).input_mode(InputMode::Text).autocomplete(Autocomplete::StreetAddress)
            .autocapitalize(Autocapitalize::Words).autocorrect(true).enter_key_hint(EnterKeyHint::Done)
            .disabled(disabled).build(&k);
        let _ = TextField::new("Code", &edit).input_mode(InputMode::Numeric).autocomplete(Autocomplete::OneTimeCode)
            .autocapitalize(Autocapitalize::None).autocorrect(false).enter_key_hint(EnterKeyHint::Go)
            .disabled(disabled).build(&k);
    }
}

const HINT: &str = "Describe what changed, why it changed, and how a reviewer can check it";

struct Form {
    scale: f32,
    filled: bool,
}

fn form(s: &Form, _: &mut rune::Cx<Form>) -> El {
    let k = Kit::neutral(true).with_text_scale(s.scale);
    let edit = TextEdit::new(if s.filled { HINT } else { "" });
    rune::col()
        .items_start()
        .child(TextArea::new(&edit).placeholder(HINT).width(Some(200.0)).build(&k).key("area"))
        .child(rune::el().key("below").size(200.0, 20.0))
}

fn area_and_below(scale: f32, filled: bool) -> ([f32; 4], [f32; 4]) {
    let mut rt = rune::Runtime::new(Form { scale, filled }, form, (400.0, 2000.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    let rect = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect;
    (rect("area"), rect("below"))
}

#[test]
fn an_empty_text_area_grows_to_hold_its_wrapped_placeholder_at_every_text_scale() {
    let (rest, _) = area_and_below(1.0, false);
    assert_eq!(rest[3], 80.0, "a placeholder that fits leaves the area at its minimum");
    for scale in [1.0, 1.6, 2.5] {
        let (area, below) = area_and_below(scale, false);
        let (filled, _) = area_and_below(scale, true);
        assert_eq!(area[3], filled[3], "{scale}: the area is not as tall as the same words typed");
        assert!(below[1] >= area[1] + area[3], "{scale}: the row below overlaps the area: {area:?} {below:?}");
    }
    assert!(area_and_below(2.5, false).0[3] > 80.0, "the test's placeholder should outgrow the minimum at 2.5");
}
