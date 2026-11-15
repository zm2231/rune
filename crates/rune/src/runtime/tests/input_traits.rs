//! `Runtime::focused_input_traits`: what a host tells the soft keyboard about the focused field.

use super::*;
use crate::input::traits::InputTraits;
use crate::{Autocapitalize, Autocomplete, EnterKeyHint, InputMode, TextEdit, TextStyle, edit_access, text_input};

struct Form {
    email: TextEdit,
    code: TextEdit,
    pin: TextEdit,
    note: TextEdit,
    rows: Vec<TextEdit>,
    /// The note asks for a search keyboard and a Search key.
    searching: bool,
}

fn form() -> Form {
    Form {
        email: TextEdit::new(""), code: TextEdit::new(""), pin: TextEdit::new("").secure(), note: TextEdit::new(""),
        rows: (0..50).map(|_| TextEdit::new("")).collect(), searching: false,
    }
}

fn field(edit: &TextEdit) -> El {
    text_input(edit, TextStyle::new(16.0, 400.0)).single_line().size(200.0, 30.0)
}

fn view(s: &Form, _: &mut Cx<Form>) -> El {
    let note = field(&s.note).key("note").spellcheck(true).on_edit(|s: &mut Form| &mut s.note);
    let note = if s.searching { note.input_mode(InputMode::Search).enter_key_hint(EnterKeyHint::Search) } else { note };
    let rows = col().key("rows").w_full().growth(crate::GrowthSpec::default()).virtual_rows(s.rows.len(), 30.0,
        |_: &Form, i| std::borrow::Cow::Owned(format!("row{i}")),
        |s: &Form, i| field(&s.rows[i]).autocomplete(Autocomplete::PostalCode).on_edit(edit_access(move |s: &mut Form| &mut s.rows[i])));
    col().size(200.0, 400.0)
        .child(field(&s.email).key("email").autocomplete(Autocomplete::Email).enter_key_hint(EnterKeyHint::Next)
            .on_edit(|s: &mut Form| &mut s.email))
        .child(field(&s.code).key("code").input_mode(InputMode::Numeric).on_edit(|s: &mut Form| &mut s.code))
        .child(field(&s.pin).key("pin").input_mode(InputMode::Numeric).autocomplete(Autocomplete::NewPassword)
            .autocapitalize(Autocapitalize::Words).autocorrect(true).on_edit(|s: &mut Form| &mut s.pin))
        .child(note)
        // The keyboard builders do nothing on an element that is not a text field.
        .child(el().key("button").focusable(true).size(200.0, 30.0).input_mode(InputMode::Email)
            .autocomplete(Autocomplete::Email).autocapitalize(Autocapitalize::None).autocorrect(false)
            .enter_key_hint(EnterKeyHint::Go))
        .child(col().key("list").w_full().h(120.0).scroll(crate::ScrollSpec { stick_bottom: false }).child(rows))
}

