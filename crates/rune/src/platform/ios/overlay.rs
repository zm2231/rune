use std::time::Duration;

use crate::machine::{Attempt, Effects, Seq};
use crate::overlay::Placement;
use crate::{OverlayInput, OverlayPlacement};

pub(crate) fn frame(at: Placement, placement: Option<OverlayPlacement>, bounds: (f32, f32), safe: [f32; 4]) -> [f32; 4] {
    match placement {
        Some(OverlayPlacement::FullScreen | OverlayPlacement::Rect { .. }) => crate::overlay::options::local_rect(at, placement, bounds),
        _ => {
            let [top, right, bottom, left] = safe;
            let area = ((bounds.0 - left - right).max(0.0), (bounds.1 - top - bottom).max(0.0));
            let [x, y, w, h] = crate::overlay::options::local_rect(at, placement, area);
            [x + left, y + top, w, h]
        }
    }
}

pub(crate) fn takes(input: OverlayInput, interactive: bool, regions: &[[f32; 4]], x: f32, y: f32) -> bool {
    match input.resolved(interactive) {
        OverlayInput::All => true,
        OverlayInput::Regions => regions.iter().any(|r| crate::overlay::options::contains(*r, x, y)),
        OverlayInput::None | OverlayInput::Legacy => false,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum State {
    #[default]
    Hidden,
    Shown,
    Hiding(Attempt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Show(Duration),
    Hide(Duration),
    Faded(Attempt),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Effect {
    Attach { alpha: f32 },
    Fade { alpha: f32, duration: Duration, attempt: Attempt },
    Detach,
}

pub(crate) fn step(seq: &mut Seq, state: &mut State, event: Event) -> Effects<Effect> {
    match (*state, event) {
        (State::Hidden, Event::Show(duration)) => {
            *state = State::Shown;
            if duration.is_zero() { return Effects::one(Effect::Attach { alpha: 1.0 }); }
            Effects::two(Effect::Attach { alpha: 0.0 }, Effect::Fade { alpha: 1.0, duration, attempt: seq.next() })
        }
        (State::Hiding(_), Event::Show(duration)) => {
            *state = State::Shown;
            Effects::one(Effect::Fade { alpha: 1.0, duration, attempt: seq.next() })
        }
        (State::Shown, Event::Hide(duration)) if duration.is_zero() => {
            *state = State::Hidden;
            Effects::one(Effect::Detach)
        }
        (State::Shown, Event::Hide(duration)) => {
            let attempt = seq.next();
            *state = State::Hiding(attempt);
            Effects::one(Effect::Fade { alpha: 0.0, duration, attempt })
        }
        (State::Hiding(_), Event::Hide(duration)) if duration.is_zero() => {
            *state = State::Hidden;
            Effects::one(Effect::Detach)
        }
        (State::Hiding(held), Event::Faded(attempt)) if held == attempt => {
            *state = State::Hidden;
            Effects::one(Effect::Detach)
        }
        _ => Effects::none(),
    }
}

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
