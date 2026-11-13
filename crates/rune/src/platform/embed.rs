//! Embedded `CAMetalLayer` host for UIKit, SwiftUI and AppKit views that own their own window
//! and event loop (e.g. a `UIViewRepresentable` driven by `CADisplayLink` on iOS).

use std::ffi::c_void;
use std::sync::{Arc, OnceLock};

use crate::compose::{Compositor, Frame};
use crate::frame::clock::FrameClock;
use crate::frame::surface_stats::{Pacing, SurfaceMeter, SurfaceStats};
use crate::frame::host_step::{self, Acquire};
use crate::platform::embed_frame::{self, EmbedSurface, FrameSummary, Serves};
use crate::render::{Gpu, Renderer};
use crate::runtime::{AppDriver, Env, Runtime, RuntimeCore, Waker};
use crate::time::Instant;
use crate::tooling::{AppSpec, runtime_for};
use crate::window::{Backoff, BackoffEvent, Missed};

static SHARED_GPU: OnceLock<Result<Arc<Gpu>, String>> = OnceLock::new();

fn shared_gpu() -> Result<Arc<Gpu>, String> {
    SHARED_GPU
        .get_or_init(|| Gpu::try_new(Gpu::instance(), None).map(Arc::new))
        .clone()
}

/// Warms the shared Metal device and compiles the default `Bgra8Unorm` uber pipeline on a
/// background thread so the first embedded `LayerHost` opens without shader-compile latency.
pub fn prewarm() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let _ = std::thread::Builder::new().name("rune-embed-prewarm".into()).spawn(|| {
            if let Ok(gpu) = shared_gpu() {
                let _ = Renderer::new(&gpu.device, wgpu::TextureFormat::Bgra8Unorm);
            }
        });
    });
}

/// Outcome of one [`LayerHost::frame`] pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmbedFrame {
    /// Whether a drawable was acquired and presented.
    pub drew: bool,
    /// True while springs, reveals, pending input or live bindings need another display-link tick.
    pub animating: bool,
    /// Seconds until the next scheduled timer wake (`None` when none is pending).
    pub wake_after_secs: Option<f64>,
    /// CPU time spent stepping layout/motion and encoding GPU work, in microseconds.
    pub cpu_us: u64,
}

impl From<FrameSummary> for EmbedFrame {
    fn from(s: FrameSummary) -> Self {
        Self {
            drew: s.drew,
            animating: s.animating,
            wake_after_secs: s.wake_after_secs,
            cpu_us: s.cpu_us,
        }
    }
}

pub(crate) struct PreparedEmbedFrame {
    frame: wgpu::SurfaceTexture,
    target: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
    size: (u32, u32),
    format: wgpu::TextureFormat,
}

struct LayerHostCore {
    gpu: Arc<Gpu>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    clock: FrameClock,
    suspended: bool,
    /// Whether the surface took its last configuration. One that didn't has no drawable to give.
    configured: bool,
    /// The wait after a frame that got no drawable.
    backoff: Backoff,
    meter: SurfaceMeter,
    /// The system clipboard, for a host that asked for it.
    clipboard: Option<crate::clipboard::Client>,
    /// The host shows pop-up menus itself.
    popup_menus: bool,
    /// The host shows file dialogs itself.
    file_dialogs: bool,
}

