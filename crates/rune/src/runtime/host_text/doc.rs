//! A field's model as the host sees it (its drawn text) and the host's edits back onto the
//! model. Plain functions of a `TextEdit`, so every case runs without a runtime.

use std::ops::Range;

use crate::edit::TextEdit;
use crate::{Ime, Key, KeyEvent};

use super::HostEdit;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) text: String,
    pub(super) anchor: usize,
    pub(super) focus: usize,
    pub(super) marked: Option<Range<usize>>,
}

pub(super) fn shape(e: &TextEdit) -> Shape {
    let (text, marked) = e.display();
    match (marked, e.preedit()) {
        (Some((a, b)), Some((_, cursor))) => {
            let (c0, c1) = cursor.unwrap_or((b - a, b - a));
            Shape { text, anchor: a + c0.min(b - a), focus: a + c1.min(b - a), marked: Some(a..b) }
        }
        _ => Shape { anchor: e.display_index(e.anchor), focus: e.display_index(e.focus), text, marked: None },
    }
}

/// Drawn byte `byte` as a model byte. Outside a composition a plain field draws its model as
/// it is; a secure field draws a bullet a char.
fn to_model(e: &TextEdit, byte: usize) -> usize {
    let at = if e.is_secure() { e.model_index(byte) } else { byte.min(e.text.len()) };
    e.text.floor_char_boundary(at)
}

pub(super) fn apply(e: &mut TextEdit, edit: HostEdit) {
    match edit {
        HostEdit::Replace { range, text } => {
            // Kept as text, the composition is what the host's offsets already count.
            e.finish_composition();
            let (a, b) = (to_model(e, range.start), to_model(e, range.end.max(range.start)));
            e.set_selection(a, b);
            e.snap_to_pills();
            if e.is_secure() {
                e.insert(&text);
            } else {
                e.ime(&Ime::Commit(text));
            }
        }
        HostEdit::Select { anchor, focus } => {
            let (marked, text) = match (e.display().1, e.preedit()) {
                (Some(m), Some((p, _))) => (Some(m), p.to_owned()),
                _ => (None, String::new()),
            };
            if let Some((a, b)) = marked {
                let inside = |x: usize| (a..=b).contains(&x);
                if inside(anchor) && inside(focus) {
                    let cursor = (floor(&text, anchor - a), floor(&text, focus - a));
                    e.ime(&Ime::Preedit(text, Some(cursor), Vec::new()));
                    return;
                }
            }
            e.finish_composition();
            let (a, f) = (to_model(e, anchor), to_model(e, focus));
            e.set_selection(a, f);
            e.snap_to_pills();
        }
        HostEdit::Compose { text, cursor } => {
            let cursor = cursor.map(|c| (floor(&text, c.start), floor(&text, c.end.max(c.start))));
            e.ime(&Ime::Preedit(text, cursor, Vec::new()));
        }
        HostEdit::FinishComposition => {
            e.finish_composition();
        }
    }
}

fn floor(s: &str, at: usize) -> usize {
    s.floor_char_boundary(at.min(s.len()))
}

/// The key a host's typed text is, as a hardware keyboard would send it: Return for a line
/// break, text otherwise.
pub(crate) fn typed_key(text: &str) -> KeyEvent {
    let key = if text == "\n" || text == "\r" { Key::Enter } else { Key::Text(text.to_owned()) };
    KeyEvent { key, mods: Default::default() }
}

#[cfg(test)]
#[path = "doc_tests.rs"]
mod tests;
