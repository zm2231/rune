//! Apple platforms: system font files found through CoreText, as DirectWrite finds them on Windows
//! (`direct_write`), so no font folder is walked and no font is parsed until a text needs it.
//! A text the app's fonts lack goes to CoreText's own fallback (`CTFontCreateForStringWithLanguage`,
//! what Safari and AppKit fall back with), which names the font that covers it; a family's
//! files come from CoreText's descriptors for the family name. The system UI font's own
//! families are found from its file (`system_face_paths`).

use std::path::PathBuf;

use objc2_core_foundation::{CFArray, CFDictionary, CFRange, CFRetained, CFString, CFType, CFURL, CFURLPathStyle};
use objc2_core_text::{CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromURL, CTFontSymbolicTraits, CTFontUIFontType, kCTFontCascadeListAttribute, kCTFontFamilyNameAttribute, kCTFontURLAttribute};

use super::apple_names::{SystemFace, cjk_candidates, fallback_files, is_hidden, is_last_resort, is_pingfang, sibling_file, system_family};
use super::chinese::ChineseFallbacks;

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
/// come from the UI font's file instead. Any other hidden name has no files: it is never
/// given to CoreText to match.
fn family_paths(name: &str) -> Vec<PathBuf> {
    if let Some(face) = system_family(name) {
        return system_face_paths(face);
    }
    if is_hidden(name) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for descriptor in matching(name) {
        // SAFETY: a CoreText descriptor, read for its file only.
        if let Some(path) = path_of(unsafe { descriptor.attribute(kCTFontURLAttribute) })
            && !out.contains(&path)
        {
            out.push(path);
        }
    }
    out
}

/// CoreText's descriptors for the faces of family `name`.
fn matching(name: &str) -> Vec<CFRetained<CTFontDescriptor>> {
    // SAFETY: CoreText descriptors built from and read for their attributes only.
    unsafe {
        let key: &CFString = kCTFontFamilyNameAttribute;
        let value = CFString::from_str(name);
        let attrs = CFDictionary::<CFString, CFType>::from_slices(&[key], &[value.as_ref()]);
        let descriptor = CTFontDescriptor::with_attributes(attrs.as_opaque());
        let Some(matches) = descriptor.matching_font_descriptors(None) else { return Vec::new() };
        // An array of font descriptors, as CoreText documents it.
        let matches: CFRetained<CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(matches);
        (0..matches.len()).filter_map(|i| matches.get(i)).collect()
    }
}

/// The files of family `name` as a stand-in for PingFang, or none when the system answers the
/// name with PingFang itself: each face CoreText matches is made a font, and that font's own
/// family and file are what count, not the name asked for.
fn chinese_family_paths(name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for descriptor in matching(name) {
        // SAFETY: a font made from CoreText's descriptor, read for its name and file only.
        let (family, path) = unsafe {
            let font = CTFont::with_font_descriptor(&descriptor, 0.0, std::ptr::null());
            (font.family_name().to_string(), path_of(font.attribute(kCTFontURLAttribute)))
        };
        if is_pingfang(&family, path.as_deref()) {
            return Vec::new();
        }
        if let Some(path) = path
            && !out.contains(&path)
        {
            out.push(path);
        }
    }
    out
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    fn CTFontGetGlyphsForCharacters(font: &CTFont, characters: *const u16, glyphs: *mut u16, count: isize) -> bool;
}

/// Whether `font` has a glyph for `ch`.
fn covers(font: &CTFont, ch: char) -> bool {
    let mut units = [0u16; 2];
    let units = ch.encode_utf16(&mut units);
    let mut glyphs = [0u16; 2];
    // SAFETY: both buffers hold `units.len()` items.
    unsafe { CTFontGetGlyphsForCharacters(font, units.as_ptr(), glyphs.as_mut_ptr(), units.len() as isize) }
}

/// A font of CoreText's cascade list. An entry is a font with a file, or only a role
/// (`.AppleJapaneseFont`, `.AppleSimplifiedChineseFont`) that says which characters it has and
/// names no file; `via` is then the system font with that one role as its whole cascade, which
/// CoreText's fallback resolves to the font standing behind the role for a character.
#[derive(Clone)]
pub(super) struct CascadeFont {
    font: CFRetained<CTFont>,
    via: Option<CFRetained<CTFont>>,
}

// SAFETY: a `CTFont` is immutable once made, and CoreText documents its font objects as safe
// to use from several threads at once. This wrapper holds two of them and nothing else: no
// cell, no lock, no state of its own that a second thread could change.
unsafe impl Send for CascadeFont {}
unsafe impl Sync for CascadeFont {}

/// What Chinese falls back to, kept in `Fallbacks` between texts.
pub(super) type Chinese = ChineseFallbacks<CascadeFont>;

