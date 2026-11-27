//! A frame only a loop asked for: a spinner's next dot, a shimmer's next step. The runtime
//! samples the loops into the instances they drew and nothing else runs (`Runtime::loop_step`:
//! no poll, view, layout or paint). A Windows window redraws and presents only where they draw
//! (`Renderer::arm_loop`); other windows and iOS views draw the whole frame from those
//! instances. Anything that reached the window or view since its last frame (input, a wake, a
//! request, an assistive client's action) makes the next one an ordinary frame, so nothing
//! waits behind a loop.

use crate::frame::clock::FrameClock;
use crate::runtime::RuntimeCore;
use crate::time::Instant;

#[derive(Clone, Copy, Default)]
pub(crate) struct LoopFrame {
    touched: bool,
}

impl LoopFrame {
    /// Something other than its own frames reached the window or view.
    pub(crate) fn touched(&mut self) { self.touched = true; }

    /// A frame begins: whether only its loops asked for it since the last. Counts afresh.
    pub(crate) fn only_loops(&mut self) -> bool { !std::mem::replace(&mut self.touched, false) }

    #[cfg(target_os = "macos")]
    pub(crate) fn untouched(&self) -> bool { !self.touched }
}

/// A loop-only step for the frame presented at `at` (`Runtime::loop_step`): the instances it
/// wrote and the seconds it stepped, or why it takes an ordinary frame. The frame clock takes
/// the step only when the runtime does.
pub(crate) fn step(clock: &mut FrameClock, rt: &mut RuntimeCore, at: Instant) -> Result<(Vec<u32>, f64), &'static str> {
    let dt = at.saturating_duration_since(clock.last()).as_secs_f64();
    let patched = rt.loop_step(dt)?;
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
