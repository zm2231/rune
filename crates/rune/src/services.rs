#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::path::PathBuf;
use std::sync::Arc;

use crate::app::{App, AppHandle};
use crate::el::Tx;
use crate::open_url::incoming::{Glue, Inbox, Listening};

#[cfg(target_os = "macos")]
mod macos;

/// The `NSMessage` an app's `NSServices` entries name, so their text and files reach
/// [`App::on_service`].
pub const SERVICE_MESSAGE: &str = "runeService";

/// What a Services menu entry sent: the entry's `NSUserData` as its name, and the text, links
/// and files selected where the user chose it.
///
/// ```
/// # use rune::ServiceInput;
/// let input = ServiceInput::new("send").with_text("Hello").with_file("notes.md");
/// assert_eq!(input.name(), "send");
/// assert_eq!(input.text(), Some("Hello"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceInput {
    name: String,
    text: Option<String>,
    urls: Vec<String>,
    files: Vec<PathBuf>,
}

impl ServiceInput {
    pub fn new(name: impl Into<String>) -> Self { Self { name: name.into(), ..Self::default() } }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.urls.push(url.into());
        self
    }

    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(path.into());
        self
    }

    /// The entry's `NSUserData`, empty when it has none.
    pub fn name(&self) -> &str { &self.name }

    pub fn text(&self) -> Option<&str> { self.text.as_deref() }

    /// Links other than files.
    pub fn urls(&self) -> &[String] { &self.urls }

    pub fn files(&self) -> &[PathBuf] { &self.files }

    pub fn is_empty(&self) -> bool { self.text.as_deref().is_none_or(str::is_empty) && self.urls.is_empty() && self.files.is_empty() }
}

pub(crate) type OnService<S> = fn(&mut S, &ServiceInput, &mut Tx);

static INBOX: Inbox<ServiceInput> = Inbox::new();

struct Native;

impl Glue for Native {
    fn handled_elsewhere(&self) -> bool { platform::handled_elsewhere() }
    fn install(&self) -> bool { platform::install() }
    fn remove(&self) { platform::remove() }
}

pub(crate) fn prepare() { INBOX.prepare(&Native); }

pub(crate) fn settle() { INBOX.settle(&Native); }

pub(crate) fn listen<S: 'static>(app: &App<S>) -> Option<Listening<ServiceInput>> {
    listen_in(&INBOX, app, &Native)
}

pub(crate) fn listen_in<S: 'static>(inbox: &'static Inbox<ServiceInput>, app: &App<S>, glue: &dyn Glue) -> Option<Listening<ServiceInput>> {
    let hook = app.on_service?;
    let handle: AppHandle<S> = app.handle();
    inbox.listen_sink(glue, Arc::new(move |input: ServiceInput| {
        handle.update(move |s, tx| {
            #[cfg(feature = "hot")]
            crate::hot::patched_or_original!("on_service", hook, s, &input, tx);
            #[cfg(not(feature = "hot"))]
            hook(s, &input, tx);
        });
    }))
}

#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod platform {
    pub(super) fn handled_elsewhere() -> bool { false }
    pub(super) fn install() -> bool { false }
    pub(super) fn remove() {}
}

#[cfg(test)]
mod tests;
