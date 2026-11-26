//! [`AppHandle`]: a running app's window and state, driven from any thread.

use std::sync::Arc;

use super::Shared;
use crate::Tx;

/// Opens, focuses, closes and feeds a running `App` from any thread. Every call wakes the event
/// loop; nothing polls. Window requests collapse to the latest.
pub struct AppHandle<S: 'static> {
    pub(crate) shared: Arc<Shared<S>>,
}

impl<S> Clone for AppHandle<S> {
    fn clone(&self) -> Self {
        AppHandle {
            shared: self.shared.clone(),
        }
    }
}

impl<S: 'static> AppHandle<S> {
    /// Wake the event loop so `AppSpec::poll` runs, also while the window is hidden; for a
    /// background thread that queued work for `poll` to drain.
    #[inline]
    pub fn wake(&self) { self.shared.core.wake_app(); }

    /// Show the window (creating it if needed), restore it if minimized, and activate the app
    /// with it in front, including the first open of a window that doesn't exist yet. A
    /// nonactivating panel takes keys without activating. The window open at launch doesn't
    /// activate the app beyond what the platform does at launch.
    #[inline]
    pub fn open(&self) { self.shared.core.open(); }

    /// Show or recreate the window without requesting focus. A minimized window stays minimized;
    /// open() also restores and activates it. Window-manager policy still applies.
    #[inline]
    pub fn show(&self) { self.shared.core.show(); }

    /// Hide the window, retaining its host and state. Does not invoke the close callback.
    #[inline]
    pub fn hide(&self) { self.shared.core.hide(); }

    /// Focus/activate an already-visible window, restoring it if minimized.
    /// On Wayland this requests attention; use activate(token) for compositor-authorized focus.
    #[inline]
    pub fn focus(&self) { self.shared.core.focus(); }

    /// Set the outer position in physical desktop pixels (negative coordinates are valid).
    /// Retained across recreation. Wayland does not support application window placement.
    #[inline]
    pub fn set_position(&self, position: (i32, i32)) { self.shared.core.set_position(position); }

    /// Place in display-local points. UnderPointer samples the pointer at this call;
    /// disconnected displays fall back to primary when the UI thread applies the request.
    /// Wayland does not permit application placement.
    #[inline]
    pub fn place(&self, display: crate::DisplayTarget, placement: crate::OverlayPlacement) {
        self.shared.core.place(display, placement);
    }

    /// Show and focus the window with an activation token another process forwarded
    /// (`XDG_ACTIVATION_TOKEN` on Wayland, `DESKTOP_STARTUP_ID` on X11), so the compositor
    /// lets it take focus. Where tokens don't exist this is `open`.
    #[inline]
    pub fn activate(&self, token: impl Into<String>) { self.shared.core.activate(token.into()); }

    /// Close the window as the close button would (`App::on_close` runs; without
    /// `keep_running` the app quits).
    #[inline]
    pub fn close(&self) { self.shared.core.close(); }

    /// Minimize the window, as `Tx::minimize` does from a handler. Does nothing for a hidden
    /// window, a panel or launcher, or one that can't minimize. After a `show` it minimizes
    /// the shown window; a later `open`, `focus`, `hide` or `close` cancels it, and it cancels
    /// an earlier `focus`.
    #[inline]
    pub fn minimize(&self) { self.shared.core.minimize(); }

    /// Quit the app: every open window's `App::on_quit` runs, the window closes without
    /// `on_close`, and `run` returns.
    ///
    /// On web this unmounts the app: its loop stops, its GPU device is destroyed, its listeners
    /// come off the page, the body style and title it changed go back, and the canvas Rune made
    /// is removed, while one passed in with [`App::canvas_id`](crate::App::canvas_id) stays, restyled as it was, for
    /// its owner. Another app may start straight away; it mounts once this one has gone. An
    /// app quit before it mounts never does.
    #[inline]
    pub fn quit(&self) { self.shared.core.quit(); }

    /// Quit the app as [`AppHandle::quit`] does, then start it again with the same arguments
    /// once this process has exited ([`Tx::relaunch`]). Web: as `quit`.
    #[inline]
    pub fn relaunch(&self) { self.shared.core.relaunch(); }

    /// macOS: show or hide the app's Dock icon (and app menu) while it runs, switching between
    /// the regular and accessory activation policies. Does nothing elsewhere.
    #[inline]
    pub fn set_dock_icon(&self, visible: bool) { self.shared.core.set_dock_icon(visible); }

    /// Override the native light or dark appearance across the whole application.
    ///
    /// On macOS this sets `NSApplication.sharedApplication.appearance` (`nil` for
    /// [`ColorScheme::System`](crate::ColorScheme::System), `NSAppearanceNameAqua` for `Light`,
    /// `NSAppearanceNameDarkAqua` for `Dark`), so native menus, open/save panels, traffic lights,
    /// system materials, and windows with no per-window override follow it. On Windows and Linux
    /// this applies to every window whose own color scheme is `ColorScheme::System`, including
    /// windows created or recreated later. Windows with an explicit per-window `Light` or `Dark`
    /// override keep their own scheme. Safe to call from any thread; no-op on web and headless.
    ///
    /// ```no_run
    /// use rune::{App, AppSpec, ColorScheme, el};
    ///
    /// let app = App::new(AppSpec::new("Notes", (480.0, 360.0), || (), |_, _| el()));
    /// let handle = app.handle();
    /// handle.set_color_scheme(ColorScheme::Dark);
    /// app.run();
    /// ```
    #[inline]
    pub fn set_color_scheme(&self, scheme: crate::ColorScheme) { self.shared.core.set_color_scheme(scheme); }

    /// Whether the window is on screen (shown, not hidden or closed). Minimized and occluded
    /// windows count as shown.
    #[inline]
    pub fn is_visible(&self) -> bool { self.shared.core.is_visible() }

    /// Why the window couldn't be created last time (no display, no GPU adapter or device),
    /// until a later creation succeeds. Needs `keep_running` or `windowless` to stay alive.
    #[inline]
    pub fn window_error(&self) -> Option<String> { self.shared.core.window_error() }

    /// Change the state from this thread: `f` runs on the UI thread, in call order, even while no
    /// window shows, and the next frame commits it with the motion its `Tx` asks for.
    pub fn update(&self, f: impl FnOnce(&mut S, &mut Tx) + Send + 'static) {
        self.update_if(move |s, tx| { f(s, tx); true });
    }

    /// `update` for a timer or feed that often finds nothing new: `f` returns whether it changed
    /// state the view reads, as `AppSpec::poll` does. On false the view doesn't re-run and the
    /// window doesn't redraw (a hidden one doesn't refresh), while what `f` asked of its `Tx`
    /// still happens: clipboard, close and quit, menus, `wake_after`, and scrolls, which take
    /// a frame. Several updates drained together commit once if any returned true.
    ///
    /// Returning false after changing state the view reads leaves the screen stale until the
    /// next change.
    pub fn update_if(&self, f: impl FnOnce(&mut S, &mut Tx) -> bool + Send + 'static) {
        self.shared.push_update(Box::new(f));
    }
}
