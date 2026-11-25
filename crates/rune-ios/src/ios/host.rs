use std::ffi::c_void;
use std::rc::Rc;

use rune::embed::{EmbedFrame, LayerHost};
use rune::{AppSpec, Haptic, TouchPhase, Waker};

/// The runtime a view drives, whatever the app's state type.
pub(crate) trait Host {
    fn frame(&mut self) -> EmbedFrame;
    fn resize(&mut self, viewport: (f32, f32), scale: f32);
    fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: f64);
    fn set_safe_area(&mut self, insets: [f32; 4]);
    fn set_dark(&mut self, dark: bool);
    fn set_reduced_motion(&mut self, reduced: bool);
    fn take_haptics(&mut self) -> Vec<Haptic>;
}

impl<S: 'static> Host for LayerHost<S> {
    fn frame(&mut self) -> EmbedFrame { LayerHost::frame(self) }
    fn resize(&mut self, viewport: (f32, f32), scale: f32) { LayerHost::resize(self, viewport, scale) }
    fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: f64) {
        self.runtime_mut().touch(id, phase, x, y, time);
    }
    fn set_safe_area(&mut self, insets: [f32; 4]) { self.runtime_mut().set_safe_area(insets) }
    fn set_dark(&mut self, dark: bool) { self.runtime_mut().set_system_dark(dark) }
    fn set_reduced_motion(&mut self, reduced: bool) { self.runtime_mut().set_system_reduced_motion(reduced) }
    fn take_haptics(&mut self) -> Vec<Haptic> { LayerHost::take_haptics(self) }
}

/// What a view needs to start its runtime once it has a size: its layer, viewport in points,
/// scale, whether the system is dark, and the waker that resumes its display link.
pub(crate) struct Mount {
    pub(crate) layer: *mut c_void,
    pub(crate) viewport: (f32, f32),
    pub(crate) scale: f32,
    pub(crate) dark: bool,
    pub(crate) wake: Waker,
}

/// Starts a fresh runtime for each view (a scene that reconnects gets a new one).
pub(crate) type Factory = Rc<dyn Fn(Mount) -> Result<Box<dyn Host>, String>>;

pub(crate) fn factory<S: 'static>(spec: AppSpec<S>) -> Factory {
    Rc::new(move |mount: Mount| {
        let state = (spec.init)();
        // SAFETY: `mount.layer` is the `CAMetalLayer` backing the view that owns the returned
        // host and drops it before the layer; the view only calls this on the main thread.
        let host = unsafe {
            LayerHost::from_core_animation_layer(mount.layer, &spec, state, mount.viewport, mount.scale, mount.dark, mount.wake)
        }?;
        Ok(Box::new(host) as Box<dyn Host>)
    })
}
