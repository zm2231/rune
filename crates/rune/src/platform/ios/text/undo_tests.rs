use super::*;

/// An `NSUndoManager` as its documentation describes it: two stacks, and where a registered
/// action lands depends on what the manager is doing.
#[derive(Default)]
struct Manager {
    undo: Vec<Action>,
    redo: Vec<Action>,
    doing: Option<Action>,
    state: State,
    /// The keys the field was sent.
    keys: Vec<Action>,
    /// A manager that runs no action when told to undo.
    mute: bool,
    /// How many times it was asked what it offers.
    asked: usize,
}

impl Manager {
    fn offered(&self) -> Offer {
        Offer { undo: !self.undo.is_empty(), redo: !self.redo.is_empty() }
    }

    fn register(&mut self, action: Action) {
        match self.doing {
            None => {
                self.undo.push(action);
                self.redo.clear();
            }
            Some(Action::Undo) => self.redo.push(action),
            Some(Action::Redo) => self.undo.push(action),
        }
    }

    /// Undoes or redoes the last group, firing its action.
    fn perform(&mut self, which: Action) {
        let stack = if which == Action::Undo { &mut self.undo } else { &mut self.redo };
        let Some(action) = stack.pop() else { return };
        if self.mute { return; }
        self.doing = Some(which);
        self.feed(Event::Fired(action));
        self.doing = None;
    }

    fn feed(&mut self, event: Event) {
        for effect in step(&mut self.state, event) {
            match effect {
                Effect::Clear => {
                    self.undo.clear();
                    self.redo.clear();
                }
                Effect::Register(action) => self.register(action),
                Effect::Undo => {
                    self.perform(Action::Undo);
                    self.feed(Event::Primed);
                }
                Effect::Key(action) => self.keys.push(action),
            }
        }
    }

    /// A frame's end, with what the focused field can do.
    fn frame(&mut self, field: Offer) {
        let manager = self.state.asks().then(|| {
            self.asked += 1;
            self.offered()
        });
        self.feed(Event::Frame { field, manager });
    }
}

const OFFERS: [Offer; 4] = [
    Offer { undo: false, redo: false },
    Offer { undo: true, redo: false },
    Offer { undo: false, redo: true },
    Offer { undo: true, redo: true },
];

#[test]
fn the_manager_offers_what_the_field_can_do_from_wherever_it_was() {
    for from in OFFERS {
        for to in OFFERS {
            let mut manager = Manager::default();
            manager.frame(from);
            assert_eq!(manager.offered(), from, "primed to {from:?}");
            manager.frame(to);
            assert_eq!(manager.offered(), to, "from {from:?} to {to:?}");
            assert!(matches!(manager.state, State::Live(_)));
            assert_eq!(manager.keys, [], "priming sends the field nothing");
            assert!(manager.undo.len() <= 1 && manager.redo.len() <= 1, "one action each at most");
        }
    }
}

#[test]
fn the_actions_on_offer_are_the_ones_that_undo_and_redo() {
    let mut manager = Manager::default();
    manager.frame(Offer { undo: true, redo: true });
    assert_eq!((manager.undo.as_slice(), manager.redo.as_slice()), ([Action::Undo].as_slice(), [Action::Redo].as_slice()));
}

#[test]
fn a_frame_that_changes_nothing_touches_nothing() {
    for offer in OFFERS {
        let mut state = State::Live(Some(offer));
        assert_eq!(step(&mut state, Event::Frame { field: offer, manager: None }), Effects::none());
        assert!(!state.asks());
    }
}

#[test]
fn the_manager_is_asked_once_after_a_prime_or_an_action_and_not_again() {
    let mut manager = Manager::default();
    let field = Offer { undo: true, redo: true };
    for _ in 0..10 { manager.frame(Offer::default()); }
    assert_eq!(manager.asked, 0, "a new manager is known to offer nothing");
    for _ in 0..10 { manager.frame(field); }
    assert_eq!(manager.asked, 1, "once, on the frame after the prime");
    manager.perform(Action::Undo);
    for _ in 0..10 { manager.frame(field); }
    assert_eq!(manager.asked, 3, "after the action ran, and after the prime that followed");
    assert_eq!(manager.offered(), field);
}

#[test]
fn a_prime_whose_undo_fires_nothing_leaves_priming() {
    let mut manager = Manager { mute: true, ..Manager::default() };
    let field = Offer { undo: false, redo: true };
    manager.frame(field);
    assert_eq!(manager.state, State::Live(None), "not stuck priming");
    assert_eq!(manager.keys, []);
    // The manager starts answering: the next frame finds it short of the field and primes it.
    manager.mute = false;
    manager.frame(field);
    manager.frame(field);
    assert_eq!(manager.offered(), field);
    assert_eq!(manager.state, State::Live(Some(field)));
    manager.perform(Action::Redo);
    assert_eq!(manager.keys, [Action::Redo], "and its actions are the user's again");
}

#[test]
fn with_no_field_the_manager_is_emptied() {
    let mut manager = Manager::default();
    manager.frame(Offer { undo: true, redo: true });
    manager.frame(Offer::default());
    assert_eq!(manager.offered(), Offer::default());
    manager.perform(Action::Undo);
    manager.perform(Action::Redo);
    assert_eq!(manager.keys, [], "nothing to run");
}

#[test]
fn an_undo_sends_the_field_one_key_and_leaves_a_redo() {
    let mut manager = Manager::default();
    manager.frame(Offer { undo: true, redo: false });
    manager.perform(Action::Undo);
    assert_eq!(manager.keys, [Action::Undo]);
    assert_eq!(manager.offered(), Offer { undo: false, redo: true }, "until the frame says what the field has");
    // The field had more steps: the manager offers undo again, and the key was sent once.
    manager.frame(Offer { undo: true, redo: true });
    assert_eq!(manager.offered(), Offer { undo: true, redo: true });
    assert_eq!(manager.keys, [Action::Undo]);
}

#[test]
fn a_redo_sends_the_field_one_key_and_leaves_an_undo() {
    let mut manager = Manager::default();
    manager.frame(Offer { undo: false, redo: true });
    manager.perform(Action::Redo);
    assert_eq!(manager.keys, [Action::Redo]);
    assert_eq!(manager.offered(), Offer { undo: true, redo: false });
}

#[test]
fn undo_then_redo_then_undo_send_one_key_each() {
    let mut manager = Manager::default();
    manager.frame(Offer { undo: true, redo: false });
    manager.perform(Action::Undo);
    manager.frame(Offer { undo: false, redo: true });
    manager.perform(Action::Redo);
    manager.frame(Offer { undo: true, redo: false });
    manager.perform(Action::Undo);
    assert_eq!(manager.keys, [Action::Undo, Action::Redo, Action::Undo]);
}

#[test]
fn two_undos_before_a_frame_send_only_what_the_manager_held() {
    let mut manager = Manager::default();
    manager.frame(Offer { undo: true, redo: false });
    manager.perform(Action::Undo);
    manager.perform(Action::Undo);
    assert_eq!(manager.keys, [Action::Undo], "the manager held one undo");
}

#[test]
fn a_frame_while_priming_waits() {
    let mut state = State::Priming;
    let event = Event::Frame { field: Offer { undo: true, redo: false }, manager: None };
    assert_eq!(step(&mut state, event), Effects::none());
    assert_eq!(state, State::Priming);
}
