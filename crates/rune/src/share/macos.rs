//! The share sheet on macOS: an `NSSharingServicePicker` beside the anchor in the window's
//! content view, which macOS shows as a popover. The picker answers when the user chooses a
//! service, with its title, or dismisses the popover (a click outside it), with none. It opens
//! from a run-loop block after the winit handler returns (`platform::macos::run_later`), so every
//! window keeps drawing while it shows. A window hidden or closed under it answers its share
//! itself (`RuntimeCore::cancel_shares`).

#![allow(unsafe_code)]

use std::cell::RefCell;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send};
use objc2_app_kit::NSWindow;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSString, NSURL};

use super::{Reply, Shown};

/// `NSRectEdgeMinY`: below the anchor in a flipped view, above it otherwise.
const MIN_Y_EDGE: usize = 1;

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and the class has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneSharePickerDelegate"]
    #[ivars = RefCell<Option<Reply>>]
    struct Delegate;

    impl Delegate {
        #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
        fn chose(&self, _picker: &AnyObject, service: Option<&AnyObject>) {
            let title = service.map(|service| {
                // SAFETY: the picker passes an `NSSharingService`, whose `title` takes no
                // arguments and returns a non-null `NSString`, retained here.
                let title: Retained<NSString> = unsafe { msg_send![service, title] };
                title.to_string()
            });
            if let Some(reply) = self.ivars().borrow_mut().take() { reply.send(Ok(title)); }
            let answered: *const Delegate = self;
            crate::platform::macos::run_later("share picker", move || {
                SHOWING.with(|showing| showing.borrow_mut().retain(|s| !std::ptr::eq(&*s.delegate, answered)));
            });
        }
    }
);

struct Showing {
    _picker: Retained<AnyObject>,
    delegate: Retained<Delegate>,
    host: Retained<NSWindow>,
}

thread_local! {
    static SHOWING: RefCell<Vec<Showing>> = const { RefCell::new(Vec::new()) };
}

/// Shows `shown` over `window`'s content view once the current winit handler returns. A
/// window hidden or closed by then answers `None`.
pub(crate) fn show(shown: Shown, window: &winit::window::Window) {
    let Shown { share, anchor, reply } = shown;
    let (Some(mtm), Some(view)) = (MainThreadMarker::new(), crate::platform::macos::content_view(window)) else {
        reply.send(Err(super::ShareError::Failed("the window has no view to show the picker in".into())));
        return;
    };
    crate::platform::macos::run_later("share picker", move || {
        let Some(host) = view.window().filter(|w| w.isVisible()) else {
            reply.send(Ok(None));
            return;
        };
        // SAFETY: `init` is `NSObject`'s designated initializer, sent to an allocated `Delegate`
        // whose ivars are set.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(mtm.alloc::<Delegate>().set_ivars(RefCell::new(Some(reply)))), init] };
        // SAFETY: an array of `NSString` and `NSURL` items makes a picker; its delegate is an
        // object that answers the delegate method above.
        let picker = unsafe {
            let items: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
            let add = |item: &AnyObject| { let _: () = msg_send![&*items, addObject: item]; };
            let text = [share.title.as_deref(), share.text.as_deref()].into_iter().flatten().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            if !text.is_empty() { add(&NSString::from_str(&text)); }
            if let Some(url) = share.url.as_deref().and_then(|url| NSURL::URLWithString(&NSString::from_str(url))) { add(&url); }
            for file in share.files.iter().filter_map(|f| f.to_str()) { add(&NSURL::fileURLWithPath(&NSString::from_str(file))); }
            let picker: Allocated<AnyObject> = msg_send![class!(NSSharingServicePicker), alloc];
            let picker: Retained<AnyObject> = msg_send![picker, initWithItems: &*items];
            let _: () = msg_send![&*picker, setDelegate: &*delegate];
            picker
        };
        SHOWING.with(|showing| {
            let mut showing = showing.borrow_mut();
            showing.retain(|s| s.host.isVisible());
            showing.push(Showing { _picker: picker.clone(), delegate, host });
        });
        // SAFETY: `bounds` is a rect and `isFlipped` a `BOOL` of the view, and the picker shows
        // relative to a rect in it.
        unsafe {
            let bounds: CGRect = msg_send![&*view, bounds];
            let flipped: bool = msg_send![&*view, isFlipped];
            let [x, y, w, h] = anchor.unwrap_or([bounds.size.width as f32 / 2.0, bounds.size.height as f32 / 2.0, 0.0, 0.0]).map(f64::from);
            let y = if flipped { y } else { bounds.size.height - y - h };
            let rect = CGRect::new(CGPoint::new(x, y), CGSize::new(w.max(1.0), h.max(1.0)));
            let _: () = msg_send![&*picker, showRelativeToRect: rect, ofView: &*view, preferredEdge: MIN_Y_EDGE];
        }
    });
}