impl LayerHostCore {
    unsafe fn new(layer: *mut c_void, title: &str, viewport: (f32, f32), scale: f32) -> Result<(Self, (f32, f32), f32, Instant), String> {
        if layer.is_null() {
            return Err("null CAMetalLayer pointer".into());
        }
        let gpu = shared_gpu()?;
        let surface = unsafe {
            gpu.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
                .map_err(|e| format!("create_surface_unsafe: {e}"))?
        };
        let scale = scale.max(1.0);
        let max_dim = gpu.device.limits().max_texture_dimension_2d.max(1);
        let px_w = ((viewport.0.max(1.0) * scale).round() as u32).clamp(1, max_dim);
        let px_h = ((viewport.1.max(1.0) * scale).round() as u32).clamp(1, max_dim);
        let srgb_blend = true;
        let (config, _) = crate::window::surface_config(&surface, &gpu.adapter, (px_w, px_h), srgb_blend)
            .ok_or_else(|| "CAMetalLayer has no supported wgpu surface configuration".to_owned())?;
        crate::window::configure(&surface, &gpu, &config)?;
        let target_format = config.format.remove_srgb_suffix();
        let mut renderer = Renderer::new(&gpu.device, target_format);
        renderer.set_srgb_blend(true);
        let size = (px_w as f32 / scale, px_h as f32 / scale);
        let now = Instant::now();
        let core = Self {
            gpu,
            surface,
            config,
            renderer,
            clock: FrameClock::new(now),
            suspended: viewport.0 <= 0.0 || viewport.1 <= 0.0,
            configured: true,
            backoff: Backoff::default(),
            meter: SurfaceMeter::new(title, Arc::default()),
            clipboard: None,
            popup_menus: false,
            file_dialogs: false,
        };
        Ok((core, size, scale, now))
    }

    fn configure_runtime(&mut self, rt: &mut RuntimeCore, now: Instant) {
        rt.set_srgb_blend(true);
        rt.set_os_drag(false);
        rt.wall_start = Some(now);
        rt.frame.record_events = false;
        rt.host.surface_stats = Some(self.meter.shared());
        let target_format = if rt.srgb_blend() { self.config.format.remove_srgb_suffix() } else { self.config.format };
        crate::window::rebuild_renderer_core(&self.gpu.device, &mut self.renderer, target_format, rt);
    }

    fn resize(&mut self, rt: &mut RuntimeCore, viewport: (f32, f32), scale: f32) {
        if viewport.0 <= 0.0 || viewport.1 <= 0.0 {
            self.suspended = true;
            return;
        }
        self.suspended = false;
        let scale = scale.max(1.0);
        let max_dim = self.gpu.device.limits().max_texture_dimension_2d.max(1);
        let px_w = ((viewport.0 * scale).round() as u32).clamp(1, max_dim);
        let px_h = ((viewport.1 * scale).round() as u32).clamp(1, max_dim);
        if self.config.width != px_w || self.config.height != px_h || !self.configured {
            self.config.width = px_w;
            self.config.height = px_h;
            self.configure();
        }
        rt.set_viewport((px_w as f32 / scale, px_h as f32 / scale), scale);
    }

