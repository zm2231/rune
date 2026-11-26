use std::ops::Range;

use crate::machine::Effects;

pub(crate) const REACH: f32 = 24.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ends {
    pub(crate) selection: Range<usize>,
    pub(crate) start: [f32; 4],
    pub(crate) end: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum State {
    #[default]
    Idle,
    Dragging { touch: u64, fixed: usize, at: usize, grip: (f32, f32) },
    MenuOwed,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    Down { touch: u64, at: (f32, f32), ends: Option<Ends> },
    Move { touch: u64, at: (f32, f32), nearest: usize },
    Up { touch: u64, cancelled: bool },
    Frame,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Effect {
    Select(Range<usize>),
    LoupeBegin((f32, f32)),
    LoupeMove((f32, f32)),
    LoupeEnd,
    Menu,
}

impl State {
    pub(crate) fn dragging(&self) -> bool {
        matches!(self, State::Dragging { .. })
    }

    pub(crate) fn aim(&self, (x, y): (f32, f32)) -> Option<(f32, f32)> {
        let State::Dragging { grip, .. } = self else { return None };
        Some((x - grip.0, y - grip.1))
    }
}

fn grip([x, y, _, h]: [f32; 4], (px, py): (f32, f32), above: bool) -> Option<(f32, f32)> {
    let (top, bottom) = if above { (y - REACH, y + h + REACH / 2.0) } else { (y - REACH / 2.0, y + h + REACH) };
    ((px - x).abs() <= REACH && (top..=bottom).contains(&py)).then_some((px - x, py - (y + h / 2.0)))
}

pub(crate) fn step(state: &mut State, event: Event) -> Effects<Effect> {
    match (std::mem::take(state), event) {
        (State::Idle | State::MenuOwed, Event::Down { touch, at, ends: Some(ends) }) if !ends.selection.is_empty() => {
            let start = grip(ends.start, at, true).map(|g| (g, ends.selection.end, ends.selection.start));
            let end = grip(ends.end, at, false).map(|g| (g, ends.selection.start, ends.selection.end));
            let distance = |g: &(f32, f32)| g.0.hypot(g.1);
            let chosen = match (start, end) {
                (Some(s), Some(e)) => Some(if distance(&s.0) < distance(&e.0) { s } else { e }),
                (s, e) => s.or(e),
            };
            match chosen {
                Some((grip, fixed, at_end)) => {
                    *state = State::Dragging { touch, fixed, at: at_end, grip };
                    Effects::one(Effect::LoupeBegin(at))
                }
                None => Effects::none(),
            }
        }
        (State::Dragging { touch, fixed, at, grip }, Event::Move { touch: moved, at: point, nearest }) if touch == moved => {
            let mut effects = Effects::one(Effect::LoupeMove(point));
            let at = if nearest != fixed && nearest != at {
                effects.push(Effect::Select(fixed.min(nearest)..fixed.max(nearest)));
                nearest
            } else {
                at
            };
            *state = State::Dragging { touch, fixed, at, grip };
            effects
        }
        (State::Dragging { touch, .. }, Event::Up { touch: lifted, cancelled }) if touch == lifted => {
            if !cancelled { *state = State::MenuOwed; }
            Effects::one(Effect::LoupeEnd)
        }
        (State::MenuOwed, Event::Frame) => Effects::one(Effect::Menu),
        (State::MenuOwed, Event::Move { .. } | Event::Up { .. }) => {
            *state = State::MenuOwed;
            Effects::none()
        }
        (dragging @ State::Dragging { .. }, _) => {
            *state = dragging;
            Effects::none()
        }
        (State::Idle | State::MenuOwed, _) => Effects::none(),
    }
}

#[cfg(test)]
#[path = "handles_tests.rs"]
mod tests;
