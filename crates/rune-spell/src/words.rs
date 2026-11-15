//! A thread's checked words, for a checker with dictionaries to choose (Windows, iOS, a page
//! in a browser): each word's answer is kept, so a key typed into a long draft checks the word
//! it changed.
//!
//! The caches are bounded: a word over [`MAX_WORD_BYTES`] is never checked or kept, and past
//! [`MAX_WORDS`] words or lines a cache starts over, except while a text is still being
//! checked a few words a frame, whose progress it would throw away.
//!
//! A cap is where a cache starts over, not a ceiling. The words' is looked at when a scan
//! finds words with no answer, and counts the words kept then and that scan's; the lines' is
//! looked at before a text adds its own. So the words kept can pass [`MAX_WORDS`] by a text's
//! own words when it has more than that, and by the words that were on the checker thread
//! when it was looked at, and the lines by one text's lines, until the next scan with a new
//! word starts them over. What a text adds is bounded by the text, which has no cap of its
//! own: its different words of at most [`MAX_WORD_BYTES`] bytes, and its lines.
//!
//! Where the checker answers at once (iOS), a text is checked [`MAX_FRESH`] words a scan, and
//! while it is part-way through the caches are held for it: its scans look at neither cap,
//! since starting over would check its first words again each time and a text of more than
//! [`MAX_WORDS`] words would never be whole. The caches are held for [`MAX_PARTIAL`] texts at
//! most. So the words kept can reach [`MAX_WORDS`] and the different words of up to
//! [`MAX_PARTIAL`] such texts, and stay there once those are whole, until a scan of a text
//! not held for finds a new word and starts the cache over. That is bounded by the fields'
//! texts and by no constant. The input that reaches it is a field holding a long run of
//! different words, such as a pasted log or word list.
//!
//! A kept word costs its own bytes, 8 bytes for each misspelled part (most words have one or
//! none) and 38 to 75 bytes of map: a 32-byte entry and a control byte, at a load between 7/8
//! and 7/16. That is at most 211 bytes for a word with one part, before the allocator's
//! rounding, so [`MAX_WORDS`] of the longest are about 1.7 MB (8192 × 211) and 8192 words of
//! prose well under 1 MB. A kept line costs 29 to 57 bytes of map (a 24-byte entry and a
//! control byte) and 8 bytes for each misspelled part in it. A held text therefore costs at
//! most its own bytes and 83 bytes for each different word: a megabyte of different 8-byte
//! words, about 116,000 of them, keeps about 10 MB until the cache starts over.
use std::collections::{HashMap, HashSet};

/// A word's misspelled parts, as byte ranges in the word.
pub(super) type Parts = Box<[(u32, u32)]>;

/// The most words, and the most lines, kept before a cache starts over.
pub(super) const MAX_WORDS: usize = 8192;

/// The longest word checked, in bytes. No dictionary word comes near it; what does is a URL,
/// a hash or pasted data, which goes unmarked, and never becomes a key of the cache.
pub(super) const MAX_WORD_BYTES: usize = 128;

/// The most words a checker that answers at once is given in one scan of a text, which is one
/// field's paint. In a debug build on the iOS simulator `UITextChecker` took about 80 µs for
/// each word it rejected and next to nothing for one it accepted, so 32 words are under 3 ms
/// of a frame when every one is misspelled and far less for prose. A field of 8,000 different
/// words fills in over 250 frames.
pub(super) const MAX_FRESH: usize = 32;

/// The most texts whose checking is under way that the caches are held for.
const MAX_PARTIAL: usize = 8;

/// How a scan gets the words it hasn't seen checked.
pub(super) enum Checker<'a> {
    /// The checker answers at once, on this thread: at most `budget` words a scan.
    Now { check: &'a mut dyn FnMut(&[&str]) -> Vec<(Box<str>, Parts)>, budget: usize },
    /// The checker answers on another thread: every word is asked for, and comes back
    /// through [`Words::learned`].
    Later(&'a mut dyn FnMut(Vec<String>)),
}

/// A thread's checked words, for one generation of the chosen dictionaries.
#[derive(Default)]
pub(super) struct Words {
    pub(super) generation: u64,
    /// Each checked word's misspelled parts.
    known: HashMap<Box<str>, Parts>,
    /// Each line's misspelled words once all its words are checked, by the line's hash.
    lines: HashMap<u64, Parts>,
    /// Words on the checker thread.
    asked: HashSet<Box<str>>,
    /// The texts a scan left with words still to check, by their hashes.
    partial: Vec<u64>,
}

/// `line`'s words and where each starts in it.
fn words(line: &str) -> impl Iterator<Item = (usize, &str)> + '_ {
    line.split_whitespace().map(move |s| (s.as_ptr() as usize - line.as_ptr() as usize, s))
}

