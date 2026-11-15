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
    fn scan(&mut self, cache: &mut Words, text: &str, budget: usize) -> (Vec<(usize, usize)>, bool, usize) {
        let before = self.words;
        let (found, whole) = cache.scan(text, line_key(text), Checker::Now { check: &mut |words| self.check(words), budget });
        (found, whole, self.words - before)
    }

    /// Scans until the text is whole, as frames do while the paint asks for another: the
    /// marks, and how many scans it took.
    fn settle(&mut self, cache: &mut Words, text: &str, budget: usize) -> (Vec<(usize, usize)>, usize) {
        for scans in 1..=100_000 {
            let (found, whole, _) = self.scan(cache, text, budget);
            if whole { return (found, scans); }
        }
        panic!("the text never became whole");
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
fn a_short_text_is_whole_after_one_scan() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let text = "the xat sat\non the xmat";
    let (found, whole, given) = fake.scan(&mut cache, text, MAX_FRESH);
    assert!(whole);
    assert_eq!(found, expected(text));
    assert_eq!((fake.calls, given), (1, 5), "each different word once, in one call");
    let (again, whole, given) = fake.scan(&mut cache, text, MAX_FRESH);
    assert!(whole);
    assert_eq!((again, given), (found, 0), "a whole text asks the checker nothing");
    assert_eq!(fake.calls, 1);
}

#[test]
fn a_scan_gives_the_checker_no_more_than_its_budget() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let text = prose(1000, 10);
    let mut scans = 0;
    loop {
        let (found, whole, given) = fake.scan(&mut cache, &text, 64);
        scans += 1;
        assert!(given <= 64, "scan {scans} gave {given} words");
        assert!(found.iter().all(|range| expected(&text).contains(range)), "what is marked so far is right");
        if whole {
            assert_eq!(found, expected(&text), "the marks converge on the whole text's");
            break;
        }
        assert_eq!(given, 64, "a scan that leaves words uses its whole budget");
    }
    assert_eq!(scans, 16, "1000 words at 64 a scan");
    assert_eq!((fake.calls, fake.words), (16, 1000), "one call a scan, each word once");
    let (_, whole, given) = fake.scan(&mut cache, &text, 64);
    assert!(whole && given == 0, "once whole, a scan asks for nothing and for no frame");
}

#[test]
fn the_text_fills_in_from_its_start() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let text = prose(300, 10);
    let (found, whole, _) = fake.scan(&mut cache, &text, 64);
    assert!(!whole);
    let want: Vec<_> = expected(&text).into_iter().take(found.len()).collect();
    assert_eq!(found, want, "the first words' marks, in order");
    assert!(!found.is_empty());
}

#[test]
fn a_text_with_more_words_than_the_cache_keeps_still_becomes_whole() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    // One line, so no line's answer is kept until every word has one.
    let text = prose(MAX_WORDS + 500, MAX_WORDS + 500);
    let (found, scans) = fake.settle(&mut cache, &text, 64);
    assert_eq!(found, expected(&text));
    assert_eq!(scans, (MAX_WORDS + 500).div_ceil(64));
    assert_eq!(fake.words, MAX_WORDS + 500, "no word was checked twice: the cache held while the text was under way");
}

#[test]
fn two_long_texts_under_way_together_both_become_whole() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let a = prose(MAX_WORDS - 100, MAX_WORDS);
    let b: String = a.replace('w', "v").replace('x', "xx");
    let mut done = (false, false);
    for _ in 0..1000 {
        done = (fake.scan(&mut cache, &a, 64).1, fake.scan(&mut cache, &b, 64).1);
        if done == (true, true) { break; }
    }
    assert_eq!(done, (true, true));
    assert_eq!(fake.scan(&mut cache, &a, 64).0, expected(&a));
    assert_eq!(fake.scan(&mut cache, &b, 64).0, expected(&b));
}

#[test]
fn a_full_cache_starts_over_and_the_open_lines_other_words_are_checked_again() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    // Fill the cache to its brim with another text's words.
    let filler = prose(MAX_WORDS, MAX_WORDS);
    fake.settle(&mut cache, &filler, usize::MAX);
    // A draft whose line holds a word the cache has (`x0`) and one it hasn't.
    let text = "x0 fresh xnew";
    let (found, whole, given) = fake.scan(&mut cache, text, MAX_FRESH);
    assert!(whole, "the cache started over, and every word of the line was checked");
    assert_eq!(found, expected(text), "the word the cache had lost keeps its mark");
    assert_eq!(given, 3, "all three: the kept one went with the cache");
}

