//! What CoreText resolves on this Mac: the system font's families by file, and the fallbacks
//! for scripts the app's fonts lack. A Mac without a font skips what is said about it.

use super::*;

/// A first lookup: nothing kept from an earlier one.
fn first_lookup(chars: &[char], locale: Option<&str>) -> Vec<PathBuf> {
    fallback_family_files(chars, locale, &mut Chinese::default())
}

/// Characters the macOS system UI font (`.AppleSystemUIFont`) already covers must skip
/// `public_family` so `SFNS.ttf`'s descriptors are never read and discarded.
#[test]
fn system_font_chars_skip_public_family_scan() {
    let cyrillic: Vec<char> = "абвгдежзийклмноп".chars().collect();
    let mut scans = 0usize;
    let files = fallback_family_files_with(&cyrillic, None, &mut Chinese::default(), |font| {
        scans += 1;
        public_family(font)
    });
    assert!(files.is_empty(), "system-font chars should add no fallback files: {files:?}");
    assert_eq!(scans, 0, "system-font chars must not call public_family");
}

/// Multiple characters from the same hidden fallback family (e.g. Japanese
/// `.Hiragino Kaku Gothic Interface`) resolve its public family at most once per call.
#[test]
fn hidden_fallback_family_deduplicates_public_family_scan() {
    let japanese: Vec<char> = "あいうえおかきくけこ".chars().collect();
    let mut scans = 0usize;
    let files = fallback_family_files_with(&japanese, Some("ja"), &mut Chinese::default(), |font| {
        scans += 1;
        public_family(font)
    });
    assert!(!files.is_empty(), "Japanese fallback should find system font files");
    assert!(scans <= 1, "expected at most 1 public_family scan for one hidden family, got {scans}");
    assert_eq!(files, first_lookup(&['あ'], Some("ja")));
}

/// `PublicFamilyCache` caches `Some` and `None` lookups while enforcing both its entry and
/// byte caps and ignoring duplicate or oversized entries.
#[test]
fn public_family_cache_bounds_entries_and_bytes() {
    let mut cache = PublicFamilyCache::new();
    cache.insert("/System/Library/Fonts/a.ttc".into(), Some("Hiragino Sans".into()));
    cache.insert("/System/Library/Fonts/b.ttf".into(), None);
    assert_eq!(cache.get("/System/Library/Fonts/a.ttc"), Some(Some("Hiragino Sans".into())));
    assert_eq!(cache.get("/System/Library/Fonts/b.ttf"), Some(None));
    assert_eq!(cache.get("/System/Library/Fonts/missing.ttf"), None);

    // Duplicate keys do not grow entries or bytes.
    let bytes_before = cache.bytes;
    let len_before = cache.entries.len();
    cache.insert("/System/Library/Fonts/a.ttc".into(), Some("Hiragino Sans".into()));
    assert_eq!(cache.entries.len(), len_before);
    assert_eq!(cache.bytes, bytes_before);

    // Oversized entries are skipped without evicting existing ones.
    let huge = "x".repeat(MAX_PUBLIC_FAMILY_BYTES + 1);
    cache.insert(huge.clone(), Some("Too Big".into()));
    assert_eq!(cache.get(&huge), None);
    assert_eq!(cache.entries.len(), len_before);

    // Entry cap evicts the oldest entries first.
    for i in 0..(MAX_PUBLIC_FAMILY_ENTRIES + 10) {
        cache.insert(format!("/f/{i}.ttf"), Some(format!("Family {i}")));
        assert!(cache.entries.len() <= MAX_PUBLIC_FAMILY_ENTRIES);
        assert!(cache.bytes <= MAX_PUBLIC_FAMILY_BYTES);
    }
    assert_eq!(cache.get("/System/Library/Fonts/a.ttc"), None);

    // Byte cap evicts older entries even when entry count is below the entry cap.
    let mut byte_cache = PublicFamilyCache::new();
    let big_val = "v".repeat(2048);
    for i in 0..20 {
        byte_cache.insert(format!("/big/{i}.ttc"), Some(big_val.clone()));
        assert!(byte_cache.entries.len() <= MAX_PUBLIC_FAMILY_ENTRIES);
        assert!(byte_cache.bytes <= MAX_PUBLIC_FAMILY_BYTES);
    }
    assert!(byte_cache.entries.len() < 20);
    assert_eq!(byte_cache.get("/big/0.ttc"), None);
    assert_eq!(byte_cache.get("/big/19.ttc"), Some(Some(big_val)));
}

