use super::{FullScreenRequest, Shell, Tx};

impl Tx {
    /// Close the window after this handler (`App::on_close` runs; bypasses
    /// `App::on_close_request` so an approved close completes without re-asking).
    pub fn close_window(&mut self) {
        self.shell = self.shell.max(Some(Shell::Close));
    }

    /// Requests closing the window on the user's behalf (macOS unhandled ⌘W / Window › Close,
    /// assistive activation of the custom caption close button), routing through
    /// `App::on_close_request` before closing. Programmatic app code calls `Self::close_window`.
    pub(crate) fn request_user_close(&mut self) {
        self.user_close_request = true;
    }

    /// Quit the app after this handler, whether or not it keeps running without a window. Every
    /// open window's `App::on_quit` runs as it ends; no `on_close` does.
    pub fn quit(&mut self) {
        self.shell = self.shell.max(Some(Shell::Quit));
    }

    /// Quit the app as [`Tx::quit`] does, then start it again, with the same arguments, once
    /// this process has exited, as Electron's `app.relaunch()` and `app.quit()` do: a menu's
    /// Restart, or an update put in place. A system shutdown or logoff that ends the app first
    /// doesn't start it again.
    pub fn relaunch(&mut self) {
        self.shell = Some(Shell::Relaunch);
    }

    /// Minimize this window after this handler, as its minimize button would (winit's
    /// `set_minimized` off macOS). On Windows and Linux, resizable borderless windows
    /// (`WindowOptions { borderless: true, ..Default::default() }`) minimize when
    /// `WindowButtons::MINIMIZE` is enabled; does nothing (logging in debug builds) for a
    /// hidden window, a panel or launcher, a click-through overlay, a non-resizable
    /// borderless window, a borderless window on macOS, or a window without a minimize button.
    /// A close or quit asked in the same change wins.
    pub fn minimize(&mut self) {
        self.minimize_order = Some(crate::app::window_request_order());
    }

    /// Maximize this window after this handler, or restore it when it is maximized, as its
    /// maximize button would (macOS zooms). Does nothing for overlays, macOS non-activating
    /// panels, and headless runs. Read the confirmed OS state with [`crate::Cx::is_maximized`].
    pub fn toggle_maximized(&mut self) {
        self.toggle_maximized = true;
    }

    /// Take this window full screen after this handler, or bring it back, as macOS's green
    /// button and Electron's `setFullScreen` do: on Windows and Linux a borderless window over
    /// its display, which hides a custom titlebar's caption buttons (`Cx::chrome_insets`).
    /// Does nothing for overlays, macOS non-activating panels, and headless runs. When both
    /// `toggle_full_screen` and [`Self::set_full_screen`] run in one turn, the last call wins.
    /// Read the current full-screen state with [`crate::Cx::is_full_screen`] (flips at transition
    /// start on macOS and on OS confirmation on Windows and Linux).
    pub fn toggle_full_screen(&mut self) {
        self.full_screen = Some(FullScreenRequest::Toggle);
    }

    /// Enter (`true`) or exit (`false`) full screen after this handler, as Electron's
    /// `BrowserWindow.setFullScreen(flag)` does: on Windows and Linux a borderless window over
    /// its display, on macOS a native full-screen Space. Does nothing for overlays, macOS
    /// non-activating panels, and headless runs, and is a no-op when the window is already in
    /// the requested state. When multiple calls to `set_full_screen` or
    /// [`Self::toggle_full_screen`] run in one turn, the last call wins. Read the current
    /// full-screen state with [`crate::Cx::is_full_screen`] (flips at transition start on macOS
    /// and on OS confirmation on Windows and Linux).
    ///
    /// ```
    /// # use rune::{el, Tx};
    /// let _button = el().on_press_tx(|_: &mut (), tx: &mut Tx| tx.set_full_screen(true));
    /// ```
    pub fn set_full_screen(&mut self, full_screen: bool) {
        self.full_screen = Some(FullScreenRequest::Set(full_screen));
    }