#[test]
fn a_checker_that_answers_later_is_asked_again_for_what_a_full_cache_lost() {
    let mut cache = Words::default();
    let mut fake = Fake::default();
    fake.settle(&mut cache, &prose(MAX_WORDS, MAX_WORDS), usize::MAX);
    let text = "x0 fresh xnew";
    let mut asked: Vec<String> = Vec::new();
    let (_, whole) = cache.scan(text, line_key(text), Checker::Later(&mut |words| asked.extend(words)));
    assert!(!whole);
    assert_eq!(asked, ["x0", "fresh", "xnew"], "every word of the open line, not only the new ones");
    let mut again: Vec<String> = Vec::new();
    cache.scan(text, line_key(text), Checker::Later(&mut |words| again.extend(words)));
    assert_eq!(again, Vec::<String>::new(), "words on their way are not asked for twice");
    for (word, parts) in fake.check(&["x0", "fresh", "xnew"]) { cache.learned(word, Some(parts)); }
    let (found, whole) = cache.scan(text, line_key(text), Checker::Later(&mut |_| panic!("nothing left to ask")));
    assert!(whole);
    assert_eq!(found, expected(text));
}

#[test]
fn a_checker_that_answers_later_is_given_every_word_at_once() {
    let mut cache = Words::default();
    let text = prose(1000, 10);
    let mut asked = 0;
    cache.scan(&text, line_key(&text), Checker::Later(&mut |words| asked += words.len()));
    assert_eq!(asked, 1000, "the budget is for a checker that answers on this thread");
}

#[test]
fn a_word_too_long_to_be_one_is_never_checked_or_kept() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    let long = "x".repeat(MAX_WORD_BYTES + 1);
    let edge = "x".repeat(MAX_WORD_BYTES);
    let text = format!("{long} xat {edge}\n{}", "x".repeat(4 << 20));
    let (found, whole, given) = fake.scan(&mut cache, &text, MAX_FRESH);
    assert!(whole, "a line of nothing but an over-long word is whole");
    assert_eq!(given, 2, "the two words short enough");
    assert_eq!(found, expected(&text));
    assert_eq!(found.len(), 2);
    assert!(cache.known.keys().all(|word| word.len() <= MAX_WORD_BYTES), "no over-long key");
}

#[test]
fn the_cache_holds_no_more_than_its_cap_once_texts_are_whole() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    for round in 0..40 {
        let text: String = (0..1000).map(|n| format!("r{round}w{n}")).collect::<Vec<_>>().join(" ");
        fake.settle(&mut cache, &text, 64);
        assert!(cache.known.len() <= MAX_WORDS + 1000, "round {round}: {} words", cache.known.len());
        assert!(cache.partial.is_empty(), "a whole text is not held for");
    }
}

#[test]
fn a_cache_a_long_text_took_past_its_cap_starts_over_at_the_next_text() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    // One line, so every word stays in the word cache until the text is whole.
    let long = prose(MAX_WORDS + 500, MAX_WORDS + 500);
    fake.settle(&mut cache, &long, 64);
    assert_eq!(cache.known.len(), MAX_WORDS + 500, "held past the cap while the text was under way");
    assert!(cache.partial.is_empty(), "whole, so no longer held for");
    let (_, whole, given) = fake.scan(&mut cache, &long, 64);
    assert!(whole && given == 0);
    assert_eq!(cache.known.len(), MAX_WORDS + 500, "a scan with no new word leaves the cache as it is");
    let text = "another xtext";
    let (found, whole, given) = fake.scan(&mut cache, text, MAX_FRESH);
    assert!(whole);
    assert_eq!((found, given), (expected(text), 2));
    assert!(cache.known.len() <= MAX_WORDS, "{} words", cache.known.len());
    assert_eq!(cache.known.len(), 2, "the cache started over with the new text's words");
}

#[test]
fn texts_left_unfinished_stop_holding_the_cache() {
    let (mut fake, mut cache) = (Fake::default(), Words::default());
    // Fields that went away part-way through: each scanned once and never again.
    for n in 0..100 {
        let text: String = (0..200).map(|w| format!("t{n}w{w}")).collect::<Vec<_>>().join(" ");
        fake.scan(&mut cache, &text, 64);
        assert!(cache.partial.len() <= MAX_PARTIAL);
    }
    assert!(cache.known.len() <= MAX_WORDS + 200, "{} words", cache.known.len());
}
