//! Apple platforms: system font files found through CoreText, as DirectWrite finds them on Windows
//! (`direct_write`), so no font folder is walked and no font is parsed until a text needs it.
//! A text the app's fonts lack goes to CoreText's own fallback (`CTFontCreateForStringWithLanguage`,
//! what Safari and AppKit fall back with), which names the font that covers it; a family's
//! files come from CoreText's descriptors for the family name. The system UI font's own
//! families are found from its file (`system_face_paths`).

#![allow(unsafe_code)]

use std::path::PathBuf;
use std::sync::Mutex;

use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRange, CFRetained, CFString, CFType, CFURL, CFURLPathStyle};
use objc2_core_text::{
    CTFont, CTFontCollection, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromURL,
    CTFontSymbolicTraits, CTFontUIFontType, kCTFontCascadeListAttribute, kCTFontFamilyNameAttribute,
    kCTFontSymbolicTrait, kCTFontTraitsAttribute, kCTFontURLAttribute, kCTFontVariationAxesAttribute,
    kCTFontVariationAxisIdentifierKey, kCTFontVariationAxisMaximumValueKey,
    kCTFontVariationAxisMinimumValueKey, kCTFontWeightTrait,
};

use super::apple_names::{SystemFace, cjk_candidates, fallback_files, is_hidden, is_last_resort, is_pingfang, sibling_file, system_family};
use super::chinese::ChineseFallbacks;

/// The file behind a CoreText attribute holder's `kCTFontURLAttribute`.
fn path_of(url: Option<CFRetained<CFType>>) -> Option<PathBuf> {
    let url = url?.downcast::<CFURL>().ok()?;
    Some(PathBuf::from(url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?.to_string()))
}

/// Every available public font family from CoreText's font collection, read from CoreText's
/// descriptor metadata without walking font directories or opening font files.
pub(super) fn installed_families() -> Vec<super::installed::RawFamily> {
    const WGHT_TAG: i64 = u32::from_be_bytes(*b"wght") as i64;
    let mut out = Vec::new();
    // SAFETY: CoreText collection and descriptor attributes are read-only and retain-counted.
    unsafe {
        let collection = CTFontCollection::from_available_fonts(None);
        let Some(descriptors) = collection.matching_font_descriptors() else {
            return out;
        };
        let descriptors: CFRetained<CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(descriptors);
        for i in 0..descriptors.len() {
            let Some(descriptor) = descriptors.get(i) else { continue };
            let Some(name) = descriptor
                .attribute(kCTFontFamilyNameAttribute)
                .and_then(|v| v.downcast::<CFString>().ok())
                .map(|s| s.to_string())
            else {
                continue;
            };
            if name.is_empty() || name.starts_with('.') {
                continue;
            }
            let mut monospace = false;
            let mut weights = Vec::new();
            if let Some(traits_raw) = descriptor.attribute(kCTFontTraitsAttribute)
                && let Ok(traits) = traits_raw.downcast::<CFDictionary>()
            {
                let traits: CFRetained<CFDictionary<CFString, CFType>> = CFRetained::cast_unchecked(traits);
                if let Some(sym) = traits
                    .get(kCTFontSymbolicTrait)
                    .and_then(|v| v.downcast::<CFNumber>().ok())
                    .and_then(|n| n.as_i64().or_else(|| n.as_i32().map(i64::from)))
                {
                    monospace = ((sym as u32) & CTFontSymbolicTraits::TraitMonoSpace.0) != 0;
                }
                if let Some(weight) = traits
                    .get(kCTFontWeightTrait)
                    .and_then(|v| v.downcast::<CFNumber>().ok())
                    .and_then(|n| n.as_f64())
                {
                    weights.push(super::installed::apple_weight_trait_to_css(weight));
                }
            }
            if let Some(axes_raw) = descriptor.attribute(kCTFontVariationAxesAttribute)
                && let Ok(axes) = axes_raw.downcast::<CFArray>()
            {
                let axes: CFRetained<CFArray<CFDictionary>> = CFRetained::cast_unchecked(axes);
                for j in 0..axes.len() {
                    let Some(axis_raw) = axes.get(j) else { continue };
                    let axis: CFRetained<CFDictionary<CFString, CFType>> = CFRetained::cast_unchecked(axis_raw);
                    let tag = axis
                        .get(kCTFontVariationAxisIdentifierKey)
                        .and_then(|v| v.downcast::<CFNumber>().ok())
                        .and_then(|n| n.as_i64().or_else(|| n.as_i32().map(i64::from)));
                    if tag != Some(WGHT_TAG) {
                        continue;
                    }
                    let min = axis
                        .get(kCTFontVariationAxisMinimumValueKey)
                        .and_then(|v| v.downcast::<CFNumber>().ok())
                        .and_then(|n| n.as_f64());
                    let max = axis
                        .get(kCTFontVariationAxisMaximumValueKey)
                        .and_then(|v| v.downcast::<CFNumber>().ok())
                        .and_then(|n| n.as_f64());
                    if let (Some(min), Some(max)) = (min, max) {
                        super::installed::push_weight_range(&mut weights, min as f32, max as f32);
                    }
                }
            }
            out.push(super::installed::RawFamily { name, monospace, weights });
        }
    }
    out
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

// SAFETY: the wrapper holds two retained `CTFont`s and nothing else. Moving it to another
// thread moves those references, and Core Foundation's retain and release are atomic, so the
// thread that drops it may be any thread.
unsafe impl Send for CascadeFont {}
// SAFETY: a `CTFont` is immutable once made, and Core Text documents its functions as
// thread-safe and its font objects as usable by several threads at once. Through `&self` the
// wrapper only hands out `&CTFont` for those calls; it has no cell, lock or other state of its
// own that a second thread could change.
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
    fallback_family_files_with(chars, locale, chinese, public_family)
}

