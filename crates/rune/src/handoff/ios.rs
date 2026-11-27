#![allow(unsafe_code)]

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;

use super::{Activity, apple};

#[derive(Default)]
pub(crate) struct Advertised(Option<Retained<AnyObject>>);

impl Advertised {
    pub(crate) fn set(&mut self, activity: Option<&Activity>) {
        self.retire();
        let Some(activity) = activity else { return };
        let made = apple::make(activity);
        // SAFETY: `becomeCurrent` on the `NSUserActivity` just made, on the main thread.
        unsafe { let _: () = msg_send![&*made, becomeCurrent]; }
        self.0 = Some(made);
    }

    fn retire(&mut self) {
        // SAFETY: `invalidate` on an `NSUserActivity` this window made.
        if let Some(old) = self.0.take() { unsafe { let _: () = msg_send![&*old, invalidate]; } }
    }
}

impl Drop for Advertised {
    fn drop(&mut self) { self.retire(); }
}

/// # Safety
/// `activity` is an `NSUserActivity`.
pub(crate) unsafe fn continued(activity: &AnyObject) {
    // SAFETY: as the caller promises.
    let activity = unsafe { apple::read(activity) };
    if activity.kind() == apple::BROWSING_WEB || activity.kind() == crate::platform::ios::scenes::ACTIVITY { return; }
    if activity.kind() == crate::spotlight::SPOTLIGHT_ACTIVITY {
        if let Some(id) = activity.info(&crate::spotlight::apple::activity_identifier_key()) { crate::spotlight::opened(id.to_owned()); }
        return;
    }
    super::receive(activity);
}
