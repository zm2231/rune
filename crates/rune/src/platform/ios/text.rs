//! Text entry on iOS: the keyboard's copy of the focused field (`doc`), where the field's
//! characters are (`geometry`), what the field asks of the keyboard as UIKit counts it
//! (`traits`), what the field can undo for the system's undo gestures (`undo`), and the
//! `UITextInput` responder that answers UIKit from all four (`input`).

pub(crate) mod doc;
pub(crate) mod geometry;
pub(crate) mod handles;
#[cfg(target_os = "ios")]
pub(crate) mod input;
#[cfg(target_os = "ios")]
pub(crate) mod selection;
pub(crate) mod traits;
pub(crate) mod undo;
