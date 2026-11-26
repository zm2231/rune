//! Picking photos and videos: [`Tx::pick_media`]. iOS shows the system's photo picker; every
//! other platform shows an Open dialog for the image and video files it names.

use std::cell::RefCell;
use std::rc::Rc;

use super::{AnswerFn, Call, DialogAnswer, FileDialogFilesResult, FileDialogKind, FileDialogOptions, Request};
use crate::Tx;

const IMAGES: &[&str] = &["png", "jpg", "jpeg", "heic", "heif", "gif", "webp", "avif", "tiff", "tif", "bmp"];
const VIDEOS: &[&str] = &["mov", "mp4", "m4v", "webm", "mkv", "avi"];

/// What a media picker offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MediaKind {
    #[default]
    Images,
    Videos,
    ImagesAndVideos,
}

impl MediaKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Videos => "videos",
            Self::ImagesAndVideos => "images-and-videos",
        }
    }

    pub(crate) fn images(self) -> bool { self != Self::Videos }

    pub(crate) fn videos(self) -> bool { self != Self::Images }
}

/// A request for photos or videos, for [`Tx::pick_media`]: one image by default.
///
/// ```
/// # use rune::{MediaKind, MediaPicker};
/// let picker = MediaPicker::new(MediaKind::ImagesAndVideos).limit(5);
/// assert_eq!((picker.kind(), picker.selection_limit()), (MediaKind::ImagesAndVideos, 5));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaPicker {
    kind: MediaKind,
    limit: usize,
}

impl Default for MediaPicker {
    fn default() -> Self { Self::new(MediaKind::Images) }
}

impl MediaPicker {
    pub fn new(kind: MediaKind) -> Self { Self { kind, limit: 1 } }

    /// How many items the user may pick; 0 is no limit. Desktop dialogs pick one file or any
    /// number of them, so any limit but 1 lets them pick several.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn kind(&self) -> MediaKind { self.kind }

    pub fn selection_limit(&self) -> usize { self.limit }

    /// The Open dialog that stands in for the photo picker off iOS.
    fn dialog_options(&self) -> FileDialogOptions {
        let mut options = FileDialogOptions::default();
        if self.kind.images() && self.kind.videos() {
            let all: Vec<&str> = IMAGES.iter().chain(VIDEOS).copied().collect();
            options = options.filter("Images and videos", &all);
        }
        if self.kind.images() { options = options.filter("Images", IMAGES); }
        if self.kind.videos() { options = options.filter("Videos", VIDEOS); }
        options
    }
}

impl Tx {
    /// Asks the user for photos or videos once this handler's frame is done. `on_answer` runs,
    /// as an action on a later frame, with the picked files in the order picked, `Ok(None)`
    /// when the user cancels, or an error when no picker could show (as for
    /// [`Tx::open_files`]).
    ///
    /// On iOS this is the system's photo picker, which needs no permission to the photo
    /// library: each item is copied into the app's temporary folder, which the system may
    /// empty while the app isn't running, so move what the app keeps. Elsewhere it is an Open
    /// dialog for image and video files. In headless `--tree` output `fileDialog` carries
    /// `"media"`, and scripts answer it as a multi-file Open.
    ///
    /// ```
    /// # use std::path::PathBuf;
    /// # use rune::{MediaKind, MediaPicker, Tx, el};
    /// # struct App { photos: Vec<PathBuf> }
    /// # let _ = el()
    /// .on_press_tx(|_: &mut App, tx: &mut Tx| {
    ///     tx.pick_media(MediaPicker::new(MediaKind::Images).limit(0), |s: &mut App, result, _| {
    ///         if let Ok(Some(paths)) = result { s.photos.extend(paths); }
    ///     });
    /// })
    /// # ;
    /// ```
    pub fn pick_media<S: 'static>(&mut self, picker: MediaPicker, on_answer: impl FnOnce(&mut S, FileDialogFilesResult, &mut Tx) + 'static) {
        let on_answer: AnswerFn = Box::new(move |s, result, tx| {
            if let Some(s) = s.cast::<S>() { on_answer(s, result.map(DialogAnswer::into_many), tx); }
        });
        self.file_dialogs.push(Call(Rc::new(RefCell::new(Some(Request {
            kind: FileDialogKind::Open,
            multiple: picker.limit != 1,
            options: picker.dialog_options(),
            on_answer,
            media: Some(picker),
        })))));
    }
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
