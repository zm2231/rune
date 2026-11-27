//! `UNUserNotificationCenter`, on macOS and iOS. The system only shows notifications for a
//! bundled, signed app, so outside one (`cargo run`, tests) nothing posts. The first post asks the person for
//! permission; until they allow it, posts are dropped and each one is reported to
//! `on_post_error`. A `remove` while a post waits for that answer calls the post off. A notification's buttons and text box are a
//! category of its own (one per set of actions, registered with the center before the post); a
//! press comes back as the click, with the action and the words typed, without the app coming
//! forward (the actions don't ask for the foreground). Attachments are copied into the app's
//! `NSTemporaryDirectory()` first (inside its container on iOS), because `UNNotificationAttachment` moves the file it is given into the system's store.

#![allow(unsafe_code)]

use super::inflight::{Inflight, Settled};
use super::status::{Authorization, Failure, PostError, report};
use super::{Action, Click, Notification, attachment};
use block2::{Block, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, MutexGuard};

static INFLIGHT: LazyLock<Mutex<Inflight>> = LazyLock::new(Mutex::default);

fn inflight() -> MutexGuard<'static, Inflight> {
    INFLIGHT.lock().unwrap_or_else(|e| e.into_inner())
}

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {
    static UNNotificationAttachmentOptionsTypeHintKey: *mut AnyObject;
}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    fn NSTemporaryDirectory() -> *mut AnyObject;
    fn NSLog(format: *mut AnyObject, ...);
}

fn debug_log(message: &str) {
    if cfg!(debug_assertions) {
        // SAFETY: `%@` takes the one `NSString` argument; the pool owns both autoreleased strings.
        objc2::rc::autoreleasepool(|_| unsafe { NSLog(ns("rune-notify: %@"), ns(message)) });
    }
}

unsafe fn described(error: *mut AnyObject) -> String {
    if error.is_null() {
        return "no error given".into();
    }
    // SAFETY: caller passes a live `NSError` inside an autorelease pool.
    unsafe {
        let text = string(msg_send![error, localizedDescription]).unwrap_or_default();
        let domain = string(msg_send![error, domain]).unwrap_or_default();
        let code: isize = msg_send![error, code];
        format!("{text} ({domain} {code})")
    }
}

fn refused(id: &str, failure: Failure) {
    debug_log(&PostError { id: id.to_owned(), failure: failure.clone() }.to_string());
    report(id, failure);
}

fn staging_dir() -> std::path::PathBuf {
    // SAFETY: `NSTemporaryDirectory` returns an autoreleased `NSString`, read inside this pool.
    let dir = objc2::rc::autoreleasepool(|_| unsafe { string(NSTemporaryDirectory()) });
    dir.map_or_else(std::env::temp_dir, std::path::PathBuf::from).join("rune-notify")
}

// UNAuthorizationOptionBadge | Sound | Alert. Notifications never set `content.badge`; the badge
// option keeps macOS from suppressing `NSDockTile.badgeLabel` (`WindowHandle::set_taskbar`). The
// sound option only allows one: a notification plays it when `Notification::sound` sets it.
const AUTH_OPTIONS: usize = (1 << 0) | (1 << 1) | (1 << 2);
const PRESENT_LIST_BANNER: usize = (1 << 3) | (1 << 4);
const PRESENT_SOUND: usize = 1 << 1;

pub(super) fn bundled() -> bool {
    // SAFETY: `NSBundle.mainBundle`, `bundleIdentifier`, and `bundlePath` are thread-safe
    // Foundation getters; `string` checks for null.
    unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        let id: *mut AnyObject = msg_send![bundle, bundleIdentifier];
        let path: *mut AnyObject = msg_send![bundle, bundlePath];
        !id.is_null() && string(path).is_some_and(|p| p.ends_with(".app"))
    }
}

unsafe fn center() -> *mut AnyObject {
    // SAFETY: callers gate on `bundled()`, so `currentNotificationCenter` returns the app's
    // singleton `UNUserNotificationCenter` without throwing.
    unsafe { msg_send![class!(UNUserNotificationCenter), currentNotificationCenter] }
}

pub(super) unsafe fn ns(s: &str) -> *mut AnyObject {
    let c = std::ffi::CString::new(s.replace('\0', "")).unwrap_or_default();
    // SAFETY: `c` is a NUL-terminated UTF-8 C string that outlives the call; caller holds an
    // autorelease pool for the returned autoreleased `NSString`.
    unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
}

