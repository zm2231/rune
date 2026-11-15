//! What a platform's checker speaks, as Rune's: a checker's UTF-16 ranges as byte ranges of the
//! text, the words every dictionary marks, and UIKit's names for dictionaries as language tags.

/// Each UTF-16 unit's byte offset in `text`, then the text's length: what turns a checker's
/// UTF-16 offset into a byte offset. A character outside the Basic Multilingual Plane (most
/// emoji) is two units, and both map to its first byte.
pub(super) fn offsets(text: &str) -> Vec<usize> {
    let mut bytes = Vec::with_capacity(text.len() + 1);
    for (i, c) in text.char_indices() {
        bytes.extend(std::iter::repeat_n(i, c.len_utf16()));
    }
    bytes.push(text.len());
    bytes
}

/// The byte range of the `length` UTF-16 units from `location`, by the text's `offsets`. None
/// for a range that is empty, reaches past the text or holds no whole character.
pub(super) fn byte_range(offsets: &[usize], location: usize, length: usize) -> Option<(usize, usize)> {
    let (a, b) = (*offsets.get(location)?, *offsets.get(location.checked_add(length)?)?);
    (a < b).then_some((a, b))
}

/// The ranges every list holds: a word is misspelled when each dictionary says so. No lists
/// (no dictionary) mark nothing. Each list is in the text's order, as a checker reports its
/// words, so each is walked once.
#[cfg(any(test, target_os = "ios"))]
pub(super) fn in_every(lists: impl IntoIterator<Item = Vec<(usize, usize)>>) -> Vec<(usize, usize)> {
    let mut lists = lists.into_iter();
    let Some(first) = lists.next() else { return Vec::new() };
    lists.fold(first, |mut found, other| {
        let mut others = other.into_iter().peekable();
        found.retain(|range| {
            while others.next_if(|o| o < range).is_some() {}
            others.peek() == Some(range)
        });
        found
    })
}

/// UIKit's name for a dictionary (`en_US`) as a language tag (`en-US`).
#[cfg(any(test, target_os = "ios"))]
pub(super) fn tag(identifier: &str) -> String {
    identifier.replace('_', "-")
}

/// UIKit's names for the dictionaries `tags` name, in `tags`' order: each is the one of
/// `identifiers` whose tag it is. A tag UIKit has no dictionary for is left out.
#[cfg(any(test, target_os = "ios"))]
pub(super) fn identifiers(tags: &[String], identifiers: &[String]) -> Vec<String> {
    tags.iter().filter_map(|wanted| identifiers.iter().find(|id| tag(id).eq_ignore_ascii_case(wanted)).cloned()).collect()
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