    /// Configures the surface, saying so when the GPU refuses: the host then draws nothing
    /// until a resize configures it again.
    fn configure(&mut self) {
        self.configured = match crate::window::configure(&self.surface, &self.gpu, &self.config) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("rune: an embedded surface refused its configuration: {error}");
                false
            }
        };
        let _ = self.backoff.step(BackoffEvent::Lapsed);
    }

    /// Seconds until the host should call `frame` again with nothing moving: the runtime's
    /// next timer, or the retry of a frame that got no drawable, whichever is sooner.
    fn wake_after(&self, rt: &RuntimeCore, now: Instant) -> Option<f64> {
        sooner(embed_frame::wake_after_secs(rt), self.retry_after(now))
    }

    /// Seconds until a frame that got no drawable is tried again.
    fn retry_after(&self, now: Instant) -> Option<f64> {
        self.backoff.deadline().map(|at| at.saturating_duration_since(now).as_secs_f64())
    }

    /// The next drawable, as a window takes one: an outdated or lost surface is configured
    /// again and tried once more, and one whose configuration didn't take is never asked.
    fn acquire(&mut self) -> Acquire<wgpu::SurfaceTexture> {
        self.meter.redraw(true);
        match crate::window::acquire_checked(&self.surface, &self.gpu, &self.config, self.configured, 1, &mut self.meter, true) {
            Ok(frame) => Acquire::Ready(frame),
            Err(missed) => Acquire::Missed(missed),
        }
    }

    /// A frame got no drawable at `now`. One the GPU refuses to configure rests until a resize;
    /// every other miss waits, the host's display link pacing a timeout's retry. Returns whether
    /// the surface now waits.
    fn missed(&mut self, missed: Missed, now: Instant) -> bool {
        self.meter.set_pacing(Pacing::Idle);
        if missed == Missed::Unconfigured {
            self.configured = false;
            return true;
        }
        let _ = self.backoff.step(BackoffEvent::Missed { missed, paced: true, now });
        self.backoff.deadline().is_some()
    }

    /// A frame presented: the waits start over, and the display link keeps pacing while
    /// another frame follows.
    fn presented(&mut self, ticking: bool) {
        self.meter.presented();
        let _ = self.backoff.step(BackoffEvent::Presented);
        self.meter.set_pacing(if ticking { Pacing::DisplayLink } else { Pacing::Idle });
    }

    fn reblend(&mut self, rt: &mut RuntimeCore) -> bool {
        let mut alpha = None;
        crate::window::reblend_core(&self.surface, &self.gpu, &mut self.config, &mut self.renderer, &mut alpha, rt)
    }

    fn prepare_draw(&mut self, rt: &mut RuntimeCore, frame: wgpu::SurfaceTexture) -> PreparedEmbedFrame {
        let target_format = if rt.srgb_blend() { self.config.format.remove_srgb_suffix() } else { self.config.format };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: (target_format != self.config.format).then_some(target_format),
            ..Default::default()
        });
        let size = (self.config.width, self.config.height);
        host_step::prepare_gpu_resources(&mut self.renderer, &self.gpu, rt, size, true);
        let encoder = self.gpu.device.create_command_encoder(&Default::default());
        PreparedEmbedFrame { frame, target, encoder, size, format: target_format }
    }

    fn draw_default(&mut self, rt: &mut RuntimeCore, prepared: &mut PreparedEmbedFrame) {
        let clear = rt.clear_color();
        host_step::draw_ui(&mut self.renderer, &self.gpu, &mut prepared.encoder, &prepared.target, rt, prepared.size, clear);
    }

    fn submit_and_present(&mut self, prepared: PreparedEmbedFrame) -> Instant {
        self.renderer.submit(&self.gpu.queue, prepared.encoder);
        self.gpu.queue.present(prepared.frame);
        crate::render::gpu_events::presented();
        Instant::now()
    }
}

/// The sooner of two waits, either of which may be unset.
fn sooner(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Drives one Rune [`Runtime`] and [`Renderer`] directly onto a `CAMetalLayer`.
///
/// Pause display-link calls while the host app is backgrounded on iOS: Metal forbids drawable
/// acquisition and command-buffer submission after `didEnterBackground`.
pub struct LayerHost<S: 'static> {
    core: LayerHostCore,
    compositor: Option<Box<dyn Compositor<S>>>,
    rt: Runtime<S>,
}

