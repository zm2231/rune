//! UIKit's `UITextChecker`, which checks one language at a time: one checker, asked once per
//! chosen dictionary (`super::languages`). UIKit keeps the class to the main thread, where a
//! view's fields are drawn and their menus open; asked anywhere else, these answer nothing.

#![allow(unsafe_code)]

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, class, msg_send};
use objc2_foundation::{NSArray, NSRange, NSString};

use super::convert;

/// The checker runs in this process: words are checked where they're asked for.
pub(super) const OFF_THREAD: bool = false;

/// `NSNotFound`, the location of a range that found nothing.
const NOT_FOUND: usize = isize::MAX as usize;

/// The main thread's checker and UIKit's names for the dictionaries it checks with, for the
/// generation they were chosen in.
struct Checker {
    checker: Retained<AnyObject>,
    generation: u64,
    languages: Vec<Retained<NSString>>,
}

thread_local! {
    static CHECKER: RefCell<Option<Checker>> = const { RefCell::new(None) };
}

/// Runs `f` with the checker and the chosen dictionaries. None off the main thread.
fn with_checker<T>(f: impl FnOnce(&AnyObject, &[Retained<NSString>]) -> T) -> Option<T> {
    MainThreadMarker::new()?;
    CHECKER.with_borrow_mut(|slot| {
        let generation = super::generation();
        // SAFETY: `new` makes a `UITextChecker`, on the main thread.
        let held = slot.get_or_insert_with(|| Checker { checker: unsafe { msg_send![class!(UITextChecker), new] }, generation: u64::MAX, languages: Vec::new() });
        if held.generation != generation {
            let chosen = convert::identifiers(&super::languages(), &identifiers());
            held.languages = chosen.iter().map(|id| NSString::from_str(id)).collect();
            held.generation = generation;
        }
        Some(f(&held.checker, &held.languages))
    })
}

/// UIKit's own names for its dictionaries (`en_US`), in the user's order of preference.
fn identifiers() -> Vec<String> {
    if MainThreadMarker::new().is_none() {
        return Vec::new();
    }
    // SAFETY: a class property that answers an array of strings, read on the main thread.
    let list: Retained<NSArray<NSString>> = unsafe { msg_send![class!(UITextChecker), availableLanguages] };
    list.iter().map(|id| id.to_string()).collect()
}

pub(super) fn misspelled(text: &str) -> Vec<(usize, usize)> {
    let offsets = convert::offsets(text);
    let string = NSString::from_str(text);
    let (all, end) = (NSRange::new(0, string.length()), string.length());
    with_checker(|checker, languages| {
        // A word is misspelled when every dictionary says so, as on Windows.
        convert::in_every(languages.iter().map(|language| {
            let mut found = Vec::new();
            let mut from = 0usize;
            while from < end {
                // SAFETY: the checker is the main thread's; the range is all of `string`, the
                // offset an `NSInteger` inside it, and `wrap` a `BOOL`. The answer is an
                // `NSRange` in UTF-16 units.
                let range: NSRange = unsafe {
                    msg_send![checker, rangeOfMisspelledWordInString: &*string, range: all, startingAt: from as isize, wrap: false, language: &**language]
                };
                if range.location == NOT_FOUND || range.length == 0 || range.location < from {
                    break;
                }
                found.extend(convert::byte_range(&offsets, range.location, range.length));
                from = range.location + range.length;
            }
            found
        }))
    })
    .unwrap_or_default()
}

/// Each chosen dictionary's guesses in turn, the first dictionary's first.
pub(super) fn suggest(word: &str) -> Vec<String> {
    let string = NSString::from_str(word);
    let range = NSRange::new(0, string.length());
    with_checker(|checker, languages| {
        let mut found: Vec<String> = Vec::new();
        for language in languages {
            // SAFETY: as in `misspelled`; `range` is all of `string`, and the answer is an
            // array of strings or nil.
            let guesses: Option<Retained<NSArray<NSString>>> = unsafe { msg_send![checker, guessesForWordRange: range, inString: &*string, language: &**language] };
            for guess in guesses.iter().flat_map(|list| list.iter()).map(|guess| guess.to_string()) {
                if !found.contains(&guess) {
                    found.push(guess);
                }
            }
        }
        found
    })
    .unwrap_or_default()
}

/// The dictionaries UIKit has (`UITextChecker.availableLanguages`), as language tags.
pub(super) fn available() -> Vec<String> {
    identifiers().iter().map(|id| convert::tag(id)).collect()
}

/// The user's languages, most preferred first (`NSLocale.preferredLanguages`).
pub(super) fn preferred() -> Vec<String> {
    // SAFETY: a class property that answers an array of strings, on any thread.
    let list: Retained<NSArray<NSString>> = unsafe { msg_send![class!(NSLocale), preferredLanguages] };
    list.iter().map(|tag| tag.to_string()).collect()
}

/// The language's English name, "French (France)".
pub(super) fn english_name(tag: &str) -> String {
    name_in(tag, "en").unwrap_or_else(|| tag.to_owned())
}

/// `tag`'s name in `locale`'s language, as Foundation's `NSLocale` names a locale.
pub(super) fn name_in(tag: &str, locale: &str) -> Option<String> {
    // SAFETY: a locale made from an identifier, asked to name another; the name is a string
    // or nil. Foundation's locales answer on any thread.
    let name: Option<Retained<NSString>> = unsafe {
        let locale: Retained<AnyObject> = msg_send![class!(NSLocale), localeWithLocaleIdentifier: &*NSString::from_str(locale)];
        msg_send![&*locale, localizedStringForLocaleIdentifier: &*NSString::from_str(tag)]
    };
    name.map(|name| name.to_string()).filter(|name| !name.is_empty())
}

/// Left to the caller, which sorts the names by their lowercase.
pub(super) fn sort_in<T>(_: &mut [(String, T)], _: &str) -> bool { false }

/// Adds `word` to the words UIKit has learned, for every app's checker and every language.
pub(super) fn learn(word: &str) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: a class method that takes a string, on the main thread.
    let _: () = unsafe { msg_send![class!(UITextChecker), learnWord: &*NSString::from_str(word)] };
}
