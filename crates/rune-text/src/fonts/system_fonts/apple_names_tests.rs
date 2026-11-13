use std::path::PathBuf;

use super::apple_names::*;

#[test]
fn the_system_fonts_names_stand_for_its_faces_on_both_platforms() {
    for name in ["System Font", ".SF NS", ".SF UI"] {
        assert_eq!(system_family(name), Some(SystemFace::Ui), "{name}");
    }
    for name in [".SF NS Mono", ".SF UI Mono"] {
        assert_eq!(system_family(name), Some(SystemFace::UiMono), "{name}");
    }
    for name in ["Menlo", "SF Pro", ".SF Arabic", ".SF NSMono", ".SF NS ", ".SF", "system font", ""] {
        assert_eq!(system_family(name), None, "{name}");
    }
}

#[test]
fn any_family_named_after_the_ui_fonts_is_a_file_beside_it() {
    for (name, rest, suffix) in [
        (".SF NS Rounded", "Rounded", "Rounded"),
        (".SF UI Rounded", "Rounded", "Rounded"),
        (".SF NS Symbols", "Symbols", "Symbols"),
        (".SF NS Compact Rounded", "Compact Rounded", "CompactRounded"),
    ] {
        assert_eq!(system_family(name), Some(SystemFace::UiSibling(rest)), "{name}");
        assert_eq!(SystemFace::UiSibling(rest).suffix().as_deref(), Some(suffix), "{name}");
    }
    assert_eq!(SystemFace::Ui.suffix(), None);
    assert_eq!(SystemFace::UiMono.suffix().as_deref(), Some("Mono"));
}

#[test]
fn a_sibling_face_is_named_after_the_ui_fonts_file() {
    assert_eq!(sibling_file("SFNS", "Mono"), "SFNSMono.ttf");
    assert_eq!(sibling_file("SFNS", "MonoItalic"), "SFNSMonoItalic.ttf");
    assert_eq!(sibling_file("SFUI", "Mono"), "SFUIMono.ttf");
    assert_eq!(sibling_file("SFUI", "MonoItalic"), "SFUIMonoItalic.ttf");
    assert_eq!(sibling_file("SFNS", "Rounded"), "SFNSRounded.ttf");
    assert_eq!(sibling_file("SFUI", "Rounded"), "SFUIRounded.ttf");
}

#[test]
fn a_hidden_family_adds_the_fonts_own_file_and_is_never_matched_by_name() {
    let own = PathBuf::from("/System/Library/Fonts/Core/SFArabic.ttf");
    for family in [".SF Arabic", ".PingFang UI SC", ".AppleSystemUIFont"] {
        let files = fallback_files(Some(own.clone()), family, |name| panic!("{name} matched by name"));
        assert_eq!(files, [own.clone()], "{family}");
        assert!(fallback_files(None, family, |name| panic!("{name} matched by name")).is_empty(), "{family}");
    }
}

#[test]
fn a_public_family_adds_its_own_file_first_then_the_rest_of_the_family() {
    let (w4, w3) = (PathBuf::from("W4.ttc"), PathBuf::from("W3.ttc"));
    let family = |name: &str| {
        assert_eq!(name, "Hiragino Sans");
        vec![w3.clone(), w4.clone()]
    };
    assert_eq!(fallback_files(Some(w4.clone()), "Hiragino Sans", family), [w4.clone(), w3.clone()]);
    assert_eq!(fallback_files(None, "Hiragino Sans", family), [w3.clone(), w4.clone()]);
}

#[test]
fn the_last_resort_font_is_known_by_its_hidden_and_public_names() {
    assert!(is_last_resort(".LastResort"));
    assert!(is_last_resort("LastResort"));
    for family in [".SF Arabic", "Last Resort Sans", "", "."] {
        assert!(!is_last_resort(family), "{family}");
    }
}
