#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, Bool};
use objc2::{DefinedClass, MainThreadMarker, class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};

use super::super::overlay::{self, Effect, Event};
use super::super::surface::{Mount, OverlaySlot};
use super::RuneView;
use super::link::MainOnly;
use crate::platform::objc_boundary::guarded;
use crate::machine::{Attempt, Seq};
use crate::overlay::ios::{Control, Standby, Turn};
use crate::platform::main_queue::{after_main, on_main};

pub(crate) struct Overlay {
    control: Rc<RefCell<Control>>,
    mount: Option<Mount>,
    view: Option<Retained<RuneView>>,
    seq: Seq,
    state: overlay::State,
    standby: Standby,
}

impl MainOnly {
    fn pump_overlays(&self) {
        if let Some(view) = self.0.load() { view.pump_overlays(); }
    }

    fn overlay_timer(&self, at: usize, attempt: Attempt) {
        if let Some(view) = self.0.load() { view.overlay_timer(at, attempt); }
    }
}

impl RuneView {
    pub(super) fn adopt_overlays(&self, slots: Vec<OverlaySlot>) {
        if slots.is_empty() { return; }
        let queued = Arc::new(AtomicBool::new(false));
        let owner = Arc::new(MainOnly(Weak::new(self)));
        for slot in &slots {
            let (owner, queued) = (owner.clone(), queued.clone());
            slot.control.borrow().connect(Arc::new(move || {
                if queued.swap(true, Ordering::AcqRel) { return; }
                let (owner, queued) = (owner.clone(), queued.clone());
                on_main(move || {
                    queued.store(false, Ordering::Release);
                    owner.pump_overlays();
                });
            }));
        }
        let overlays = slots.into_iter().map(|slot| Overlay { control: slot.control, mount: Some(slot.mount), view: None, seq: Seq::default(), state: overlay::State::default(), standby: slot.standby });
        self.ivars().overlays.borrow_mut().extend(overlays);
        self.pump_overlays();
    }

    pub(super) fn pump_overlays(&self) {
        let count = self.ivars().overlays.borrow().len();
        for at in 0..count {
            let turn = self.ivars().overlays.borrow_mut()[at].standby.new_turn();
            self.wake_if(at, turn);
            let turn = {
                let mut overlays = self.ivars().overlays.borrow_mut();
                let hidden = overlays[at].state == overlay::State::Hidden;
                overlays[at].standby.queued(hidden)
            };
            self.wake_if(at, turn);
            let changes = self.ivars().overlays.borrow()[at].control.borrow_mut().take();
            if changes.moved { self.place_overlay(at); }
            if let Some((show, fade)) = changes.visibility {
                self.step_overlay(at, if show { Event::Show(fade) } else { Event::Hide(fade) });
            }
            match changes.keys {
                Some(true) => if let Some(view) = self.ivars().overlays.borrow()[at].view.clone() {
                    // SAFETY: a `UIResponder` method of this signature.
                    let _: bool = unsafe { msg_send![&*view, becomeFirstResponder] };
                },
                Some(false) => self.take_keys(),
                None => {}
            }
            let (hidden, woken, view) = {
                let overlays = self.ivars().overlays.borrow();
                let overlay = &overlays[at];
                let woken = overlay.control.borrow().take_woken() || overlay.standby.has_updates();
                (overlay.state == overlay::State::Hidden, woken, overlay.view.clone())
            };
            if hidden {
                let turn = self.ivars().overlays.borrow_mut()[at].standby.end_of_turn();
                self.wake_if(at, turn);
                self.step_hidden(at, turn.run);
            } else if woken && let Some(view) = view {
                view.pace(super::Event::Wake);
            }
        }
    }

    fn wake_if(&self, at: usize, turn: Turn) {
        if turn.wake { self.ivars().overlays.borrow()[at].control.borrow().wake(); }
    }

    fn step_hidden(&self, at: usize, run: bool) {
        let view = self.ivars().overlays.borrow()[at].view.clone();
        let timer = {
            let mut overlays = self.ivars().overlays.borrow_mut();
            let standby = &mut overlays[at].standby;
            match view.and_then(|view| view.with_surface(|surface| standby.step(run, Some(surface.hidden_parts())))) {
                Some(timer) => timer,
                None => standby.step(run, None),
            }
        };
        if let Some((attempt, seconds)) = timer {
            let owner = MainOnly(Weak::new(self));
            after_main(seconds, move || owner.overlay_timer(at, attempt));
        }
    }

    fn overlay_timer(&self, at: usize, attempt: Attempt) {
        let due = self.ivars().overlays.borrow_mut()[at].standby.timer(attempt);
        if due { self.step_hidden(at, false); }
    }

    fn step_overlay(&self, at: usize, event: Event) {
        let effects = {
            let mut overlays = self.ivars().overlays.borrow_mut();
            let overlay = &mut overlays[at];
            overlay::step(&mut overlay.seq, &mut overlay.state, event)
        };
        for effect in effects {
            let turn = match effect {
                Effect::Attach { .. } => self.ivars().overlays.borrow_mut()[at].standby.shown(),
                Effect::Detach => Turn { wake: true, ..self.ivars().overlays.borrow_mut()[at].standby.hidden() },
                Effect::Fade { .. } => Turn::default(),
            };
            match effect {
                Effect::Attach { alpha } => self.attach_overlay(at, alpha),
                Effect::Fade { alpha, duration, attempt } => self.fade_overlay(at, alpha, duration.as_secs_f64(), attempt),
                Effect::Detach => if let Some(view) = self.ivars().overlays.borrow()[at].view.clone() {
                    // SAFETY: a `UIView` method of this signature.
                    let _: () = unsafe { msg_send![&*view, removeFromSuperview] };
                },
            }
            self.wake_if(at, turn);
        }
    }

