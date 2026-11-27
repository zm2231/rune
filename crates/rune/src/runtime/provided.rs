use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use super::Cx;

thread_local! {
    static NEXT: Cell<u64> = const { Cell::new(1) };
}

/// A shared value a view hands to [`Cx::provide`]: cheap to clone, and identified by when it was
/// made, so memos built under it build again once a new one replaces it, even an equal one.
pub struct Provided<T: 'static>(Rc<Inner<T>>);

struct Inner<T> {
    id: u64,
    value: T,
}

impl<T: 'static> Provided<T> {
    /// `value`, with an id no earlier value made on this thread has.
    pub fn new(value: T) -> Self {
        let id = NEXT.with(|n| n.replace(n.get() + 1));
        Self(Rc::new(Inner { id, value }))
    }

    /// Unique among the values made on this thread, and never reused.
    pub fn id(&self) -> u64 {
        self.0.id
    }
}

impl<T: 'static> Clone for Provided<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> Deref for Provided<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0.value
    }
}

impl<T: fmt::Debug + 'static> fmt::Debug for Provided<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Provided").field(&self.0.id).field(&self.0.value).finish()
    }
}

type Entries = Rc<RefCell<Vec<(TypeId, Rc<dyn Any>)>>>;

#[derive(Default)]
pub(crate) struct Stack(Entries);

impl Clone for Stack {
    fn clone(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }
}

impl Stack {
    fn find<T: 'static>(&self) -> Option<Provided<T>> {
        let entries = self.0.borrow();
        let (_, any) = entries.iter().rev().find(|(t, _)| *t == TypeId::of::<T>())?;
        let inner = any.clone().downcast::<Inner<T>>().ok()?;
        Some(Provided(inner))
    }

    fn push(&self, entry: (TypeId, Rc<dyn Any>)) -> Pop {
        let mut entries = self.0.borrow_mut();
        let depth = entries.len();
        entries.push(entry);
        Pop { entries: self.0.clone(), depth }
    }
}

struct Pop {
    entries: Entries,
    depth: usize,
}

impl Drop for Pop {
    fn drop(&mut self) {
        self.entries.borrow_mut().truncate(self.depth);
    }
}

impl<S> Cx<S> {
    /// Runs `build` with `value` in scope: inside it, [`Self::provided`] returns `value` for `T`,
    /// over any outer one, and outside it the outer one again. Elements built inside take what
    /// they read then. Memos built inside, and virtual rows the view creates, build again when
    /// a different `Provided` (by [`Provided::id`]) is in scope, so a theme swap repaints them.
    pub fn provide<T: 'static, R>(&mut self, value: &Provided<T>, build: impl FnOnce(&mut Self) -> R) -> R {
        let _salt = crate::memo::salt(value.id());
        let _pop = self.core.provided.push((TypeId::of::<T>(), value.0.clone()));
        build(self)
    }

    /// The innermost value of type `T` a [`Self::provide`] around this build put in scope.
    pub fn provided<T: 'static>(&self) -> Option<Provided<T>> {
        self.core.provided.find()
    }
}

#[cfg(test)]
#[path = "provided_tests.rs"]
mod tests;
