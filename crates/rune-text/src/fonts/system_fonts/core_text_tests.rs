//! What CoreText resolves on this Mac: the system font's families by file, and the fallbacks
//! for scripts the app's fonts lack. A Mac without a font skips what is said about it.

use super::core_text::{Chinese, family_files};

/// A first lookup: nothing kept from an earlier one.
fn fallback_family_files(chars: &[char], locale: Option<&str>) -> Vec<std::path::PathBuf> {
    super::core_text::fallback_family_files(chars, locale, &mut Chinese::default())
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
    let files = names(&fallback_family_files(&['\u{3042}'], Some("ja")));
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
        assert_eq!(names(&fallback_family_files(&[ch], Some(locale))), files, "{ch}");
    }
}

/// The system font has ⌘; no font has the unassigned U+0378, for which CoreText offers its
/// last-resort font of placeholder boxes.
#[test]
fn a_character_the_system_font_has_or_no_font_has_adds_nothing() {
    assert!(fallback_family_files(&['\u{2318}'], None).is_empty());
    assert!(fallback_family_files(&['\u{0378}'], None).is_empty());
    assert!(fallback_family_files(&['\u{0378}', '\u{2318}'], Some("en")).is_empty());
    assert!(fallback_family_files(&[], None).is_empty());
}

/// CoreText offers PingFang for Chinese, which Rune can't draw: Simplified takes Hiragino Sans
/// GB and Traditional Heiti TC, once however many characters ask.
#[test]
fn chinese_loads_a_family_that_is_not_pingfang() {
    if let Some(simplified) = installed(&["Hiragino Sans GB.ttc"]) {
        assert_eq!(names(&fallback_family_files(&['\u{4E2D}'], Some("zh-Hans"))), simplified);
        assert_eq!(names(&fallback_family_files(&['\u{4E2D}', '\u{4EEC}', '\u{9F8D}'], Some("zh-Hans"))), simplified);
    }
    if let Some(traditional) = installed(&["STHeiti Light.ttc", "STHeiti Medium.ttc"]) {
        assert_eq!(names(&fallback_family_files(&['\u{4E2D}'], Some("zh-Hant"))), traditional);
    }
}

/// What the first Chinese character found answers the later ones, through one kept state.
#[test]
fn later_chinese_characters_take_the_family_already_found() {
    let Some(simplified) = installed(&["Hiragino Sans GB.ttc"]) else { return };
    let mut chinese = Chinese::default();
    for ch in ['\u{4E2D}', '\u{4EEC}', '\u{9F8D}'] {
        assert_eq!(names(&super::core_text::fallback_family_files(&[ch], Some("zh-Hans"), &mut chinese)), simplified, "{ch}");
    }
}
