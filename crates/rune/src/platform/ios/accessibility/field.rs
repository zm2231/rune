//! A text field's element. VoiceOver says "text field" (or "secure text field") for the traits a
//! `UITextField` has, read from one that is never placed in a view, never first responder and
//! never an element. While the field is edited, and is not a password, the element answers
//! UIKit's text input protocols through the view's `RuneTextInput`, so the reader reads and
//! moves through the same copy of the text the keyboard edits.

use std::cell::Cell;
use std::ffi::CStr;
use std::ptr;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, AnyProtocol, Bool, Sel};
use objc2::{MainThreadMarker, class, ffi, msg_send};

use crate::runtime::HashSet;

const PROTOCOLS: [&CStr; 3] = [c"UITextInput", c"UIKeyInput", c"UITextInputTraits"];

thread_local! {
    static TRAITS: Cell<[u64; 2]> = const { Cell::new([0, 0]) };
    static FORWARDED: HashSet<Sel> = forwarded();
}

/// The traits of a `UITextField`, plain or secure. UIKit fills them in once its accessibility has
/// loaded, which a client asking for elements does: until then they read as none, and are read
/// again the next time.
pub(super) fn field_traits(_mtm: MainThreadMarker, secure: bool) -> u64 {
    let at = usize::from(secure);
    TRAITS.with(|cache| {
        let mut all = cache.get();
        if all[at] == 0 {
            all[at] = read(secure);
            cache.set(all);
        }
        all[at]
    })
}

fn read(secure: bool) -> u64 {
    // SAFETY: on the main thread, `UITextField`'s designated initializer and properties of
    // these types; the field is dropped here, outside every view.
    unsafe {
        let field: Allocated<AnyObject> = msg_send![class!(UITextField), alloc];
        let field: Retained<AnyObject> = msg_send![field, init];
        let _: () = msg_send![&*field, setHidden: true];
        let _: () = msg_send![&*field, setIsAccessibilityElement: false];
        let _: () = msg_send![&*field, setSecureTextEntry: secure];
        msg_send![&*field, accessibilityTraits]
    }
}

fn forwarded() -> HashSet<Sel> {
    let mut all = HashSet::default();
    for protocol in PROTOCOLS.iter().filter_map(|name| AnyProtocol::get(name)) {
        for required in [true, false] {
            let mut count = 0;
            // SAFETY: a protocol the runtime knows; the list it returns holds `count`
            // descriptions and is freed with `free`.
            unsafe {
                let list = ffi::protocol_copyMethodDescriptionList(protocol, Bool::new(required), Bool::YES, &mut count);
                if list.is_null() { continue }
                all.extend(std::slice::from_raw_parts(list, count as usize).iter().filter_map(|description| description.name));
                ffi::free(list.cast());
            }
        }
    }
    all
}

pub(super) fn forwards(sel: Sel) -> bool {
    FORWARDED.with(|all| all.contains(&sel))
}

pub(super) fn is_text_protocol(protocol: &AnyProtocol) -> bool {
    PROTOCOLS.iter().filter_map(|name| AnyProtocol::get(name)).any(|known| ptr::eq(known, protocol))
}
