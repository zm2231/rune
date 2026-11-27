//! Non-Metal per-frame pipeline logic shared by [`super::embed::LayerHost`], compiled on all
//! test targets so the frame-step ordering, clock hand-off and request draining are verified
//! off-Apple too.

use crate::frame::clock::FrameClock;
use crate::frame::host_step::{
    self, Acquire, FrameHost, Gate, PostStepCheck, ReacquireReason, RenderPrep, StepInfo,
};
use crate::frame::loop_frame;
use crate::runtime::{AppDriver, RuntimeCore};
use crate::time::Instant;
use crate::window::Missed;

/// Consecutive lost/outdated drawable frames an embedded layer retries before stopping its
/// display-link `animating` signal until a resize or a drawn frame clears the run.
pub(crate) const EMBED_MAX_LOST_RETRIES: u8 = 5;

/// Health of an embedded layer's drawable acquisition across frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EmbedSurfaceHealth {
    #[default]
    Ready,
    Retrying { failures: u8 },
    GaveUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmbedSurfaceEvent {
    Presented,
    Missed(Missed),
    Resized,
}

impl EmbedSurfaceHealth {
    /// Steps the embedded surface state and returns whether the missed frame should ask the
    /// display link to keep ticking immediately.
    pub(crate) fn step(&mut self, event: EmbedSurfaceEvent) -> bool {
        match (*self, event) {
            (_, EmbedSurfaceEvent::Presented | EmbedSurfaceEvent::Resized) => {
                *self = Self::Ready;
                false
            }
            (Self::Ready, EmbedSurfaceEvent::Missed(Missed::Outdated)) => {
                *self = Self::Retrying { failures: 1 };
                true
            }
            (Self::Retrying { failures }, EmbedSurfaceEvent::Missed(Missed::Outdated)) => {
                let next = failures.saturating_add(1);
                if next >= EMBED_MAX_LOST_RETRIES {
                    *self = Self::GaveUp;
                    false
                } else {
                    *self = Self::Retrying { failures: next };
                    true
                }
            }
            (_, EmbedSurfaceEvent::Missed(Missed::Unconfigured)) => {
                *self = Self::GaveUp;
                false
            }
            (_, EmbedSurfaceEvent::Missed(_)) => false,
        }
    }
}

/// Summary produced after presenting (or missing) one embedded layer frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FrameSummary {
    pub(crate) drew: bool,
    pub(crate) animating: bool,
    pub(crate) wake_after_secs: Option<f64>,
    pub(crate) cpu_us: u64,
    pub(crate) drawn: Option<Drawn>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Drawn {
    pub(crate) started: Instant,
    pub(crate) submitted: Instant,
    pub(crate) update_ms: f64,
}

/// What an embedded host serves itself. Everything else an app asks of its window is refused
/// or dropped, since an embedded view has nothing of its own to show it with.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Serves {
    /// Copies go to the system clipboard (`LayerHost::with_system_clipboard`), not to memory.
    pub(crate) clipboard: bool,
    /// The host shows pop-up menus itself (`LayerHost::with_popup_menus`), not cancelling them.
    pub(crate) popup_menus: bool,
    /// The host shows file dialogs itself (`LayerHost::with_file_dialogs`), not refusing them.
    pub(crate) file_dialogs: bool,
    /// The host shows share sheets itself (`LayerHost::with_share_sheets`), not refusing them.
    pub(crate) shares: bool,
    /// The host takes window close requests itself (`LayerHost::with_shell`), not dropping them.
    pub(crate) shell: bool,
}

/// Surface and draw callbacks supplied by [`super::embed::LayerHost`] (or a fake surface in
/// off-Apple unit tests) to [`run_embed_frame`].
pub(crate) trait EmbedSurface {
    type Drawable;
    type Prepared;

