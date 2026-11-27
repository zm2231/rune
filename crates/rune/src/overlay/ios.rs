use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(crate) use super::standby::{Mounted, Parked, Standby, Turn, Unmounted};
use super::requests::{KeyRequest, Visibility};
use super::{Availability, OverlayHandle, Placement, Shared, SharedCore};
use crate::compose::Compositor;
use crate::el::Tx;
use crate::runtime::{Env, Runtime, Waker};
use crate::text::TextSystem;
use crate::tooling::AppSpec;
use crate::{ConfiguredOverlay, KeyPolicy, OverlayCapabilities, OverlayInput, OverlayLevel, OverlayPlacement, OverlaySupport};

pub(crate) struct Built<T: 'static> {
    pub(crate) name: &'static str,
    pub(crate) rt: Unmounted<T>,
    pub(crate) compositor: Option<Box<dyn Compositor<T>>>,
    pub(crate) setup: Option<fn(&mut T, Waker)>,
    pub(crate) on_availability: Option<fn(&mut T, Availability, &mut Tx)>,
    pub(crate) feed: Feed<T>,
    pub(crate) control: Control,
    pub(crate) standby: Standby,
    pub(crate) inherits_font_smoothing: bool,
}

pub(crate) trait Pending {
    fn inherit_font_smoothing(&mut self, mode: crate::FontSmoothing);
    fn into_slot(self: Box<Self>) -> crate::platform::ios::OverlaySlot;
}

impl<T: 'static> Pending for Built<T> {
    fn inherit_font_smoothing(&mut self, mode: crate::FontSmoothing) {
        if !self.inherits_font_smoothing { return; }
        if let Some(parked) = self.rt.borrow_mut().as_mut() { parked.rt.set_font_smoothing(mode); }
    }

    fn into_slot(self: Box<Self>) -> crate::platform::ios::OverlaySlot { crate::platform::ios::overlay_slot(*self) }
}

pub(crate) struct Feed<T: 'static> {
    shared: Arc<Shared<T>>,
    sizing: super::sizing::SizingConfig,
    placement: Option<OverlayPlacement>,
    size: (f32, f32),
}

impl<T: 'static> Feed<T> {
    pub(crate) fn drain(&self, rt: &mut Runtime<T>) {
        let updates = std::mem::take(&mut *self.shared.updates.lock().unwrap());
        for f in updates { rt.update_if(f); }
    }

    pub(crate) fn close_after_hidden(&self) {
        let pending = self.shared.after_hidden.lock().unwrap_or_else(std::sync::PoisonError::into_inner).close();
        drop(pending);
        self.shared.core.wake();
    }

    pub(crate) fn available(&self, rt: &mut Runtime<T>, on_availability: Option<fn(&mut T, Availability, &mut Tx)>) {
        let core = &self.shared.core;
        let s = OverlaySupport::Supported;
        core.native.lock().unwrap().1 = OverlayCapabilities::placed(OverlaySupport::Unsupported, s, OverlaySupport::Unsupported);
        if core.availability.swap(1, Ordering::AcqRel) == 1 { return; }
        if let Some(f) = on_availability { rt.update(|state, tx| f(state, Availability::Available, tx)); }
    }

    pub(crate) fn fit(&mut self, rt: &mut Runtime<T>) {
        let Some(measured) = rt.core_mut().host.overlay_fit.take() else { return };
        if self.sizing.mode.is_fixed() { return; }
        let scale = rt.window_scale();
        let next = super::sizing::resolve_content_size(self.sizing, self.placement, self.size, measured, None, scale, super::sizing::DEFAULT_MAX_PHYS_DIM);
        if !super::sizing::size_changed(self.size, next, scale) { return; }
        self.size = next;
        self.shared.core.request(|q| q.fit_size = Some(next));
    }
}

pub(crate) struct Changes {
    pub(crate) visibility: Option<(bool, Duration)>,
    pub(crate) moved: bool,
    pub(crate) keys: Option<bool>,
}

pub(crate) struct Control {
    core: Arc<SharedCore>,
    at: Placement,
    placement: Option<OverlayPlacement>,
    input: OverlayInput,
    interactive: bool,
    keys: KeyPolicy,
    level: OverlayLevel,
    woken: Arc<AtomicBool>,
}

impl Control {
    pub(crate) fn connect(&self, wake: Waker) {
        *self.core.wake.lock().unwrap() = Some(wake);
        self.core.wake();
    }

