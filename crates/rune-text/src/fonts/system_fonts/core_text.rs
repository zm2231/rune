//! Apple platforms: system font files found through CoreText, as DirectWrite finds them on Windows
//! (`direct_write`), so no font folder is walked and no font is parsed until a text needs it.
//! A text the app's fonts lack goes to CoreText's own fallback (`CTFontCreateForStringWithLanguage`,
//! what Safari and AppKit fall back with), which names the font that covers it; a family's
//! files come from CoreText's descriptors for the family name. The system UI font's own
//! families are found from its file (`system_face_paths`).

use std::path::PathBuf;

use objc2_core_foundation::{CFDictionary, CFRange, CFRetained, CFString, CFType, CFURL, CFURLPathStyle};
use objc2_core_text::{CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromURL, CTFontSymbolicTraits, CTFontUIFontType, kCTFontFamilyNameAttribute, kCTFontURLAttribute};

use super::apple_names::{SystemFace, sibling_file, system_family};

/// The file behind a CoreText attribute holder's `kCTFontURLAttribute`.
fn path_of(url: Option<CFRetained<CFType>>) -> Option<PathBuf> {
    let url = url?.downcast::<CFURL>().ok()?;
    Some(PathBuf::from(url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?.to_string()))
}

/// The files of the installed families `names`, in order, each file once.
pub(super) fn family_files(names: &[&str]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for name in names {
        for path in family_paths(name) {
            if !out.contains(&path) { out.push(path); }
        }
    }
    out
}

/// The files of `face` of the system UI font. The UI font and its italic say where their own
/// files are; SF Mono and the other families named after it report no file, and sit beside the
/// UI font's, named after it (`SFNS.ttf` and `SFNSMono.ttf` on macOS, `SFUI.ttf` and
/// `SFUIMono.ttf` on iOS). A family with no such file has none.
fn system_face_paths(face: SystemFace<'_>) -> Vec<PathBuf> {
    // SAFETY: the system UI font and its italic, read for their files only.
    let (file, italic) = unsafe {
        let Some(ui) = CTFont::new_ui_font_for_language(CTFontUIFontType::System, 0.0, None) else { return Vec::new() };
        let Some(file) = path_of(ui.attribute(kCTFontURLAttribute)) else { return Vec::new() };
        let italic = ui
            .copy_with_symbolic_traits(0.0, std::ptr::null(), CTFontSymbolicTraits::TraitItalic, CTFontSymbolicTraits::TraitItalic)
            .and_then(|f| path_of(f.attribute(kCTFontURLAttribute)));
        (file, italic)
    };
    let Some(suffix) = face.suffix() else {
        return std::iter::once(file.clone()).chain(italic.filter(|p| *p != file)).collect();
    };
    let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else { return Vec::new() };
    [suffix.clone(), format!("{suffix}Italic")].into_iter().map(|suffix| file.with_file_name(sibling_file(stem, &suffix))).filter(|p| p.exists()).collect()
}

/// Every face file of family `name`, as CoreText matches the family name. The system UI font's
/// families (`System Font`, `.SF NS`, `.SF UI Mono`...) are hidden from CoreText's lists and
/// come from the UI font's file instead.
fn family_paths(name: &str) -> Vec<PathBuf> {
    if let Some(face) = system_family(name) {
        return system_face_paths(face);
    }
    // SAFETY: CoreText descriptors built from and read for their attributes only.
    unsafe {
        let key: &CFString = kCTFontFamilyNameAttribute;
        let value = CFString::from_str(name);
        let attrs = CFDictionary::<CFString, CFType>::from_slices(&[key], &[value.as_ref()]);
        let descriptor = CTFontDescriptor::with_attributes(attrs.as_opaque());
        let Some(matches) = descriptor.matching_font_descriptors(None) else { return Vec::new() };
        // An array of font descriptors, as CoreText documents it.
        let matches: CFRetained<objc2_core_foundation::CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(matches);
        let mut out = Vec::new();
        for i in 0..matches.len() {
            let Some(d) = matches.get(i) else { continue };
            if let Some(path) = path_of(d.attribute(kCTFontURLAttribute))
                && !out.contains(&path)
            {
                out.push(path);
            }
        }
        out
    }
}

/// The files of the fonts CoreText falls back to for `chars` in `locale` (a BCP 47 tag), each
/// with the rest of its family's files, so a bold label draws in the family's own bold (Hiragino
/// Sans W6, not W3 emboldened). Chars no installed font covers add nothing.
pub(super) fn fallback_family_files(chars: &[char], locale: Option<&str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut families: Vec<String> = Vec::new();
    // SAFETY: CoreText fonts made for the lookup and read for their names and files only.
    unsafe {
        let Some(base) = CTFont::new_ui_font_for_language(CTFontUIFontType::System, 0.0, None) else { return out };
        let base_name = base.family_name().to_string();
        let language = locale.map(CFString::from_str);
        for ch in chars {
            let text = CFString::from_str(ch.encode_utf8(&mut [0; 4]));
            let range = CFRange { location: 0, length: text.length() };
            let font = CTFont::for_string_with_language(&base, &text, range, language.as_deref());
            let family = font.family_name().to_string();
            // PingFang (CoreText's Chinese fallback) draws with `hvgl` outlines, which no
            // outline reader here can draw: Chinese takes Hiragino Sans GB, or Heiti TC for
            // Traditional, as fontique does.
            if family.trim_start_matches('.').starts_with("PingFang") {
                let traditional = family.contains("TC") || family.contains("HK") || locale.is_some_and(|l| l.contains("Hant") || l.contains("TW") || l.contains("HK") || l.contains("MO"));
                let instead = if traditional { "Heiti TC" } else { "Hiragino Sans GB" };
                if !families.iter().any(|f| f == instead) {
                    for path in family_paths(instead) {
                        if !out.contains(&path) { out.push(path); }
                    }
                    families.push(instead.to_owned());
                }
                continue;
            }
            // A hidden family (`.Hiragino Kaku Gothic Interface`, CoreText's Japanese fallback)
            // matches no descriptors, so its weights would never load: take the family of its
            // file's first public face instead (Hiragino Sans, W0-W9).
            let family = if family.starts_with('.') { public_family(&font).unwrap_or(family) } else { family };
            // The system font itself: it had the character, nothing to add.
            if family == base_name || families.contains(&family) { continue; }
            if let Some(path) = path_of(font.attribute(kCTFontURLAttribute))
                && !out.contains(&path)
            {
                out.push(path);
            }
            for path in family_paths(&family) {
                if !out.contains(&path) { out.push(path); }
            }
            families.push(family);
        }
    }
    out
}

/// The family of the first face in `font`'s file whose family isn't hidden (`.`-prefixed).
fn public_family(font: &CTFont) -> Option<String> {
    // SAFETY: CoreText's descriptors for the file, read for their family names only.
    unsafe {
        let url = font.attribute(kCTFontURLAttribute)?.downcast::<CFURL>().ok()?;
        let faces = CTFontManagerCreateFontDescriptorsFromURL(&url)?;
        // An array of font descriptors, as CoreText documents it.
        let faces: CFRetained<objc2_core_foundation::CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(faces);
        (0..faces.len()).filter_map(|i| faces.get(i)).find_map(|d| {
            let name = d.attribute(kCTFontFamilyNameAttribute)?.downcast::<CFString>().ok()?.to_string();
            (!name.starts_with('.')).then_some(name)
        })
    }
}