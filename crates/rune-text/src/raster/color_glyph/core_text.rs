//! iOS: a colour glyph drawn by Core Text, for the images swash has no decoder for. Apple's
//! emoji font there holds each image in a compressed format of Apple's own, which Core Text
//! reads. Core Text is handed the font's bytes as they already sit in memory: no file is
//! opened, and Rune hands the bytes over without copying. The glyph is drawn into a bitmap
//! of this module's.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use objc2_core_foundation::{CFAllocator, CFAllocatorContext, CFArray, CFData, CFIndex, CFRetained, CGFloat, CGPoint};
use objc2_core_graphics::{CGBitmapContextCreate, CGColorSpace, CGImageAlphaInfo, kCGColorSpaceSRGB};
use objc2_core_text::{CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromData, CTFontOrientation};
use parley::fontique::Blob;
use swash::zeno::Placement;

use rune_image::pixels::Pixels;

/// The widest and tallest image drawn, in pixels, as for a COLR glyph: a larger glyph has none.
const MAX_SIDE: usize = 1024;

/// Clear pixels left around the glyph's bounds on each side.
const MARGIN: f64 = 1.0;

/// One face of a font as Core Text opened it from the font's bytes. It holds the descriptor
/// alone: a font is made from it at the size of each glyph drawn. Core Text reads the bytes
/// in place, so what a face adds to memory is what Core Text keeps for the font it parsed.
pub(crate) struct Face {
    descriptor: CFRetained<CTFontDescriptor>,
}

/// Takes a count on the `Arc<Blob<u8>>` behind `info` for the allocator being made.
unsafe extern "C-unwind" fn retain_blob(info: *const c_void) -> *const c_void {
    // SAFETY: `info` is the pointer `Arc::into_raw` gave `font_data`, whose count is still
    // held while Core Foundation makes the allocator.
    unsafe { Arc::increment_strong_count(info.cast::<Blob<u8>>()) };
    info
}

/// Gives back the allocator's count on the `Arc<Blob<u8>>` behind `info`.
unsafe extern "C-unwind" fn release_blob(info: *const c_void) {
    // SAFETY: `info` is the pointer `retain_blob` took a count on for this allocator, and an
    // allocator releases its context once, when it is destroyed.
    unsafe { Arc::decrement_strong_count(info.cast::<Blob<u8>>()) };
}

/// Frees nothing: the bytes belong to the blob, which goes when the allocator does.
unsafe extern "C-unwind" fn leave_bytes(_bytes: *mut c_void, _info: *mut c_void) {}

/// `blob`'s bytes as a `CFData` that points at them where they are.
fn font_data(blob: &Blob<u8>) -> Option<CFRetained<CFData>> {
    let bytes: &[u8] = blob.as_ref();
    let length = CFIndex::try_from(bytes.len()).ok().filter(|&length| length > 0)?;
    let info = Arc::into_raw(Arc::new(blob.clone()));
    let mut context = CFAllocatorContext {
        version: 0,
        info: info.cast_mut().cast(),
        retain: Some(retain_blob),
        release: Some(release_blob),
        copyDescription: None,
        allocate: None,
        reallocate: None,
        deallocate: Some(leave_bytes),
        preferredSize: None,
    };
    // SAFETY: the `CFData` points into `blob`'s bytes without copying them, so they must
    // outlive it. The deallocator given to it is an allocator whose context owns a clone of
    // the blob: Core Foundation takes a count on that clone when it makes the allocator
    // (`retain_blob`) and gives it back when the allocator is destroyed (`release_blob`), and
    // the `CFData` holds the allocator until it has let go of the bytes. A blob's bytes
    // neither move nor change while a clone of it lives, so they outlive the `CFData`,
    // whoever keeps it and on whichever thread it is released (a `Blob` is `Send` and
    // `Sync`). The count `Arc::into_raw` took is given back here, whether or not the
    // allocator was made; `context` is read during the call only.
    unsafe {
        let deallocator = CFAllocator::new(None, &mut context);
        drop(Arc::from_raw(info));
        let deallocator = deallocator?;
        CFData::with_bytes_no_copy(None, bytes.as_ptr(), length, Some(&*deallocator))
    }
}

