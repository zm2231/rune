//! What Chinese text falls back to on Apple platforms, worked out once and kept. CoreText's
//! Chinese font is PingFang, which Rune can't draw, so a family is picked in its place. That
//! happens where a text is shaped, the first time it has a Chinese character; what is found
//! is kept here, so the next character asks CoreText for no family or cascade list again.

use std::path::PathBuf;

/// The locales whose cascade is kept. A ninth replaces the one kept longest.
pub(super) const MAX_LOCALES: usize = 8;
/// The fonts kept of one cascade list; CoreText's has about forty.
pub(super) const MAX_CASCADE: usize = 64;

/// The answers kept: `F` is a font of CoreText's cascade list.
pub(super) struct ChineseFallbacks<F> {
    /// By first candidate (one entry for Simplified, one for Traditional): the files of the
    /// candidate family chosen, or None when the system has no candidate Rune can draw.
    families: Vec<(&'static str, Option<Vec<PathBuf>>)>,
    /// By locale: the cascade list's fonts, at most `MAX_LOCALES` lists of `MAX_CASCADE`.
    cascades: Vec<(Option<String>, Vec<F>)>,
}

impl<F> Default for ChineseFallbacks<F> {
    fn default() -> Self {
        ChineseFallbacks { families: Vec::new(), cascades: Vec::new() }
    }
}

impl<F: Clone> ChineseFallbacks<F> {
    /// The files of the first of `candidates` the system can draw Chinese in: `files_of` gives
    /// a family's files, none when it isn't installed or isn't a font Rune draws. None when no
    /// candidate has any. Worked out the first time a list is asked about; the answer, None
    /// too, is kept.
    pub(super) fn family(&mut self, candidates: &'static [&'static str], mut files_of: impl FnMut(&str) -> Vec<PathBuf>) -> Option<&[PathBuf]> {
        let key = *candidates.first()?;
        if !self.families.iter().any(|(first, _)| *first == key) {
            let chosen = candidates.iter().map(|name| files_of(name)).find(|files| !files.is_empty());
            self.families.push((key, chosen));
        }
        self.families.iter().find(|(first, _)| *first == key).and_then(|(_, files)| files.as_deref())
    }

    /// The cascade list's fonts for `locale`: `build` lists them, the first time only.
    pub(super) fn cascade(&mut self, locale: Option<&str>, build: impl FnOnce() -> Vec<F>) -> Vec<F> {
        if let Some((_, fonts)) = self.cascades.iter().find(|(l, _)| l.as_deref() == locale) {
            return fonts.clone();
        }
        let mut fonts = build();
        fonts.truncate(MAX_CASCADE);
        if self.cascades.len() == MAX_LOCALES {
            self.cascades.remove(0);
        }
        self.cascades.push((locale.map(str::to_owned), fonts.clone()));
        fonts
    }
}