fn runtime() -> Runtime<Form> {
    let mut rt = Runtime::new(form(), view, (200.0, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn focus(rt: &mut Runtime<Form>, key: &str) {
    let path = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).path.clone();
    rt.set_focus(Some(path), true);
    rt.frame(0.0);
}

const EMAIL: InputTraits = InputTraits {
    mode: InputMode::Email, autocomplete: Autocomplete::Email, autocapitalize: Autocapitalize::None,
    autocorrect: Some(false), spellcheck: Some(false), enter_key_hint: EnterKeyHint::Next,
};
const CODE: InputTraits = InputTraits {
    mode: InputMode::Numeric, autocomplete: Autocomplete::Off, autocapitalize: Autocapitalize::None,
    autocorrect: Some(false), spellcheck: Some(false), enter_key_hint: EnterKeyHint::Enter,
};

#[test]
fn only_a_focused_text_field_has_traits() {
    let mut rt = runtime();
    assert_eq!(rt.core().focused_input_traits(), None, "nothing has the focus");
    focus(&mut rt, "email");
    assert_eq!(rt.core().focused_input_traits(), Some(EMAIL));
    focus(&mut rt, "button");
    assert!(rt.has_focus());
    assert_eq!(rt.core().focused_input_traits(), None, "the focus is not on a text field");
    rt.set_focus(None, false);
    rt.frame(0.0);
    assert_eq!(rt.core().focused_input_traits(), None);
}

#[test]
fn the_traits_follow_the_focus_from_field_to_field() {
    let mut rt = runtime();
    focus(&mut rt, "email");
    assert_eq!(rt.core().focused_input_traits(), Some(EMAIL));
    focus(&mut rt, "code");
    assert_eq!(rt.core().focused_input_traits(), Some(CODE));
    for _ in 0..3 {
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.core().focused_input_traits(), Some(CODE), "a frame that changes nothing answers the same");
    }
    focus(&mut rt, "note");
    assert_eq!(rt.core().focused_input_traits(), Some(InputTraits { spellcheck: Some(true), ..InputTraits::default() }),
        "a field that asks for nothing but its own spell checking");
    focus(&mut rt, "email");
    assert_eq!(rt.core().focused_input_traits(), Some(EMAIL));
}

#[test]
fn a_focused_secure_field_is_a_password_that_keeps_its_digits() {
    let mut rt = runtime();
    focus(&mut rt, "pin");
    assert!(rt.has_secure_focus());
    assert_eq!(rt.core().focused_input_traits(), Some(InputTraits {
        mode: InputMode::Numeric, autocomplete: Autocomplete::NewPassword, autocapitalize: Autocapitalize::None,
        autocorrect: Some(false), spellcheck: Some(false), enter_key_hint: EnterKeyHint::Enter,
    }));
    // Revealing the password shows its characters and tells the keyboard nothing new.
    rt.update(|s, _| s.pin.set_revealed(true));
    rt.frame(0.0);
    assert!(rt.has_secure_focus());
    assert_eq!(rt.core().focused_input_traits().map(|t| (t.autocomplete, t.autocorrect)), Some((Autocomplete::NewPassword, Some(false))));
}

#[test]
fn a_field_that_changes_what_it_asks_while_focused_changes_its_traits() {
    let mut rt = runtime();
    focus(&mut rt, "note");
    let before = rt.core().focused_input_traits().unwrap();
    assert_eq!((before.mode, before.enter_key_hint), (InputMode::Text, EnterKeyHint::Enter));
    rt.update(|s, _| s.searching = true);
    rt.frame(0.0);
    let after = rt.core().focused_input_traits().unwrap();
    assert_ne!(before, after, "which is what a host compares to reload its keyboard");
    assert_eq!((after.mode, after.enter_key_hint, after.autocapitalize), (InputMode::Search, EnterKeyHint::Search, Autocapitalize::Sentences));
}

/// A mounted row's field answers as any field does. Scrolled out of the list's window, the
/// focused field is one the list builds unmounted, and still answers.
#[test]
fn a_field_in_a_virtual_row_has_its_traits_mounted_or_scrolled_away() {
    let mut rt = runtime();
    focus(&mut rt, "row2");
    let path = rt.kbd.focus.path().unwrap().to_owned();
    let postal = |rt: &Runtime<Form>| {
        let traits = rt.core().focused_input_traits().expect("the row's field has the focus");
        assert_eq!((traits.autocomplete, traits.mode, traits.autocapitalize, traits.autocorrect),
            (Autocomplete::PostalCode, InputMode::Text, Autocapitalize::Characters, Some(false)));
    };
    assert!(rt.node(&path).is_some());
    postal(&rt);
    rt.update(|_, tx| tx.scroll_to_bottom("list"));
    rt.frame(0.0);
    while rt.frame(1.0 / 60.0) {}
    assert!(rt.node(&path).is_none() && rt.commit.virtual_inputs.contains_key(&path), "the row left the window with its focus");
    postal(&rt);
}

/// `Runtime::focused_history`: what a host that offers undo itself asks of the focused field.
#[test]
fn the_focused_field_says_what_it_can_undo_and_redo() {
    use crate::{Ime, Key, KeyEvent, Mods};
    let undo = |shift| KeyEvent { key: Key::Command('z'), mods: Mods { cmd: true, shift, ..Mods::default() } };
    let mut rt = runtime();
    assert_eq!(rt.core().focused_history(), None, "no field has the focus");
    focus(&mut rt, "button");
    assert_eq!(rt.core().focused_history(), None, "the focus is not on a text field");
    focus(&mut rt, "note");
    assert_eq!(rt.core().focused_history(), Some((false, false)), "a field nothing was typed into");
    rt.ime(Ime::Commit("a".into()));
    rt.frame(0.0);
    assert_eq!(rt.core().focused_history(), Some((true, false)));
    rt.key(undo(false));
    rt.frame(0.0);
    assert_eq!(rt.core().focused_history(), Some((false, true)));
    rt.key(undo(true));
    rt.frame(0.0);
    assert_eq!(rt.core().focused_history(), Some((true, false)));
    focus(&mut rt, "pin");
    rt.ime(Ime::Commit("1".into()));
    rt.frame(0.0);
    assert_eq!(rt.core().focused_history(), Some((false, false)), "a secure field keeps no history");
    focus(&mut rt, "note");
    assert_eq!(rt.core().focused_history(), Some((true, false)), "the field's history is its own");
}
