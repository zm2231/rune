use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::runtime::{Runtime, Waker};

/// One app state that several windows show and change, each with its own view, focus, layout
/// and clocks. Give it to each window with [`WindowSpec::shared`](crate::WindowSpec::shared).
///
/// The windows of one app take turns: Rune moves the state into a window's runtime for each
/// turn that reaches it (input, a frame, a timer, an update) and back out when the turn ends,
/// so views and handlers get the usual `&S` and `&mut S`. Only the box holding the state
/// changes hands, never the state itself, so a turn costs the same for any `S`. When a
/// window's turn changes the state (a handler without [`Tx::live_only`](crate::Tx::live_only),
/// a `poll` that returns true, an update), the other windows run their views again on their
/// next frame. A window that isn't in its turn holds a placeholder no view or handler sees.
///
/// `poll` runs in each window's frames: work for the whole app belongs in one window's `poll`,
/// or checks [`Tx::window`](crate::Tx::window). Use the state from the UI thread only.
///
/// ```no_run
/// use rune::{AppSpec, SharedState, WindowSpec, el};
///
/// #[derive(Default)]
/// struct Notes { open: Vec<String> }
///
/// let notes = SharedState::new(Notes::default());
/// let spec = |title: &'static str| AppSpec::new(title, (480.0, 360.0), Notes::default, |_: &Notes, _| el());
/// rune::run_windows(move |_| [
///     WindowSpec::new("list", spec("Notes")).shared(&notes),
///     WindowSpec::new("note-1", spec("Note")).shared(&notes),
/// ]);
/// ```
pub struct SharedState<S: 'static> {
    cell: Rc<Shared<S>>,
}

impl<S: 'static> Clone for SharedState<S> {
    fn clone(&self) -> Self { Self { cell: self.cell.clone() } }
}

type Hook<S> = fn(&mut S, &str);

struct Shared<S: 'static> {
    home: RefCell<Box<S>>,
    placeholder: fn() -> S,
    resident: RefCell<Option<Rc<str>>>,
    generation: Cell<u64>,
    on_enter: Cell<Option<Hook<S>>>,
    on_leave: Cell<Option<Hook<S>>>,
    members: RefCell<Vec<(u64, Waker)>>,
    next: Cell<u64>,
    taken: Cell<bool>,
}

impl<S: Default + 'static> SharedState<S> {
    /// Shares `state`; the windows not in their turn hold `S::default()`.
    pub fn new(state: S) -> Self { Self::with_placeholder(state, S::default) }
}

impl<S: 'static> SharedState<S> {
    /// Shares `state`; the windows not in their turn hold what `placeholder` makes, which no
    /// view or handler sees. For a state with no cheap `Default`.
    pub fn with_placeholder(state: S, placeholder: fn() -> S) -> Self {
        Self {
            cell: Rc::new(Shared {
                home: RefCell::new(Box::new(state)),
                placeholder,
                resident: RefCell::new(None),
                generation: Cell::new(0),
                on_enter: Cell::new(None),
                on_leave: Cell::new(None),
                members: RefCell::new(Vec::new()),
                next: Cell::new(0),
                taken: Cell::new(false),
            }),
        }
    }

    /// Runs with the window's key at the start of each of its turns, after the state moved in
    /// and before anything else sees it: swap the window's own fields into place here. What
    /// it and [`Self::on_leave`] change is the window's own and wakes no other window, since
    /// they run in every turn; a change the other windows show goes through a handler, `poll`
    /// or an update.
    pub fn on_enter(self, f: fn(&mut S, &str)) -> Self {
        self.cell.on_enter.set(Some(f));
        self
    }

    /// Runs with the window's key at the end of each of its turns, before the state moves out.
    pub fn on_leave(self, f: fn(&mut S, &str)) -> Self {
        self.cell.on_leave.set(Some(f));
        self
    }

    pub(crate) fn placeholder(&self) -> S { (self.cell.placeholder)() }

    pub(crate) fn take_for_one(&self) -> S {
        assert!(!self.cell.taken.replace(true), "rune: a SharedState that one window already took was given to another");
        std::mem::replace(&mut **self.cell.home.borrow_mut(), self.placeholder())
    }

    pub(crate) fn member(&self, key: &str) -> Member<S> {
        let id = self.cell.next.get();
        self.cell.next.set(id + 1);
        Member { cell: self.cell.clone(), id, key: key.into(), seen: self.cell.generation.get(), changes: 0, resident: false }
    }
}

impl<S: 'static> crate::WindowSpec<S> {
    /// This window shows and changes `shared` with the other windows given it, instead of the
    /// state its `AppSpec` makes. See [`SharedState`].
    pub fn shared(mut self, shared: &SharedState<S>) -> Self {
        self.app.shared_state = Some(shared.clone());
        self
    }
}

pub(crate) struct Member<S: 'static> {
    cell: Rc<Shared<S>>,
    id: u64,
    key: Rc<str>,
    seen: u64,
    changes: u64,
    resident: bool,
}

impl<S: 'static> Member<S> {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn connect(&self, wake: Waker) {
        let mut members = self.cell.members.borrow_mut();
        members.retain(|(id, _)| *id != self.id);
        members.push((self.id, wake));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn resident(&self) -> bool { self.resident }

    pub(crate) fn enter(&mut self, rt: &mut Runtime<S>) -> bool {
        if self.resident { return false; }
        if let Some(other) = self.cell.resident.borrow().as_deref() {
            panic!("rune: window {:?} began a turn while window {other:?} still holds their SharedState", &*self.key);
        }
        std::mem::swap(&mut rt.state, &mut *self.cell.home.borrow_mut());
        *self.cell.resident.borrow_mut() = Some(self.key.clone());
        self.resident = true;
        let generation = self.cell.generation.get();
        if generation != self.seen {
            self.seen = generation;
            rt.core_mut().invalidate_shared();
        }
        self.changes = rt.core().state_changes();
        if let Some(f) = self.cell.on_enter.get() { f(&mut rt.state, &self.key); }
        true
    }

    pub(crate) fn leave(&mut self, rt: &mut Runtime<S>) {
        if !self.resident { return; }
        if let Some(f) = self.cell.on_leave.get() { f(&mut rt.state, &self.key); }
        let changed = rt.core().state_changes() != self.changes;
        self.release(rt);
        if changed { self.changed(); }
    }

    pub(crate) fn release(&mut self, rt: &mut Runtime<S>) {
        if !self.resident { return; }
        std::mem::swap(&mut rt.state, &mut *self.cell.home.borrow_mut());
        *self.cell.resident.borrow_mut() = None;
        self.resident = false;
    }

    fn changed(&mut self) {
        let generation = self.cell.generation.get() + 1;
        self.cell.generation.set(generation);
        self.seen = generation;
        let mut i = 0;
        while let Some(wake) = self.cell.members.borrow().get(i).map(|(id, wake)| (*id != self.id).then(|| wake.clone())) {
            if let Some(wake) = wake { wake(); }
            i += 1;
        }
    }
}

impl<S: 'static> Drop for Member<S> {
    fn drop(&mut self) {
        debug_assert!(!self.resident || std::thread::panicking(), "rune: a window over a SharedState went away holding it");
        self.cell.members.borrow_mut().retain(|(id, _)| *id != self.id);
    }
}

#[cfg(test)]
mod tests;