    /// Set this window's title after this handler, in its title bar and the Window menu. The
    /// window starts with `AppSpec`'s title and keeps the last one set when it's closed and
    /// opened again. Headless `--tree` output shows it as `"title"`. An overlay has no title.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    /// Override this window's native light or dark chrome after this handler, retained across
    /// native window recreation.
    ///
    /// On Windows this updates the native titlebar (`DWMWA_USE_IMMERSIVE_DARK_MODE`), on macOS
    /// `NSWindow.appearance`, on Wayland client-side decorations, and on X11 `_GTK_THEME_VARIANT`.
    /// Passing [`ColorScheme::System`](crate::ColorScheme::System) clears the per-window override
    /// so the window inherits any app-wide override set via
    /// [`AppHandle::set_color_scheme`](crate::AppHandle::set_color_scheme) or the operating
    /// system's appearance. In an overlay's handler this sets that overlay's scheme:
    /// [`Cx::system_dark`](crate::Cx::system_dark) follows it, and on macOS so does the overlay
    /// panel's `NSAppearance`. Ignored on web and headless.
    ///
    /// ```no_run
    /// use rune::{ColorScheme, El, el};
    ///
    /// let button: El = el().on_press_tx(|_: &mut (), tx| tx.set_color_scheme(ColorScheme::Dark));
    /// let _ = button;
    /// ```
    pub fn set_color_scheme(&mut self, scheme: crate::ColorScheme) {
        self.color_scheme = Some(scheme);
    }

    /// Requests a whole-window system backdrop material ([`WindowBackdrop`](crate::WindowBackdrop))
    /// after this handler, retained across native window recreation (last call in a turn wins).
    ///
    /// On Windows 11 Build 22621+ transparent DirectComposition GPU windows and overlays, this
    /// applies `DWMWA_SYSTEMBACKDROP_TYPE`. Never flips
    /// [`Cx::window_backdrop`](crate::Cx::window_backdrop) optimistically: views observe the new
    /// backdrop only once the OS accepts the attribute (Windows draws the live material only while
    /// the window is the foreground window and shows a flat fallback while unfocused). Ignored on
    /// web and headless.
    ///
    /// ```no_run
    /// use rune::{El, WindowBackdrop, el};
    ///
    /// let button: El = el().on_press_tx(|_: &mut (), tx| {
    ///     tx.set_window_backdrop(WindowBackdrop::Mica);
    /// });
    /// let _ = button;
    /// ```
    pub fn set_window_backdrop(&mut self, backdrop: crate::WindowBackdrop) {
        self.window_backdrop = Some(backdrop);
    }

    /// Sets or clears this window's represented document path on macOS (`NSWindow.representedURL`),
    /// updating AppKit's titlebar proxy icon and path menu without performing filesystem I/O.
    /// Pass `None` (or an empty path) to clear the represented file. When the window opted into
    /// [`MacWindowOptions::title_from_file`](crate::MacWindowOptions::title_from_file), the
    /// native title updates to the file's final path component (and restores the window's
    /// configured title when cleared); calling [`Tx::set_title`](Self::set_title) in the same
    /// turn takes precedence. Retained across native window recreation. No-op off macOS, on
    /// overlays, and in headless runs.
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use rune::{El, el};
    ///
    /// let button: El = el().on_press_tx(|_: &mut (), tx| {
    ///     tx.set_represented_file(Some(PathBuf::from("/tmp/notes.md")));
    /// });
    /// let _ = button;
    /// ```
    pub fn set_represented_file(&mut self, path: Option<std::path::PathBuf>) {
        self.represented_file = Some(crate::window::spec::normalize_represented_file(path));
    }

    /// Sets this window's document-edited state on macOS (`NSWindow.documentEdited`), showing
    /// (`true`) or hiding (`false`) the unsaved-changes dot inside the close button. Retained
    /// across native window recreation. No-op off macOS, on overlays, and in headless runs.
    ///
    /// Following AppKit's default, marking a document as edited disables dragging its proxy icon
    /// until `set_document_edited(false)` is called.
    ///
    /// ```no_run
    /// use rune::{El, el};
    ///
    /// let button: El = el().on_press_tx(|_: &mut (), tx| tx.set_document_edited(true));
    /// let _ = button;
    /// ```
    pub fn set_document_edited(&mut self, edited: bool) {
        self.document_edited = Some(edited);
    }

    /// Have a screen reader say `text` after this handler, without moving focus: a reply that
    /// finished, a request that arrived, an item that moved. `Polite` waits for the reader to
    /// finish what it's saying; `Assertive` interrupts it (errors and requests that block
    /// work). Each window keeps one announcement, a live region of its own that draws nothing;
    /// the last call in a frame wins, and saying the same words again says them again. With no
    /// assistive technology listening it costs one string.
    pub fn announce(&mut self, text: impl Into<String>, how: crate::LiveRegion) {
        if how != crate::LiveRegion::Off { self.announce = Some((text.into(), how)); }
    }