fn fallback_family_files_with(
    chars: &[char],
    locale: Option<&str>,
    chinese: &mut Chinese,
    mut resolve_public: impl FnMut(&CTFont) -> Option<String>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut families: Vec<String> = Vec::new();
    // Raw family names CoreText already answered with in this call: later characters from the
    // same family add nothing and are skipped before reading font descriptors again.
    let mut seen: Vec<String> = Vec::new();
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
            // The system font itself (`.AppleSystemUIFont`, which is hidden too) already covered
            // the character, and `.LastResort` means no installed font covers it (loading it as a
            // fallback face would box every later fallback symbol). Check both before
            // `resolve_public`, which reads every face name in the file (369 faces in `SFNS.ttf`).
            if family == base_name || is_last_resort(&family) { continue; }
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
            } else if seen.contains(&family) {
                continue;
            } else {
                seen.push(family.clone());
            }
            // A hidden family (`.Hiragino Kaku Gothic Interface`, CoreText's Japanese fallback)
            // can't be asked for by name, so its weights would never load: take the family of
            // its file's first public face instead (Hiragino Sans, W0-W9). A file with no
            // public face (`.SF Arabic`'s) is loaded as the font says where it is, and no more.
            let family = if is_hidden(&family) { resolve_public(&font).unwrap_or(family) } else { family };
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

/// Bounds for `PublicFamilyCache` (entry count and slot + heap bytes).
const MAX_PUBLIC_FAMILY_ENTRIES: usize = 64;
const MAX_PUBLIC_FAMILY_BYTES: usize = 16 * 1024;

/// Bounded cache mapping a hidden-family font file path to the first public family name in that
/// file, so later fallback calls for the same hidden family avoid re-reading its descriptors.
struct PublicFamilyCache {
    entries: Vec<(String, Option<String>)>,
    bytes: usize,
}

impl PublicFamilyCache {
    const fn new() -> Self {
        Self { entries: Vec::new(), bytes: 0 }
    }

    fn entry_bytes(key: &str, value: &Option<String>) -> usize {
        std::mem::size_of::<(String, Option<String>)>()
            + key.len()
            + value.as_deref().map_or(0, str::len)
    }

    fn get(&self, key: &str) -> Option<Option<String>> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn insert(&mut self, key: String, value: Option<String>) {
        if self.entries.iter().any(|(k, _)| k == &key) {
            return;
        }
        let cost = Self::entry_bytes(&key, &value);
        if cost > MAX_PUBLIC_FAMILY_BYTES {
            return;
        }
        while self.entries.len() >= MAX_PUBLIC_FAMILY_ENTRIES || self.bytes + cost > MAX_PUBLIC_FAMILY_BYTES {
            let (old_key, old_val) = self.entries.remove(0);
            self.bytes -= Self::entry_bytes(&old_key, &old_val);
        }
        self.bytes += cost;
        self.entries.push((key, value));
    }
}

/// The family of the first face in `font`'s file whose family isn't hidden (`.`-prefixed).
fn public_family(font: &CTFont) -> Option<String> {
    static CACHE: Mutex<PublicFamilyCache> = Mutex::new(PublicFamilyCache::new());
    // SAFETY: `font` is a live `CTFont` and `kCTFontURLAttribute` is CoreText's constant URL attribute key.
    let url = unsafe { font.attribute(kCTFontURLAttribute) }?.downcast::<CFURL>().ok()?;
    let Some(key) = url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle).map(|p| p.to_string()) else {
        return scan_public_family(&url);
    };
    if let Some(hit) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return hit;
    }
    let found = scan_public_family(&url);
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).insert(key, found.clone());
    found
}

/// Reads the descriptors in `url` and returns the first non-hidden family name.
fn scan_public_family(url: &CFURL) -> Option<String> {
    // SAFETY: CoreText's descriptors for the file, read for their family names only.
    unsafe {
        let faces = CTFontManagerCreateFontDescriptorsFromURL(url)?;
        // An array of font descriptors, as CoreText documents it.
        let faces: CFRetained<CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(faces);
        (0..faces.len()).filter_map(|i| faces.get(i)).find_map(|d| {
            let name = d.attribute(kCTFontFamilyNameAttribute)?.downcast::<CFString>().ok()?.to_string();
            (!is_hidden(&name)).then_some(name)
        })
    }
}

#[cfg(test)]
#[path = "core_text_tests.rs"]
mod tests;

