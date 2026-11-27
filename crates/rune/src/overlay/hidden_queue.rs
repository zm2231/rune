use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::Shared;
use super::hide_wait::{self, Phase};
use crate::platform::embed_frame::{Serves, flush_requests};
use crate::runtime::{AppDriver, Runtime, RuntimeCore};

pub(crate) type Unmounted<T> = Rc<RefCell<Option<Runtime<T>>>>;

pub(crate) fn settle(core: &mut RuntimeCore, app: &mut dyn AppDriver, serves: Serves) {
    flush_requests(core, app, Serves { popup_menus: false, file_dialogs: false, shares: false, ..serves });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    pub(crate) wake: bool,
    pub(crate) run: bool,
}

pub(crate) struct AfterHidden {
    phase: Phase,
    queue: Box<dyn HiddenQueue>,
}

impl AfterHidden {
    pub(in crate::overlay) fn new<T: 'static>(shared: Arc<Shared<T>>, unmounted: Unmounted<T>) -> Self {
        AfterHidden { phase: Phase::Idle, queue: Box::new(Queue { shared, pending: Vec::new(), unmounted, clipboard: crate::clipboard::Client::default() }) }
    }

    pub(crate) fn new_turn(&mut self) -> Turn { self.step(hide_wait::Event::NewTurn) }

    pub(crate) fn queued(&mut self, hidden: bool) -> Turn {
        if !self.queue.take() { return Turn::default() }
        self.step(hide_wait::Event::Queued { hidden, uses_sync: false })
    }

    pub(crate) fn shown(&mut self) -> Turn { self.step(hide_wait::Event::Shown) }

    pub(crate) fn hidden(&mut self) -> Turn { self.step(hide_wait::Event::Hidden { uses_sync: false, surface_destroyed: false }) }

    pub(crate) fn end_of_turn(&mut self) -> Turn { self.step(hide_wait::Event::AboutToWaitHidden) }

    pub(crate) fn run(&mut self, mounted: Option<&mut dyn Any>) { self.queue.run(mounted); }

    #[cfg(test)]
    fn use_memory(&mut self, clipboard: &crate::clipboard::MemoryClipboard) { self.queue.use_memory(clipboard); }

    fn step(&mut self, event: hide_wait::Event) -> Turn {
        let mut turn = Turn::default();
        for effect in hide_wait::step(&mut self.phase, event) {
            match effect {
                hide_wait::Effect::Wake => turn.wake = true,
                hide_wait::Effect::RunCallbacks => turn.run = true,
                hide_wait::Effect::DropCallbacks => self.queue.drop_pending(),
                hide_wait::Effect::StartSync | hide_wait::Effect::CancelSync => {}
            }
        }
        turn
    }
}

trait HiddenQueue {
    fn take(&mut self) -> bool;
    fn run(&mut self, mounted: Option<&mut dyn Any>);
    fn drop_pending(&mut self);
    #[cfg(test)]
    fn use_memory(&mut self, clipboard: &crate::clipboard::MemoryClipboard);
}

struct Queue<T: 'static> {
    shared: Arc<Shared<T>>,
    pending: Vec<super::requests::AfterHidden<T>>,
    unmounted: Unmounted<T>,
    clipboard: crate::clipboard::Client,
}

impl<T: 'static> HiddenQueue for Queue<T> {
    fn take(&mut self) -> bool {
        let mut queue = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*queue, super::requests::AfterHiddenQueue::Closed) {
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

    fn run(&mut self, mounted: Option<&mut dyn Any>) {
        self.take();
        let callbacks = std::mem::take(&mut self.pending);
        let mut unmounted = self.unmounted.borrow_mut();
        if let Some(rt) = unmounted.as_mut() {
            for f in callbacks { rt.update(f); }
            let (core, mut app) = rt.split_mut();
            settle(core, &mut app, Serves { clipboard: true, ..Serves::default() });
            self.clipboard.flush_core(core);
        } else if let Some(rt) = mounted.and_then(|rt| rt.downcast_mut::<Runtime<T>>()) {
            for f in callbacks { rt.update(f); }
        }
    }

    #[cfg(test)]
    fn use_memory(&mut self, clipboard: &crate::clipboard::MemoryClipboard) { self.clipboard.use_memory(clipboard); }

    fn drop_pending(&mut self) {
        let queued = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        drop((queued, std::mem::take(&mut self.pending)));
    }
}

impl<T: 'static> Drop for Queue<T> {
    fn drop(&mut self) {
        let queued = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner).close();
        drop((queued, std::mem::take(&mut self.pending)));
    }
}

#[cfg(test)]
#[path = "hidden_queue_tests.rs"]
mod tests;