/// `line`'s words that are checked: those short enough to be a word.
fn checked(line: &str) -> impl Iterator<Item = (usize, &str)> + '_ {
    words(line).filter(|(_, s)| s.len() <= MAX_WORD_BYTES)
}

fn line_key(line: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    line.hash(&mut h);
    h.finish()
}

impl Words {
    /// No words yet, for the dictionaries of `generation`.
    pub(super) fn of(generation: u64) -> Self {
        Self { generation, ..Self::default() }
    }

    /// A word the checker thread brought back.
    pub(super) fn learned(&mut self, word: Box<str>, parts: Option<Parts>) {
        self.asked.remove(&word);
        if let Some(parts) = parts {
            self.known.insert(word, parts);
        }
    }

    /// The words of `open` lines with no answer and none on its way, each once, in the text's
    /// order.
    fn fresh<'t>(&self, open: &[(&'t str, u64)]) -> Vec<&'t str> {
        let mut seen = HashSet::new();
        open.iter().flat_map(|&(line, _)| checked(line).map(|(_, s)| s))
            .filter(|s| !self.known.contains_key(*s) && !self.asked.contains(*s) && seen.insert(*s)).collect()
    }

    /// `text`'s misspelled words, from each line's and word's kept answers, and whether every
    /// word had one. `key` is the text's hash. The words not yet checked go to `checker`
    /// together: all of them where it answers later, and the first `budget` of the text where
    /// it answers now, so a long text is whole after as many scans as that takes. Words are
    /// what whitespace separates: the checker marks nothing across it, and repeated words,
    /// its only check that reads neighbours, go unmarked. A line's answer is kept once all its
    /// words have one, so a key in a long draft looks up the lines it didn't change, not their
    /// words.
    pub(super) fn scan(&mut self, text: &str, key: u64, checker: Checker) -> (Vec<(usize, usize)>, bool) {
        // A text part-way through its words keeps the answers it has until it is whole.
        let held = self.partial.contains(&key);
        if !held && self.lines.len() > MAX_WORDS { self.lines.clear(); }
        // The lines without a kept answer.
        // Each line is hashed once: a key in a 2,000-line draft hashes its lines, not twice.
        let keys: Vec<u64> = text.split('\n').map(line_key).collect();
        let open: Vec<(&str, u64)> = text.split('\n').zip(keys.iter().copied()).filter(|(_, k)| !self.lines.contains_key(k)).collect();
        let mut fresh = self.fresh(&open);
        let now = matches!(checker, Checker::Now { .. });
        if !fresh.is_empty() {
            if !held && self.known.len() + fresh.len() > MAX_WORDS {
                // The open lines' other words went with the cache: they are asked for again.
                self.known.clear();
                fresh = self.fresh(&open);
            }
            match checker {
                Checker::Now { check, budget } => {
                    fresh.truncate(budget.max(1));
                    self.known.extend(check(&fresh));
                }
                Checker::Later(ask) => {
                    self.asked.extend(fresh.iter().map(|&s| Box::<str>::from(s)));
                    ask(fresh.iter().map(|&s| s.to_owned()).collect());
                }
            }
        }
        let mut whole = true;
        for &(line, k) in &open {
            if checked(line).all(|(_, s)| self.known.contains_key(s)) {
                let parts: Parts = checked(line).flat_map(|(o, s)| self.known.get(s).into_iter().flatten().map(move |&(a, b)| (o as u32 + a, o as u32 + b))).collect();
                self.lines.insert(k, parts);
            } else {
                whole = false;
            }
        }
        let mut found = Vec::new();
        let mut at = 0;
        for (line, k) in text.split('\n').zip(&keys) {
            match self.lines.get(k) {
                Some(parts) => found.extend(parts.iter().map(|&(a, b)| (at + a as usize, at + b as usize))),
                None => found.extend(checked(line).flat_map(|(o, s)| self.known.get(s).into_iter().flatten().map(move |&(a, b)| (at + o + a as usize, at + o + b as usize)))),
            }
            at += line.len() + 1;
        }
        self.partial.retain(|k| *k != key);
        if now && !whole {
            if self.partial.len() >= MAX_PARTIAL { self.partial.clear(); }
            self.partial.push(key);
        }
        (found, whole)
    }
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
