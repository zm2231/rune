//! Text entry on iOS: the keyboard's copy of the focused field (`doc`), where the field's
//! characters are (`geometry`), what the field asks of the keyboard as UIKit counts it
//! (`traits`), and the `UITextInput` responder that answers UIKit from all three (`input`).

pub(crate) mod doc;
pub(crate) mod geometry;
#[cfg(target_os = "ios")]
pub(crate) mod input;
pub(crate) mod traits;