impl<S: 'static> LayerHost<S> {
    /// Attaches a new Rune runtime and Metal surface to `layer` (`CAMetalLayer *`), blending in
    /// sRGB space (`srgb_blend = true`, matching Apple's native text compositing).
    ///
    /// # Safety
    /// `layer` must be a valid `CAMetalLayer` pointer that outlives this `LayerHost`, and surface
    /// creation / presentation must run on a thread allowed to touch that layer (the UI thread).
    pub unsafe fn from_core_animation_layer(
        layer: *mut c_void,
        spec: &AppSpec<S>,
        state: S,
        viewport: (f32, f32),
        scale: f32,
        dark: bool,
        wake: Waker,
    ) -> Result<Self, String> {
        let (mut core, size, scale, now) = unsafe { LayerHostCore::new(layer, spec.title, viewport, scale) }?;
        let mut rt = runtime_for(spec, state, size, Env::new(dark, scale), wake);
        core.configure_runtime(rt.core_mut(), now);
        Ok(Self { core, compositor: None, rt })
    }

    /// Attaches a custom [`Compositor`] (e.g. for [`El::effect`](crate::El::effect) shader passes)
    /// and turns on `Runtime::track_marks` so `rt.marks` aligns with `rt.instances`.
    pub fn with_compositor(mut self, compositor: impl Compositor<S>) -> Self {
        self.rt.track_marks = true;
        self.compositor = Some(Box::new(compositor));
        self
    }

    pub fn runtime(&self) -> &Runtime<S> {
        &self.rt
    }

    pub fn runtime_mut(&mut self) -> &mut Runtime<S> {
        &mut self.rt
    }

    /// Copies and pastes go to the system clipboard, as an app's own window sends them: a copy
    /// after the frame whose handler made it, a paste when the clipboard's worker answers,
    /// which calls `wake` for the frame that delivers it. Without this a host reads
    /// [`Runtime::copy_request`] itself and pastes arrive only from a clipboard a test supplies.
    pub fn with_system_clipboard(mut self, wake: Waker) -> Self {
        let mut clipboard = crate::clipboard::Client::default();
        clipboard.connect(wake);
        self.core.clipboard = Some(clipboard);
        self
    }

    /// The host shows pop-up menus itself: it reads [`Runtime::popup_menu`] after a frame, puts
    /// the menu up and answers with [`Runtime::resolve_popup`]. Without this every menu a
    /// handler asks for is answered `None` at once.
    pub fn with_popup_menus(mut self) -> Self {
        self.core.popup_menus = true;
        self
    }

    /// The host shows file dialogs itself: it reads [`Runtime::file_dialog`] after a frame,
    /// shows its own picker and answers with [`Runtime::resolve_file_dialog`]. Without this
    /// every dialog a handler asks for is refused.
    pub fn with_file_dialogs(mut self) -> Self {
        self.core.file_dialogs = true;
        self
    }

    /// Links a handler opens with [`Tx::open_url`](crate::Tx::open_url) open in the system, as
    /// from an app's own window, once the handler's frame is done. Without this nothing opens:
    /// the host takes the checked links with [`Runtime::take_url_requests`] and opens them
    /// itself.
    pub fn with_system_links(mut self) -> Self {
        self.rt.host.open_urls_natively = true;
        self
    }

    /// A key from the host's keyboard. With [`LayerHost::with_system_clipboard`], the paste
    /// shortcut over something that takes a paste reads the system clipboard, as in an app's
    /// own window; every other key is [`Runtime::key`].
    pub fn key(&mut self, key: crate::KeyEvent) {
        match &self.core.clipboard {
            Some(clipboard) if key.edit_command() == Some('v') && self.rt.accepts_paste() => clipboard.read_core(self.rt.core()),
            _ => self.rt.key(key),
        }
    }

    /// Sends on what the app copied or cleared since the last frame, as a frame does: for a
    /// hook that runs when no frame will follow it.
    pub(crate) fn flush_clipboard(&mut self) {
        match &self.core.clipboard {
            Some(clipboard) => clipboard.flush_core(self.rt.core_mut()),
            None => crate::clipboard::flush_memory(self.rt.core_mut()),
        }
    }

    /// Rests the view, as a window rests that has drawn nothing for a while (`all` false) or
    /// can't be seen (`all` true): the runtime's caches go back to what the screen on show
    /// uses, `on_rest` (an app's `App::on_rest`) runs for the app's own, and the renderer
    /// gives back what it keeps between frames, with `all` the targets of the layers on
    /// screen and the staging chunks too, which the next frame makes again. For a view that
    /// goes out of sight, as an iOS scene does when it enters the background.
    ///
    /// It only drops what is held: nothing is encoded, written to the queue or submitted, and
    /// it waits for no submission, so it is safe after `didEnterBackground`, where Metal
    /// takes no work. With `all` the device is polled without waiting, which frees what
    /// finished submissions held and takes back the staging chunks they used. What a
    /// submission still on the GPU holds goes at a later poll or the next frame. Call it
    /// between frames.
    pub fn rest(&mut self, on_rest: Option<fn(&mut S, &mut crate::Tx) -> bool>, all: bool) {
        self.rt.rest(on_rest, Some(&mut self.core.renderer), all);
        if all {
            let _ = self.core.gpu.device.poll(wgpu::PollType::Poll);
            self.core.renderer.trim_uploads();
        }
    }

    /// What this surface asked for, drew and missed, as totals and per-second rates
    /// ([`Runtime::surface_stats`] reads the same counts).
    pub fn surface_stats(&self) -> SurfaceStats {
        crate::frame::surface_stats::read(&self.core.meter.shared())
    }

    /// Updates the viewport in points and display scale, reconfiguring the Metal swapchain only
    /// when its clamped device-pixel dimensions change.
    pub fn resize(&mut self, viewport: (f32, f32), scale: f32) {
        self.core.resize(self.rt.core_mut(), viewport, scale);
    }

    /// [`Self::frame_at`] for a host with no presentation time: the frame is stepped to now.
    pub fn frame(&mut self) -> EmbedFrame {
        self.frame_to(None)
    }

    /// Steps animations and layout to `target`, the time the frame will be on screen, and
    /// presents one Metal frame, following the shared [`crate::frame::host_step`] order. A
    /// display link's target time is the one to pass (`CADisplayLink.targetTimestamp`, as far
    /// ahead of now as it is of `CACurrentMediaTime()`), so motion is drawn where it will be
    /// when the frame shows, and a late frame loses no time.
    ///
    /// A frame that gets no drawable costs that frame, never the host: a covered surface is
    /// tried again on a wait that doubles, an outdated one is configured again, and one the GPU
    /// refuses to configure rests until a resize. `EmbedFrame::wake_after_secs` carries the wait.
    pub fn frame_at(&mut self, target: Instant) -> EmbedFrame {
        self.frame_to(Some(target))
    }

    fn frame_to(&mut self, target: Option<Instant>) -> EmbedFrame {
        let now = Instant::now();
        if !self.core.suspended && self.core.backoff.waiting(now) {
            return EmbedFrame { wake_after_secs: self.core.wake_after(self.rt.core(), now), ..EmbedFrame::default() };
        }
        let _ = self.core.backoff.step(BackoffEvent::Lapsed);
        embed_frame::run_embed_frame_at(self, target).into()
    }
}

