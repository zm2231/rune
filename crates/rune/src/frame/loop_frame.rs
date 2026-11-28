//! A frame only a loop asked for: a spinner's next dot, a shimmer's next step. The runtime
//! samples the loops into the instances they drew and nothing else runs (`Runtime::loop_step`:
//! no poll, view, layout or paint). A Windows window redraws and presents only where they draw
//! (`Renderer::arm_loop`); other windows and iOS views draw the whole frame from those
//! instances. A macOS window draws whole too: Metal can't keep a drawable's pixels, and the
//! render pass into a kept frame plus its copy cost more than drawing the window again. Anything that reached the window or view since its last frame (input, a request,
//! an assistive client's action) makes the next one an ordinary frame, so nothing waits behind
//! a loop. An app wake or a due timer runs the app's poll first, and only a poll that changed
//! something takes an ordinary frame.

use crate::frame::clock::FrameClock;
use crate::runtime::{AppDriver, RuntimeCore};
use crate::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LoopFrame {
    #[default]
    Loops,
    Woken,
    Touched,
}

impl LoopFrame {
    /// Something other than its own frames or an app wake reached the window or view.
    pub(crate) fn touched(&mut self) { *self = LoopFrame::Touched; }

    pub(crate) fn woke(&mut self) {
        if *self == LoopFrame::Loops { *self = LoopFrame::Woken; }
    }

    pub(crate) fn take(&mut self) -> LoopFrame { std::mem::take(self) }

    /// A frame begins: whether a loop-only step may stand for it. Counts afresh.
    #[cfg_attr(not(any(target_os = "ios", test)), allow(dead_code))]
    pub(crate) fn only_loops(&mut self) -> bool { self.take() != LoopFrame::Touched }

    #[cfg(target_os = "macos")]
    pub(crate) fn untouched(&self) -> bool { *self == LoopFrame::Loops }
}

/// A loop-only step for the frame presented at `at` (`Runtime::loop_step`): the instances it
/// wrote and the seconds it stepped, or why it takes an ordinary frame. With `poll`, the app's
/// driver and whether an app wake asked for the frame, a frame that owes the app its poll runs
/// it first (`Runtime::loop_step_polled`). The frame clock takes the step only when the
/// runtime does.
pub(crate) fn step(clock: &mut FrameClock, rt: &mut RuntimeCore, poll: Option<(&mut dyn AppDriver, bool)>, at: Instant) -> Result<(Vec<u32>, f64), &'static str> {
    let dt = at.saturating_duration_since(clock.last()).as_secs_f64();
    let patched = match poll {
        Some((app, woke)) => rt.loop_step_polled(app, dt, woke)?,
        None => rt.loop_step(dt)?,
    };
    clock.frame_step(at, true);
    rt.frame.set_frame_target(at);
    Ok((patched, dt))
}

/// frame-profile, `RUNE_MISS_LOOP_FRAMES=<n>`: this loop-only frame is the nth since the last
/// forced miss, and acts as if its drawable never came (a diagnostic).
#[cfg(feature = "frame-profile")]
pub(crate) fn forced_miss() -> bool {
    use std::sync::atomic::{AtomicU64, Ordering};
    static EVERY: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    static SEEN: AtomicU64 = AtomicU64::new(0);
    let every = *EVERY.get_or_init(|| rune_core::env::var("RUNE_MISS_LOOP_FRAMES").and_then(|v| v.parse().ok()).unwrap_or(0));
    every > 0 && SEEN.fetch_add(1, Ordering::Relaxed) % every == every - 1
}
