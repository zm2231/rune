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
