//! The `NSUndoManager` a `RuneTextInput` answers UIKit with, and the machine's effects on it.

#![allow(unsafe_code)]

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};

use super::{Action, Effect, Event, Offer, State, step};

pub(crate) struct Undo {
    /// An `NSUndoManager` of the text input's own, which holds the two actions and nothing
    /// else.
    manager: Retained<AnyObject>,
    state: Cell<State>,
    /// A text field has the focus: only then is the manager UIKit's to ask.
    field: Cell<bool>,
}

impl Undo {
    pub(crate) fn new() -> Self {
        // SAFETY: `new` makes an `NSUndoManager`, and `setGroupsByEvent:` takes a `BOOL`.
        // With it off, each action registered here is a group of its own, where the manager
        // would otherwise put every action of one turn of the run loop in one group.
        let manager: Retained<AnyObject> = unsafe {
            let manager: Retained<AnyObject> = msg_send![class!(NSUndoManager), new];
            let _: () = msg_send![&*manager, setGroupsByEvent: false];
            manager
        };
        Self { manager, state: Cell::new(State::default()), field: Cell::new(false) }
    }

    /// What `undoManager` answers: the manager while a text field has the focus.
    pub(crate) fn manager(&self) -> Option<Retained<AnyObject>> {
        self.field.get().then(|| self.manager.clone())
    }

    /// After a frame: what the focused field can undo and redo, none with no field focused.
    /// `target` is the text input, which the manager's actions call. The manager is asked
    /// what it offers only after it was primed or ran an action; a frame that changes
    /// nothing sends it no message.
    pub(crate) fn frame(&self, target: &AnyObject, field: Option<Offer>) {
        self.field.set(field.is_some());
        // SAFETY: two `BOOL` properties of the manager.
        let manager = self.state.get().asks().then(|| unsafe { Offer { undo: msg_send![&*self.manager, canUndo], redo: msg_send![&*self.manager, canRedo] } });
        self.feed(target, Event::Frame { field: field.unwrap_or_default(), manager });
    }

    /// The manager ran `action`. Answers the key to send the field, when UIKit asked for it.
    pub(crate) fn fired(&self, target: &AnyObject, action: Action) -> Option<Action> {
        self.feed(target, Event::Fired(action))
    }

    /// Steps the machine and carries its effects out on the manager. The state is put back
    /// before the effects run: undoing fires an action, which comes back in here.
    fn feed(&self, target: &AnyObject, event: Event) -> Option<Action> {
        let mut state = self.state.get();
        let effects = step(&mut state, event);
        self.state.set(state);
        let mut key = None;
        for effect in effects {
            // SAFETY: `NSUndoManager`'s methods of these signatures. The actions' selectors
            // are methods of `target`, the text input, that take an object or nil. The
            // manager does not retain `target`: the text input owns this `Undo`, whose drop
            // empties the manager, so no action outlives its target even if UIKit still
            // holds the manager.
            unsafe {
                match effect {
                    Effect::Clear => { let _: () = msg_send![&*self.manager, removeAllActions]; }
                    Effect::Register(action) => {
                        let selector = match action {
                            Action::Undo => sel!(runeUndo:),
                            Action::Redo => sel!(runeRedo:),
                        };
                        // The manager groups what is registered while it undoes or redoes.
                        let (undoing, redoing): (bool, bool) = (msg_send![&*self.manager, isUndoing], msg_send![&*self.manager, isRedoing]);
                        let own_group = !undoing && !redoing;
                        if own_group { let _: () = msg_send![&*self.manager, beginUndoGrouping]; }
                        let _: () = msg_send![&*self.manager, registerUndoWithTarget: target, selector: selector, object: None::<&AnyObject>];
                        if own_group { let _: () = msg_send![&*self.manager, endUndoGrouping]; }
                    }
                    Effect::Undo => {
                        let _: () = msg_send![&*self.manager, undo];
                        self.feed(target, Event::Primed);
                    }
                    Effect::Key(action) => key = Some(action),
                }
            }
        }
        key
    }
}

impl Drop for Undo {
    /// The text input is going away, and the manager's actions call it.
    fn drop(&mut self) {
        // SAFETY: an `NSUndoManager` method that takes nothing.
        let _: () = unsafe { msg_send![&*self.manager, removeAllActions] };
    }
}