    /// Open `url` in the system's default handler once this handler's frame is done: a browser
    /// for `http:` and `https:`, the mail app for `mailto:`. Other schemes (`file:`, an app's
    /// own) and malformed links are refused here with the reason and nothing opens. Headless
    /// runs and tests open nothing; read the requests with `Runtime::take_url_requests`.
    /// [`Tx::open_url_then`] also says whether the open worked.
    pub fn open_url(&mut self, url: &str) -> Result<(), crate::open_url::OpenUrlError> {
        self.open_urls.push(crate::open_url::Link { url: crate::open_url::check(url)?, same_tab: false, opened: None });
        Ok(())
    }

    /// `open_url`, but on web the link replaces this page instead of opening a tab: the app's
    /// [`App::same_tab_router`](crate::App::same_tab_router) gets it first (a single-page app
    /// routes it with `history.pushState`), else the browser loads it with `location.assign`.
    /// On web a link without a scheme (`/notes/42`) is relative to the page. Native opens it as
    /// `open_url` does, and headless runs record it with the others.
    pub fn open_url_same_tab(&mut self, url: &str) -> Result<(), crate::open_url::OpenUrlError> {
        self.open_urls.push(crate::open_url::Link { url: crate::open_url::check_same_tab(url)?, same_tab: true, opened: None });
        Ok(())
    }

    /// Resize this window's content to `size` points, as an onboarding window changes height
    /// per step. The top edge stays put and the window stays on its screen. The frame moves
    /// on this change's spring (`Tx::motion`, else `tok::motion::DEFAULT`), so content laid
    /// out against the window's edges follows it frame by frame; `Tx::instant` or reduced
    /// motion jumps. On macOS it animates; Windows and Linux resize at once, and Wayland
    /// can't keep the window on screen. Headless runs resize the viewport on the virtual
    /// clock, so `--tree` and scripts see it; their frames keep the launch canvas size.
    /// Overlays ignore it: use `OverlayHandle::resize`.
    pub fn resize_window(&mut self, size: (f32, f32)) {
        self.frame_request(crate::window::frame::FrameRequest { size: Some(size), animate: true, ..Default::default() });
    }

    /// [`Tx::resize_window`] without the animation.
    pub fn resize_window_instant(&mut self, size: (f32, f32)) {
        self.frame_request(crate::window::frame::FrameRequest { size: Some(size), ..Default::default() });
    }

    /// Centre the window on its screen as AppKit's `center` does: centred across, slightly
    /// above the middle. With [`Tx::resize_window`] in the same change it centres the new
    /// size, on the same spring; alone it jumps, or retargets a resize already moving.
    /// Wayland and headless runs ignore it.
    pub fn center_window(&mut self) {
        self.frame_request(crate::window::frame::FrameRequest { center: true, ..Default::default() });
    }

    /// Start an interactive OS/compositor-driven window resize from `edge` (winit's
    /// `Window::drag_resize_window`: Win32 sizing borders, X11 `_NET_WM_MOVERESIZE`, Wayland
    /// `xdg_toplevel.resize`). Call from a press or [`El::on_drag`](crate::El::on_drag)
    /// [`Phase::Down`](crate::Phase::Down) handler on a custom resize grip; for a plain resize
    /// edge or corner without custom logic, prefer
    /// [`El::window_resize`](crate::El::window_resize), which also supplies the matching resize
    /// cursor. Ignored on macOS, on the web, in overlays, and in headless runs.
    ///
    /// ```
    /// # use rune::{Cursor, Phase, ResizeEdge, el};
    /// # struct App;
    /// # let _ =
    /// el().size(12.0, 12.0).cursor(Cursor::ResizeNwse).on_drag(|_: &mut App, d, tx| {
    ///     if d.phase == Phase::Down {
    ///         tx.drag_resize_window(ResizeEdge::SouthEast);
    ///     }
    /// })
    /// # ;
    /// ```
    pub fn drag_resize_window(&mut self, edge: crate::ResizeEdge) {
        self.frame_request(crate::window::frame::FrameRequest { resize_drag: Some(edge), ..Default::default() });
    }
}
