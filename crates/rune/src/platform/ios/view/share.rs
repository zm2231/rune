#![allow(unsafe_code)]

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSString, NSURL};

use super::RuneView;
use crate::platform::objc_boundary::guarded;
use crate::share::{ShareError, Shown};

impl RuneView {
    pub(super) fn show_share(&self, shown: Shown) {
        let Shown { share, anchor, reply } = shown;
        let Some(screen) = self.view_controller() else {
            reply.send(Err(ShareError::Failed("the view is on no screen".into())));
            return;
        };
        // SAFETY: an array of `NSString` and `NSURL` items makes the controller, with no
        // activities of the app's own. Its completion handler is a block of an activity type
        // (an `NSString` or nil), a `BOOL`, the returned items and an error (each an object or
        // nil), which it copies and calls on the main thread. Its popover controller, nil on
        // an iPhone, takes a source view and a `CGRect` in it, and an arrow direction mask; a
        // view controller presents another with no completion block.
        unsafe {
            let items: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
            let add = |item: &AnyObject| { let _: () = msg_send![&*items, addObject: item]; };
            if let Some(text) = share.text.as_deref().filter(|t| !t.is_empty()) { add(&NSString::from_str(text)); }
            if let Some(url) = share.url.as_deref().and_then(|url| NSURL::URLWithString(&NSString::from_str(url))) { add(&url); }
            for file in share.files.iter().filter_map(|f| f.to_str()) { add(&NSURL::fileURLWithPath(&NSString::from_str(file))); }
            let controller: Allocated<AnyObject> = msg_send![class!(UIActivityViewController), alloc];
            let controller: Retained<AnyObject> = msg_send![controller, initWithActivityItems: &*items, applicationActivities: None::<&AnyObject>];
            let done = RcBlock::new(move |activity: *mut NSString, completed: Bool, _items: *mut AnyObject, error: *mut AnyObject| {
                guarded("share sheet completion handler", || {
                    let result = if let Some(error) = error.as_ref() {
                        let what: Retained<NSString> = msg_send![error, localizedDescription];
                        Err(ShareError::Failed(what.to_string()))
                    } else if completed.as_bool() {
                        Ok(Some(activity.as_ref().map_or_else(String::new, |a| a.to_string())))
                    } else {
                        Ok(None)
                    };
                    reply.send(result);
                })
            });
            let _: () = msg_send![&*controller, setCompletionWithItemsHandler: &*done];
            let popover: Option<Retained<AnyObject>> = msg_send![&*controller, popoverPresentationController];
            if let Some(popover) = popover {
                let bounds: CGRect = msg_send![self, bounds];
                let rect = match anchor {
                    Some([x, y, w, h]) => CGRect::new(CGPoint::new(x.into(), y.into()), CGSize::new(w.into(), h.into())),
                    None => CGRect::new(CGPoint::new(bounds.size.width / 2.0, bounds.size.height / 2.0), CGSize::new(0.0, 0.0)),
                };
                let _: () = msg_send![&*popover, setSourceView: self];
                let _: () = msg_send![&*popover, setSourceRect: rect];
                if anchor.is_none() { let _: () = msg_send![&*popover, setPermittedArrowDirections: 0usize]; }
            }
            let _: () = msg_send![&*screen, presentViewController: &*controller, animated: true, completion: None::<&block2::Block<dyn Fn()>>];
        }
    }
}