    fn suspended(&self) -> bool;
    fn surface_health_mut(&mut self) -> &mut EmbedSurfaceHealth;
    fn renderer_srgb_blend(&self) -> bool;
    fn rt_srgb_blend(&self) -> bool;
    fn reblend(&mut self) -> bool;
    fn acquire(&mut self) -> Acquire<Self::Drawable>;
    /// A frame got no drawable at `now`: the surface's own bookkeeping. Returns whether the
    /// surface now waits before it is tried again, which stops the display link meanwhile.
    fn missed(&mut self, _missed: Missed, _now: Instant) -> bool { false }
    /// Seconds until a surface waiting after a missed drawable is tried again.
    fn retry_after(&self, _now: Instant) -> Option<f64> { None }
    /// A frame presented; `ticking` says whether another follows on the next tick.
    fn presented(&mut self, _ticking: bool) {}
    /// After the step: brings presenting in Core Animation transactions in line with whether
    /// the frame hosts native views. True when it just turned on, so the drawable acquired
    /// before the step is dropped and another taken: the layer reads the setting on acquire.
    fn sync_hosts(&mut self) -> bool { false }
    /// What the host serves itself instead of refusing.
    fn serves(&self) -> Serves { Serves::default() }
    /// Takes what the system clipboard answered since the last frame, before the runtime steps.
    fn receive_clipboard(&mut self) {}
    /// Sends what the frame copied to the system clipboard, once the runtime has stepped.
    fn flush_clipboard(&mut self) {}
    fn prepare(&mut self, drawable: Self::Drawable, looped: Option<Vec<u32>>) -> Self::Prepared;
    fn compositor_animating(&self) -> bool;
    fn render(&mut self, prepared: &mut Self::Prepared, dt: f64, update_ms: f64) -> bool;
    fn submit_and_present(&mut self, prepared: Self::Prepared) -> Instant;
    fn clock_and_rt_mut(&mut self) -> (&mut FrameClock, &mut RuntimeCore);
    fn with_driver<R>(&mut self, f: impl FnOnce(&mut FrameClock, &mut RuntimeCore, &mut dyn AppDriver) -> R) -> R;
}

struct EmbedFrameHost<'a, E: EmbedSurface> {
    surface: &'a mut E,
    /// When the frame will be on screen, from the host's display link; `None` steps to now.
    target: Option<Instant>,
    only_loops: bool,
    looped: Option<Vec<u32>>,
    t0: Option<Instant>,
    summary: FrameSummary,
}

