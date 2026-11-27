//! A window's paint: the system's `RedrawRequested`, or one the loop runs for a window whose
//! paint the system keeps putting off.

use winit::event_loop::ActiveEventLoop;

use super::Host;
use crate::window::chrome::sync_chrome_core;
#[cfg(not(target_os = "macos"))]
use crate::window::surface::RedrawEvent;
#[cfg(target_os = "macos")]
use crate::window::surface::Win;

impl Host {
    /// The system's paint (`RedrawRequested`), or one the loop runs for a window whose paint
    /// the system keeps putting off (`Redraw`).
    pub(in crate::window) fn paint(&mut self, event_loop: &ActiveEventLoop) {
        // The frame a due timer asked for drew, or ran covered in `draw`.
        if let Some(l) = &mut self.win { l.timer_frame.drawn(); }
        // AppKit paints off its own display link; elsewhere a paint holds what it asks for.
        #[cfg(not(target_os = "macos"))]
        if let Some(l) = &self.win { l.redraw_step(RedrawEvent::Paint); }
        #[cfg(target_os = "macos")]
        let satisfied = self.win.as_ref().is_some_and(Win::take_redraw_on_paint);
        // A new accent, scroller style, light/dark, reduced-motion or double-click preference asks
        // every window to redraw.
        {
            let rt = self.app.rt_mut();
            rt.set_system_accent(crate::system_accent());
            rt.set_system_scroll_bars_state(crate::platform::scroll_bars::current().with_override(self.args.scroll_bars));
            #[cfg(target_os = "linux")]
            {
                rt.set_system_reduced_motion(crate::platform::preferences::reduced_motion());
                rt.double_click_interval = crate::platform::preferences::double_click_interval();
            }
            if self.args.theme.is_none() && let Some(dark) = self.win.as_ref().and_then(|l| crate::platform::preferences::desktop_dark(&l.window)) {
                rt.set_system_dark(dark);
            }
        }
        #[cfg(target_os = "macos")]
        if self.win.as_ref().is_some_and(|l| l.native.full_screen.take()) {
            if let Some(l) = self.win.as_ref() {
                sync_chrome_core(self.app.rt_mut(), l, self.chrome, self.titlebar);
            }
            self.record_live_window_state();
        }
        // A `DisplayTick` that already drew and settled the queued work leaves a stale
        // `RedrawRequested` in winit's queue. Skip it while resting with nothing changed so it
        // cannot bump a coasting display link back to `Active` for a duplicate frame.
        #[cfg(target_os = "macos")]
        if satisfied && self.win.as_ref().is_some_and(|l| l.presented.is_some()) {
            let rt = self.app.rt();
            if !rt.stale() && !rt.live_pending() && !rt.has_pending_input() && rt.host.window_frame.is_none() {
                return;
            }
        }
        // The window procedure asks for this paint when a caption button's hover or
        // press changes.
        #[cfg(target_os = "windows")]
        if let Some(l) = &self.win { sync_chrome_core(self.app.rt_mut(), l, self.chrome, self.titlebar); }
        // A Linux frame of Rune's own takes the desktop's button layout, which the settings
        // portal asks for this paint to show, and whether the window is maximized.
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
        if crate::titlebar::linux::chosen() && let Some(l) = &self.win { sync_chrome_core(self.app.rt_mut(), l, self.chrome, self.titlebar); }
        // X11 `PropertyNotify` on EWMH/GTK WM-state atoms sets a per-window flag and asks for
        // `RedrawRequested`; sync chrome only when that flag fired (zero IPC on ordinary redraws).
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
        if !self.wayland && let Some(l) = &self.win
            && winit::platform::x11::WindowExtX11::take_wm_state_changed(&*l.window)
        {
            sync_chrome_core(self.app.rt_mut(), l, self.chrome, self.titlebar);
        }
        self.redraw();
        self.shell(event_loop);
        // What asked for a frame while it painted waits for the turn's end.
        #[cfg(not(target_os = "macos"))]
        if let Some(l) = &self.win { l.redraw_step(RedrawEvent::Painted); }
    }
}