    fn attach_overlay(&self, at: usize, alpha: f32) {
        let view = {
            let mut overlays = self.ivars().overlays.borrow_mut();
            let overlay = &mut overlays[at];
            match (&overlay.view, overlay.mount.take()) {
                (Some(view), _) => view.clone(),
                (None, Some(mount)) => {
                    let view = RuneView::overlay(MainThreadMarker::from(self), mount, overlay.control.clone());
                    overlay.view = Some(view.clone());
                    view
                }
                (None, None) => return,
            }
        };
        // SAFETY: `UIView` methods of these signatures.
        unsafe {
            let _: () = msg_send![&*view, setAlpha: f64::from(alpha)];
            let _: () = msg_send![self, addSubview: &*view];
        }
        self.place_overlay(at);
        self.stack_overlays();
    }

    fn stack_overlays(&self) {
        let mut shown: Vec<(crate::OverlayLevel, Retained<RuneView>)> = self.ivars().overlays.borrow().iter()
            .filter_map(|overlay| Some((overlay.control.borrow().level(), overlay.view.clone()?))).collect();
        shown.sort_by_key(|(level, _)| *level as u8);
        for (_, view) in shown {
            // SAFETY: a `UIView` method of this signature; a view not among the subviews is
            // left alone.
            let _: () = unsafe { msg_send![self, bringSubviewToFront: &*view] };
        }
    }

    fn fade_overlay(&self, at: usize, alpha: f32, seconds: f64, attempt: Attempt) {
        let Some(view) = self.ivars().overlays.borrow()[at].view.clone() else { return };
        let target = Weak::new(&*view);
        let animations = RcBlock::new(move || {
            guarded("overlay fade animations", || {
                // SAFETY: a `UIView` property of this type.
                if let Some(view) = target.load() { let _: () = unsafe { msg_send![&*view, setAlpha: f64::from(alpha)] }; }
            })
        });
        let owner = Weak::new(self);
        let completion = RcBlock::new(move |_finished: Bool| {
            guarded("overlay fade completion", || if let Some(owner) = owner.load() { owner.step_overlay(at, Event::Faded(attempt)); })
        });
        // SAFETY: `animateWithDuration:animations:completion:` takes a duration and two blocks,
        // which it copies.
        unsafe { let _: () = msg_send![class!(UIView), animateWithDuration: seconds, animations: &*animations, completion: &*completion]; }
    }

    fn place_overlay(&self, at: usize) {
        let (view, control) = {
            let overlays = self.ivars().overlays.borrow();
            let Some(view) = overlays[at].view.clone() else { return };
            (view, overlays[at].control.clone())
        };
        let ((width, height), _) = self.metrics();
        let [x, y, w, h] = control.borrow().frame((width, height), self.safe_area());
        let frame = CGRect::new(CGPoint::new(f64::from(x), f64::from(y)), CGSize::new(f64::from(w), f64::from(h)));
        // SAFETY: a `UIView` property of this type.
        let _: () = unsafe { msg_send![&*view, setFrame: frame] };
    }

    pub(super) fn place_overlays(&self) {
        for at in 0..self.ivars().overlays.borrow().len() { self.place_overlay(at); }
    }

    pub(super) fn overlay_takes(&self, x: f32, y: f32) -> Option<bool> {
        let control = self.ivars().control.borrow().clone()?;
        Some(self.with_surface(|surface| surface.takes_touch(&control.borrow(), x, y)).unwrap_or(false))
    }

    pub(super) fn takes_keys_on_touch(&self) -> bool {
        self.ivars().control.borrow().as_ref().is_none_or(|control| control.borrow().takes_keys_on_touch())
    }

    pub(super) fn over_overlay(&self, x: f32, y: f32) -> bool {
        let point = CGPoint::new(f64::from(x), f64::from(y));
        // SAFETY: `hitTest:withEvent:` takes a point in the view's space and a nullable event,
        // and answers a nullable view.
        let hit: Option<Retained<AnyObject>> = unsafe { msg_send![self, hitTest: point, withEvent: None::<&AnyObject>] };
        hit.is_some_and(|hit| {
            // SAFETY: `isKindOfClass:` takes a class and answers a `BOOL`.
            let rune: bool = unsafe { msg_send![&*hit, isKindOfClass: class!(RuneView)] };
            rune && !std::ptr::eq(Retained::as_ptr(&hit).cast::<RuneView>(), self)
        })
    }

    pub(super) fn overlay_elements(&self, list: Retained<AnyObject>) -> Retained<AnyObject> {
        let shown: Vec<Retained<RuneView>> = self.ivars().overlays.borrow().iter()
            .filter(|overlay| overlay.state != overlay::State::Hidden).filter_map(|overlay| overlay.view.clone()).collect();
        if shown.is_empty() { return list; }
        // SAFETY: `arrayWithArray:` copies an array; `addObject:` appends an object.
        unsafe {
            let all: Retained<AnyObject> = msg_send![class!(NSMutableArray), arrayWithArray: &*list];
            for view in shown { let _: () = msg_send![&*all, addObject: &*view]; }
            all
        }
    }
}