impl<E: EmbedSurface> FrameHost for EmbedFrameHost<'_, E> {
    type Drawable = E::Drawable;
    type Prepared = E::Prepared;

    #[inline]
    fn gate(&mut self) -> Gate {
        if self.surface.suspended() {
            return Gate::Skip;
        }
        let (_, rt) = self.surface.clock_and_rt_mut();
        let _ = rt.follow_font_families();
        Gate::Draw
    }

    #[inline]
    fn needs_reblend(&self) -> bool {
        self.surface.renderer_srgb_blend() != self.surface.rt_srgb_blend()
    }

    #[inline]
    fn reblend(&mut self) -> bool {
        self.surface.reblend()
    }

    #[inline]
    fn acquire(&mut self) -> Acquire<Self::Drawable> {
        self.surface.acquire()
    }

    #[inline]
    fn step_runtime(&mut self) -> StepInfo {
        let t0 = crate::time::now();
        self.t0 = Some(t0);
        let target = self.target.unwrap_or(t0);
        let compositing = self.surface.compositor_animating();
        if self.only_loops && !compositing {
            let stepped = if self.surface.clock_and_rt_mut().1.poll_timer_due() {
                self.surface.with_driver(|clock, rt, app| loop_frame::step(clock, rt, Some((app, false)), target))
            } else {
                let (clock, rt) = self.surface.clock_and_rt_mut();
                loop_frame::step(clock, rt, None, target)
            };
            if let Ok((patched, dt)) = stepped {
                let animating = self.surface.clock_and_rt_mut().1.animating;
                self.looped = Some(patched);
                return StepInfo { runtime_animating: animating, animating, dt, update_ms: t0.elapsed().as_secs_f64() * 1000.0, presents_at: self.target };
            }
        }
        let serves = self.surface.serves();
        self.surface.receive_clipboard();
        let (animating, dt, _) = self.surface.with_driver(|clock, rt, app| {
            step_and_flush(clock, rt, app, target, compositing, serves)
        });
        self.surface.flush_clipboard();
        StepInfo {
            runtime_animating: animating,
            animating,
            dt,
            update_ms: t0.elapsed().as_secs_f64() * 1000.0,
            presents_at: self.target,
        }
    }

    #[inline]
    fn post_step_check(&mut self, _info: &mut StepInfo) -> PostStepCheck {
        if self.needs_reblend() {
            PostStepCheck::Reacquire(ReacquireReason::Reblend)
        } else if self.surface.sync_hosts() {
            PostStepCheck::Reacquire(ReacquireReason::NativeMaterials)
        } else {
            PostStepCheck::Keep
        }
    }

    #[inline]
    fn prepare_reacquire(&mut self, reason: ReacquireReason, _info: &mut StepInfo) -> bool {
        match reason {
            ReacquireReason::Reblend => self.surface.reblend(),
            #[cfg(any(test, target_os = "macos", target_os = "ios"))]
            ReacquireReason::NativeMaterials => true,
            #[cfg(not(target_arch = "wasm32"))]
            ReacquireReason::ContentResize => true,
        }
    }

    #[inline]
    fn on_missed(&mut self, missed: Missed, stepped: bool) {
        let t0 = self.t0.unwrap_or_else(crate::time::now);
        let retry_now = self.surface.surface_health_mut().step(EmbedSurfaceEvent::Missed(missed));
        let comp_animating = self.surface.compositor_animating();
        let waiting = self.surface.missed(missed, t0);
        let mut summary = if stepped {
            let (_, rt) = self.surface.clock_and_rt_mut();
            missed_after_step(rt, t0, comp_animating, retry_now)
        } else {
            let serves = self.surface.serves();
            self.surface.receive_clipboard();
            let summary = self.surface.with_driver(|_, rt, app| missed_step(rt, app, t0, comp_animating, retry_now, serves));
            self.surface.flush_clipboard();
            summary
        };
        if waiting {
            summary.animating = false;
        }
        if let Some(retry) = self.surface.retry_after(t0) {
            summary.wake_after_secs = Some(summary.wake_after_secs.map_or(retry, |wake| wake.min(retry)));
        }
        self.summary = summary;
    }

    #[inline]
    fn prepare_render(&mut self, drawable: Self::Drawable, _info: &mut StepInfo) -> RenderPrep<Self::Prepared> {
        RenderPrep::Ready(self.surface.prepare(drawable, self.looped.take()))
    }

    #[inline]
    fn render(&mut self, prepared: &mut Self::Prepared, info: &StepInfo) -> bool {
        self.surface.render(prepared, info.dt, info.update_ms)
    }

    #[inline]
    fn submit_and_present(&mut self, prepared: Self::Prepared, _info: &StepInfo) -> Instant {
        self.surface.submit_and_present(prepared)
    }

    #[inline]
    fn present_targets(&mut self) -> (Option<&mut FrameClock>, &mut RuntimeCore) {
        let (clock, rt) = self.surface.clock_and_rt_mut();
        (Some(clock), rt)
    }

    #[inline]
    fn settle(&mut self, submitted: Instant, info: &StepInfo) {
        let t0 = self.t0.unwrap_or(submitted);
        self.surface.surface_health_mut().step(EmbedSurfaceEvent::Presented);
        let (_, rt) = self.surface.clock_and_rt_mut();
        self.summary = frame_summary_presented(rt, submitted, t0, info.animating);
        self.summary.drawn = Some(Drawn { started: t0, submitted, update_ms: info.update_ms });
        self.surface.presented(self.summary.animating);
    }
}

/// Runs one embedded layer frame through the shared [`crate::frame::host_step::run`] sequencer.
#[cfg(test)]
pub(crate) fn run_embed_frame<E: EmbedSurface>(surface: &mut E) -> FrameSummary {
    run_embed_frame_at(surface, None, false)
}

/// [`run_embed_frame`], stepping the runtime to `target`, the time the frame will be on screen,
/// when the host's display link gives one.
#[inline]
pub(crate) fn run_embed_frame_at<E: EmbedSurface>(surface: &mut E, target: Option<Instant>, only_loops: bool) -> FrameSummary {
    let mut host = EmbedFrameHost {
        surface,
        target,
        only_loops,
        looped: None,
        t0: None,
        summary: FrameSummary::default(),
    };
    host_step::run(&mut host);
    host.summary
}

