#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::{MainThreadMarker, ffi, msg_send, sel};
use objc2_app_kit::NSApplication;

use super::{Activity, apple};

static ACTIVE: AtomicBool = AtomicBool::new(false);

thread_local! {
    static DELEGATE: Cell<*const AnyClass> = const { Cell::new(std::ptr::null()) };
    static WINDOW_CLASSES: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

static INFO_KEY: u8 = 0;

unsafe extern "C" {
    fn class_addMethod(class: *const AnyClass, selector: Sel, implementation: *const c_void, types: *const c_char) -> Bool;
}

fn set_info(window: &AnyObject, info: Option<&AnyObject>) {
    let info = info.map_or(std::ptr::null_mut(), |info| (info as *const AnyObject).cast_mut());
    // SAFETY: the window retains the dictionary under Rune's own key until it is replaced or the
    // window is deallocated.
    unsafe { ffi::objc_setAssociatedObject((window as *const AnyObject).cast_mut(), (&raw const INFO_KEY).cast(), info, ffi::OBJC_ASSOCIATION_RETAIN_NONATOMIC) };
}

fn add(class: &AnyClass, selector: Sel, imp: *const c_void, types: &CStr) -> bool {
    // SAFETY: each caller passes a function with the selector's ABI as `types` spells it, and
    // adding a method the class lacks leaves its other methods and instances alone.
    unsafe { class_addMethod(class, selector, imp, types.as_ptr()) }.as_bool()
}

fn types(apple_silicon: &'static CStr, intel: &'static CStr) -> &'static CStr {
    if cfg!(target_arch = "aarch64") { apple_silicon } else { intel }
}

extern "C" fn will_continue(_: *mut AnyObject, _: Sel, _: *mut AnyObject, _: *mut AnyObject) -> Bool {
    Bool::new(ACTIVE.load(Ordering::Relaxed))
}

extern "C" fn continue_activity(_: *mut AnyObject, _: Sel, _: *mut AnyObject, activity: *mut AnyObject, _: *mut c_void) -> Bool {
    if !ACTIVE.load(Ordering::Relaxed) { return Bool::NO; }
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the `NSUserActivity` to continue.
        let Some(activity) = (unsafe { activity.as_ref() }) else { return false };
        // SAFETY: as above.
        let activity = unsafe { apple::read(activity) };
        if activity.kind() == apple::BROWSING_WEB {
            let Some(page) = activity.web_url() else { return false };
            crate::open_url::incoming::receive(page.to_owned());
        } else if activity.kind() == crate::spotlight::SPOTLIGHT_ACTIVITY {
            let Some(id) = activity.info(&crate::spotlight::apple::activity_identifier_key()) else { return false };
            crate::spotlight::opened(id.to_owned());
        } else {
            super::receive(activity);
        }
        true
    }));
    Bool::new(caught.unwrap_or_else(|_| { eprintln!("rune: continue-activity handler panicked"); false }))
}

fn delegate_class(mtm: MainThreadMarker) -> Option<&'static AnyClass> {
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: the app's delegate is an object or nil.
    let delegate: *mut AnyObject = unsafe { msg_send![&app, delegate] };
    // SAFETY: a live delegate's class lives for the process.
    Some(unsafe { delegate.as_ref() }?.class())
}

pub(super) fn handled_elsewhere() -> bool {
    let Some(class) = MainThreadMarker::new().and_then(delegate_class) else { return false };
    !DELEGATE.with(|d| std::ptr::eq(d.get(), class)) && class.instance_method(sel!(application:continueUserActivity:restorationHandler:)).is_some()
}

pub(super) fn install() -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let Some(class) = delegate_class(mtm) else { return false };
    if !DELEGATE.with(|d| std::ptr::eq(d.get(), class)) {
        let will = will_continue as extern "C" fn(_, _, _, _) -> _;
        let cont = continue_activity as extern "C" fn(_, _, _, _, _) -> _;
        let added = add(class, sel!(application:willContinueUserActivityWithType:), will as *const c_void, types(c"B@:@@", c"c@:@@"))
            && add(class, sel!(application:continueUserActivity:restorationHandler:), cont as *const c_void, types(c"B@:@@@?", c"c@:@@@?"));
        if !added { return false; }
        DELEGATE.with(|d| d.set(class));
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: re-setting the same delegate makes AppKit look again at the methods it answers.
        unsafe {
            let delegate: *mut AnyObject = msg_send![&app, delegate];
            let _: () = msg_send![&app, setDelegate: delegate];
        }
    }
    ACTIVE.store(true, Ordering::Relaxed);
    true
}

pub(super) fn remove() { ACTIVE.store(false, Ordering::Relaxed); }

extern "C" fn update_state(window: *mut AnyObject, _: Sel, activity: *mut AnyObject) {
    // SAFETY: AppKit passes the window's own `NSUserActivity`, which takes the dictionary Rune
    // made for this window.
    let Some(activity) = (unsafe { activity.as_ref() }) else { return };
    // SAFETY: the dictionary `set_info` attached to this window, or null, kept alive by it.
    let Some(info) = (unsafe { ffi::objc_getAssociatedObject(window, (&raw const INFO_KEY).cast()).as_ref() }) else { return };
    // SAFETY: as above.
    unsafe { let _: () = msg_send![activity, addUserInfoEntriesFromDictionary: info]; }
}

fn hook_window_class(window: &AnyObject) {
    let class = window.class();
    let key = class as *const AnyClass as usize;
    if WINDOW_CLASSES.with(|c| c.borrow().contains(&key)) { return; }
    let imp = update_state as extern "C" fn(_, _, _);
    if !add(class, sel!(updateUserActivityState:), imp as *const c_void, c"v@:@") {
        eprintln!("rune: the window class already answers updateUserActivityState:; its Handoff info may be cleared");
    }
    WINDOW_CLASSES.with(|c| c.borrow_mut().push(key));
}

pub(crate) fn advertise(window: &winit::window::Window, activity: Option<&Activity>) {
    let Some(ns_window) = crate::platform::macos::ns_window(window) else { return };
    let ns_window: &AnyObject = &ns_window;
    // SAFETY: `userActivity` is an `NSUserActivity` or nil, which `invalidate` retires.
    unsafe {
        let old: Option<Retained<AnyObject>> = msg_send![ns_window, userActivity];
        if let Some(old) = old { let _: () = msg_send![&*old, invalidate]; }
    }
    let Some(activity) = activity else {
        set_info(ns_window, None);
        // SAFETY: clearing the window's activity.
        unsafe { let _: () = msg_send![ns_window, setUserActivity: None::<&AnyObject>]; }
        return;
    };
    hook_window_class(ns_window);
    set_info(ns_window, Some(&apple::info_dictionary(activity)));
    let made = apple::make(activity);
    // SAFETY: the window keeps its `userActivity`; `isMainWindow` is a `BOOL`, and a main
    // window's activity is the one to make current.
    unsafe {
        let _: () = msg_send![ns_window, setUserActivity: &*made];
        let main: Bool = msg_send![ns_window, isMainWindow];
        if main.as_bool() { let _: () = msg_send![&*made, becomeCurrent]; }
    }
}
