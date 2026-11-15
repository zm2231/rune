use super::*;

/// The byte range of `word` in `text`, found through its UTF-16 range as a checker reports it.
fn through_utf16(text: &str, word: &str) -> Option<(usize, usize)> {
    let at = text.find(word).expect("the word is in the text");
    let location = text[..at].encode_utf16().count();
    byte_range(&offsets(text), location, word.encode_utf16().count())
}

#[test]
fn ascii_units_are_bytes() {
    assert_eq!(offsets("abc"), [0, 1, 2, 3]);
    assert_eq!(through_utf16("teh cat", "teh"), Some((0, 3)));
    assert_eq!(through_utf16("the caat", "caat"), Some((4, 8)));
}

#[test]
fn an_emoji_is_two_units_and_four_bytes() {
    let text = "🙂 wrold 🙂";
    assert_eq!(offsets("🙂a"), [0, 0, 4, 5]);
    assert_eq!(through_utf16(text, "wrold"), Some((5, 10)));
    assert_eq!(&text[5..10], "wrold");
}

#[test]
fn a_joined_emoji_keeps_what_follows_in_place() {
    // A family: four people joined by zero-width joiners, eleven UTF-16 units, 25 bytes.
    let text = "👨‍👩‍👧‍👦 helo";
    let (a, b) = through_utf16(text, "helo").unwrap();
    assert_eq!(&text[a..b], "helo");
    assert_eq!(a, "👨‍👩‍👧‍👦 ".len());
}

#[test]
fn a_combining_mark_is_its_own_unit() {
    // "e" then a combining acute: two units, three bytes.
    let text = "cafe\u{301} wrold";
    let (a, b) = through_utf16(text, "wrold").unwrap();
    assert_eq!(&text[a..b], "wrold");
    let (a, b) = through_utf16(text, "cafe\u{301}").unwrap();
    assert_eq!(&text[a..b], "cafe\u{301}");
}

#[test]
fn cjk_is_one_unit_and_three_bytes() {
    let text = "日本語 wrold 中文";
    assert_eq!(offsets("日本"), [0, 3, 6]);
    let (a, b) = through_utf16(text, "wrold").unwrap();
    assert_eq!(&text[a..b], "wrold");
    assert_eq!(through_utf16(text, "中文"), Some((16, 22)));
}

#[test]
fn a_range_at_the_texts_end_reaches_its_length() {
    assert_eq!(byte_range(&offsets("a 🙂"), 2, 2), Some((2, 6)));
}

#[test]
fn a_range_past_the_text_or_empty_is_none() {
    let offsets = offsets("abc");
    assert_eq!(byte_range(&offsets, 1, 0), None, "empty");
    assert_eq!(byte_range(&offsets, 2, 5), None, "ends past the text");
    assert_eq!(byte_range(&offsets, 9, 1), None, "starts past the text");
    assert_eq!(byte_range(&offsets, isize::MAX as usize, 0), None, "a checker's not-found");
    assert_eq!(byte_range(&offsets, usize::MAX, 2), None, "overflows");
}

#[test]
fn half_an_emoji_is_no_range() {
    // Both units of the pair map to the emoji's first byte.
    assert_eq!(byte_range(&offsets("🙂"), 0, 1), None);
    assert_eq!(byte_range(&offsets("🙂"), 0, 2), Some((0, 4)));
}

#[test]
fn a_word_is_misspelled_when_every_dictionary_says_so() {
    let english = vec![(0, 4), (10, 15)];
    let spanish = vec![(0, 4), (5, 9)];
    assert_eq!(in_every([english.clone(), spanish]), [(0, 4)]);
    assert_eq!(in_every([english.clone()]), english);
    assert_eq!(in_every([english, Vec::new()]), []);
    assert_eq!(in_every(Vec::<Vec<(usize, usize)>>::new()), []);
}

#[test]
fn lists_in_the_texts_order_are_each_walked_once() {
    // Every word, every second word and every third: the sixths are in all three.
    let words = |step: usize| (0..600).step_by(step).map(|n| (n * 10, n * 10 + 4)).collect::<Vec<_>>();
    assert_eq!(in_every([words(1), words(2), words(3)]), words(6));
    assert_eq!(in_every([words(3), words(1), words(2)]), words(6), "in whatever order the dictionaries come");
    // Ranges that start together but end apart are different words.
    assert_eq!(in_every([vec![(0, 4), (5, 9)], vec![(0, 3), (5, 9), (20, 22)]]), [(5, 9)]);
}

#[test]
fn uikit_names_dictionaries_with_underscores() {
    assert_eq!(tag("en_US"), "en-US");
    assert_eq!(tag("pt_BR"), "pt-BR");
    assert_eq!(tag("es"), "es");
}

#[test]
fn chosen_tags_find_uikits_names_in_their_own_order() {
    let has = ["en_US", "en_GB", "es", "pt_BR"].map(String::from);
    let tags = ["pt-BR", "xx-XX", "en-us", "es"].map(String::from);
    assert_eq!(identifiers(&tags, &has), ["pt_BR", "en_US", "es"]);
    assert_eq!(identifiers(&[], &has), Vec::<String>::new());
}
