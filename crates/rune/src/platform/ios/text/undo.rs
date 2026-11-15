//! The system's undo gestures (a shake, a three-finger swipe or tap) for the focused field.
//! UIKit offers undo and redo when the first responder's `NSUndoManager` says it can do them,
//! and does them by asking that manager. The field's own history stays the only history: the
//! manager holds at most one undo action and one redo action, there so that it answers
//! `canUndo` and `canRedo` as the field would, and each sends the field ⌘Z or ⇧⌘Z when UIKit
//! runs it.
//!
//! An `NSUndoManager` can do what its stacks hold. An action registered while it is doing
//! nothing lands on its undo stack and empties its redo stack; one registered while it undoes
//! lands on its redo stack, and one registered while it redoes on its undo stack. So the
//! manager is primed to offer a redo by registering an undo action and having it undo that:
//! the action then fires, and what it registers is the redo. An action that fires while
//! priming sends the field nothing.

use crate::machine::Effects;

#[cfg(target_os = "ios")]
#[path = "undo/uikit.rs"]
pub(crate) mod uikit;

/// Which of undo and redo there is to do: the field's, or the manager's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Offer {
    pub(crate) undo: bool,
    pub(crate) redo: bool,
}

/// The two actions the manager holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Undo,
    Redo,
}

/// What the manager is known to offer, and whether an action that fires is the user's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// The manager's actions are UIKit's to run: one that fires is a gesture. It holds what
    /// it offers when that is known; after the manager was primed or ran an action it is
    /// not, and the next frame asks the manager.
    Live(Option<Offer>),
    /// The manager is undoing an action so that it has a redo to offer: the action that fires
    /// is that one.
    Priming,
}

impl Default for State {
    /// A new manager offers nothing.
    fn default() -> Self { State::Live(Some(Offer::default())) }
}

impl State {
    /// Whether the next frame must say what the manager offers.
    pub(crate) fn asks(self) -> bool {
        self == State::Live(None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// After a frame: what the focused field can do (nothing, with no field focused), and
    /// what the manager offers when the state asked for it.
    Frame { field: Offer, manager: Option<Offer> },
    /// The manager ran one of its actions.
    Fired(Action),
    /// The undo that primes the manager has returned, whether or not its action fired.
    Primed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// Empty both of the manager's stacks.
    Clear,
    /// Register the action, as a group of its own.
    Register(Action),
    /// Have the manager undo its last group, which fires that group's action, then feed
    /// `Event::Primed`.
    Undo,
    /// Send the focused field ⌘Z, or ⇧⌘Z for a redo.
    Key(Action),
}

/// One transition.
pub(crate) fn step(state: &mut State, event: Event) -> Effects<Effect> {
    match (*state, event) {
        (State::Live(known), Event::Frame { field, manager }) => {
            // What the manager offers: as last known, else as it says, else unknown, which
            // primes it.
            if known.or(manager) == Some(field) {
                *state = State::Live(Some(field));
                return Effects::none();
            }
            let mut effects = Effects::one(Effect::Clear);
            if field.undo { effects.push(Effect::Register(Action::Undo)); }
            if field.redo {
                *state = State::Priming;
                effects.extend([Effect::Register(Action::Undo), Effect::Undo]);
            } else {
                // What it offers now is for the next frame to ask.
                *state = State::Live(None);
            }
            effects
        }
        // The action undone to prime the manager: what it registers is the redo on offer.
        (State::Priming, Event::Fired(_)) => Effects::one(Effect::Register(Action::Redo)),
        // Priming is over even when the manager fired nothing: the next frame asks what it
        // offers, and primes it again if that is not the field's.
        (State::Priming, Event::Primed) => {
            *state = State::Live(None);
            Effects::none()
        }
        // A gesture. The field does the step, and the manager is left able to take it back
        // until the next frame says what the field can do now.
        (State::Live(_), Event::Fired(action)) => {
            *state = State::Live(None);
            let back = if action == Action::Undo { Action::Redo } else { Action::Undo };
            Effects::two(Effect::Key(action), Effect::Register(back))
        }
        _ => Effects::none(),
    }
}

#[cfg(test)]
#[path = "undo_tests.rs"]
mod tests;
