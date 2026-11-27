#![allow(unsafe_code)]

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use objc2_foundation::{NSString, NSURL};

use super::Activity;

pub(crate) const BROWSING_WEB: &str = "NSUserActivityTypeBrowsingWeb";

fn string(object: Option<Retained<AnyObject>>) -> Option<String> {
    let object = object?;
    // SAFETY: `isKindOfClass:` is an `NSObject` method; a string's description is itself.
    unsafe {
        let is_string: Bool = msg_send![&*object, isKindOfClass: class!(NSString)];
        if !is_string.as_bool() { return None; }
        let string: Retained<NSString> = Retained::cast_unchecked(object);
        Some(string.to_string())
    }
}

/// # Safety
/// `activity` is an `NSUserActivity`.
pub(crate) unsafe fn read(activity: &AnyObject) -> Activity {
    // SAFETY: `activityType` is an `NSString`, `title` an `NSString` or nil, `webpageURL` an
    // `NSURL` or nil and `userInfo` an `NSDictionary` or nil, whose `allKeys` is an `NSArray`
    // with objects below `count`.
    unsafe {
        let kind: Option<Retained<NSString>> = msg_send![activity, activityType];
        let mut out = Activity::new(kind.map(|k| k.to_string()).unwrap_or_default());
        let title: Option<Retained<NSString>> = msg_send![activity, title];
        if let Some(title) = title { out = out.with_title(title.to_string()); }
        let page: Option<Retained<NSURL>> = msg_send![activity, webpageURL];
        if let Some(page) = page.and_then(|p| p.absoluteString()) { out = out.with_web_url(page.to_string()); }
        let info: Option<Retained<AnyObject>> = msg_send![activity, userInfo];
        if let Some(info) = info {
            let keys: Retained<AnyObject> = msg_send![&*info, allKeys];
            let count: usize = msg_send![&*keys, count];
            for i in 0..count {
                let key: Option<Retained<AnyObject>> = msg_send![&*keys, objectAtIndex: i];
                let value: Option<Retained<AnyObject>> = key.as_deref().and_then(|key| msg_send![&*info, objectForKey: key]);
                if let (Some(key), Some(value)) = (string(key), string(value)) { out = out.with_info(key, value); }
            }
        }
        out
    }
}

pub(crate) fn info_dictionary(activity: &Activity) -> Retained<AnyObject> {
    // SAFETY: an `NSMutableDictionary` of `NSString` keys and values.
    unsafe {
        let dict: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), dictionary];
        for (key, value) in activity.infos() {
            let (key, value) = (NSString::from_str(key), NSString::from_str(value));
            let _: () = msg_send![&*dict, setObject: &*value, forKey: &*key];
        }
        dict
    }
}

pub(crate) fn make(activity: &Activity) -> Retained<AnyObject> {
    let kind = NSString::from_str(activity.kind());
    // SAFETY: `initWithActivityType:` makes an `NSUserActivity`; its `title` takes an
    // `NSString`, `webpageURL` an `NSURL` (only http and https are allowed), `userInfo` an
    // `NSDictionary` of plist values, and `eligibleForHandoff` a `BOOL`.
    unsafe {
        let made: Allocated<AnyObject> = msg_send![class!(NSUserActivity), alloc];
        let made: Retained<AnyObject> = msg_send![made, initWithActivityType: &*kind];
        if let Some(title) = activity.title() { let _: () = msg_send![&*made, setTitle: &*NSString::from_str(title)]; }
        if let Some(url) = activity.web_url().filter(|u| u.starts_with("https://") || u.starts_with("http://"))
            && let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
            let _: () = msg_send![&*made, setWebpageURL: &*url];
        }
        let _: () = msg_send![&*made, setUserInfo: &*info_dictionary(activity)];
        let _: () = msg_send![&*made, setEligibleForHandoff: Bool::YES];
        made
    }
}