/// The fonts of `base`'s cascade list for `language`, PingFang's own entries left out.
fn cascade_fonts(base: &CTFont, language: Option<&CFString>) -> Vec<CascadeFont> {
    // SAFETY: CoreText's cascade list, an array of font descriptors as CoreText documents it,
    // fonts made from them, and copies of `base` with one of them as their cascade list.
    unsafe {
        let languages = language.map(|l| CFArray::from_objects(&[l]));
        let Some(list) = base.default_cascade_list_for_languages(languages.as_ref().map(|l| l.as_opaque())) else { return Vec::new() };
        let list: CFRetained<CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(list);
        (0..list.len())
            .filter_map(|i| list.get(i))
            .filter_map(|descriptor| {
                let font = CTFont::with_font_descriptor(&descriptor, 0.0, std::ptr::null());
                let path = path_of(font.attribute(kCTFontURLAttribute));
                if is_pingfang(&font.family_name().to_string(), path.as_deref()) { return None; }
                let via = path.is_none().then(|| {
                    let key: &CFString = kCTFontCascadeListAttribute;
                    let only = CFArray::from_objects(&[&*descriptor]);
                    let attrs = CFDictionary::<CFString, CFType>::from_slices(&[key], &[only.as_ref()]);
                    base.copy_with_attributes(0.0, std::ptr::null(), Some(&CTFontDescriptor::with_attributes(attrs.as_opaque())))
                });
                Some(CascadeFont { font, via })
            })
            .collect()
    }
}

/// The first of `fonts` that has `ch` and, once a role is resolved to the font behind it, is
/// in a file and is neither PingFang nor the last-resort font.
fn stand_in(fonts: &[CascadeFont], ch: char, text: &CFString) -> Option<CFRetained<CTFont>> {
    fonts.iter().find_map(|entry| {
        if !covers(&entry.font, ch) { return None; }
        // SAFETY: CoreText's fallback for one character, read for its name and file only.
        unsafe {
            let font = match &entry.via {
                Some(via) => CTFont::for_string(via, text, CFRange { location: 0, length: text.length() }),
                None => entry.font.clone(),
            };
            let family = font.family_name().to_string();
            let path = path_of(font.attribute(kCTFontURLAttribute))?;
            (!is_pingfang(&family, Some(&path)) && !is_last_resort(&family)).then_some(font)
        }
    })
}

/// The files of the fonts CoreText falls back to for `chars` in `locale` (a BCP 47 tag), each
/// with the rest of its family's files, so a bold label draws in the family's own bold (Hiragino
/// Sans W6, not W3 emboldened). A char no installed font covers adds nothing: CoreText answers
/// with its last-resort font, which is skipped. `chinese` keeps what Chinese falls back to
/// from one call to the next.
pub(super) fn fallback_family_files(chars: &[char], locale: Option<&str>, chinese: &mut Chinese) -> Vec<PathBuf> {
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
            let mut font = CTFont::for_string_with_language(&base, &text, range, language.as_deref());
            let mut family = font.family_name().to_string();
            if is_last_resort(&family) { continue; }
            // PingFang (CoreText's Chinese fallback) draws with `hvgl` outlines, which no
            // outline reader here can draw. Chinese takes the first of `cjk_candidates` the
            // system has as a font of its own: Hiragino Sans GB, or Heiti TC for Traditional.
            // Where it has none (or has Heiti only as another name for PingFang), the
            // character goes to the first font of CoreText's cascade that has it and isn't
            // PingFang, or to none. `chinese` remembers the family and the cascade, so only
            // the first Chinese character of a kind asks CoreText for them.
            if is_pingfang(&family, path_of(font.attribute(kCTFontURLAttribute)).as_deref()) {
                if let Some(files) = chinese.family(cjk_candidates(&family, locale), chinese_family_paths) {
                    for path in files {
                        if !out.contains(path) { out.push(path.clone()); }
                    }
                    continue;
                }
                let fonts = chinese.cascade(locale, || cascade_fonts(&base, language.as_deref()));
                let Some(other) = stand_in(&fonts, *ch, &text) else { continue };
                family = other.family_name().to_string();
                font = other;
            }
            // A hidden family (`.Hiragino Kaku Gothic Interface`, CoreText's Japanese fallback)
            // can't be asked for by name, so its weights would never load: take the family of
            // its file's first public face instead (Hiragino Sans, W0-W9). A file with no
            // public face (`.SF Arabic`'s) is loaded as the font says where it is, and no more.
            let family = if is_hidden(&family) { public_family(&font).unwrap_or(family) } else { family };
            // The system font itself: it had the character, nothing to add.
            if family == base_name || families.contains(&family) { continue; }
            for path in fallback_files(path_of(font.attribute(kCTFontURLAttribute)), &family, family_paths) {
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
        let faces: CFRetained<CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(faces);
        (0..faces.len()).filter_map(|i| faces.get(i)).find_map(|d| {
            let name = d.attribute(kCTFontFamilyNameAttribute)?.downcast::<CFString>().ok()?.to_string();
            (!is_hidden(&name)).then_some(name)
        })
    }
}