#![allow(unsafe_code)]

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, extern_class, msg_send};

use super::super::pacing::Event;
use super::lifecycle::EdgeInsets;
use super::{FILL_SUPERVIEW, RuneView, UIResponder, UIView};

extern_class!(
    // SAFETY: UIKit's class of this name is a `UIView` subclass for the main thread only.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UIScrollView"]
    pub(super) struct UIScrollView;
);

const ALWAYS_INSETS: isize = 3;

define_class!(
    // SAFETY: `UIScrollView` is made to be subclassed, and `Mirror` has no `Drop`.
    #[unsafe(super(UIScrollView, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneScrollMirror"]
    pub(super) struct Mirror;

    impl Mirror {
        #[unsafe(method_id(hitTest:withEvent:))]
        fn hit_test(&self, _point: CGPoint, _event: Option<&AnyObject>) -> Option<Retained<AnyObject>> {
            None
        }
    }
);

impl RuneView {
    pub(super) fn mirror_main_scroll(&self, mtm: MainThreadMarker) {
        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
        let mirror = Mirror::alloc(mtm).set_ivars(());
        // SAFETY: `initWithFrame:` is `UIView`'s designated initializer, which `UIScrollView`
        // keeps. The rest are `UIScrollView` and `UIView` properties of these types; the
        // delegate is not retained, and this view outlives the mirror it holds.
        let mirror: Retained<Mirror> = unsafe {
            let mirror: Retained<Mirror> = msg_send![super(mirror), initWithFrame: frame];
            let _: () = msg_send![&*mirror, setAutoresizingMask: FILL_SUPERVIEW];
            let _: () = msg_send![&*mirror, setContentInsetAdjustmentBehavior: ALWAYS_INSETS];
            let _: () = msg_send![&*mirror, setShowsVerticalScrollIndicator: false];
            let _: () = msg_send![&*mirror, setShowsHorizontalScrollIndicator: false];
            let _: () = msg_send![&*mirror, setScrollsToTop: true];
            let _: () = msg_send![&*mirror, setIsAccessibilityElement: false];
            let _: () = msg_send![&*mirror, setAccessibilityElementsHidden: true];
            let _: () = msg_send![&*mirror, setDelegate: self];
            let _: () = msg_send![self, addSubview: &*mirror];
            mirror
        };
        *self.ivars().mirror.borrow_mut() = Some(mirror);
    }

    pub(super) fn sync_mirror(&self) {
        let Some(mirror) = self.ivars().mirror.borrow().clone() else { return };
        let [offset, max] = self.with_surface(|surface| surface.main_scroll()).flatten().unwrap_or([0.0, 0.0]);
        // SAFETY: `UIScrollView` and `UIView` properties of these types.
        let (bounds, inset, size, at): (CGRect, EdgeInsets, CGSize, CGPoint) = unsafe {
            (msg_send![&*mirror, bounds], msg_send![&*mirror, adjustedContentInset], msg_send![&*mirror, contentSize], msg_send![&*mirror, contentOffset])
        };
        let width = (bounds.size.width - inset.left - inset.right).max(0.0);
        let height = (bounds.size.height - inset.top - inset.bottom).max(0.0) + f64::from(max);
        let wanted = (CGSize::new(width, height), CGPoint::new(-inset.left, f64::from(offset) - inset.top));
        if (size, at) == wanted { return; }
        // SAFETY: `UIScrollView` properties of these types.
        unsafe {
            let _: () = msg_send![&*mirror, setContentSize: wanted.0];
            let _: () = msg_send![&*mirror, setContentOffset: wanted.1];
        }
    }

    pub(super) fn status_bar_tapped(&self) {
        if self.with_surface(|surface| surface.scroll_to_top()) == Some(true) { self.pace(Event::Wake); }
    }
}
