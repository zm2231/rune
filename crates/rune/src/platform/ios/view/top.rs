//! A tap on the status bar scrolls the app's main list to its top. UIKit sends that tap to
//! scroll views, not to views: it asks each scroll view in the window that lies under the
//! tap's x whether it should scroll to its top, and scrolls the ones that say yes. A
//! `RuneView` has no scroll view, so it keeps one for UIKit to ask: a strip across its top
//! edge, one point high, with nothing in it. Asked, the strip says no and the runtime scrolls
//! its own list.
//!
//! The strip is a scroll view that could scroll to its top, which is one UIKit asks: it takes
//! touches, it is not hidden or clear, and it holds three points of content at an offset of
//! one, which its answer of no leaves there. It takes no insets from the safe area, which
//! would put that offset at its top. Which of these UIKit needs is not established; this set
//! is asked. It stays out of the way of fingers by answering every hit test with no view.

#![allow(unsafe_code)]

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, extern_class, msg_send};

use super::super::pacing::Event;
use super::{RuneView, UIResponder, UIView};

extern_class!(
    // SAFETY: UIKit's class of this name is a `UIView` subclass for the main thread only.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UIScrollView"]
    struct UIScrollView;
);

/// `UIViewAutoresizingFlexibleWidth`.
const FILL_WIDTH: usize = 2;
/// `UIScrollViewContentInsetAdjustmentNever`.
const NO_INSETS: isize = 2;
/// The strip's height in points.
const HEIGHT: f64 = 1.0;
/// Its content's height: taller than the strip by more than the offset it rests at.
const CONTENT: f64 = 3.0;
/// How far down its content it rests: off its top.
const OFFSET: f64 = 1.0;

define_class!(
    // SAFETY: `UIScrollView` is made to be subclassed, and `TopStrip` has no `Drop`.
    #[unsafe(super(UIScrollView, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTopStrip"]
    struct TopStrip;

    impl TopStrip {
        /// No touch lands on the strip: what is under it gets them.
        #[unsafe(method_id(hitTest:withEvent:))]
        fn hit_test(&self, _point: CGPoint, _event: Option<&AnyObject>) -> Option<Retained<AnyObject>> {
            None
        }
    }
);

impl RuneView {
    /// Puts the strip UIKit asks about a tap on the status bar across the view's top edge.
    pub(super) fn watch_status_bar(&self, mtm: MainThreadMarker) {
        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, HEIGHT));
        let strip = TopStrip::alloc(mtm).set_ivars(());
        // SAFETY: `initWithFrame:` is `UIView`'s designated initializer, which `UIScrollView`
        // keeps. The rest are `UIScrollView` and `UIView` properties of these types; the
        // delegate is not retained, and this view outlives the strip it holds.
        unsafe {
            let strip: Retained<TopStrip> = msg_send![super(strip), initWithFrame: frame];
            let _: () = msg_send![&*strip, setAutoresizingMask: FILL_WIDTH];
            let _: () = msg_send![&*strip, setContentInsetAdjustmentBehavior: NO_INSETS];
            let _: () = msg_send![&*strip, setShowsVerticalScrollIndicator: false];
            let _: () = msg_send![&*strip, setShowsHorizontalScrollIndicator: false];
            let _: () = msg_send![&*strip, setContentSize: CGSize::new(0.0, CONTENT)];
            let _: () = msg_send![&*strip, setContentOffset: CGPoint::new(0.0, OFFSET)];
            let _: () = msg_send![&*strip, setScrollsToTop: true];
            // A reader finds nothing in it, and the view lists its own elements besides.
            let _: () = msg_send![&*strip, setIsAccessibilityElement: false];
            let _: () = msg_send![&*strip, setAccessibilityElementsHidden: true];
            let _: () = msg_send![&*strip, setDelegate: self];
            let _: () = msg_send![self, addSubview: &*strip];
        }
    }

    /// The status bar was tapped: the runtime sends its main list to its top, and a frame
    /// follows when there was a list to send.
    pub(super) fn status_bar_tapped(&self) {
        if self.with_surface(|surface| surface.scroll_to_top()) == Some(true) { self.pace(Event::Wake); }
    }
}