pub(super) unsafe fn string(s: *mut AnyObject) -> Option<String> {
    if s.is_null() {
        return None;
    }
    // SAFETY: caller guarantees non-null `s` is a live `NSString`; `UTF8String` returns a
    // NUL-terminated buffer valid for `s`'s lifetime (or null).
    let utf8: *const std::ffi::c_char = unsafe { msg_send![s, UTF8String] };
    // SAFETY: `utf8` is checked non-null and points to a NUL-terminated C string owned by `s`.
    (!utf8.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(utf8) }.to_string_lossy().into_owned())
}

pub(super) unsafe fn request_authorization(center: *mut AnyObject, done: &Block<dyn Fn(Bool, *mut AnyObject)>) {
    // SAFETY: caller passes a live `UNUserNotificationCenter` (or test stand-in) and a valid
    // ObjC completion block matching `requestAuthorizationWithOptions:completionHandler:`.
    unsafe {
        let _: () = msg_send![center, requestAuthorizationWithOptions: AUTH_OPTIONS, completionHandler: done];
    }
}

pub(super) fn post(n: Notification) {
    // SAFETY: `post` is gated on `bundled()`, so `center()` is the app's notification center.
    unsafe { post_to(center(), n) }
}

pub(super) unsafe fn post_to(center: *mut AnyObject, n: Notification) {
    let ticket = inflight().start(&n.id);
    let done = RcBlock::new(move |granted: Bool, error: *mut AnyObject| {
        if granted.as_bool() {
            // SAFETY: `center` is the live center that asked; `add` holds its own autorelease pool.
            unsafe { add(center, &n, ticket) };
            return;
        }
        if inflight().settle(&n.id, ticket) != Settled::Wanted {
            return;
        }
        let failure = if error.is_null() {
            Failure::Denied
        } else {
            // SAFETY: a non-nil `error` is the request's live `NSError`, read inside this pool.
            Failure::System(objc2::rc::autoreleasepool(|_| unsafe { described(error) }))
        };
        refused(&n.id, failure);
    });
    // SAFETY: `center` is live; `done` is a valid ObjC block the center copies.
    unsafe { request_authorization(center, &done) }
}

unsafe fn add(center: *mut AnyObject, n: &Notification, ticket: u64) {
    if !inflight().wanted(&n.id, ticket) {
        inflight().settle(&n.id, ticket);
        return;
    }
    let id = n.id.clone();
    let done = RcBlock::new(move |error: *mut AnyObject| {
        let settled = inflight().settle(&id, ticket);
        if settled == Settled::Withdrawn {
            // SAFETY: `center` outlives its own completion handler.
            unsafe { withdraw(center, &[&id]) };
        } else if settled == Settled::Wanted && !error.is_null() {
            // SAFETY: a non-nil `error` is the add's live `NSError`, read inside this pool.
            refused(&id, Failure::System(objc2::rc::autoreleasepool(|_| unsafe { described(error) })));
        }
    });
    // SAFETY: invoked inside an autorelease pool for a live center; every Foundation and
    // `UNUserNotificationCenter` message matches its selector and signature.
    objc2::rc::autoreleasepool(|_| unsafe {
        let (content, staged) = content(n);
        let request: *mut AnyObject = msg_send![
            class!(UNNotificationRequest),
            requestWithIdentifier: ns(&n.id),
            content: &*content,
            trigger: std::ptr::null::<AnyObject>()
        ];
        let wanted = inflight().wanted(&n.id, ticket);
        if wanted {
            let _: () = msg_send![center, addNotificationRequest: request, withCompletionHandler: &*done];
        } else {
            inflight().settle(&n.id, ticket);
            for copy in staged {
                std::fs::remove_file(copy).ok();
            }
        }
    });
}

pub(super) fn authorization(f: impl FnOnce(Authorization) + Send + 'static) {
    // SAFETY: `authorization` is gated on `bundled()`, so `center()` is the app's notification center.
    unsafe { authorization_from(center(), f) }
}

pub(super) unsafe fn authorization_from(center: *mut AnyObject, f: impl FnOnce(Authorization) + Send + 'static) {
    let f = std::cell::Cell::new(Some(f));
    let done = RcBlock::new(move |settings: *mut AnyObject| {
        let status: isize = if settings.is_null() {
            0
        } else {
            // SAFETY: a non-nil `settings` is the center's live `UNNotificationSettings`.
            unsafe { msg_send![settings, authorizationStatus] }
        };
        if let Some(f) = f.take() {
            f(status_authorization(status));
        }
    });
    // SAFETY: `center` is live; `&*done` matches `getNotificationSettingsWithCompletionHandler:`.
    unsafe {
        let _: () = msg_send![center, getNotificationSettingsWithCompletionHandler: &*done];
    }
}

