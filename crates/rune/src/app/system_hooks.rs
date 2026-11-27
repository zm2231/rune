use super::App;
use crate::el::Tx;

impl<S: 'static> App<S> {
    /// macOS: runs on the main thread when the app is reopened while it runs: its Dock icon
    /// clicked, or opened again from Finder, Spotlight or `open`, also in an accessory app.
    /// `has_visible_windows` says whether any Rune window that reopening could bring back is on
    /// screen and not minimized (launcher panels and overlays don't count). Return true when the
    /// hook handled it, which skips Rune's default; false lets the default run. When no
    /// such window shows, the default shows and activates a minimized window, else the window Rune
    /// hid last (closed with `keep_running`, or hidden), else the main window. It never brings
    /// back a launcher or other nonactivating panel, a window that closed for good, or anything
    /// in an accessory app. With several windows, every window's hook runs, in window order,
    /// and any true skips the default. Windows: runs when the app is launched again without a
    /// link while it runs, once `rune::single_instance` made it the app's one process; the
    /// default then also brings the front window forward, restored. Linux: the same, and on
    /// `org.freedesktop.Application.Activate`; the default uses the launch's activation token.
    /// Never called on other platforms, iOS among them. Headless scripts deliver it with `{"reopen": true}`.
    pub fn on_reopen(mut self, f: fn(&mut S, bool, &mut Tx) -> bool) -> Self {
        self.on_reopen = Some(f);
        self
    }

    /// macOS: runs on the main thread with each link the system opens with this app
    /// (`myapp://...`, from `open`, a browser or another app), including the one that launched
    /// it, which runs before the window's first frame. Each runs in arrival order as an
    /// `AppHandle::update`, so a window it opens through a handle opens in the same turn. The
    /// string is passed through as received; it comes from outside the app, so parse and
    /// validate it before acting on it. Bringing a window forward is up to the app
    /// (`AppHandle::open`). The app's bundle must declare the scheme (`CFBundleURLTypes`, see
    /// the README). With several windows whose apps have the hook, the first still running
    /// receives links.
    ///
    /// Windows: the links among the launch arguments (`scheme://…`), as Windows passes a link to
    /// the process it starts, and with `rune::single_instance` the links of every later launch,
    /// which that launch hands to this process instead of opening a window. The app registers its
    /// scheme itself (`HKEY_CURRENT_USER\Software\Classes\<scheme>`, usually at install), and
    /// names it with `rune::url_schemes` so only arguments on it are links: without that a flag's
    /// URL value (`--server http://localhost:4096`) is one too. Linux: the same (never a `file:`
    /// URI), and `org.freedesktop.Application.Open`'s links; see the README.
    ///
    /// iOS: the links the system opens with an app `run` started, the launching one before the
    /// first frame: its bundle's schemes' and universal links, which arrive as their `https://`
    /// URL. A view in another app (`ios::view`) gets the ones that app passes on with
    /// `ios::open_url`; see `IOS.md`.
    ///
    /// Never called on other platforms.
    pub fn on_open_url(mut self, f: fn(&mut S, &str, &mut Tx)) -> Self {
        self.on_open_url = Some(f);
        self
    }

    /// macOS: runs on the main thread with each batch of files the system opens with this app
    /// (Finder double-click or "Open With", files dropped on the Dock icon, or `open -a MyApp
    /// file.ext`), including the `kAEOpenDocuments` (`'odoc'`) Apple Event that launched it,
    /// which runs before the window's first frame. Each batch runs in arrival order as an
    /// `AppHandle::update`, so a window it opens through a handle opens in the same turn. Never
    /// called with an empty slice (non-file URLs in the event are skipped). Paths come from
    /// outside the app, so validate them before acting. Bringing a window forward is up to the app
    /// (`AppHandle::open`). The app's bundle must declare its document types
    /// (`CFBundleDocumentTypes`, see the README). With several windows whose apps have the hook,
    /// the first still running receives files.
    ///
    /// Linux: the `file://` URIs among the launch arguments (`Exec=myapp %U`, not `%F`), with
    /// `rune::single_instance` a later launch's too, and `org.freedesktop.Application.Open`'s;
    /// see the README. Never called on Windows, which passes opened files to a new process as
    /// launch arguments. Headless scripts deliver it with `{"open_files": ["a.torrent"]}`.
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use rune::{App, AppSpec, el};
    ///
    /// let spec = AppSpec::new("Viewer", (640.0, 420.0), Vec::<PathBuf>::new, |_, _| el());
    /// let app = App::new(spec).on_open_files(|opened, paths, _| {
    ///     opened.extend(paths.iter().cloned());
    /// });
    /// let _ = app;
    /// ```
    pub fn on_open_files(mut self, f: fn(&mut S, &[std::path::PathBuf], &mut Tx)) -> Self {
        self.on_open_files = Some(f);
        self
    }

    /// macOS: runs on the main thread with what the user sends to this app from another app's
    /// Services menu ("Send to My App"): the selected text, links and files, and the entry's
    /// `NSUserData` as [`ServiceInput::name`](crate::ServiceInput::name), so one app can offer
    /// several entries. A service that launches the app arrives before the window's first frame.
    /// The bundle declares each entry under `NSServices` with `NSMessage` set to
    /// [`SERVICE_MESSAGE`](crate::SERVICE_MESSAGE) (`runeService`); see the README. Input comes
    /// from another app, so validate it before acting. Never called on other platforms.
    ///
    /// ```no_run
    /// use rune::{App, AppSpec, el};
    ///
    /// let spec = AppSpec::new("Inbox", (640.0, 420.0), Vec::<String>::new, |_, _| el());
    /// let app = App::new(spec).on_service(|notes, input, _| {
    ///     notes.extend(input.text().map(str::to_owned));
    /// });
    /// let _ = app;
    /// ```
    pub fn on_service(mut self, f: fn(&mut S, &crate::ServiceInput, &mut Tx)) -> Self {
        self.on_service = Some(f);
        self
    }

    /// macOS and iOS: runs on the main thread with an activity the user continues here from
    /// another of their devices through Handoff, one a window advertised with
    /// [`Tx::advertise`](crate::Tx::advertise). One that launches the app arrives before the
    /// window's first frame. The bundle lists each activity type in `NSUserActivityTypes`, and
    /// both devices are signed in to the same account with Handoff on. A continued web page
    /// (`NSUserActivityTypeBrowsingWeb`) goes to [`Self::on_open_url`] instead. Never called on
    /// other platforms.
    ///
    /// ```no_run
    /// use rune::{App, AppSpec, el};
    ///
    /// let spec = AppSpec::new("Notes", (640.0, 420.0), || None::<String>, |_, _| el());
    /// let app = App::new(spec).on_continue_activity(|open, activity, _| {
    ///     if activity.kind() == "com.example.notes.editing" { *open = activity.info("note").map(str::to_owned); }
    /// });
    /// let _ = app;
    /// ```
    pub fn on_continue_activity(mut self, f: fn(&mut S, &crate::Activity, &mut Tx)) -> Self {
        self.on_continue_activity = Some(f);
        self
    }

    /// macOS and iOS: runs on the main thread with the id of a [`SearchItem`](crate::SearchItem)
    /// the user opened from Spotlight, one [`Tx::index_search_items`](crate::Tx::index_search_items)
    /// indexed. One that launches the app arrives before the window's first frame. Never called on
    /// other platforms.
    ///
    /// ```no_run
    /// use rune::{App, AppSpec, el};
    ///
    /// let spec = AppSpec::new("Notes", (640.0, 420.0), || None::<String>, |_, _| el());
    /// let app = App::new(spec).on_open_search_item(|open, id, _| *open = Some(id.to_owned()));
    /// let _ = app;
    /// ```
    pub fn on_open_search_item(mut self, f: fn(&mut S, &str, &mut Tx)) -> Self {
        self.on_open_search_item = Some(f);
        self
    }

    /// Runs on the main thread when the system's search index refuses a request
    /// [`Tx::index_search_items`](crate::Tx::index_search_items) or a removal made, or when
    /// Rune drops one ([`SearchIndexError`](crate::SearchIndexError)). Rune also prints each.
    pub fn on_search_index_error(mut self, f: fn(&mut S, &crate::SearchIndexError, &mut Tx)) -> Self {
        self.on_search_index_error = Some(f);
        self
    }
}
