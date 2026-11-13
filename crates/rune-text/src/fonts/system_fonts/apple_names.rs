//! The names Apple's system fonts go by, and the files they're in. San Francisco's families are
//! hidden (`.`-prefixed) and their names differ per platform (`.SF NS` on macOS, `.SF UI` on
//! iOS), so a name here says which face of the system UI font is meant, and the face's file is
//! found from the UI font's own, never by matching the name.

use std::path::PathBuf;

/// A face of the system UI font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SystemFace<'a> {
    /// San Francisco, upright and italic.
    Ui,
    /// SF Mono, upright and italic.
    UiMono,
    /// Another family in a file beside the UI font's, by what follows the UI family's name:
    /// `Rounded` for `.SF NS Rounded`, `Symbols` for `.SF NS Symbols`.
    UiSibling(&'a str),
}

impl SystemFace<'_> {
    /// What the face's file name adds to the UI font's (`Mono` in `SFNSMono.ttf`), or None for
    /// the UI font itself.
    pub(super) fn suffix(self) -> Option<String> {
        match self {
            SystemFace::Ui => None,
            SystemFace::UiMono => Some("Mono".to_owned()),
            SystemFace::UiSibling(rest) => Some(rest.replace(' ', "")),
        }
    }
}

/// The face of the system UI font that family `name` stands for, if it is one of its names:
/// `System Font`, or the UI family's hidden name on either platform, alone or followed by
/// another family's (`.SF NS Mono`, `.SF UI Rounded`).
pub(super) fn system_family(name: &str) -> Option<SystemFace<'_>> {
    if name == "System Font" {
        return Some(SystemFace::Ui);
    }
    let rest = name.strip_prefix(".SF NS").or_else(|| name.strip_prefix(".SF UI"))?;
    match rest.strip_prefix(' ') {
        None => rest.is_empty().then_some(SystemFace::Ui),
        Some("Mono") => Some(SystemFace::UiMono),
        Some(family) => (!family.trim().is_empty()).then_some(SystemFace::UiSibling(family)),
    }
}

/// The file name of the UI font's sibling face: `ui_stem` is the UI font's file name without
/// its extension (`SFNS` on macOS, `SFUI` on iOS), `suffix` the face (`Mono`, `MonoItalic`).
pub(super) fn sibling_file(ui_stem: &str, suffix: &str) -> String {
    format!("{ui_stem}{suffix}.ttf")
}

/// Whether family `name` is hidden from the system's font lists (`.SF Arabic`,
/// `.Hiragino Kaku Gothic Interface`). CoreText hands such a font out itself, as a fallback;
/// a client that asks for one by name can be given another font than the one it meant.
pub(super) fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Whether `family` is CoreText's last-resort font, what it answers with when no font has a
/// character: placeholder boxes, not glyphs, so there is nothing in it to load.
pub(super) fn is_last_resort(family: &str) -> bool {
    family.trim_start_matches('.') == "LastResort"
}

/// The files a fallback font of `family` adds: the file the font is in (`own`), then the rest
/// of its family's as `by_name` matches them. A hidden family is never matched by name, so it
/// adds its own file only.
pub(super) fn fallback_files(own: Option<PathBuf>, family: &str, by_name: impl FnOnce(&str) -> Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = own.into_iter().collect();
    if !is_hidden(family) {
        for path in by_name(family) {
            if !files.contains(&path) { files.push(path); }
        }
    }
    files
}
