use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};
use objc2::rc::{Retained, Weak};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{NSObject, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};
use rune::Waker;
use rune::machine::Attempt;

use super::view::RuneView;
use crate::pacing::PacingEvent;

define_class!(
    // The display link retains its target, so the target holds the view weakly.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneDisplayTick"]
    #[ivars = Weak<RuneView>]
    struct Tick;

    impl Tick {
        #[unsafe(method(tick:))]
        fn tick(&self, link: &CADisplayLink) {
            if let Some(view) = self.ivars().load() {
                view.tick(link);
            }
        }
    }
);

/// A view's `CADisplayLink` on the main run loop, asking for 120 Hz where the screen has it.
pub(crate) struct Link(Retained<CADisplayLink>);

impl Link {
    pub(crate) fn new(view: &RuneView, mtm: MainThreadMarker, paused: bool) -> Self {
        let tick = mtm.alloc::<Tick>().set_ivars(Weak::new(view));
        // SAFETY: `NSObject`'s `init` on a freshly allocated object whose ivars are set.
        let tick: Retained<Tick> = unsafe { msg_send![super(tick), init] };
        // SAFETY: `Tick` answers `tick:`, which takes the display link.
        let link = unsafe { CADisplayLink::displayLinkWithTarget_selector(&tick, sel!(tick:)) };
        link.setPreferredFrameRateRange(CAFrameRateRange { minimum: 80.0, maximum: 120.0, preferred: 120.0 });
        link.setPaused(paused);
        // SAFETY: the main run loop, from the main thread; the common modes keep the link
        // ticking while UIKit tracks a touch.
        unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
        Self(link)
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        self.0.setPaused(paused);
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.0.invalidate();
    }
}

/// Feeds `PacingEvent::Timer { attempt }` to `view` after `after` seconds, unless the view is
/// gone by then.
pub(crate) fn arm_timer(view: &RuneView, mtm: MainThreadMarker, attempt: Attempt, after: f64) {
    let view = MainThreadBound::new(Weak::new(view), mtm);
    let delay = Duration::try_from_secs_f64(after.max(0.0)).unwrap_or(Duration::MAX);
    let when = DispatchTime::try_from(delay).unwrap_or(DispatchTime::FOREVER);
    let fire = move || {
        let mtm = MainThreadMarker::new().unwrap_or_else(|| unreachable!("the main queue runs on the main thread"));
        if let Some(view) = view.get(mtm).load() {
            view.pace(PacingEvent::Timer { attempt });
        }
    };
    if DispatchQueue::main().after(when, fire).is_err() {
        eprintln!("rune-ios: the main queue refused a {after:.3} s timer");
    }
}

/// The app's waker: from any thread, resumes the view's display link on the main queue. Wakes
/// that arrive before the queued one runs fold into it.
pub(crate) fn waker(view: &RuneView, mtm: MainThreadMarker) -> Waker {
    struct Shared {
        view: MainThreadBound<Weak<RuneView>>,
        queued: AtomicBool,
    }
    let shared = Arc::new(Shared { view: MainThreadBound::new(Weak::new(view), mtm), queued: AtomicBool::new(false) });
    Arc::new(move || {
        if shared.queued.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = Arc::clone(&shared);
        DispatchQueue::main().exec_async(move || {
            shared.queued.store(false, Ordering::Release);
            let mtm = MainThreadMarker::new().unwrap_or_else(|| unreachable!("the main queue runs on the main thread"));
            if let Some(view) = shared.view.get(mtm).load() {
                view.pace(PacingEvent::Wake);
            }
        });
    })
}
