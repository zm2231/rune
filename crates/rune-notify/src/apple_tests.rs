#![cfg(target_os = "macos")]

use super::*;
use crate::status::error_handler;
use std::cell::{Cell, RefCell};
use std::sync::{Arc, mpsc};

type AuthDone = RcBlock<dyn Fn(Bool, *mut AnyObject)>;
type AddDone = RcBlock<dyn Fn(*mut AnyObject)>;

thread_local! {
    static ASKED: RefCell<Vec<AuthDone>> = const { RefCell::new(Vec::new()) };
    static ADDED: RefCell<Vec<(String, AddDone)>> = const { RefCell::new(Vec::new()) };
    static REMOVED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static STATUS: Cell<isize> = const { Cell::new(0) };
}

extern "C" fn request_auth(_: *mut AnyObject, _: Sel, _: usize, done: &Block<dyn Fn(Bool, *mut AnyObject)>) {
    ASKED.with_borrow_mut(|a| a.push(done.copy()));
}

extern "C" fn add_request(_: *mut AnyObject, _: Sel, request: *mut AnyObject, done: &Block<dyn Fn(*mut AnyObject)>) {
    // SAFETY: `add` passes a live `UNNotificationRequest`.
    let id = unsafe { string(msg_send![request, identifier]) }.unwrap_or_default();
    ADDED.with_borrow_mut(|a| a.push((id, done.copy())));
}

extern "C" fn remove_ids(_: *mut AnyObject, _: Sel, ids: *mut AnyObject) {
    // SAFETY: `withdraw` passes a live `NSArray` of `NSString`s.
    unsafe {
        let n: usize = msg_send![ids, count];
        for i in 0..n {
            let id = string(msg_send![ids, objectAtIndex: i]).unwrap_or_default();
            REMOVED.with_borrow_mut(|r| r.push(id));
        }
    }
}

extern "C" fn settings(_: *mut AnyObject, _: Sel, done: &Block<dyn Fn(*mut AnyObject)>) {
    // SAFETY: the stand-in settings class is a registered `NSObject` subclass.
    let settings: Retained<AnyObject> = unsafe { msg_send![settings_class(), new] };
    done.call((Retained::as_ptr(&settings).cast_mut(),));
}

extern "C" fn status(_: *mut AnyObject, _: Sel) -> isize {
    STATUS.get()
}

fn settings_class() -> &'static AnyClass {
    AnyClass::get(c"RuneNotifyFakeSettings").unwrap_or_else(|| {
        let mut b = ClassBuilder::new(c"RuneNotifyFakeSettings", class!(NSObject)).unwrap();
        // SAFETY: `status` matches the ABI of the `authorizationStatus` getter.
        unsafe { b.add_method(sel!(authorizationStatus), status as extern "C" fn(_, _) -> _) };
        b.register()
    })
}

fn fake_center() -> Retained<AnyObject> {
    let class = AnyClass::get(c"RuneNotifyFakeCenter").unwrap_or_else(|| {
        let mut b = ClassBuilder::new(c"RuneNotifyFakeCenter", class!(NSObject)).unwrap();
        // SAFETY: each function matches the ABI and argument types of the selector it is added for.
        unsafe {
            b.add_method(sel!(requestAuthorizationWithOptions:completionHandler:), request_auth as extern "C" fn(_, _, _, _));
            b.add_method(sel!(addNotificationRequest:withCompletionHandler:), add_request as extern "C" fn(_, _, _, _));
            b.add_method(sel!(removeDeliveredNotificationsWithIdentifiers:), remove_ids as extern "C" fn(_, _, _));
            b.add_method(sel!(removePendingNotificationRequestsWithIdentifiers:), remove_ids as extern "C" fn(_, _, _));
            b.add_method(sel!(getNotificationSettingsWithCompletionHandler:), settings as extern "C" fn(_, _, _));
        }
        b.register()
    });
    // SAFETY: `class` is a registered `NSObject` subclass.
    unsafe { msg_send![class, new] }
}

fn answer(granted: bool, error: *mut AnyObject) {
    let done = ASKED.with_borrow_mut(|a| a.remove(0));
    objc2::rc::autoreleasepool(|_| done.call((Bool::new(granted), error)));
}

fn added() -> Vec<String> {
    ADDED.with_borrow(|a| a.iter().map(|(id, _)| id.clone()).collect())
}

fn finish_adds() {
    for (_, done) in ADDED.with_borrow_mut(std::mem::take) {
        done.call((std::ptr::null_mut(),));
    }
}

fn post(center: &Retained<AnyObject>, id: &str) {
    // SAFETY: the stand-in center lives for the test.
    unsafe { post_to(Retained::as_ptr(center).cast_mut(), Notification::new(id, "Question")) };
}

fn remove(center: &Retained<AnyObject>, id: &str) {
    // SAFETY: the stand-in center lives for the test.
    unsafe { remove_from(Retained::as_ptr(center).cast_mut(), &[id]) };
}

#[test]
fn a_remove_while_permission_is_asked_keeps_the_post_from_showing() {
    let center = fake_center();
    post(&center, "asked-1");
    remove(&center, "asked-1");
    answer(true, std::ptr::null_mut());
    assert!(added().is_empty(), "the withdrawn post is not added");

    post(&center, "asked-1");
    answer(true, std::ptr::null_mut());
    assert_eq!(added(), ["asked-1"], "posting the id again still shows");
    finish_adds();
}

#[test]
fn a_remove_racing_the_add_takes_it_out_once_the_add_lands() {
    let center = fake_center();
    post(&center, "racing-1");
    answer(true, std::ptr::null_mut());
    assert_eq!(added(), ["racing-1"]);
    remove(&center, "racing-1");
    REMOVED.with_borrow_mut(Vec::clear);
    finish_adds();
    assert_eq!(REMOVED.with_borrow(Clone::clone), ["racing-1", "racing-1"], "delivered and pending, again");
}

#[test]
fn a_denied_or_failed_permission_request_reaches_on_post_error() {
    let got = Arc::new(Mutex::new(Vec::new()));
    let sink = got.clone();
    crate::on_post_error(move |e| {
        if e.id.starts_with("refused-") {
            sink.lock().unwrap().push(e);
        }
    });
    let center = fake_center();
    post(&center, "refused-1");
    answer(false, std::ptr::null_mut());
    post(&center, "refused-2");
    // SAFETY: inside an autorelease pool; `errorWithDomain:code:userInfo:` takes a nil user info.
    objc2::rc::autoreleasepool(|_| unsafe {
        let error: *mut AnyObject = msg_send![class!(NSError), errorWithDomain: ns("UNErrorDomain"), code: 1isize, userInfo: std::ptr::null::<AnyObject>()];
        answer(false, error);
    });
    *error_handler().lock().unwrap() = None;

    let got = got.lock().unwrap();
    assert!(added().is_empty());
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], PostError { id: "refused-1".into(), failure: Failure::Denied });
    assert_eq!(got[1].id, "refused-2");
    assert!(matches!(&got[1].failure, Failure::System(why) if why.contains("UNErrorDomain 1")), "{:?}", got[1]);
}

#[test]
fn authorization_reads_the_centers_status() {
    let center = fake_center();
    for (status, want) in [(1, Authorization::Denied), (3, Authorization::Provisional)] {
        STATUS.set(status);
        let (to, from) = mpsc::channel();
        // SAFETY: the stand-in center lives for the test.
        unsafe { authorization_from(Retained::as_ptr(&center).cast_mut(), move |a| to.send(a).unwrap()) };
        assert_eq!(from.try_recv(), Ok(want));
    }
}
