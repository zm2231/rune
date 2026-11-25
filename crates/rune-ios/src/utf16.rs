//! UIKit counts text in UTF-16 units; Rune's `HostText` counts UTF-8 bytes. Offsets go across
//! here, kept free of UIKit so tests run on any host.

use std::ops::Range;

/// The UTF-16 offset of byte `byte` of `text`, rounding a byte inside a character down to it.
pub(crate) fn units(text: &str, byte: usize) -> usize {
    let byte = text.floor_char_boundary(byte.min(text.len()));
    text[..byte].chars().map(char::len_utf16).sum()
}

/// The byte at UTF-16 offset `unit` of `text`. An offset inside a surrogate pair rounds down to
/// its character; one past the end is the end.
pub(crate) fn byte(text: &str, unit: usize) -> usize {
    let mut at = 0;
    for (i, ch) in text.char_indices() {
        let next = at + ch.len_utf16();
        if next > unit { return i; }
        at = next;
    }
    text.len()
}

pub(crate) fn len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

pub(crate) fn unit_range(text: &str, bytes: Range<usize>) -> Range<usize> {
    units(text, bytes.start)..units(text, bytes.end)
}

pub(crate) fn byte_range(text: &str, units: Range<usize>) -> Range<usize> {
    let (a, b) = (byte(text, units.start), byte(text, units.end));
    a.min(b)..a.max(b)
}

/// `text`'s UTF-16 units `range`, clamped to it, as a string.
pub(crate) fn slice(text: &str, range: Range<usize>) -> &str {
    let r = byte_range(text, range);
    &text[r]
}

/// The offset `by` characters' worth of units from `from`, clamped to the text: UIKit's
/// `positionFromPosition:offset:` counts units, but never lands inside a character.
pub(crate) fn step(text: &str, from: usize, by: isize) -> Option<usize> {
    let to = from.checked_add_signed(by)?;
    if to > len(text) { return None; }
    let at = byte(text, to);
    let down = units(text, at);
    if down == to || by <= 0 { return Some(down); }
    Some(units(text, at + text[at..].chars().next().map_or(0, char::len_utf8)))
}

#[cfg(test)]
#[path = "utf16_tests.rs"]
mod tests;