    pub(crate) fn wake(&self) { self.core.wake(); }

    pub(crate) fn woke(&self) {
        self.woken.store(true, Ordering::Release);
        self.core.wake();
    }

    pub(crate) fn waker(&self) -> Waker {
        let (core, woken) = (self.core.clone(), self.woken.clone());
        Arc::new(move || {
            woken.store(true, Ordering::Release);
            core.wake();
        })
    }

    pub(crate) fn take_woken(&self) -> bool { self.woken.swap(false, Ordering::AcqRel) }

    pub(crate) fn take(&mut self) -> Changes {
        let mut q = std::mem::take(&mut *self.core.requests.lock().unwrap());
        for reply in q.tokens.take() { reply(None); }
        let before = (self.at, self.placement);
        if let Some(placement) = q.placement { self.placement = placement; }
        if let Some(edge) = q.edge { self.at.edge = edge; }
        if let Some(margin) = q.margin { self.at.margin = margin; }
        if let Some((align, inset)) = q.align { (self.at.align, self.at.inset) = (align, inset); }
        if let Some(size) = q.size.or(q.fit_size) { self.at.size = size; }
        if let Some(input) = q.input { self.input = input; }
        let keys = q.keys.filter(|_| self.keys != KeyPolicy::None).map(|keys| keys == KeyRequest::Take);
        let visibility = q.visibility.map(|v| match v {
            Visibility::Show(fade) => (true, fade),
            Visibility::Hide(fade) => (false, fade),
        });
        Changes { visibility, moved: before != (self.at, self.placement), keys }
    }

    pub(crate) fn frame(&self, bounds: (f32, f32), safe: [f32; 4]) -> [f32; 4] {
        crate::platform::ios::overlay::frame(self.at, self.placement, bounds, safe)
    }

    pub(crate) fn takes(&self, regions: &[[f32; 4]], x: f32, y: f32) -> bool {
        crate::platform::ios::overlay::takes(self.input, self.interactive, regions, x, y)
    }

    pub(crate) fn takes_keys_on_touch(&self) -> bool { self.keys != KeyPolicy::None }

    pub(crate) fn level(&self) -> OverlayLevel { self.level }
}

pub(crate) fn build<S, T: 'static>(configured: ConfiguredOverlay<T>, state: T, app: &AppSpec<S>, app_font_smoothing: crate::FontSmoothing) -> (Built<T>, OverlayHandle<T>) {
    let ConfiguredOverlay { spec, options, on_key, on_availability, poll, compositor, setup, mut runtime, font_smoothing, sizing, min_size, max_size, .. } = configured;
    let inherits_font_smoothing = font_smoothing.is_none();
    runtime.font_smoothing = font_smoothing.unwrap_or(app_font_smoothing);
    let shared = Arc::new(Shared { core: Arc::new(SharedCore::default()), updates: Default::default(), after_hidden: Default::default() });
    let text = TextSystem::for_window(app.fonts, app.families);
    let mut rt = Runtime::with_text(state, spec.view, spec.size, Env::default(), text);
    rt.configure(runtime);
    rt.track_marks = compositor.is_some();
    rt.on_key = on_key;
    rt.poll = poll.or_else(|| app.poll.and_then(|f| {
        (&f as &dyn std::any::Any).downcast_ref::<fn(&mut T, f64, &mut Tx) -> bool>().copied()
    }));
    rt.set_live_visible(false);
    rt.core_mut().host.content_sizing = sizing;
    rt.core_mut().host.content_limits = (min_size, max_size);
    let at = Placement { size: spec.size, edge: spec.edge, margin: spec.margin, align: spec.align, inset: spec.inset };
    let control = Control {
        core: shared.core.clone(),
        at,
        placement: options.placement,
        input: options.input,
        interactive: spec.interactive,
        keys: options.keys,
        level: options.level,
        woken: Arc::default(),
    };
    let feed = Feed { shared: shared.clone(), sizing: super::sizing::SizingConfig { mode: sizing, min_size, max_size }, placement: options.placement, size: spec.size };
    rt.wall_start = Some(crate::time::now());
    let rt = Rc::new(RefCell::new(Some(Parked { rt, clipboard: crate::clipboard::Client::default() })));
    let standby = Standby::new(shared.clone(), rt.clone());
    let built = Built { name: spec.name, rt, compositor, setup, on_availability, feed, control, standby, inherits_font_smoothing };
    (built, OverlayHandle { shared })
}
