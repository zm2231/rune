//! One-shot `OverlayHandle::after_hidden` callbacks: waits for a hide to take effect before
//! running on the event thread.
//!
//! On Wayland, hiding destroys the overlay's surface and this state machine sends a
//! `wl_display.sync` roundtrip on winit's shared display. While the sync is out the overlay
//! counts as live for `wayland_wake` so the compositor's `wl_callback.done` turns the loop even
//! with no layer surface mapped. Showing the overlay again before the answer arrives cancels the
//! sync and keeps the callbacks waiting for the next hide. Elsewhere, callbacks run in the
//! event-loop turn after the hide. Closing the overlay, quitting the app, or losing the display
//! drops pending callbacks unrun.

use super::*;
use super::hide_wait::{Effect, Event, Phase, step};
use crate::machine::Effects;

/// Owned subsystem state for `OverlayHandle::after_hidden` inside [`OverlayCore`].
#[derive(Default)]
pub(in crate::overlay) struct AfterHiddenState {
    pub(super) phase: Phase,
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    pub(super) display: Option<winit::raw_window_handle::WaylandDisplayHandle>,
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    pub(super) sync: Option<crate::platform::layer_shell::HideSync>,
}

impl AfterHiddenState {
    fn uses_sync(&self) -> bool {
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
        return self.display.is_some();
        #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
        false
    }
}

impl OverlayCore {
    pub(super) fn record_wayland_display(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
        {
            if self.after_hidden.display.is_some() || !crate::window::is_wayland(event_loop) {
                return;
            }
            use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
            if let Ok(display) = event_loop.display_handle()
                && let RawDisplayHandle::Wayland(handle) = display.as_raw()
            {
                self.after_hidden.display = Some(handle);
            }
        }
        #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
        let _ = event_loop;
    }

    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    pub(super) fn has_live_wayland_hide_sync(&self) -> bool {
        self.after_hidden.sync.as_ref().is_some_and(crate::platform::layer_shell::HideSync::is_live)
    }

    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    pub(super) fn has_pending_wayland_hide_sync_events(&self) -> bool {
        self.after_hidden
            .sync
            .as_ref()
            .is_some_and(crate::platform::layer_shell::HideSync::has_pending_events)
    }

    pub(super) fn after_hidden_queued(&mut self, app: &mut dyn OverlayApp) {
        let event = Event::Queued {
            hidden: !self.presence.drawing(),
            uses_sync: self.after_hidden.uses_sync(),
        };
        let effects = step(&mut self.after_hidden.phase, event);
        self.apply_after_hidden(app, effects);
    }

    pub(super) fn after_hidden_shown(&mut self, app: &mut dyn OverlayApp) {
        let effects = step(&mut self.after_hidden.phase, Event::Shown);
        self.apply_after_hidden(app, effects);
    }

    pub(super) fn after_hidden_did_hide(&mut self, app: &mut dyn OverlayApp, surface_destroyed: bool) {
        let event = Event::Hidden {
            uses_sync: self.after_hidden.uses_sync(),
            surface_destroyed,
        };
        let effects = step(&mut self.after_hidden.phase, event);
        self.apply_after_hidden(app, effects);
    }

    pub(super) fn after_hidden_new_turn(&mut self, app: &mut dyn OverlayApp) {
        let effects = step(&mut self.after_hidden.phase, Event::NewTurn);
        self.apply_after_hidden(app, effects);
    }

    pub(super) fn poll_after_hidden(&mut self, app: &mut dyn OverlayApp) {
        if self.presence.drawing() {
            return;
        }
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
        if let Some(sync) = self.after_hidden.sync.as_mut() {
            match sync.dispatch() {
                crate::platform::layer_shell::SyncPoll::Pending => return,
                crate::platform::layer_shell::SyncPoll::Answered => {
                    self.after_hidden.sync = None;
                    let effects = step(&mut self.after_hidden.phase, Event::SyncAnswered);
                    self.apply_after_hidden(app, effects);
                    return;
                }
                crate::platform::layer_shell::SyncPoll::Failed => {
                    let effects = step(&mut self.after_hidden.phase, Event::SyncFailed);
                    self.apply_after_hidden(app, effects);
                    return;
                }
            }
        }
        let effects = step(&mut self.after_hidden.phase, Event::AboutToWaitHidden);
        self.apply_after_hidden(app, effects);
    }

    pub(super) fn cancel_after_hidden(&mut self, app: &mut dyn OverlayApp) {
        let effects = step(&mut self.after_hidden.phase, Event::Cancel);
        self.apply_after_hidden(app, effects);
        app.drop_after_hidden();
    }

    pub(super) fn close_after_hidden(&mut self, app: &mut dyn OverlayApp) {
        let effects = step(&mut self.after_hidden.phase, Event::Cancel);
        self.apply_after_hidden(app, effects);
        app.close_after_hidden();
    }

    fn apply_after_hidden(&mut self, app: &mut dyn OverlayApp, effects: Effects<Effect>) {
        for effect in effects {
            match effect {
                Effect::StartSync => {
                    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
                    {
                        self.after_hidden.sync = None;
                        let sync = self
                            .after_hidden
                            .display
                            .and_then(crate::platform::layer_shell::HideSync::new);
                        if let Some(sync) = sync {
                            self.after_hidden.sync = Some(sync);
                        } else {
                            let fail = step(&mut self.after_hidden.phase, Event::SyncFailed);
                            self.apply_after_hidden(app, fail);
                        }
                    }
                }
                Effect::CancelSync => {
                    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
                    {
                        self.after_hidden.sync = None;
                    }
                }
                Effect::Wake => {
                    self.shared.wake();
                }
                Effect::RunCallbacks => {
                    if app.run_after_hidden() {
                        self.clipboard.flush_core(app.rt_mut());
                        self.open_popup(app);
                        app.rt_mut().schedule_pending_wakes();
                        self.take_window_requests(app.rt_mut());
                        self.dirty = true;
                        if self.retained() && app.rt().needs_commit() {
                            self.hidden_pace.changed(crate::time::now());
                        }
                    }
                }
                Effect::DropCallbacks => {
                    app.drop_after_hidden();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "after_hidden_tests.rs"]
mod tests;
