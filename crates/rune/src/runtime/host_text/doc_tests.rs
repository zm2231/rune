use super::*;

fn at(text: &str, anchor: usize, focus: usize) -> TextEdit {
    let mut e = TextEdit::new(text);
    e.focused = true;
    e.set_selection(anchor, focus);
    e
}

fn compose(e: &mut TextEdit, text: &str, cursor: Option<Range<usize>>) {
    apply(e, HostEdit::Compose { text: text.into(), cursor });
}

#[test]
fn a_plain_field_shows_its_text_and_selection() {
    let e = at("hello", 1, 4);
    assert_eq!(shape(&e), Shape { text: "hello".into(), anchor: 1, focus: 4, marked: None });
}

#[test]
fn a_composition_sits_in_the_text_over_the_selection_it_replaces() {
    let mut e = at("abcd", 1, 3);
    compose(&mut e, "かな", None);
    let s = shape(&e);
    assert_eq!(s.text, "aかなd");
    assert_eq!(s.marked, Some(1..7));
    assert_eq!((s.anchor, s.focus), (7, 7), "with no cursor of its own the caret ends the composition");
    compose(&mut e, "かな", Some(0..3));
    let s = shape(&e);
    assert_eq!((s.anchor, s.focus), (1, 4), "the composition's own selection, in the drawn text");
}

#[test]
fn writing_over_the_marked_range_commits_the_conversion() {
    let mut e = at("ab", 1, 1);
    compose(&mut e, "かんじ", None);
    let marked = shape(&e).marked.unwrap();
    apply(&mut e, HostEdit::Replace { range: marked, text: "漢字".into() });
    assert_eq!(e.text, "a漢字b");
    assert!(!e.is_composing());
    assert_eq!(e.range(), (7, 7));
}

#[test]
fn a_composition_over_a_selection_replaces_it_when_kept() {
    let mut e = at("abcd", 1, 3);
    compose(&mut e, "x", None);
    apply(&mut e, HostEdit::FinishComposition);
    assert_eq!(e.text, "axd");
    assert_eq!(shape(&e), Shape { text: "axd".into(), anchor: 2, focus: 2, marked: None });
}

#[test]
fn selecting_inside_the_composition_moves_its_cursor() {
    let mut e = at("ab", 1, 1);
    compose(&mut e, "にほん", None);
    apply(&mut e, HostEdit::Select { anchor: 4, focus: 4 });
    assert!(e.is_composing());
    assert_eq!(e.preedit(), Some(("にほん", Some((3, 3)))));
    assert_eq!(e.text, "ab", "the model holds no composed text yet");
}

#[test]
fn selecting_outside_the_composition_keeps_it_as_text_first() {
    let mut e = at("ab", 1, 1);
    compose(&mut e, "にほん", None);
    // "aにほんb": 11 is after "b" in the drawn text.
    apply(&mut e, HostEdit::Select { anchor: 11, focus: 11 });
    assert!(!e.is_composing());
    assert_eq!(e.text, "aにほんb");
    assert_eq!(e.range(), (11, 11));
}

#[test]
fn autocorrect_replaces_a_word_behind_the_caret() {
    let mut e = at("I saw teh ", 10, 10);
    apply(&mut e, HostEdit::Replace { range: 6..9, text: "the".into() });
    assert_eq!(e.text, "I saw the ");
    assert_eq!(e.range(), (9, 9));
}

#[test]
fn offsets_inside_a_character_round_down_to_its_start() {
    let mut e = at("a👍🏽b", 0, 0);
    apply(&mut e, HostEdit::Select { anchor: 3, focus: 3 });
    assert_eq!(e.range(), (1, 1));
    apply(&mut e, HostEdit::Replace { range: 1..9, text: "🙂".into() });
    assert_eq!(e.text, "a🙂b");
}

#[test]
fn a_secure_field_shows_one_bullet_a_character_and_never_composes() {
    let mut e = at("pä🔑", 0, 0);
    e.set_secure(true);
    e.set_single_line(true);
    let len = e.text.len();
    e.set_selection(len, len);
    let s = shape(&e);
    assert_eq!(s.text, "•••");
    assert_eq!((s.anchor, s.focus, s.marked), (9, 9, None));
    assert!(!s.text.contains('p'), "the secret never reaches the host");
    compose(&mut e, "x", None);
    assert_eq!(shape(&e).marked, None);
    apply(&mut e, HostEdit::Replace { range: 3..6, text: "o".into() });
    assert_eq!(e.text, "po🔑", "a bullet's offsets reach the character it masks");
}

#[test]
fn a_return_from_the_keyboard_is_the_return_key() {
    assert_eq!(typed_key("\n").key, Key::Enter);
    assert_eq!(typed_key("é").key, Key::Text("é".into()));
}
