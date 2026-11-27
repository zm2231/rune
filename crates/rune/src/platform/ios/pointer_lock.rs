use crate::machine::{Attempt, Effects, Seq};

pub(crate) const ANSWER_SECS: f64 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum State {
    #[default]
    Free,
    Asking(Attempt),
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Grab,
    Ungrab,
    Changed { locked: bool },
    Due { attempt: Attempt, locked: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Effect {
    Prefer(bool),
    Motion(bool),
    Granted,
    Lost,
    AnswerAfter(Attempt, f64),
}

pub(crate) fn step(seq: &mut Seq, state: &mut State, event: Event) -> Effects<Effect> {
    match (*state, event) {
        (State::Free, Event::Grab) => {
            let attempt = seq.next();
            *state = State::Asking(attempt);
            Effects::two(Effect::Prefer(true), Effect::AnswerAfter(attempt, ANSWER_SECS))
        }
        (State::Asking(_), Event::Ungrab) => {
            *state = State::Free;
            Effects::one(Effect::Prefer(false))
        }
        (State::Held, Event::Ungrab) => {
            *state = State::Free;
            Effects::two(Effect::Motion(false), Effect::Prefer(false))
        }
        (State::Asking(_), Event::Changed { locked: true }) => {
            *state = State::Held;
            Effects::two(Effect::Motion(true), Effect::Granted)
        }
        (State::Asking(held), Event::Due { attempt, locked }) if held == attempt => {
            if locked {
                *state = State::Held;
                Effects::two(Effect::Motion(true), Effect::Granted)
            } else {
                *state = State::Free;
                Effects::two(Effect::Prefer(false), Effect::Lost)
            }
        }
        (State::Held, Event::Changed { locked: false }) => {
            *state = State::Free;
            let mut effects = Effects::two(Effect::Motion(false), Effect::Prefer(false));
            effects.push(Effect::Lost);
            effects
        }
        (State::Free, Event::Ungrab | Event::Changed { .. } | Event::Due { .. })
        | (State::Asking(_) | State::Held, Event::Grab)
        | (State::Asking(_), Event::Changed { locked: false } | Event::Due { .. })
        | (State::Held, Event::Changed { locked: true } | Event::Due { .. }) => Effects::none(),
    }
}

#[cfg(test)]
#[path = "pointer_lock_tests.rs"]
mod tests;
