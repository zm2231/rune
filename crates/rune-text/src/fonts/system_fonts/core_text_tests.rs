//! What CoreText resolves on this Mac: the system font's families by file, and the fallbacks
//! for scripts the app's fonts lack. A Mac without a font skips what is said about it.

use super::core_text::*;

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
