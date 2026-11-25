//! When a view's display link ticks. It runs while something moves and pauses once a frame
//! settles, so an idle view costs no CPU; input, the app's waker and a due timer resume it.
//!
//! Plain data in and [`Effects`] out, free of UIKit, so every transition runs in tests on any
//! host. The view turns [`PacingEffect`]s into `CADisplayLink.paused` and a main-queue timer.

use rune::machine::{Attempt, Effects, Seq};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Pacing {
    /// Ticking: every display refresh runs a frame.
    #[default]
    Running,
    /// Paused with nothing scheduled: only a wake resumes it.
    Paused,
    /// Paused until a timer for `attempt` fires at `due` (seconds on the host's clock), or a
    /// wake comes first.
    Sleeping { attempt: Attempt, due: f64 },
    /// The app is in the background, where Metal may not draw. Only the foreground resumes it.
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PacingEvent {
    /// Something may need a frame: a touch, the app's waker, a resize, a trait change.
    Wake,
    /// A frame ran at `now`. `animating` and `wake_after` are the frame's own answer; `period`
    /// is the display's refresh period, all in seconds.
    Framed { animating: bool, wake_after: Option<f64>, now: f64, period: f64 },
    Timer { attempt: Attempt },
    Background,
    Foreground,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PacingEffect {
    Resume,
    Pause,
    /// Feed `PacingEvent::Timer { attempt }` back in after `after` seconds.
    ArmTimer { attempt: Attempt, after: f64 },
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Pacer {
    pub(crate) state: Pacing,
    seq: Seq,
}

/// One transition. A timer whose attempt the state no longer holds changes nothing, so a wake
/// that came first leaves no stale timer behind.
pub(crate) fn step(pacer: &mut Pacer, event: PacingEvent) -> Effects<PacingEffect> {
    let was_running = pacer.state == Pacing::Running;
    match (pacer.state, event) {
        (Pacing::Background, PacingEvent::Foreground) => {
            pacer.state = Pacing::Running;
            Effects::one(PacingEffect::Resume)
        }
        (Pacing::Background, _) => Effects::none(),
        (_, PacingEvent::Background) => {
            pacer.state = Pacing::Background;
            if was_running { Effects::one(PacingEffect::Pause) } else { Effects::none() }
        }
        (Pacing::Sleeping { attempt: armed, .. }, PacingEvent::Timer { attempt }) if armed == attempt => resume(pacer),
        (_, PacingEvent::Timer { .. }) => Effects::none(),
        (_, PacingEvent::Wake | PacingEvent::Foreground) => resume(pacer),
        (_, PacingEvent::Framed { animating, wake_after, now, period }) => {
            let soon = wake_after.is_some_and(|after| after <= period);
            if animating || soon {
                return resume(pacer);
            }
            let mut effects = Effects::none();
            if was_running {
                effects.push(PacingEffect::Pause);
            }
            match wake_after {
                Some(after) => {
                    let attempt = pacer.seq.next();
                    pacer.state = Pacing::Sleeping { attempt, due: now + after };
                    effects.push(PacingEffect::ArmTimer { attempt, after });
                }
                None => pacer.state = Pacing::Paused,
            }
            effects
        }
    }
}

fn resume(pacer: &mut Pacer) -> Effects<PacingEffect> {
    let was_running = pacer.state == Pacing::Running;
    pacer.state = Pacing::Running;
    if was_running { Effects::none() } else { Effects::one(PacingEffect::Resume) }
}

#[cfg(test)]
#[path = "pacing_tests.rs"]
mod tests;
