#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::{App, AppHandle};
use crate::el::Tx;
use crate::open_url::incoming::{Glue, Inbox, Listening};

#[cfg(target_vendor = "apple")]
pub(crate) mod apple;
#[cfg(target_os = "ios")]
pub(crate) mod ios;
#[cfg(target_os = "macos")]
pub(crate) mod macos;

/// What a window is doing, for Handoff: another of the user's devices offers to continue it,
/// and [`App::on_continue_activity`] receives it there. The type is reverse-DNS and must be
/// listed in the bundle's `NSUserActivityTypes`; `info` carries what the other side needs to
/// pick up where the user was.
///
/// ```
/// # use rune::Activity;
/// let activity = Activity::new("com.example.notes.editing").with_title("Groceries").with_info("note", "42");
/// assert_eq!(activity.info("note"), Some("42"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    kind: String,
    title: Option<String>,
    web_url: Option<String>,
    info: BTreeMap<String, String>,
}

impl Activity {
    pub fn new(kind: impl Into<String>) -> Self { Self { kind: kind.into(), ..Self::default() } }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A web page that shows the same thing, opened in a browser on a device without the app.
    pub fn with_web_url(mut self, url: impl Into<String>) -> Self {
        self.web_url = Some(url.into());
        self
    }

    pub fn with_info(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.info.insert(key.into(), value.into());
        self
    }

    pub fn kind(&self) -> &str { &self.kind }

    pub fn title(&self) -> Option<&str> { self.title.as_deref() }

    pub fn web_url(&self) -> Option<&str> { self.web_url.as_deref() }

    pub fn info(&self, key: &str) -> Option<&str> { self.info.get(key).map(String::as_str) }

    /// Every info entry, sorted by key.
    pub fn infos(&self) -> impl Iterator<Item = (&str, &str)> { self.info.iter().map(|(k, v)| (k.as_str(), v.as_str())) }
}

pub(crate) type OnContinue<S> = fn(&mut S, &Activity, &mut Tx);

static INBOX: Inbox<Activity> = Inbox::new();

struct Native;

impl Glue for Native {
    fn handled_elsewhere(&self) -> bool { platform::handled_elsewhere() }
    fn install(&self) -> bool { platform::install() }
    fn remove(&self) { platform::remove() }
}

pub(crate) fn prepare() { INBOX.prepare(&Native); }

pub(crate) fn settle() { INBOX.settle(&Native); }

pub(crate) fn listen<S: 'static>(app: &App<S>) -> Option<Listening<Activity>> { listen_in(&INBOX, app, &Native) }

pub(crate) fn listen_in<S: 'static>(inbox: &'static Inbox<Activity>, app: &App<S>, glue: &dyn Glue) -> Option<Listening<Activity>> {
    let hook = app.on_continue_activity?;
    let handle: AppHandle<S> = app.handle();
    inbox.listen_sink(glue, Arc::new(move |activity: Activity| {
        handle.update(move |s, tx| {
            #[cfg(feature = "hot")]
            crate::hot::patched_or_original!("on_continue_activity", hook, s, &activity, tx);
            #[cfg(not(feature = "hot"))]
            hook(s, &activity, tx);
        });
    }))
}

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) fn receive(activity: Activity) { INBOX.receive(activity); }

#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod platform {
    pub(super) fn handled_elsewhere() -> bool { false }
    pub(super) fn install() -> bool { cfg!(target_os = "ios") }
    pub(super) fn remove() {}
}

impl Tx {
    /// Advertises what this window is doing for Handoff once this handler's frame is done,
    /// replacing what it advertised before. macOS sets it as the window's `userActivity`, current
    /// while the window is main; iOS makes it current for the window's scene. Overlays, embedded
    /// views and other platforms ignore it.
    ///
    /// ```
    /// # use rune::{Activity, Tx, el};
    /// # struct Notes { open: u32 }
    /// # let _ = el()
    /// .on_press_tx(|s: &mut Notes, tx: &mut Tx| {
    ///     s.open = 42;
    ///     tx.advertise(Activity::new("com.example.notes.editing").with_info("note", s.open.to_string()));
    /// })
    /// # ;
    /// ```
    pub fn advertise(&mut self, activity: Activity) { self.activity = Some(Some(activity)); }

    /// Stops advertising this window's activity.
    pub fn stop_advertising(&mut self) { self.activity = Some(None); }
}

#[cfg(test)]
mod tests;