impl Face {
    /// Face `index` of the font file in `blob`, or None when Core Text finds no such face in it.
    pub(super) fn open(blob: &Blob<u8>, index: u32) -> Option<Face> {
        let data = font_data(blob)?;
        // SAFETY: `data` is a live `CFData`, which Core Text keeps for as long as a descriptor
        // or font made from it reads the bytes. What comes back is an array of font
        // descriptors, as Core Text documents it: an empty one for data it can't read, never
        // NULL, which the binding would panic on.
        //
        // The descriptor at `index` is taken as the file's face `index`. Core Text doesn't
        // document the array's order; this assumes it is the order of the faces in the file.
        let descriptors: CFRetained<CFArray<CTFontDescriptor>> = unsafe { CFRetained::cast_unchecked(CTFontManagerCreateFontDescriptorsFromData(&data)) };
        Some(Face { descriptor: descriptors.get(index as usize)? })
    }
}

/// `glyph` of `face` as an image `size` px tall: straight RGBA, rows top first, with its
/// placement about the pen. None when the glyph has no bounds or is too large to draw.
pub(super) fn rasterize(face: &Face, glyph: u16, size: f32) -> Option<(Pixels, Placement)> {
    // SAFETY: a font made from a live descriptor, with no matrix; one glyph id is read from
    // `glyph`, and a null pointer asks for the overall bounds alone. Core Text documents that
    // it always answers a descriptor with a font, a fallback one when nothing matches, never
    // NULL, which the binding would panic on.
    let (font, bounds) = unsafe {
        let font = CTFont::with_font_descriptor(&face.descriptor, size as CGFloat, std::ptr::null());
        let bounds = font.bounding_rects_for_glyphs(CTFontOrientation::Default, NonNull::from(&glyph), std::ptr::null_mut(), 1);
        (font, bounds)
    };
    if !(bounds.size.width > 0.0 && bounds.size.height > 0.0) {
        return None;
    }
    // The bitmap is the glyph's bounds rounded out to whole pixels and then one pixel more on
    // each side, so that an edge Core Text antialiases past the bounds isn't cut off. Its
    // corner sits that pixel outside the bounds: hence the margin taken off here, and the
    // glyph drawn that far in from the bitmap's corner below.
    let (x0, y0) = (bounds.origin.x.floor() - MARGIN, bounds.origin.y.floor() - MARGIN);
    let x1 = (bounds.origin.x + bounds.size.width).ceil() + MARGIN;
    let y1 = (bounds.origin.y + bounds.size.height).ceil() + MARGIN;
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let mut data = vec![0u8; w * h * 4];
    // SAFETY: `kCGColorSpaceSRGB` is a constant string of Core Graphics.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: `data` is `h` rows of `w` four-byte pixels, the layout the context is told of,
    // and outlives the context, which is released before `data` is read. One glyph id and
    // one position are read from `glyph` and `at`.
    unsafe {
        let context = CGBitmapContextCreate(data.as_mut_ptr().cast(), w, h, 8, w * 4, Some(&*space), CGImageAlphaInfo::PremultipliedLast.0)?;
        // The pen, in the bitmap's own coordinates: its origin is the bottom-left pixel, y up.
        let at = CGPoint::new(-x0, -y0);
        font.draw_glyphs(NonNull::from(&glyph), NonNull::from(&at), 1, &context);
    }
    // Core Graphics draws premultiplied; a glyph's image has straight alpha, as swash's do.
    for pixel in data.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha != 0 && alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    let pixels = Pixels::new();
    pixels.set(w as u32, h as u32, data);
    // The bitmap's first row in memory is its top one, `h` pixels above its bottom edge at `y0`.
    Some((pixels, Placement { left: x0 as i32, top: y1 as i32, width: w as u32, height: h as u32 }))
}
