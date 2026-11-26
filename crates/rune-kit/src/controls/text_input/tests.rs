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
