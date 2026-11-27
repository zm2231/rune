use std::sync::Arc;

use crate::app::{Shared, WindowRequests};
use crate::compose::{Compositor, Frame};
use crate::render::Renderer;
use crate::runtime::{AppDriver, Runtime, RuntimeCore};
use crate::tooling::Kind;

pub(super) struct DesktopRenderArgs<'a> {
    pub(super) device: &'a wgpu::Device,
    pub(super) queue: &'a wgpu::Queue,
    pub(super) encoder: &'a mut wgpu::CommandEncoder,
    pub(super) target: &'a wgpu::TextureView,
    pub(super) format: wgpu::TextureFormat,
    pub(super) size: (u32, u32),
    pub(super) dt: f64,
    pub(super) update_ms: f64,
    pub(super) ui: &'a mut Renderer,
}

pub(super) trait DesktopApp {
    fn rt(&self) -> &RuntimeCore;
    fn rt_mut(&mut self) -> &mut RuntimeCore;
    fn with_driver(&mut self, f: &mut dyn FnMut(&mut RuntimeCore, &mut dyn AppDriver));
    fn run_on_close_request(&mut self) -> Option<crate::CloseResponse>;
    fn run_on_quit_request(&mut self) -> Option<crate::QuitResponse>;
    fn run_on_close(&mut self);
    fn run_on_reopen(&mut self, has_visible_windows: bool) -> bool;
    fn drain_updates(&mut self, menus: &mut crate::native_menu::MenuGuard) -> WindowRequests;
    fn has_updates(&self) -> bool;
    fn has_compositor(&self) -> bool;
    fn compositor_features(&self) -> wgpu::Features;
    fn render_compositor(&mut self, args: DesktopRenderArgs<'_>) -> bool;
    fn compositor_summary(&mut self, device: &wgpu::Device) -> Option<String>;
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl dyn DesktopApp + '_ {
    fn driver<R>(&mut self, f: impl FnOnce(&mut RuntimeCore, &mut dyn AppDriver) -> R) -> R {
        let mut f = Some(f);
        let mut out = None;
        self.with_driver(&mut |core, app| {
            out = Some(f.take().unwrap()(core, app));
        });
        out.unwrap()
    }

    pub(super) fn content_height(&mut self) -> f32 { self.driver(|rt, app| rt.content_height(app)) }
    pub(super) fn set_live_visible(&mut self, visible: bool) { self.driver(|rt, app| rt.set_live_visible(app, visible)); }
    pub(super) fn cancel_popups(&mut self) { self.driver(|rt, app| rt.cancel_popups(app)); }
    pub(super) fn cancel_file_dialogs(&mut self) { self.driver(|rt, app| rt.cancel_file_dialogs(app)); }
    pub(super) fn open_popup_headless(&mut self) { self.driver(|rt, app| rt.open_popup_headless(app)); }
    pub(super) fn resolve_popup(&mut self, picked: Option<&str>) -> Result<(), crate::PopupMenuError> {
        self.driver(|rt, app| rt.resolve_popup(app, picked))
    }
    pub(super) fn frame(&mut self, dt: f64) -> bool { self.driver(|rt, app| rt.frame(app, dt)) }
    pub(super) fn window_focus(&mut self, focused: bool) { self.driver(|rt, app| rt.window_focus(app, focused)); }
    pub(super) fn script_input(&mut self, kind: &Kind, at: (f32, f32)) {
        self.driver(|rt, app| { crate::tooling::script_input(rt, app, kind, at); });
    }
}

pub(super) struct TypedDesktopApp<S: 'static> {
    pub(super) rt: Runtime<S>,
    pub(super) shared: Arc<Shared<S>>,
    pub(super) comp: Option<Box<dyn Compositor<S>>>,
    pub(super) on_close_request: Option<fn(&mut S, &mut crate::Tx) -> crate::CloseResponse>,
    pub(super) on_quit_request: Option<fn(&mut S, &mut crate::Tx) -> crate::QuitResponse>,
    pub(super) on_close: Option<fn(&mut S, &mut crate::Tx)>,
    pub(super) on_reopen: Option<fn(&mut S, bool, &mut crate::Tx) -> bool>,
}

impl<S: 'static> DesktopApp for TypedDesktopApp<S> {
    fn rt(&self) -> &RuntimeCore { self.rt.core() }
    fn rt_mut(&mut self) -> &mut RuntimeCore { self.rt.core_mut() }
    fn with_driver(&mut self, f: &mut dyn FnMut(&mut RuntimeCore, &mut dyn AppDriver)) {
        let (core, mut driver) = self.rt.split_mut();
        f(core, &mut driver);
    }
    fn run_on_close_request(&mut self) -> Option<crate::CloseResponse> {
        let f = self.on_close_request?;
        let mut resp = crate::CloseResponse::Close;
        self.rt.update(|s, tx| resp = f(s, tx));
        Some(resp)
    }
    fn run_on_quit_request(&mut self) -> Option<crate::QuitResponse> {
        let f = self.on_quit_request?;
        let mut resp = crate::QuitResponse::Quit;
        self.rt.update(|s, tx| resp = f(s, tx));
        Some(resp)
    }
    fn run_on_close(&mut self) {
        if let Some(f) = self.on_close { self.rt.update(f); }
    }
    fn run_on_reopen(&mut self, has_visible_windows: bool) -> bool {
        let Some(f) = self.on_reopen else { return false };
        let mut handled = false;
        self.rt.update(|s, tx| handled = f(s, has_visible_windows, tx));
        handled
    }
    fn drain_updates(&mut self, menus: &mut crate::native_menu::MenuGuard) -> WindowRequests {
        self.shared.drain(|f| {
            self.rt.update_if(f);
            menus.drain();
        })
    }
    fn has_updates(&self) -> bool { self.shared.has_updates() }
    fn has_compositor(&self) -> bool { self.comp.is_some() }
    fn compositor_features(&self) -> wgpu::Features {
        self.comp.as_ref().map_or(wgpu::Features::empty(), |c| c.features())
    }
    fn render_compositor(&mut self, args: DesktopRenderArgs<'_>) -> bool {
        self.render_compositor_in_turn(args)
    }
    fn compositor_summary(&mut self, device: &wgpu::Device) -> Option<String> {
        self.comp.as_mut().and_then(|c| c.summary(device))
    }
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

impl<S: 'static> TypedDesktopApp<S> {
    fn render_compositor_in_turn(&mut self, args: DesktopRenderArgs<'_>) -> bool {
        let Some(c) = self.comp.as_mut() else { return false };
        let mut f = Frame {
            device: args.device,
            queue: args.queue,
            encoder: args.encoder,
            target: args.target,
            format: args.format,
            size: args.size,
            dt: args.dt,
            update_ms: args.update_ms,
            rt: &mut self.rt,
            ui: args.ui,
        };
        c.render(&mut f);
        c.animating()
    }
}