fn status_authorization(status: isize) -> Authorization {
    match status {
        1 => Authorization::Denied,
        2 => Authorization::Authorized,
        3 => Authorization::Provisional,
        4 => Authorization::Ephemeral,
        _ => Authorization::NotDetermined,
    }
}

pub(super) unsafe fn content(n: &Notification) -> (Retained<AnyObject>, Vec<PathBuf>) {
    // SAFETY: caller holds an autorelease pool; each setter matches `UNMutableNotificationContent`.
    unsafe {
        let content: Retained<AnyObject> = msg_send![class!(UNMutableNotificationContent), new];
        let c = Retained::as_ptr(&content).cast_mut();
        let _: () = msg_send![c, setTitle: ns(&n.title)];
        let _: () = msg_send![c, setBody: ns(&n.body)];
        let _: () = msg_send![c, setUserInfo: dictionary(&n.info)];
        if let Some(category) = category(&n.actions) {
            let _: () = msg_send![c, setCategoryIdentifier: ns(&category)];
        }
        if let Some(thread) = &n.thread {
            let _: () = msg_send![c, setThreadIdentifier: ns(&thread.id)];
        }
        if n.sound {
            let sound: *mut AnyObject = msg_send![class!(UNNotificationSound), defaultSound];
            let _: () = msg_send![c, setSound: sound];
        }
        let (made, staged): (Vec<*mut AnyObject>, Vec<PathBuf>) = n.attachments.iter().enumerate().filter_map(|(i, a)| attach(&n.id, i, a)).unzip();
        if !made.is_empty() {
            let array: *mut AnyObject = msg_send![class!(NSArray), arrayWithObjects: made.as_ptr(), count: made.len()];
            let _: () = msg_send![c, setAttachments: array];
        }
        (content, staged)
    }
}

unsafe fn attach(id: &str, i: usize, a: &attachment::Attachment) -> Option<(*mut AnyObject, PathBuf)> {
    let copy = match attachment::stage(a, &staging_dir()) {
        Ok(copy) => copy,
        Err(e) => {
            debug_log(&format!("attachment left out: {e}"));
            return None;
        }
    };
    let mut error: *mut AnyObject = std::ptr::null_mut();
    // SAFETY: caller holds an autorelease pool; `error` is a valid out pointer for
    // `attachmentWithIdentifier:URL:options:error:`.
    let made: *mut AnyObject = unsafe {
        let url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: ns(&copy.to_string_lossy())];
        let options: *mut AnyObject = msg_send![
            class!(NSDictionary),
            dictionaryWithObject: ns(a.uniform_type()),
            forKey: UNNotificationAttachmentOptionsTypeHintKey
        ];
        msg_send![
            class!(UNNotificationAttachment),
            attachmentWithIdentifier: ns(&format!("rune.{i}")),
            URL: url,
            options: options,
            error: &mut error
        ]
    };
    if made.is_null() {
        // SAFETY: a nil attachment comes with its `NSError`, or none.
        debug_log(&format!("{} refused as an attachment: {}", a.path().display(), unsafe { described(error) }));
        std::fs::remove_file(&copy).ok();
        return None;
    }
    if cfg!(debug_assertions) {
        let size = std::fs::metadata(&copy).map_or(0, |m| m.len());
        debug_log(&format!("{id} attachment rune.{i}: {} ({size} bytes, {}) staged at {}", a.path().display(), a.uniform_type(), copy.display()));
    }
    Some((made, copy))
}

