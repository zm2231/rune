#![allow(unsafe_code)]

use std::path::PathBuf;
use std::sync::OnceLock;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
use objc2::{ClassType, MainThreadMarker, class, msg_send, sel};
use objc2_foundation::{NSString, NSURL};

use super::ServiceInput;

const PLAIN_TEXT: &str = "public.utf8-plain-text";

extern "C" fn serve(_: &AnyObject, _: Sel, pasteboard: *mut AnyObject, user_data: *mut NSString, _error: *mut *mut NSString) {
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the service's `NSPasteboard` and the entry's `NSUserData`
        // string, either of which may be nil.
        let (Some(pasteboard), user_data) = (unsafe { pasteboard.as_ref() }, unsafe { user_data.as_ref() }) else { return };
        let input = read(pasteboard, user_data.map(NSString::to_string).unwrap_or_default());
        if !input.is_empty() { super::INBOX.receive(input); }
    }));
    if caught.is_err() { eprintln!("rune: services handler panicked"); }
}

fn read(pasteboard: &AnyObject, name: String) -> ServiceInput {
    // SAFETY: `pasteboard` is an `NSPasteboard`. `stringForType:` answers an `NSString` or nil;
    // `readObjectsForClasses:options:` an `NSArray` of `NSURL`s or nil, whose indexes below
    // `count` hold objects. The class array holds the `NSURL` class object, which is an object.
    autoreleasepool(|_| unsafe {
        let mut input = ServiceInput::new(name);
        let kind = NSString::from_str(PLAIN_TEXT);
        let text: Option<Retained<NSString>> = msg_send![pasteboard, stringForType: &*kind];
        let url_class = &*(NSURL::class() as *const AnyClass).cast::<AnyObject>();
        let classes: Retained<AnyObject> = msg_send![class!(NSArray), arrayWithObject: url_class];
        let urls: Option<Retained<AnyObject>> = msg_send![pasteboard, readObjectsForClasses: &*classes, options: None::<&AnyObject>];
        let urls: Vec<Retained<NSURL>> = urls.map(|urls| {
            let count: usize = msg_send![&*urls, count];
            (0..count).map(|i| msg_send![&*urls, objectAtIndex: i]).collect()
        }).unwrap_or_default();
        for url in urls {
            if url.isFileURL() {
                if let Some(path) = url.path() { input = input.with_file(PathBuf::from(path.to_string())); }
            } else if let Some(link) = url.absoluteString() {
                input = input.with_url(link.to_string());
            }
        }
        if let Some(text) = text.map(|t| t.to_string()).filter(|t| !t.is_empty()) { input = input.with_text(text); }
        input
    })
}

fn provider() -> Option<*mut AnyObject> {
    static PROVIDER: OnceLock<usize> = OnceLock::new();
    let made = *PROVIDER.get_or_init(|| {
        const NAME: &std::ffi::CStr = c"RuneServicesProvider";
        let class = AnyClass::get(NAME).or_else(|| {
            let mut builder = ClassBuilder::new(NAME, NSObject::class())?;
            // SAFETY: the function matches `runeService:userData:error:` (an object, a string
            // and an out-pointer to a string; void).
            unsafe { builder.add_method(sel!(runeService:userData:error:), serve as extern "C" fn(_, _, _, _, _)); }
            Some(builder.register())
        });
        let Some(class) = class else { return 0 };
        // SAFETY: `new` on an NSObject subclass; the +1 reference is kept for the process.
        let made: *mut AnyObject = unsafe { msg_send![class, new] };
        made as usize
    });
    (made != 0).then_some(made as *mut AnyObject)
}

fn current() -> Option<*mut AnyObject> {
    let mtm = MainThreadMarker::new()?;
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    // SAFETY: `servicesProvider` is an object or nil.
    let current: *mut AnyObject = unsafe { msg_send![&app, servicesProvider] };
    Some(current)
}

pub(super) fn handled_elsewhere() -> bool {
    current().is_some_and(|current| !current.is_null() && Some(current) != provider())
}

pub(super) fn install() -> bool {
    let (Some(mtm), Some(provider)) = (MainThreadMarker::new(), provider()) else { return false };
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    // SAFETY: the provider lives for the process and answers the service message.
    unsafe { let _: () = msg_send![&app, setServicesProvider: provider]; }
    true
}

pub(super) fn remove() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if current() != provider() { return; }
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    // SAFETY: clearing the app's services provider.
    unsafe { let _: () = msg_send![&app, setServicesProvider: None::<&AnyObject>]; }
}
