#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::ptr::null_mut;

use fontconfig_sys::constants::{FC_CHARSET, FC_COLOR, FC_FAMILY, FC_LANG};
use fontconfig_sys::statics::LIB_RESULT;
use fontconfig_sys::{FcBool, FcCharSet, FcChar8, FcMatchPattern, FcResultMatch};

const HAN_SAMPLE: u32 = 0x4E00;

pub(super) fn text_family_for(ch: char, locale: Option<&str>) -> Option<String> {
    let fc = LIB_RESULT.as_ref().ok()?;
    let lang = locale.and_then(|l| CString::new(l).ok());
    let code = u32::from(ch);
    // SAFETY: `fc` holds function pointers from the loaded `libfontconfig`. Every pattern,
    // charset and font set created here is destroyed exactly once before returning, the
    // pattern copies the charset and string it is given, and the patterns, charsets and
    // strings read out of the sorted set are used only while that set is alive.
    unsafe {
        let pattern = (fc.FcPatternCreate)();
        if pattern.is_null() {
            return None;
        }
        let charset = (fc.FcCharSetCreate)();
        if !charset.is_null() {
            (fc.FcCharSetAddChar)(charset, code);
            (fc.FcPatternAddCharSet)(pattern, FC_CHARSET.as_ptr(), charset);
            (fc.FcCharSetDestroy)(charset);
        }
        if let Some(lang) = &lang {
            (fc.FcPatternAddString)(pattern, FC_LANG.as_ptr(), lang.as_ptr().cast::<FcChar8>());
        }
        (fc.FcConfigSubstitute)(null_mut(), pattern, FcMatchPattern);
        (fc.FcDefaultSubstitute)(pattern);
        let mut result = 0;
        let set = (fc.FcFontSort)(null_mut(), pattern, 0, null_mut(), &mut result);
        (fc.FcPatternDestroy)(pattern);
        if set.is_null() {
            return None;
        }
        let count = usize::try_from((*set).nfont).unwrap_or(0);
        let fonts = if count == 0 || (*set).fonts.is_null() { &[][..] } else { std::slice::from_raw_parts((*set).fonts, count) };
        let mut han_font = None;
        let mut found = None;
        for &font in fonts {
            let mut charset: *mut FcCharSet = null_mut();
            if (fc.FcPatternGetCharSet)(font, FC_CHARSET.as_ptr(), 0, &mut charset) != FcResultMatch
                || (fc.FcCharSetHasChar)(charset, code) == 0
            {
                continue;
            }
            let mut color: FcBool = 0;
            if (fc.FcPatternGetBool)(font, FC_COLOR.as_ptr(), 0, &mut color) == FcResultMatch && color != 0 {
                continue;
            }
            if (fc.FcCharSetHasChar)(charset, HAN_SAMPLE) == 0 {
                found = Some(font);
                break;
            }
            han_font.get_or_insert(font);
        }
        let family = found.or(han_font).and_then(|font| {
            let mut family: *mut FcChar8 = null_mut();
            ((fc.FcPatternGetString)(font, FC_FAMILY.as_ptr(), 0, &mut family) == FcResultMatch && !family.is_null())
                .then(|| CStr::from_ptr(family.cast()).to_string_lossy().into_owned())
        });
        (fc.FcFontSetDestroy)(set);
        family
    }
}
