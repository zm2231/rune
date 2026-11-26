//! File dialogs on iOS: the system's document picker, over the screen the view is on.
//!
//! - Open: the picker hands back a copy of the chosen file inside the app's own container, so
//!   the path the app gets is one it may read with no further permission.
//! - Folder: the picker hands back the folder itself. The app is let into it for as long as
//!   it runs (the access the system scopes to the pick is started and kept), so it may read
//!   and write there.
//! - Save: iOS has no dialog that names a file not yet written, so the user picks the folder
//!   and the answer is the dialog's file name inside it. The folder is the app's for as long
//!   as it runs, exactly as a Folder pick's is.

#![allow(unsafe_code)]

use std::path::PathBuf;

use objc2_foundation::{NSString, NSURL};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, class, msg_send};

use super::super::pacing::Event;
use super::RuneView;
use crate::file_dialog::{DialogAnswer, FileDialogKind, FileDialogOptions};

/// The name a saved file gets when the dialog gives none.
const UNTITLED: &str = "Untitled";

/// The dialog whose picker is up, and what its answer is made of.
pub(super) struct Picking {
    id: u64,
    kind: FileDialogKind,
    /// An Open that takes several files: its answer is all of them.
    multiple: bool,
    /// A save's file name, which goes inside the folder picked.
    file_name: String,
}

// SAFETY: the block declares nothing. Linking the framework brings in `UTType`, the class
// `pick_file` looks up by name.
#[link(name = "UniformTypeIdentifiers", kind = "framework")]
unsafe extern "C" {}

impl RuneView {
    /// Shows the picker for dialog `id`, one that picks several files for an Open with
    /// `multiple`. A view on no screen answers with an error.
    pub(super) fn pick_file(&self, id: u64, kind: FileDialogKind, multiple: bool, options: &FileDialogOptions) {
        let Some(screen) = self.view_controller() else {
            self.with_surface(|surface| surface.file_dialog_answer(id, Err("the view is on no screen".into())));
            return;
        };
        let file_name = options.file_name.clone().filter(|name| !name.is_empty()).unwrap_or_else(|| UNTITLED.into());
        let multiple = multiple && kind == FileDialogKind::Open;
        *self.ivars().file_dialog.borrow_mut() = Some(Picking { id, kind, multiple, file_name });
        let folder = kind != FileDialogKind::Open;
        // SAFETY: a picker that opens copies of files of these types, whose delegate the view
        // is and whose `directoryURL` is a file URL; a view controller presents another with no
        // completion block.
        unsafe {
            let types: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
            for extension in options.filters.iter().flat_map(|(_, extensions)| extensions).filter(|_| !folder) {
                let kind: Option<Retained<AnyObject>> = msg_send![class!(UTType), typeWithFilenameExtension: &*NSString::from_str(extension)];
                if let Some(kind) = kind { let _: () = msg_send![&*types, addObject: &*kind]; }
            }
            let count: usize = msg_send![&*types, count];
            if count == 0 {
                // No filter, or none the system knows: any file. Or a folder.
                let any: Option<Retained<AnyObject>> = msg_send![class!(UTType), typeWithIdentifier: &*NSString::from_str(if folder { "public.folder" } else { "public.item" })];
                if let Some(any) = any { let _: () = msg_send![&*types, addObject: &*any]; }
            }
            let picker: Allocated<AnyObject> = msg_send![class!(UIDocumentPickerViewController), alloc];
            let picker: Retained<AnyObject> = msg_send![picker, initForOpeningContentTypes: &*types, asCopy: !folder];
            let _: () = msg_send![&*picker, setDelegate: self];
            if multiple { let _: () = msg_send![&*picker, setAllowsMultipleSelection: true]; }
            // Where the picker opens. The system shows another folder when it can't show this one.
            if let Some(directory) = options.directory.as_deref().and_then(|directory| directory.to_str()).filter(|directory| !directory.is_empty()) {
                let _: () = msg_send![&*picker, setDirectoryURL: &*NSURL::fileURLWithPath_isDirectory(&NSString::from_str(directory), true)];
            }
            let _: () = msg_send![&*screen, presentViewController: &*picker, animated: true, completion: std::ptr::null::<std::ffi::c_void>()];
        }
    }

    /// The picker closed with `urls` (an `NSArray<NSURL *>`), or none when it was cancelled.
    pub(super) fn file_picked(&self, urls: Option<&AnyObject>) {
        let Some(Picking { id, kind, multiple, file_name }) = self.ivars().file_dialog.borrow_mut().take() else { return };
        if multiple {
            // SAFETY: every object in the array is an `NSURL`, whose `path` is an `NSString` or nil.
            let paths = urls.map(|urls| unsafe {
                let count: usize = msg_send![urls, count];
                (0..count).filter_map(|i| {
                    let url: Retained<AnyObject> = msg_send![urls, objectAtIndex: i];
                    let path: Option<Retained<NSString>> = msg_send![&*url, path];
                    path.map(|path| PathBuf::from(path.to_string()))
                }).collect::<Vec<_>>()
            });
            self.with_surface(|surface| surface.file_dialog_answer(id, Ok(DialogAnswer::Many(paths))));
            self.pace(Event::Wake);
            return;
        }
        // SAFETY: the array's first object is an `NSURL`, whose `path` is an `NSString` or
        // nil. `startAccessingSecurityScopedResource` takes nothing and answers a `BOOL`.
        let path = urls.and_then(|urls| unsafe {
            let first: Option<Retained<AnyObject>> = msg_send![urls, firstObject];
            let first = first?;
            // A picked folder's URL (a Folder's and a Save's alike) carries the access the user
            // gave. It starts here and is never stopped: the app gets a plain path, with no
            // handle to end the access through and no word from it that it is done writing.
            // Each start is balanced only by the process exiting, when the system releases what
            // it holds for it. The same folder picked again starts again, and the starts nest.
            // NO is not a failure to report: it is also the answer for a folder that needs no
            // such access (one inside the app's own container), whose path works as it is.
            if kind != FileDialogKind::Open { let _: bool = msg_send![&*first, startAccessingSecurityScopedResource]; }
            let path: Option<Retained<NSString>> = msg_send![&*first, path];
            path.map(|path| PathBuf::from(path.to_string()))
        });
        let path = path.map(|path| if kind == FileDialogKind::Save { path.join(file_name) } else { path });
        self.with_surface(|surface| surface.file_dialog_answer(id, Ok(DialogAnswer::Single(path))));
        self.pace(Event::Wake);
    }

    /// The view controller whose screen the view is on: the first one up the responder chain.
    pub(super) fn view_controller(&self) -> Option<Retained<AnyObject>> {
        // SAFETY: `nextResponder` is a responder or nil, and `isKindOfClass:` answers a `BOOL`.
        unsafe {
            let mut next: Option<Retained<AnyObject>> = msg_send![self, nextResponder];
            while let Some(responder) = next {
                let found: bool = msg_send![&*responder, isKindOfClass: class!(UIViewController)];
                if found { return Some(responder); }
                next = msg_send![&*responder, nextResponder];
            }
            None
        }
    }
}
