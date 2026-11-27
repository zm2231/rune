use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::Shared;
use super::hide_wait::{self, Phase};
use crate::runtime::Runtime;

pub(crate) type Unmounted<T> = Rc<RefCell<Option<Runtime<T>>>>;

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
        AfterHidden { phase: Phase::Idle, queue: Box::new(Queue { shared, pending: Vec::new(), unmounted }) }
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
}

struct Queue<T: 'static> {
    shared: Arc<Shared<T>>,
    pending: Vec<super::requests::AfterHidden<T>>,
    unmounted: Unmounted<T>,
}

impl<T: 'static> HiddenQueue for Queue<T> {
    fn take(&mut self) -> bool {
        let newly = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        let any = !newly.is_empty();
        self.pending.extend(newly);
        any
    }

    fn run(&mut self, mounted: Option<&mut dyn Any>) {
        self.take();
        let callbacks = std::mem::take(&mut self.pending);
        let mut unmounted = self.unmounted.borrow_mut();
        let rt = unmounted.as_mut().or_else(|| mounted.and_then(|rt| rt.downcast_mut::<Runtime<T>>()));
        if let Some(rt) = rt {
            for f in callbacks { rt.update(f); }
        }
    }

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
