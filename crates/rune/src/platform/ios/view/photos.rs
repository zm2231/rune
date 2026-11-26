//! The photo picker (`Tx::pick_media`): PhotosUI's `PHPickerViewController`, which runs out of
//! the app's process and needs no permission to the photo library. The picker lends each
//! item as a file that is gone once its handler returns, so the handler copies it into the
//! app's temporary folder, on the thread the picker calls it on. Once every item is copied
//! the paths go to the app in the order picked.

#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2::rc::{Allocated, Retained, Weak};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, class, msg_send};
use objc2_foundation::{NSString, NSURL};

use super::super::pacing::Event;
use super::RuneView;
use super::link::MainOnly;
use crate::file_dialog::{DialogAnswer, MediaKind, MediaPicker};
use crate::platform::main_queue::on_main;

// SAFETY: the block declares nothing. Linking the framework brings in the picker's classes,
// which `pick_media` looks up by name.
#[link(name = "PhotosUI", kind = "framework")]
unsafe extern "C" {}

const IMAGE: &str = "public.image";
const MOVIE: &str = "public.movie";

/// The items still copying, and where each one landed, in the order picked.
struct Copies {
    left: usize,
    paths: Vec<Option<PathBuf>>,
}

impl RuneView {
    /// Shows the photo picker for dialog `id`. A view on no screen answers with an error.
    pub(super) fn pick_media(&self, id: u64, picker: &MediaPicker) {
        let Some(screen) = self.view_controller() else {
            self.with_surface(|surface| surface.file_dialog_answer(id, Err("the view is on no screen".into())));
            return;
        };
        self.ivars().media_dialog.set(Some(id));
        // SAFETY: a configuration's `selectionLimit` is an `NSInteger` and its `filter` a
        // `PHPickerFilter`; the picker is made from the configuration, its delegate is the
        // view, and a view controller presents another with no completion block.
        unsafe {
            let config: Allocated<AnyObject> = msg_send![class!(PHPickerConfiguration), alloc];
            let config: Retained<AnyObject> = msg_send![config, init];
            let _: () = msg_send![&*config, setSelectionLimit: picker.selection_limit() as isize];
            let _: () = msg_send![&*config, setFilter: &*filter(picker.kind())];
            let controller: Allocated<AnyObject> = msg_send![class!(PHPickerViewController), alloc];
            let controller: Retained<AnyObject> = msg_send![controller, initWithConfiguration: &*config];
            let _: () = msg_send![&*controller, setDelegate: self];
            let _: () = msg_send![&*screen, presentViewController: &*controller, animated: true, completion: std::ptr::null::<std::ffi::c_void>()];
        }
    }

    /// The picker closed with `results`, an `NSArray<PHPickerResult *>`, empty when cancelled.
    pub(super) fn media_picked(&self, picker: &AnyObject, results: &AnyObject) {
        // SAFETY: the picker is a view controller the view presented; it dismisses itself
        // with no completion block. `count` is an `NSUInteger`.
        let count: usize = unsafe {
            let _: () = msg_send![picker, dismissViewControllerAnimated: true, completion: std::ptr::null::<std::ffi::c_void>()];
            msg_send![results, count]
        };
        let Some(id) = self.ivars().media_dialog.take() else { return };
        if count == 0 {
            self.with_surface(|surface| surface.file_dialog_answer(id, Ok(DialogAnswer::Many(None))));
            self.pace(Event::Wake);
            return;
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let folder = Arc::new(std::env::temp_dir().join(format!("Picked-{nanos}")));
        let copies = Arc::new(Mutex::new(Copies { left: count, paths: vec![None; count] }));
        let view = Arc::new(MainOnly(Weak::new(self)));
        for index in 0..count {
            // SAFETY: every object in the array is a `PHPickerResult`, whose `itemProvider` is
            // an `NSItemProvider`. `hasItemConformingToTypeIdentifier:` answers a `BOOL`, and
            // `loadFileRepresentationForTypeIdentifier:completionHandler:` copies the block
            // and calls it once, on a thread of its own, with a file URL or nil and an error
            // or nil; it returns an `NSProgress` the view doesn't need.
            unsafe {
                let result: Retained<AnyObject> = msg_send![results, objectAtIndex: index];
                let provider: Retained<AnyObject> = msg_send![&*result, itemProvider];
                let has = |kind: &str| -> bool { msg_send![&*provider, hasItemConformingToTypeIdentifier: &*NSString::from_str(kind)] };
                let Some(kind) = [MOVIE, IMAGE].into_iter().find(|kind| has(kind)) else {
                    finish(&copies, &view, id, index, None);
                    continue;
                };
                let (copies, view, folder) = (copies.clone(), view.clone(), folder.clone());
                let block = RcBlock::new(move |url: *mut NSURL, _error: *mut AnyObject| {
                    let copied = url.as_ref().and_then(|url| copy(url, &folder.join(index.to_string())));
                    finish(&copies, &view, id, index, copied);
                });
                let _: Option<Retained<AnyObject>> = msg_send![&*provider, loadFileRepresentationForTypeIdentifier: &*NSString::from_str(kind), completionHandler: &*block];
            }
        }
    }
}

/// The picker's filter for `kind`.
fn filter(kind: MediaKind) -> Retained<AnyObject> {
    // SAFETY: `imagesFilter` and `videosFilter` return filters, and
    // `anyFilterMatchingSubfilters:` takes an array of them.
    unsafe {
        let images = || -> Retained<AnyObject> { msg_send![class!(PHPickerFilter), imagesFilter] };
        let videos = || -> Retained<AnyObject> { msg_send![class!(PHPickerFilter), videosFilter] };
        match kind {
            MediaKind::Images => images(),
            MediaKind::Videos => videos(),
            MediaKind::ImagesAndVideos => {
                let both: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
                let _: () = msg_send![&*both, addObject: &*images()];
                let _: () = msg_send![&*both, addObject: &*videos()];
                msg_send![class!(PHPickerFilter), anyFilterMatchingSubfilters: &*both]
            }
        }
    }
}

/// Copies the file the picker lent into `folder`, keeping its name.
fn copy(url: &NSURL, folder: &Path) -> Option<PathBuf> {
    let from = PathBuf::from(url.path()?.to_string());
    let to = folder.join(from.file_name()?);
    std::fs::create_dir_all(folder).ok()?;
    std::fs::copy(&from, &to).ok()?;
    Some(to)
}

/// Item `index` is copied, or failed: once none is left the paths go to the view.
fn finish(copies: &Mutex<Copies>, view: &Arc<MainOnly>, id: u64, index: usize, path: Option<PathBuf>) {
    let mut copies = copies.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    copies.paths[index] = path;
    copies.left -= 1;
    if copies.left > 0 { return; }
    let paths: Vec<PathBuf> = copies.paths.drain(..).flatten().collect();
    let view = view.clone();
    on_main(move || {
        let Some(view) = view.0.load() else { return };
        let answer = if paths.is_empty() { Err("none of the picked items could be copied".into()) } else { Ok(DialogAnswer::Many(Some(paths))) };
        view.with_surface(|surface| surface.file_dialog_answer(id, answer));
        view.pace(Event::Wake);
    });
}
