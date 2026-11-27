#![allow(unsafe_code)]

use crate::platform::objc_boundary::guarded;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use std::cell::Cell;

use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, extern_class, msg_send};

use super::appearance::status_bar_style;
use super::view::UIResponder;

extern_class!(
    // SAFETY: UIKit's class of this name is a `UIResponder` subclass for the main thread only.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UIViewController"]
    pub(crate) struct UIViewController;
);

define_class!(
    // SAFETY: `UIViewController` is made to be subclassed, and `RuneViewController` has no `Drop`.
    #[unsafe(super(UIViewController, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneViewController"]
    #[ivars = Cell<bool>]
    pub(crate) struct RuneViewController;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for RuneViewController {}

    impl RuneViewController {
        #[unsafe(method(preferredStatusBarStyle))]
        fn preferred_status_bar_style(&self) -> isize {
            guarded("preferredStatusBarStyle", || {
                // SAFETY: `traitCollection` is never nil, and `userInterfaceStyle` is an `NSInteger` enum.
                let style: isize = unsafe {
                    let traits: Retained<AnyObject> = msg_send![self, traitCollection];
                    msg_send![&*traits, userInterfaceStyle]
                };
                status_bar_style(style)
            })
        }

        #[unsafe(method(prefersPointerLocked))]
        fn prefers_pointer_locked(&self) -> bool {
            guarded("prefersPointerLocked", || self.ivars().get())
        }

        #[unsafe(method(traitCollectionDidChange:))]
        fn trait_collection_did_change(&self, previous: Option<&AnyObject>) {
            guarded("traitCollectionDidChange:", || {
                // SAFETY: `UIViewController` implements both with these signatures.
                unsafe {
                    let _: () = msg_send![super(self), traitCollectionDidChange: previous];
                    let _: () = msg_send![self, setNeedsStatusBarAppearanceUpdate];
                }
            })
        }
    }
);

impl RuneViewController {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(false));
        // SAFETY: `init` is `UIViewController`'s initializer with no arguments.
        unsafe { msg_send![super(this), init] }
    }

    pub(crate) fn prefer_pointer_locked(&self, locked: bool) {
        if self.ivars().replace(locked) == locked { return; }
        // SAFETY: `UIViewController` has this method from iOS 14, which UIKit answers by asking
        // `prefersPointerLocked` again.
        unsafe { let _: () = msg_send![self, setNeedsUpdateOfPrefersPointerLocked]; }
    }

    pub(crate) fn owns(responder: Option<&AnyObject>) -> bool {
        // SAFETY: `isKindOfClass:` takes a class and answers a `BOOL`.
        responder.is_some_and(|r| unsafe { msg_send![r, isKindOfClass: Self::class()] })
    }
}
