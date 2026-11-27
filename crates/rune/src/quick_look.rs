use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::Waker;
use crate::Tx;

#[cfg(target_os = "macos")]
mod macos;

/// The files to preview and which one shows first.
///
/// ```
/// # use rune::QuickLook;
/// let look = QuickLook::new().file("notes.pdf").file("photo.heic").at(1);
/// assert_eq!(look.files().len(), 2);
/// assert_eq!(look.index(), 1);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuickLook {
    files: Vec<PathBuf>,
    index: usize,
}

impl QuickLook {
    pub fn new() -> Self { Self::default() }

    pub fn file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(path.into());
        self
    }

    pub fn files(&self) -> &[PathBuf] { &self.files }

    /// Which file shows first; the panel's arrow keys move between them. Past the end shows
    /// the last.
    pub fn at(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    pub fn index(&self) -> usize { self.index.min(self.files.len().saturating_sub(1)) }
}

#[derive(Default)]
pub(crate) struct Previewing {
    shown: Option<QuickLook>,
    sent: bool,
    closed: Rc<Cell<bool>>,
}

impl crate::runtime::RuntimeCore {
    pub(crate) fn request_quick_look(&mut self, look: Option<QuickLook>) {
        let look = look.filter(|look| !look.files.is_empty() && !self.host.native_overlay);
        let p = &mut self.host.quick_look;
        if p.shown.is_some() != look.is_some() { self.commit.dirty = true; }
        p.shown = look;
        p.sent = false;
    }

    pub(crate) fn quick_look_open(&self) -> bool { self.host.quick_look.shown.is_some() }

    pub(crate) fn quick_look(&self) -> Option<&QuickLook> { self.host.quick_look.shown.as_ref() }

    pub(crate) fn take_quick_look_closed(&mut self) {
        let p = &mut self.host.quick_look;
        if p.closed.take() && p.sent && p.shown.take().is_some() { self.commit.dirty = true; }
    }

    pub(crate) fn close_quick_look(&mut self) {
        let p = &mut self.host.quick_look;
        p.sent = true;
        if p.shown.take().is_some() { self.commit.dirty = true; }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn take_quick_look(&mut self) -> Option<Option<QuickLook>> {
        let p = &mut self.host.quick_look;
        if std::mem::replace(&mut p.sent, true) { return None; }
        p.closed.set(false);
        Some(p.shown.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn show_in_window(rt: &mut crate::runtime::RuntimeCore, window: Option<&winit::window::Window>, wake: impl FnOnce() -> Waker) {
    if rt.host.quick_look.sent { return; }
    #[cfg(target_os = "macos")]
    if let Some(window) = window && let Some(look) = rt.take_quick_look() {
        let closed = rt.host.quick_look.closed.clone();
        let wake = wake();
        macos::show(look, window, Box::new(move || { closed.set(true); wake(); }));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, wake);
        rt.close_quick_look();
    }
}

#[cfg(any(test, target_vendor = "apple", target_arch = "wasm32"))]
pub(crate) fn refuse(rt: &mut crate::runtime::RuntimeCore) {
    if !rt.host.quick_look.sent { rt.close_quick_look(); }
}

impl Tx {
    /// Previews `look`'s files in the system's Quick Look panel once this handler's frame is
    /// done, or, when the panel is already open, shows these files in it instead. An empty
    /// `look` closes it. macOS only; elsewhere nothing shows and
    /// [`Cx::quick_look_open`](crate::Cx::quick_look_open) stays false.
    ///
    /// ```
    /// # use rune::{Key, QuickLook, Tx, el};
    /// # struct App { selected: Option<std::path::PathBuf> }
    /// # let _ = el()
    /// .on_key(|s: &mut App, key, tx: &mut Tx| {
    ///     if key.key != Key::Text(" ".into()) { return false; }
    ///     match &s.selected {
    ///         Some(_) if tx.quick_look_open() => tx.close_quick_look(),
    ///         Some(path) => tx.quick_look(QuickLook::new().file(path)),
    ///         None => return false,
    ///     }
    ///     true
    /// })
    /// # ;
    /// ```
    pub fn quick_look(&mut self, look: QuickLook) { self.quick_look = Some(Some(look)); }

    /// Closes the Quick Look panel this window opened.
    pub fn close_quick_look(&mut self) { self.quick_look = Some(None); }

    /// Whether this window's Quick Look panel is open, as the handler started.
    pub fn quick_look_open(&self) -> bool { self.quick_look_open }
}

#[cfg(test)]
mod tests;