/// Unassigned code points and noncharacters have no covering font: CoreText answers with
/// `.LastResort`, which `fallback_family_files` must skip without adding `LastResort.otf` or
/// scanning its descriptors.
#[test]
fn uncovered_chars_skip_last_resort() {
    let uncovered = ['\u{0378}', '\u{FDD0}'];
    let mut scans = 0usize;
    let files = fallback_family_files_with(&uncovered, None, &mut Chinese::default(), |font| {
        scans += 1;
        public_family(font)
    });
    assert!(files.is_empty(), "uncovered chars must not add LastResort or other fallback files: {files:?}");
    assert_eq!(scans, 0, "LastResort must be skipped before calling public_family");
}

/// Real fallback symbols (such as `⏵`) still resolve their font files even when mixed with an
/// uncovered code point, without pulling LastResort along.
#[test]
fn symbol_resolves_without_last_resort_beside_uncovered_char() {
    let symbol = first_lookup(&['⏵'], None);
    assert!(!symbol.is_empty(), "symbol fallback should find a system font for ⏵");
    let mixed = first_lookup(&['\u{0378}', '\u{FDD0}', '⏵'], None);
    assert_eq!(mixed, symbol, "uncovered chars must not add files beside a real fallback symbol");
}

fn names(files: &[std::path::PathBuf]) -> Vec<String> {
    files.iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// Those of `files` (names in `/System/Library/Fonts`) this Mac has, as paths, or None, to
/// skip, when it lacks the first.
fn installed(files: &[&str]) -> Option<Vec<String>> {
    let paths: Vec<String> = files.iter().map(|f| format!("/System/Library/Fonts/{f}")).filter(|p| std::path::Path::new(p).exists()).collect();
    if !paths.first().is_some_and(|p| p.ends_with(files[0])) {
        eprintln!("skipping: no {}", files[0]);
        return None;
    }
    Some(paths)
}

#[test]
fn the_system_font_comes_from_the_ui_fonts_files_under_each_of_its_names() {
    let Some(ui) = installed(&["SFNS.ttf", "SFNSItalic.ttf"]) else { return };
    for name in ["System Font", ".SF NS", ".SF UI"] {
        assert_eq!(names(&family_files(&[name])), ui, "{name}");
    }
}

#[test]
fn sf_mono_and_sf_rounded_are_the_files_beside_the_ui_fonts() {
    if let Some(mono) = installed(&["SFNSMono.ttf", "SFNSMonoItalic.ttf"]) {
        assert_eq!(names(&family_files(&[".SF NS Mono"])), mono);
        // Both platforms' names in one list load the files once.
        assert_eq!(names(&family_files(&[".SF NS Mono", ".SF UI Mono"])), mono);
    }
    if let Some(rounded) = installed(&["SFNSRounded.ttf"]) {
        assert_eq!(names(&family_files(&[".SF NS Rounded"])), rounded);
        assert_eq!(names(&family_files(&[".SF UI Rounded"])), rounded);
    }
}

/// Any family named after the UI font's is looked for beside it, and has the files that are
/// there: none on a Mac without them.
#[test]
fn a_family_named_after_the_ui_fonts_has_the_files_beside_it_that_exist() {
    for family in ["Symbols", "Compact", "Compact Rounded", "No Such Family"] {
        let stem = format!("/System/Library/Fonts/SFNS{}", family.replace(' ', ""));
        let beside: Vec<String> = [format!("{stem}.ttf"), format!("{stem}Italic.ttf")].into_iter().filter(|p| std::path::Path::new(p).exists()).collect();
        assert_eq!(names(&family_files(&[&format!(".SF NS {family}")])), beside, "{family}");
    }
}

#[test]
fn a_public_family_is_still_matched_by_name() {
    for (family, file) in [("Menlo", "Menlo.ttc"), ("Apple Symbols", "Apple Symbols.ttf")] {
        let Some(files) = installed(&[file]) else { continue };
        assert_eq!(names(&family_files(&[family])), files, "{family}");
    }
}

#[test]
fn a_hidden_name_that_is_not_the_system_fonts_has_no_files() {
    for name in [".SF Arabic", ".SF Compact", ".PingFang UI SC", ".Hiragino Kaku Gothic Interface", ".LastResort"] {
        assert!(family_files(&[name]).is_empty(), "{name}");
    }
}

/// Japanese falls back to a hidden face of Hiragino Sans; the family's public name brings its
/// other weights, each file once.
#[test]
fn kana_loads_the_weights_of_hiragino_sans() {
    let files = names(&first_lookup(&['\u{3042}'], Some("ja")));
    if !files.iter().any(|f| f.ends_with(" W3.ttc")) {
        eprintln!("skipping: Japanese doesn't fall back to Hiragino Sans here ({files:?})");
        return;
    }
    assert!(files.len() > 1, "the family, not one weight: {files:?}");
    for (i, file) in files.iter().enumerate() {
        let weight = file.strip_suffix(".ttc").and_then(|f| f.rsplit_once(" W")).map(|(_, w)| w);
        assert!(weight.is_some_and(|w| w.len() == 1 && w.chars().all(|c| c.is_ascii_digit())), "{file}");
        assert!(!files[..i].contains(file), "{file} twice");
    }
}

/// These fallbacks are hidden families whose file has no public face: the file alone.
#[test]
fn a_hidden_fallback_loads_the_file_coretext_names() {
    for (ch, locale, file) in [('\u{0639}', "ar", "SFArabic.ttf"), ('\u{05D0}', "he", "SFHebrew.ttf"), ('\u{0915}', "hi", "SFIndia.ttc"), ('\u{0E01}', "th", "ThonburiUI.ttc"), ('\u{10D0}', "ka", "SFGeorgian.ttf")] {
        let Some(files) = installed(&[file]) else { continue };
        assert_eq!(names(&first_lookup(&[ch], Some(locale))), files, "{ch}");
    }
}

/// The system font has ⌘; no font has the unassigned U+0378, for which CoreText offers its
/// last-resort font of placeholder boxes.
#[test]
fn a_character_the_system_font_has_or_no_font_has_adds_nothing() {
    assert!(first_lookup(&['\u{2318}'], None).is_empty());
    assert!(first_lookup(&['\u{0378}'], None).is_empty());
    assert!(first_lookup(&['\u{0378}', '\u{2318}'], Some("en")).is_empty());
    assert!(first_lookup(&[], None).is_empty());
}

/// CoreText offers PingFang for Chinese, which Rune can't draw: Simplified takes Hiragino Sans
/// GB and Traditional Heiti TC, once however many characters ask.
#[test]
fn chinese_loads_a_family_that_is_not_pingfang() {
    if let Some(simplified) = installed(&["Hiragino Sans GB.ttc"]) {
        assert_eq!(names(&first_lookup(&['\u{4E2D}'], Some("zh-Hans"))), simplified);
        assert_eq!(names(&first_lookup(&['\u{4E2D}', '\u{4EEC}', '\u{9F8D}'], Some("zh-Hans"))), simplified);
    }
    if let Some(traditional) = installed(&["STHeiti Light.ttc", "STHeiti Medium.ttc"]) {
        assert_eq!(names(&first_lookup(&['\u{4E2D}'], Some("zh-Hant"))), traditional);
    }
}

/// What the first Chinese character found answers the later ones, through one kept state.
#[test]
fn later_chinese_characters_take_the_family_already_found() {
    let Some(simplified) = installed(&["Hiragino Sans GB.ttc"]) else { return };
    let mut chinese = Chinese::default();
    for ch in ['\u{4E2D}', '\u{4EEC}', '\u{9F8D}'] {
        assert_eq!(names(&fallback_family_files(&[ch], Some("zh-Hans"), &mut chinese)), simplified, "{ch}");
    }
}
