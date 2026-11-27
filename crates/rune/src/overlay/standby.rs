use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::Shared;
use super::hide_wait::{self, Phase};
use super::requests::{AfterHidden, AfterHiddenQueue};
use crate::clipboard::Client;
use crate::machine::{Attempt, Effects, Seq};
use crate::platform::embed_frame::{Serves, flush_requests, wake_after_secs};
use crate::runtime::{AppDriver, Runtime, RuntimeCore};

pub(crate) struct Parked<T: 'static> {
    pub(crate) rt: Runtime<T>,
    pub(crate) clipboard: Client,
}

pub(crate) type Unmounted<T> = Rc<RefCell<Option<Parked<T>>>>;

pub(crate) struct Mounted<'a> {
    pub(crate) rt: &'a mut dyn Any,
    pub(crate) clipboard: Option<&'a Client>,
    pub(crate) serves: Serves,
}

fn settle(core: &mut RuntimeCore, app: &mut dyn AppDriver, serves: Serves) {
    flush_requests(core, app, Serves { popup_menus: false, file_dialogs: false, shares: false, ..serves });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    pub(crate) wake: bool,
    pub(crate) run: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pace {
    Shown,
    Hidden { timer: Option<Attempt> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PaceEvent {
    Shown,
    Hidden,
    Stepped { wake_after: Option<f64> },
    Timer(Attempt),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PaceEffect {
    Step,
    TimerAfter(Attempt, f64),
}

fn pace(state: &mut Pace, seq: &mut Seq, event: PaceEvent) -> Effects<PaceEffect> {
    match (*state, event) {
        (_, PaceEvent::Shown) => {
            *state = Pace::Shown;
            Effects::none()
        }
        (Pace::Shown, PaceEvent::Hidden) => {
            *state = Pace::Hidden { timer: None };
            Effects::none()
        }
        (Pace::Hidden { .. }, PaceEvent::Stepped { wake_after: Some(seconds) }) => {
            let attempt = seq.next();
            *state = Pace::Hidden { timer: Some(attempt) };
            Effects::one(PaceEffect::TimerAfter(attempt, seconds.max(0.0)))
        }
        (Pace::Hidden { .. }, PaceEvent::Stepped { wake_after: None }) => {
            *state = Pace::Hidden { timer: None };
            Effects::none()
        }
        (Pace::Hidden { timer: Some(due) }, PaceEvent::Timer(attempt)) if due == attempt => {
            *state = Pace::Hidden { timer: None };
            Effects::one(PaceEffect::Step)
        }
        (Pace::Hidden { .. }, PaceEvent::Hidden | PaceEvent::Timer(_)) | (Pace::Shown, PaceEvent::Stepped { .. } | PaceEvent::Timer(_)) => Effects::none(),
    }
}

pub(crate) struct Standby {
    phase: Phase,
    pace: Pace,
    seq: Seq,
    host: Box<dyn Host>,
}

impl Standby {
    pub(in crate::overlay) fn new<T: 'static>(shared: Arc<Shared<T>>, parked: Unmounted<T>) -> Self {
        Standby { phase: Phase::Idle, pace: Pace::Hidden { timer: None }, seq: Seq::default(), host: Box::new(Typed { shared, pending: Vec::new(), parked }) }
    }

    pub(crate) fn new_turn(&mut self) -> Turn { self.after_hidden(hide_wait::Event::NewTurn) }

    pub(crate) fn queued(&mut self, hidden: bool) -> Turn {
        if !self.host.take() { return Turn::default() }
        self.after_hidden(hide_wait::Event::Queued { hidden, uses_sync: false })
    }

    pub(crate) fn shown(&mut self) -> Turn {
        let _: Effects<PaceEffect> = pace(&mut self.pace, &mut self.seq, PaceEvent::Shown);
        self.after_hidden(hide_wait::Event::Shown)
    }

    pub(crate) fn hidden(&mut self) -> Turn {
        let _: Effects<PaceEffect> = pace(&mut self.pace, &mut self.seq, PaceEvent::Hidden);
        self.after_hidden(hide_wait::Event::Hidden { uses_sync: false, surface_destroyed: false })
    }

    pub(crate) fn end_of_turn(&mut self) -> Turn { self.after_hidden(hide_wait::Event::AboutToWaitHidden) }

    pub(crate) fn has_updates(&self) -> bool { self.host.has_updates() }

    pub(crate) fn step(&mut self, run: bool, mounted: Option<Mounted<'_>>) -> Option<(Attempt, f64)> {
        let wake_after = self.host.step(run, mounted);
        pace(&mut self.pace, &mut self.seq, PaceEvent::Stepped { wake_after }).into_iter().find_map(|effect| match effect {
            PaceEffect::TimerAfter(attempt, seconds) => Some((attempt, seconds)),
            PaceEffect::Step => None,
        })
    }

    pub(crate) fn timer(&mut self, attempt: Attempt) -> bool {
        pace(&mut self.pace, &mut self.seq, PaceEvent::Timer(attempt)).into_iter().any(|effect| effect == PaceEffect::Step)
    }

    #[cfg(test)]
    fn use_memory(&mut self, clipboard: &crate::clipboard::MemoryClipboard) { self.host.use_memory(clipboard); }

    fn after_hidden(&mut self, event: hide_wait::Event) -> Turn {
        let mut turn = Turn::default();
        for effect in hide_wait::step(&mut self.phase, event) {
            match effect {
                hide_wait::Effect::Wake => turn.wake = true,
                hide_wait::Effect::RunCallbacks => turn.run = true,
                hide_wait::Effect::DropCallbacks => self.host.drop_pending(),
                hide_wait::Effect::StartSync | hide_wait::Effect::CancelSync => {}
            }
        }
        turn
    }
}

trait Host {
    fn take(&mut self) -> bool;
    fn has_updates(&self) -> bool;
    fn step(&mut self, run: bool, mounted: Option<Mounted<'_>>) -> Option<f64>;
    fn drop_pending(&mut self);
    #[cfg(test)]
    fn use_memory(&mut self, clipboard: &crate::clipboard::MemoryClipboard);
}

struct Typed<T: 'static> {
    shared: Arc<Shared<T>>,
    pending: Vec<AfterHidden<T>>,
    parked: Unmounted<T>,
}

impl<T: 'static> Host for Typed<T> {
    fn take(&mut self) -> bool {
        let mut queue = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*queue, AfterHiddenQueue::Closed) {
            drop(queue);
            drop(std::mem::take(&mut self.pending));
            return false;
        }
        let newly = queue.take();
        drop(queue);
        let any = !newly.is_empty();
        self.pending.extend(newly);
        any
    }

    fn has_updates(&self) -> bool {
        !self.shared.updates.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty()
    }

    fn step(&mut self, run: bool, mounted: Option<Mounted<'_>>) -> Option<f64> {
        self.take();
        let callbacks = if run { std::mem::take(&mut self.pending) } else { Vec::new() };
        let mut parked = self.parked.borrow_mut();
        if let Some(parked) = parked.as_mut() {
            return step_hidden(&mut parked.rt, &self.shared, callbacks, Some(&parked.clipboard), Serves { clipboard: true, ..Serves::default() });
        }
        let mounted = mounted?;
        let rt = mounted.rt.downcast_mut::<Runtime<T>>()?;
        step_hidden(rt, &self.shared, callbacks, mounted.clipboard, mounted.serves)
    }

    fn drop_pending(&mut self) {
        let queued = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        drop((queued, std::mem::take(&mut self.pending)));
    }

    #[cfg(test)]
    fn use_memory(&mut self, clipboard: &crate::clipboard::MemoryClipboard) {
        if let Some(parked) = self.parked.borrow_mut().as_mut() { parked.clipboard.use_memory(clipboard); }
    }
}

impl<T: 'static> Drop for Typed<T> {
    fn drop(&mut self) {
        let queued = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner).close();
        drop((queued, std::mem::take(&mut self.pending)));
    }
}

fn step_hidden<T: 'static>(rt: &mut Runtime<T>, shared: &Shared<T>, callbacks: Vec<AfterHidden<T>>, clipboard: Option<&Client>, serves: Serves) -> Option<f64> {
    if let Some(clipboard) = clipboard { clipboard.receive_core(rt.core_mut()); }
    let updates = std::mem::take(&mut *shared.updates.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    for f in updates { rt.update_if(f); }
    for f in callbacks { rt.update(f); }
    rt.hidden_wake();
    let (core, mut app) = rt.split_mut();
    settle(core, &mut app, serves);
    if let Some(clipboard) = clipboard { clipboard.flush_core(core); }
    wake_after_secs(core)
}

#[cfg(test)]
#[path = "standby_tests.rs"]
mod tests;