impl<S: 'static> EmbedSurface for LayerHost<S> {
    type Drawable = wgpu::SurfaceTexture;
    type Prepared = PreparedEmbedFrame;

    fn suspended(&self) -> bool { self.core.suspended }
    fn renderer_srgb_blend(&self) -> bool { self.core.renderer.srgb_blend() }
    fn rt_srgb_blend(&self) -> bool { self.rt.core().srgb_blend() }
    fn reblend(&mut self) -> bool { self.core.reblend(self.rt.core_mut()) }
    fn acquire(&mut self) -> Acquire<wgpu::SurfaceTexture> { self.core.acquire() }
    fn missed(&mut self, missed: Missed, now: Instant) -> bool { self.core.missed(missed, now) }
    fn retry_after(&self, now: Instant) -> Option<f64> { self.core.retry_after(now) }
    fn presented(&mut self, ticking: bool) { self.core.presented(ticking) }
    fn serves(&self) -> Serves { Serves { clipboard: self.core.clipboard.is_some(), popup_menus: self.core.popup_menus, file_dialogs: self.core.file_dialogs } }
    fn receive_clipboard(&mut self) {
        if let Some(clipboard) = &self.core.clipboard { clipboard.receive_core(self.rt.core_mut()); }
    }
    fn flush_clipboard(&mut self) {
        if let Some(clipboard) = &self.core.clipboard { clipboard.flush_core(self.rt.core_mut()); }
    }
    fn prepare(&mut self, drawable: wgpu::SurfaceTexture) -> PreparedEmbedFrame {
        self.core.prepare_draw(self.rt.core_mut(), drawable)
    }
    fn compositor_animating(&self) -> bool { self.compositor.as_ref().is_some_and(|c| c.animating()) }

    fn render(&mut self, prepared: &mut PreparedEmbedFrame, dt: f64, update_ms: f64) -> bool {
        if let Some(comp) = &mut self.compositor {
            let mut f = Frame {
                device: &self.core.gpu.device,
                queue: &self.core.gpu.queue,
                encoder: &mut prepared.encoder,
                target: &prepared.target,
                format: prepared.format,
                size: prepared.size,
                dt,
                update_ms,
                rt: &mut self.rt,
                ui: &mut self.core.renderer,
            };
            comp.render(&mut f);
            comp.animating()
        } else {
            self.core.draw_default(self.rt.core_mut(), prepared);
            false
        }
    }

    fn submit_and_present(&mut self, prepared: PreparedEmbedFrame) -> Instant {
        self.core.submit_and_present(prepared)
    }

    fn clock_and_rt_mut(&mut self) -> (&mut FrameClock, &mut RuntimeCore) {
        (&mut self.core.clock, self.rt.core_mut())
    }

    fn with_driver<R>(&mut self, f: impl FnOnce(&mut FrameClock, &mut RuntimeCore, &mut dyn AppDriver) -> R) -> R {
        let (rt, mut app) = self.rt.split_mut();
        f(&mut self.core.clock, rt, &mut app)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use objc2_video::{
        msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject},
    };

    use super::LayerHost;
    use crate::compose::{Compositor, Frame};
    use crate::render::KIND_EFFECT;
    use crate::tooling::AppSpec;
    use crate::{Cx, El, col, effect};

    struct MarkProbe(Arc<AtomicU32>);

    impl Compositor<()> for MarkProbe {
        fn render(&mut self, f: &mut Frame<'_, ()>) {
            for (idx, inst) in f.rt.instances.iter().enumerate() {
                if inst.params[1] == KIND_EFFECT {
                    self.0.store(f.rt.marks.get(idx).copied().unwrap_or(0), Ordering::Relaxed);
                }
            }
            f.draw_ui();
        }
    }

    #[test]
    fn layer_host_runs_compositor_with_tracked_marks() {
        let layer: Retained<AnyObject> = unsafe { msg_send![AnyClass::get(c"CAMetalLayer").expect("QuartzCore"), new] };
        let seen = Arc::new(AtomicU32::new(0));
        let view = |_: &(), _: &mut Cx<()>| -> El { col().size(120.0, 80.0).child(effect(42).size(60.0, 20.0)) };
        let spec = AppSpec::new("embed-test", (120.0, 80.0), || (), view);
        let host = unsafe {
            LayerHost::from_core_animation_layer(
                Retained::as_ptr(&layer) as *mut std::ffi::c_void,
                &spec,
                (),
                (120.0, 80.0),
                2.0,
                true,
                Arc::new(|| {}),
            )
        };
        let Ok(mut host) = host else { return };
        host = host.with_compositor(MarkProbe(seen.clone()));
        let out = host.frame();
        assert!(out.drew);
        assert_eq!(seen.load(Ordering::Relaxed), 42);
        let stats = host.surface_stats();
        assert_eq!((stats.totals.redraws, stats.totals.presents, stats.totals.failed()), (1, 1, 0));
        assert_eq!(host.runtime().surface_stats().map(|s| s.totals.presents), Some(1));
        // A frame aimed at a display tick ahead of now draws too, and the clock keeps its target.
        let target = crate::time::Instant::now() + std::time::Duration::from_millis(8);
        assert!(host.frame_at(target).drew);
        assert_eq!(host.core.clock.last(), target);
    }

    #[test]
    fn links_open_in_the_system_only_when_the_host_asks() {
        let layer: Retained<AnyObject> = unsafe { msg_send![AnyClass::get(c"CAMetalLayer").expect("QuartzCore"), new] };
        let view = |_: &(), _: &mut Cx<()>| -> El { col().size(120.0, 80.0) };
        let spec = AppSpec::new("embed-links", (120.0, 80.0), || (), view);
        let host = unsafe {
            LayerHost::from_core_animation_layer(
                Retained::as_ptr(&layer) as *mut std::ffi::c_void,
                &spec,
                (),
                (120.0, 80.0),
                2.0,
                true,
                Arc::new(|| {}),
            )
        };
        let Ok(host) = host else { return };
        assert!(!host.runtime().host.open_urls_natively);
        assert!(host.with_system_links().runtime().host.open_urls_natively);
    }
}
