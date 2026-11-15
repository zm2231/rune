use super::*;

/// A checker that calls a word misspelled when it starts with `x`, and counts what it is given.
#[derive(Default)]
struct Fake {
    calls: usize,
    words: usize,
}

impl Fake {
    fn check(&mut self, words: &[&str]) -> Vec<(Box<str>, Parts)> {
        self.calls += 1;
        self.words += words.len();
        words.iter().map(|w| ((*w).into(), if w.starts_with('x') { [(0, w.len() as u32)].into() } else { [].into() })).collect()
    }

    /// One scan, as one paint of the text's field: the marks, whether the text is whole, and
    /// how many words the checker was given.
    fn scan(&mut self, cache: &mut Words, text: &str) -> (Vec<(usize, usize)>, bool, usize) {
        let before = self.words;
        let (found, whole) = cache.scan(text, Checker::Now(&mut |words| self.check(words)));
        (found, whole, self.words - before)
    }
}

/// Where a checker that sees the whole text at once would mark it.
fn expected(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut at = 0;
    for line in text.split('\n') {
        found.extend(words(line).filter(|(_, s)| s.starts_with('x') && s.len() <= MAX_WORD_BYTES).map(|(o, s)| (at + o, at + o + s.len())));
        at += line.len() + 1;
    }
    found
}

/// `count` different words, every third misspelled, `per_line` to a line.
fn prose(count: usize, per_line: usize) -> String {
    let words: Vec<String> = (0..count).map(|n| if n % 3 == 0 { format!("x{n}") } else { format!("w{n}") }).collect();
    words.chunks(per_line).map(|line| line.join(" ")).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_text_is_whole_after_one_scan() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let text = "the xat sat\non the xmat";
    let (found, whole, given) = fake.scan(&mut cache, text);
    assert!(whole);
    assert_eq!(found, expected(text));
    assert_eq!((fake.calls, given), (1, 5), "each different word once, in one call");
    let (again, whole, given) = fake.scan(&mut cache, text);
    assert!(whole);
    assert_eq!((again, given), (found, 0), "a whole text asks the checker nothing");
    assert_eq!(fake.calls, 1);
}

#[test]
fn a_full_cache_starts_over_and_the_open_lines_other_words_are_checked_again() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    // Fill the cache to its brim with another text's words.
    let filler = prose(MAX_WORDS, MAX_WORDS);
    fake.scan(&mut cache, &filler);
    // A draft whose line holds a word the cache has (`x0`) and one it hasn't.
    let text = "x0 fresh xnew";
    let (found, whole, given) = fake.scan(&mut cache, text);
    assert!(whole, "the cache started over, and every word of the line was checked");
    assert_eq!(found, expected(text), "the word the cache had lost keeps its mark");
    assert_eq!(given, 3, "all three: the kept one went with the cache");
}

#[test]
fn a_checker_that_answers_later_is_asked_again_for_what_a_full_cache_lost() {
    let mut cache = Words::default();
    let mut fake = Fake::default();
    fake.scan(&mut cache, &prose(MAX_WORDS, MAX_WORDS));
    let text = "x0 fresh xnew";
    let mut asked: Vec<String> = Vec::new();
    let (_, whole) = cache.scan(text, Checker::Later(&mut |words| asked.extend(words)));
    assert!(!whole);
    assert_eq!(asked, ["x0", "fresh", "xnew"], "every word of the open line, not only the new ones");
    let mut again: Vec<String> = Vec::new();
    cache.scan(text, Checker::Later(&mut |words| again.extend(words)));
    assert_eq!(again, Vec::<String>::new(), "words on their way are not asked for twice");
    for (word, parts) in fake.check(&["x0", "fresh", "xnew"]) { cache.learned(word, Some(parts)); }
    let (found, whole) = cache.scan(text, Checker::Later(&mut |_| panic!("nothing left to ask")));
    assert!(whole);
    assert_eq!(found, expected(text));
}

#[test]
fn a_word_too_long_to_be_one_is_never_checked_or_kept() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let long = "x".repeat(MAX_WORD_BYTES + 1);
    let edge = "x".repeat(MAX_WORD_BYTES);
    let text = format!("{long} xat {edge}\n{}", "x".repeat(4 << 20));
    let (found, whole, given) = fake.scan(&mut cache, &text);
    assert!(whole, "a line of nothing but an over-long word is whole");
    assert_eq!(given, 2, "the two words short enough");
    assert_eq!(found, expected(&text));
    assert_eq!(found.len(), 2);
    assert!(cache.known.keys().all(|word| word.len() <= MAX_WORD_BYTES), "no over-long key");
}

#[test]
fn the_cache_holds_no_more_than_its_cap() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    for round in 0..40 {
        let text: String = (0..1000).map(|n| format!("r{round}w{n}")).collect::<Vec<_>>().join(" ");
        fake.scan(&mut cache, &text);
        assert!(cache.known.len() <= MAX_WORDS, "round {round}: {} words", cache.known.len());
    }
}