/// Drains and discards host requests an embedded layer cannot serve (window close/minimize/frame,
/// native pop-ups, native file dialogs) and flushes clipboard writes to memory unless the host
/// `serves` the system clipboard. Leaves `rt.host.haptics` intact so the embedding host drains
/// them via `LayerHost::take_haptics()`.
pub(crate) fn flush_requests(rt: &mut RuntimeCore, app: &mut dyn AppDriver, serves: Serves) {
    if !serves.clipboard {
        crate::clipboard::flush_memory(rt);
    }
    if !serves.popup_menus && rt.popup_menu().is_some() {
        rt.cancel_popups(app);
    }
    if !serves.file_dialogs {
        rt.refuse_file_dialog("native file dialogs are unsupported in embedded views");
    }
    if !serves.shares {
        rt.refuse_share("share sheets are unsupported in embedded views");
    }
    if !serves.shell { rt.shell_request = None; }
    rt.host.user_close_request = false;
    rt.ignore_minimize("an embedded layer");
    rt.host.window_frame = None;
}

/// Advances the runtime on `clock` for a frame whose drawable has already been acquired, passing
/// `compositing` so a compositor-driven animation keeps real frame gaps instead of clamping to
/// `RESUME_STEP`.
pub(crate) fn step_and_flush(
    clock: &mut FrameClock,
    rt: &mut RuntimeCore,
    app: &mut dyn AppDriver,
    t0: Instant,
    compositing: bool,
    serves: Serves,
) -> (bool, f64, f64) {
    let (animating, dt) = clock.advance_core(rt, app, t0, compositing);
    let update_ms = t0.elapsed().as_secs_f64() * 1000.0;
    flush_requests(rt, app, serves);
    (animating, dt, update_ms)
}

/// Runs due timers, `poll` and queued input when drawable acquisition failed before stepping,
/// without advancing `FrameClock` (matching `window/host/draw.rs::missed_frame`).
pub(crate) fn missed_step(
    rt: &mut RuntimeCore,
    app: &mut dyn AppDriver,
    t0: Instant,
    comp_animating: bool,
    retry_now: bool,
    serves: Serves,
) -> FrameSummary {
    rt.covered_wake(app);
    flush_requests(rt, app, serves);
    missed_after_step(rt, t0, comp_animating, retry_now)
}

/// Computes the missed-frame summary when the runtime has already stepped (e.g. a post-step
/// blend-mode switch refused reconfiguration or re-acquisition failed).
pub(crate) fn missed_after_step(
    rt: &RuntimeCore,
    t0: Instant,
    comp_animating: bool,
    retry_now: bool,
) -> FrameSummary {
    FrameSummary {
        drew: false,
        animating: retry_now || comp_animating || rt.animating || rt.stale() || rt.has_pending_input() || rt.live_pending(),
        wake_after_secs: wake_after_secs(rt),
        cpu_us: t0.elapsed().as_micros() as u64,
        drawn: None,
    }
}

/// Computes the presented-frame summary after [`host_step::run`] has recorded presentation on
/// `clock` and interaction timing.
fn frame_summary_presented(
    rt: &RuntimeCore,
    submitted: Instant,
    t0: Instant,
    animating: bool,
) -> FrameSummary {
    let cpu_us = submitted.saturating_duration_since(t0).as_micros() as u64;
    let keep_ticking = animating || rt.stale() || rt.has_pending_input() || rt.live_pending();
    FrameSummary {
        drew: true,
        animating: keep_ticking,
        wake_after_secs: wake_after_secs(rt),
        cpu_us,
        drawn: None,
    }
}

/// Records presentation on `clock` and interaction timing, then computes the frame outcome.
#[cfg(test)]
pub(crate) fn post_present(
    clock: &mut FrameClock,
    rt: &mut RuntimeCore,
    submitted: Instant,
    t0: Instant,
    animating: bool,
    comp_animating: bool,
) -> FrameSummary {
    clock.presented_core(rt, submitted);
    rt.interaction_presented(submitted, None);
    frame_summary_presented(rt, submitted, t0, animating || comp_animating)
}

pub(crate) fn wake_after_secs(rt: &RuntimeCore) -> Option<f64> {
    rt.wake_deadline().and_then(|d| {
        let elapsed = rt.wall_start?.elapsed().as_secs_f64();
        Some((d - elapsed).max(0.0))
    })
}

#[cfg(test)]
#[path = "embed_frame_tests.rs"]
mod tests;
