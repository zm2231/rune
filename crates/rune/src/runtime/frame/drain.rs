//! Bounded same-turn follow-up drain: a turn does not end with work that it itself queued still
//! waiting; it repeats follow-up passes while due work remains, at most [`MAX_TURN_FOLLOW_UPS`]
//! times, and then stops, leaving any excess work queued for a later turn so a self-requeueing
//! callback cannot spin the host.
//!
//! Used by hidden and covered wakes (draining input that `poll` or a handler queued without
//! drawing) and shared for any same-turn follow-up pass in the frame loop.

use crate::machine::Effects;

/// Maximum follow-up passes a single turn may run to drain work that it itself queued
/// (matching [`super::mount::MAX_MOUNT_ROUNDS`]), so a self-requeueing handler or poll cannot
/// spin a window.
pub(in crate::runtime) const MAX_TURN_FOLLOW_UPS: usize = super::mount::MAX_MOUNT_ROUNDS;

/// Explicit state of a bounded same-turn follow-up drain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::runtime) enum DrainState {
    /// Initial pass has not yet triggered a follow-up.
    #[default]
    Active,
    /// Follow-up pass `passes` (`1..=MAX_TURN_FOLLOW_UPS`) is running.
    Draining { passes: usize },
    /// No same-turn work remained, or [`MAX_TURN_FOLLOW_UPS`] was reached.
    Done,
}

/// Observation after a turn's initial step or after a follow-up pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runtime) enum DrainEvent {
    /// `more` is whether the turn left same-turn work waiting.
    StepDone { more: bool },
}

/// Effect emitted by [`step_drain`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runtime) enum DrainEffect {
    /// Run one bounded follow-up pass (`pass` is 1-based, up to [`MAX_TURN_FOLLOW_UPS`]).
    FollowUp { pass: usize },
}

/// Advances [`DrainState`] after a pass finishes: emits [`DrainEffect::FollowUp`] while `more`
/// holds and fewer than [`MAX_TURN_FOLLOW_UPS`] follow-up passes have run, then transitions to
/// [`DrainState::Done`].
pub(in crate::runtime) fn step_drain(
    state: &mut DrainState,
    event: DrainEvent,
) -> Effects<DrainEffect> {
    let DrainEvent::StepDone { more } = event;
    let ran = match *state {
        DrainState::Active => 0,
        DrainState::Draining { passes } => passes,
        DrainState::Done => return Effects::none(),
    };
    if more && ran < MAX_TURN_FOLLOW_UPS {
        let pass = ran + 1;
        *state = DrainState::Draining { passes: pass };
        Effects::one(DrainEffect::FollowUp { pass })
    } else {
        *state = DrainState::Done;
        Effects::none()
    }
}

/// Runs follow-up passes while `has_more` reports same-turn work queued by the current turn,
/// bounded to [`MAX_TURN_FOLLOW_UPS`]. Returns the number of follow-up passes executed.
pub(in crate::runtime) fn drain_turn<C>(
    ctx: &mut C,
    mut has_more: impl FnMut(&C) -> bool,
    mut run_pass: impl FnMut(&mut C, usize),
) -> usize {
    let mut state = DrainState::Active;
    let mut ran = 0;
    loop {
        let effects = step_drain(&mut state, DrainEvent::StepDone { more: has_more(ctx) });
        let Some(DrainEffect::FollowUp { pass }) = effects.into_iter().next() else {
            return ran;
        };
        ran = pass;
        run_pass(ctx, pass);
    }
}