thread_local! {
    /// The categories registered this run, by identifier: the center keeps one set, replaced whole.
    static CATEGORIES: std::cell::RefCell<Vec<(String, Retained<AnyObject>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The category for `actions` (made and registered the first time): None without actions.
unsafe fn category(actions: &[Action]) -> Option<String> {
    if actions.is_empty() {
        return None;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{actions:?}").hash(&mut h);
    let id = format!("rune.{:016x}", h.finish());
    if CATEGORIES.with_borrow(|c| c.iter().any(|(k, _)| *k == id)) {
        return Some(id);
    }
    // SAFETY: called from `add` inside an autorelease pool for a bundled app; `made` and `all`
    // stay live for `arrayWithObjects:count:` / `setWithObjects:count:`, and `CATEGORIES` retains
    // every `UNNotificationCategory` object.
    unsafe {
        let made: Vec<*mut AnyObject> = actions.iter().take(10).map(|a| match a {
            // Options 0: no foreground, no authentication, not destructive.
            Action::Button { id, label } => msg_send![class!(UNNotificationAction), actionWithIdentifier: ns(id), title: ns(label), options: 0usize],
            Action::Reply { id, placeholder, label } => msg_send![class!(UNTextInputNotificationAction), actionWithIdentifier: ns(id), title: ns(label), options: 0usize, textInputButtonTitle: ns(label), textInputPlaceholder: ns(placeholder)],
        }).collect();
        let array: *mut AnyObject = msg_send![class!(NSArray), arrayWithObjects: made.as_ptr(), count: made.len()];
        let empty: *mut AnyObject = msg_send![class!(NSArray), array];
        let category: Retained<AnyObject> = msg_send![class!(UNNotificationCategory), categoryWithIdentifier: ns(&id), actions: array, intentIdentifiers: empty, options: 0usize];
        CATEGORIES.with_borrow_mut(|c| c.push((id.clone(), category)));
        let all: Vec<*mut AnyObject> = CATEGORIES.with_borrow(|c| c.iter().map(|(_, k)| Retained::as_ptr(k).cast_mut()).collect());
        let set: *mut AnyObject = msg_send![class!(NSSet), setWithObjects: all.as_ptr(), count: all.len()];
        let _: () = msg_send![center(), setNotificationCategories: set];
    }
    Some(id)
}

pub(super) unsafe fn dictionary(info: &[(String, String)]) -> *mut AnyObject {
    // SAFETY: caller holds an autorelease pool; `keys` and `values` both hold `info.len()` live
    // `NSString` pointers for `dictionaryWithObjects:forKeys:count:`.
    unsafe {
        let keys: Vec<*mut AnyObject> = info.iter().map(|(k, _)| ns(k)).collect();
        let values: Vec<*mut AnyObject> = info.iter().map(|(_, v)| ns(v)).collect();
        msg_send![class!(NSDictionary), dictionaryWithObjects: values.as_ptr(), forKeys: keys.as_ptr(), count: info.len()]
    }
}

pub(super) fn remove(ids: &[&str]) {
    // SAFETY: `remove` is gated on `bundled()`, so `center()` is the app's notification center.
    unsafe { remove_from(center(), ids) }
}

pub(super) unsafe fn remove_from(center: *mut AnyObject, ids: &[&str]) {
    let mut posts = inflight();
    for id in ids {
        posts.withdraw(id);
    }
    // SAFETY: `center` is live.
    unsafe { withdraw(center, ids) };
}

unsafe fn withdraw(center: *mut AnyObject, ids: &[&str]) {
    // SAFETY: wrapped in an autorelease pool; `items` holds `ids.len()` live `NSString` pointers
    // for `arrayWithObjects:count:`, and `center` is live.
    objc2::rc::autoreleasepool(|_| unsafe {
        let items: Vec<*mut AnyObject> = ids.iter().map(|id| ns(id)).collect();
        let array: *mut AnyObject = msg_send![class!(NSArray), arrayWithObjects: items.as_ptr(), count: items.len()];
        let _: () = msg_send![center, removeDeliveredNotificationsWithIdentifiers: array];
        let _: () = msg_send![center, removePendingNotificationRequestsWithIdentifiers: array];
    });
}

/// The click's notification id and string user info, from a `UNNotificationResponse`.
pub(super) unsafe fn click(response: *mut AnyObject) -> Click {
    // SAFETY: caller passes a live `UNNotificationResponse` inside an autorelease pool; keys and
    // values are checked with `isKindOfClass:` before `string`, and `userText` is read only when
    // `response` is a `UNTextInputNotificationResponse`.
    unsafe {
        let notification: *mut AnyObject = msg_send![response, notification];
        let request: *mut AnyObject = msg_send![notification, request];
        let id = string(msg_send![request, identifier]).unwrap_or_default();
        let content: *mut AnyObject = msg_send![request, content];
        let info: *mut AnyObject = msg_send![content, userInfo];
        let mut pairs = Vec::new();
        if !info.is_null() {
            let keys: *mut AnyObject = msg_send![info, allKeys];
            let n: usize = msg_send![keys, count];
            for i in 0..n {
                let key: *mut AnyObject = msg_send![keys, objectAtIndex: i];
                let value: *mut AnyObject = msg_send![info, objectForKey: key];
                let is_string = |o: *mut AnyObject| -> bool { msg_send![o, isKindOfClass: class!(NSString)] };
                if is_string(key) && !value.is_null() && is_string(value) {
                    pairs.push((string(key).unwrap_or_default(), string(value).unwrap_or_default()));
                }
            }
            pairs.sort();
        }
        // A button: its identifier (not the notification's own default or dismiss action), and a
        // text box's words.
        let action = string(msg_send![response, actionIdentifier]).filter(|a| !a.starts_with("com.apple.UNNotification"));
        if let Some(action) = action {
            let typed: bool = msg_send![response, isKindOfClass: class!(UNTextInputNotificationResponse)];
            if typed && let Some(text) = string(msg_send![response, userText]) {
                pairs.push((format!("{}{action}", super::INPUT), text));
            }
            pairs.push((super::ACTION.to_owned(), action));
        }
        Click { id, info: pairs }
    }
}

pub(super) extern "C" fn did_receive(_this: *mut AnyObject, _cmd: Sel, _center: *mut AnyObject, response: *mut AnyObject, done: &Block<dyn Fn()>) {
    // SAFETY: `UNUserNotificationCenter` passes a live `UNNotificationResponse` for this callback,
    // and `click` runs inside this autorelease pool.
    let click = objc2::rc::autoreleasepool(|_| unsafe { click(response) });
    super::deliver(click);
    done.call(());
}

pub(super) extern "C" fn will_present(_this: *mut AnyObject, _cmd: Sel, _center: *mut AnyObject, notification: *mut AnyObject, done: &Block<dyn Fn(usize)>) {
    // SAFETY: `UNUserNotificationCenter` passes a live `UNNotification`.
    let sound: *mut AnyObject = unsafe {
        let request: *mut AnyObject = msg_send![notification, request];
        let content: *mut AnyObject = msg_send![request, content];
        msg_send![content, sound]
    };
    done.call((if sound.is_null() { PRESENT_LIST_BANNER } else { PRESENT_LIST_BANNER | PRESENT_SOUND },));
}

pub(super) fn delegate_class() -> &'static AnyClass {
    const NAME: &std::ffi::CStr = c"RuneMacNotificationDelegate";
    if let Some(c) = AnyClass::get(NAME) {
        return c;
    }
    let mut b = ClassBuilder::new(NAME, class!(NSObject)).expect("delegate class");
    if let Some(p) = AnyProtocol::get(c"UNUserNotificationCenterDelegate") {
        b.add_protocol(p);
    }
    // SAFETY: both `did_receive` and `will_present` match the Objective-C ABI and parameter types
    // of `UNUserNotificationCenterDelegate`'s selectors.
    unsafe {
        b.add_method(
            sel!(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:),
            did_receive as extern "C" fn(_, _, _, _, _),
        );
        b.add_method(
            sel!(userNotificationCenter:willPresentNotification:withCompletionHandler:),
            will_present as extern "C" fn(_, _, _, _, _),
        );
    }
    b.register()
}

pub(super) fn install_delegate() {
    static DELEGATE: std::sync::Once = std::sync::Once::new();
    // SAFETY: `install_delegate` is gated on `bundled()`; `new` allocates a retained delegate
    // kept alive for the process because the center holds its delegate weakly.
    DELEGATE.call_once(|| unsafe {
        // The center holds its delegate weakly: this one lives as long as the process.
        let delegate: *mut AnyObject = msg_send![delegate_class(), new];
        let _: () = msg_send![center(), setDelegate: delegate];
    });
}

#[cfg(target_os = "ios")]
pub(super) fn app_focused() -> bool {
    let Some(app) = AnyClass::get(c"UIApplication") else { return false };
    // SAFETY: `sharedApplication` and `applicationState` on the main thread, as `app_focused`
    // documents; 0 is `UIApplicationStateActive`.
    unsafe {
        let app: *mut AnyObject = msg_send![app, sharedApplication];
        !app.is_null() && { let state: isize = msg_send![app, applicationState]; state == 0 }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn app_focused() -> bool {
    // SAFETY: standard AppKit `NSApplication` queries (`sharedApplication`, `isActive`, `keyWindow`)
    // on the UI thread; `key` is only tested for null.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let active: bool = msg_send![app, isActive];
        let key: *mut AnyObject = msg_send![app, keyWindow];
        active && !key.is_null()
    }
}

#[cfg(target_os = "macos")]
pub(super) fn bring_to_front() {
    // SAFETY: called on the AppKit main thread (`poll`); `objectAtIndex: 0` is guarded by `n > 0`,
    // and window messages are sent only when `w` is non-null.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let mut w: *mut AnyObject = msg_send![app, mainWindow];
        if w.is_null() {
            let windows: *mut AnyObject = msg_send![app, windows];
            let n: usize = msg_send![windows, count];
            if n > 0 {
                w = msg_send![windows, objectAtIndex: 0usize];
            }
        }
        if !w.is_null() {
            let _: () = msg_send![w, deminiaturize: std::ptr::null::<AnyObject>()];
            let _: () = msg_send![w, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
        }
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}

#[cfg(test)]
#[path = "apple_tests.rs"]
mod tests;
