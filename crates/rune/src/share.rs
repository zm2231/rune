//! The system's share sheet: [`Tx::share`]. iOS shows `UIActivityViewController`, macOS an
//! `NSSharingServicePicker` beside the element it is anchored to, and the web
//! `navigator.share` where the browser has it. Windows and Linux answer
//! [`ShareError::Unsupported`]. Headless runs keep the request until a test or driver answers
//! it with [`Runtime::resolve_share`](crate::Runtime::resolve_share).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::machine::Effects;
use crate::popup_menu::PopupAnchor;
use crate::runtime::Waker;
use crate::Tx;

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web;

/// What to share: any of a text, a link and files.
///
/// ```
/// # use rune::Share;
/// let share = Share::new().text("Notes from today").url("https://example.com/notes/42");
/// assert!(!share.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Share {
    pub(crate) title: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) files: Vec<PathBuf>,
}

impl Share {
    pub fn new() -> Self { Self::default() }

    /// A title for what is shared, where the sheet shows one (the web's, and a mail's subject
    /// on some platforms).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// A file to share by its path. The web shares no files: a share with one is refused there.
    pub fn file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(path.into());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.text.as_deref().is_none_or(str::is_empty) && self.url.as_deref().is_none_or(str::is_empty) && self.files.is_empty()
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        let files: Vec<_> = self.files.iter().map(|f| f.display().to_string()).collect();
        serde_json::json!({ "title": self.title, "text": self.text, "url": self.url, "files": files })
    }
}

/// Why nothing was shared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareError {
    /// The share holds no text, link or file.
    Empty,
    /// A share sheet is already up.
    Busy,
    /// This platform, or this host, has no share sheet.
    Unsupported(String),
    /// The system's sheet couldn't share it.
    Failed(String),
}

impl std::fmt::Display for ShareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("nothing to share"),
            Self::Busy => f.write_str("a share sheet is already open"),
            Self::Unsupported(why) => write!(f, "sharing is unsupported: {why}"),
            Self::Failed(why) => write!(f, "couldn't share: {why}"),
        }
    }
}
impl std::error::Error for ShareError {}

/// What [`Tx::share`]'s callback gets: where it was shared (the system's name for the
/// activity or service the user chose), `Ok(None)` when the user closed the sheet, or why
/// nothing was shared.
pub type ShareResult = Result<Option<String>, ShareError>;

pub(crate) type DoneFn = Box<dyn FnOnce(crate::states::States<'_>, ShareResult, &mut Tx)>;

pub(crate) struct Request {
    share: Share,
    anchor: PopupAnchor,
    on_done: DoneFn,
}

/// A `Tx`'s share. `Tx` is `Clone`; clones share the request and the runtime takes it once.
#[derive(Clone)]
pub(crate) struct Call(Rc<RefCell<Option<Request>>>);

impl Call {
    pub(crate) fn take(&self) -> Option<Request> { self.0.borrow_mut().take() }
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ShareCall").field(&self.0.borrow().as_ref().map(|r| &r.share)).finish()
    }
}

impl Tx {
    /// Shows the system's share sheet for `share` once this handler's frame is done, beside
    /// `anchor` (an element's key, or a point) where the sheet points at something: an iPad's
    /// popover, a Mac's picker. `on_done` runs, as an action on a later frame, with what the
    /// user did ([`ShareResult`]). A share asked for while a sheet is up is refused with
    /// [`ShareError::Busy`].
    ///
    /// ```
    /// # use rune::{Share, Tx, el};
    /// # struct App { shared_to: Option<String> }
    /// # let _ = el()
    /// .key("share")
    /// .on_press_tx(|_: &mut App, tx: &mut Tx| {
    ///     tx.share(Share::new().url("https://example.com/"), "share", |s: &mut App, result, _| {
    ///         s.shared_to = result.ok().flatten();
    ///     });
    /// })
    /// # ;
    /// ```
    pub fn share<S: 'static>(&mut self, share: Share, anchor: impl Into<PopupAnchor>, on_done: impl FnOnce(&mut S, ShareResult, &mut Tx) + 'static) {
        let on_done: DoneFn = Box::new(move |s, result, tx| {
            if let Some(s) = s.cast::<S>() { on_done(s, result, tx); }
        });
        self.shares.push(Call(Rc::new(RefCell::new(Some(Request { share, anchor: anchor.into(), on_done })))));
    }
}

/// A share the runtime holds, from the request until it is answered.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pending {
    pub(crate) id: u64,
    pub(crate) share: Share,
    pub(crate) anchor: PopupAnchor,
}

/// The runtime's one share sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum State {
    #[default]
    Idle,
    /// Asked for; a host shows it next.
    Requested(Pending),
    /// A host has it on screen.
    Showing(Pending),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    Asked(Pending),
    /// A host has put the requested sheet on screen.
    Shown,
    Answered(u64, ShareResult),
    /// The host can't show a share sheet.
    Refused(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Effect {
    /// Share `id`'s callback runs with this result.
    Answer(u64, ShareResult),
}

pub(crate) fn step(state: &mut State, event: Event) -> Effects<Effect> {
    match (std::mem::take(state), event) {
        (State::Idle, Event::Asked(p)) if p.share.is_empty() => Effects::one(Effect::Answer(p.id, Err(ShareError::Empty))),
        (State::Idle, Event::Asked(p)) => {
            *state = State::Requested(p);
            Effects::none()
        }
        (busy, Event::Asked(p)) => {
            *state = busy;
            Effects::one(Effect::Answer(p.id, Err(ShareError::Busy)))
        }
        (State::Requested(p), Event::Shown) => {
            *state = State::Showing(p);
            Effects::none()
        }
        (State::Requested(p) | State::Showing(p), Event::Refused(why)) => Effects::one(Effect::Answer(p.id, Err(ShareError::Unsupported(why)))),
        (State::Showing(p), Event::Answered(id, result)) if p.id == id => Effects::one(Effect::Answer(id, result)),
        (other, _) => {
            *state = other;
            Effects::none()
        }
    }
}

/// Where a host sends what its sheet answered, on the main thread, any time later. The answer
/// waits for the runtime's next frame, which `wake` asks for.
#[cfg(any(test, target_os = "ios", target_os = "macos", target_arch = "wasm32"))]
#[derive(Clone)]
pub(crate) struct Reply {
    id: u64,
    answered: Rc<RefCell<Vec<(u64, ShareResult)>>>,
    wake: Waker,
}

#[cfg(any(test, target_os = "ios", target_os = "macos", target_arch = "wasm32"))]
impl Reply {
    pub(crate) fn send(&self, result: ShareResult) {
        self.answered.borrow_mut().push((self.id, result));
        (self.wake)();
    }
}

/// What a host shows: the share, where its anchor is in window points (none when its element
/// is gone), and where its answer goes.
#[cfg(any(test, target_os = "ios", target_os = "macos", target_arch = "wasm32"))]
pub(crate) struct Shown {
    pub(crate) share: Share,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) anchor: Option<[f32; 4]>,
    pub(crate) reply: Reply,
}

/// The runtime's share sheet and the callbacks waiting on it.
#[derive(Default)]
pub(crate) struct Sharing {
    state: State,
    next: u64,
    callbacks: Vec<(u64, DoneFn)>,
    answered: Rc<RefCell<Vec<(u64, ShareResult)>>>,
}

impl crate::runtime::RuntimeCore {
    pub(crate) fn request_share(&mut self, request: Request) {
        let sharing = &mut self.host.sharing;
        sharing.next += 1;
        let id = sharing.next;
        sharing.callbacks.push((id, request.on_done));
        self.share_step(Event::Asked(Pending { id, share: request.share, anchor: request.anchor }));
        if self.host.native_overlay { self.refuse_share("overlays cannot show a share sheet"); }
    }

    pub(crate) fn share_requested(&self) -> bool { matches!(self.host.sharing.state, State::Requested(_)) }

    /// The share for the host to show, marked shown, with its answer's way back.
    #[cfg(any(test, target_os = "ios", target_os = "macos", target_arch = "wasm32"))]
    pub(crate) fn take_share(&mut self, wake: Waker) -> Option<Shown> {
        let State::Requested(p) = &self.host.sharing.state else { return None };
        let (id, share) = (p.id, p.share.clone());
        let anchor = match &p.anchor {
            PopupAnchor::Key(key) => self.anchor_rect(key),
            &PopupAnchor::Point(x, y) => Some([x, y, 0.0, 0.0]),
        };
        let anchor = anchor.map(|r| self.to_window(r));
        self.share_step(Event::Shown);
        Some(Shown { share, anchor, reply: Reply { id, answered: self.host.sharing.answered.clone(), wake } })
    }

    /// Answers a share this host can't show.
    pub(crate) fn refuse_share(&mut self, why: &str) {
        if !matches!(self.host.sharing.state, State::Idle) { self.share_step(Event::Refused(why.into())); }
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(crate) fn dismiss_share(&mut self) {
        self.answer_share(Ok(None));
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(crate) fn drop_shares(&mut self) {
        self.host.sharing.state = State::Idle;
        self.host.sharing.callbacks.clear();
    }

    /// Queues what the hosts' sheets answered since the last frame.
    pub(crate) fn take_share_answers(&mut self) {
        let answered = std::mem::take(&mut *self.host.sharing.answered.borrow_mut());
        for (id, result) in answered { self.share_step(Event::Answered(id, result)); }
    }

    pub(crate) fn share_json(&self) -> Option<serde_json::Value> {
        match &self.host.sharing.state {
            State::Idle => None,
            State::Requested(p) | State::Showing(p) => Some(p.share.to_json()),
        }
    }

    pub(crate) fn resolve_share(&mut self, result: ShareResult) -> Result<(), ShareError> {
        if self.answer_share(result) { Ok(()) } else { Err(ShareError::Failed("no share sheet is open".into())) }
    }

    fn answer_share(&mut self, result: ShareResult) -> bool {
        let id = match &self.host.sharing.state {
            State::Idle => return false,
            State::Requested(p) | State::Showing(p) => p.id,
        };
        if matches!(self.host.sharing.state, State::Requested(_)) { self.share_step(Event::Shown); }
        self.share_step(Event::Answered(id, result));
        true
    }

    fn share_step(&mut self, event: Event) {
        for Effect::Answer(id, result) in step(&mut self.host.sharing.state, event) {
            let callbacks = &mut self.host.sharing.callbacks;
            if let Some(i) = callbacks.iter().position(|(c, _)| *c == id) {
                let (_, f) = callbacks.swap_remove(i);
                self.share_done(f, result);
            }
        }
    }
}

/// Shows a window's requested share: the picker on macOS, a refusal on Windows and Linux.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn show_in_window(rt: &mut crate::runtime::RuntimeCore, window: Option<&winit::window::Window>, wake: impl FnOnce() -> Waker) {
    if !rt.share_requested() { return; }
    #[cfg(target_os = "macos")]
    if let Some(window) = window && let Some(shown) = rt.take_share(wake()) { macos::show(shown, window); }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, wake);
        rt.refuse_share("this platform has no share sheet Rune can show");
    }
}

/// True when `event` is Escape for a share picker open over `window`, which it closes; the key
/// then goes no further.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn escape_closes_picker(window: &winit::window::Window, event: &winit::event::KeyEvent) -> bool {
    use winit::keyboard::{Key, NamedKey};
    if event.logical_key != Key::Named(NamedKey::Escape) { return false; }
    #[cfg(target_os = "macos")]
    { macos::escape(window, event.state.is_pressed()) }
    #[cfg(not(target_os = "macos"))]
    { let _ = window; false }
}

#[cfg(test)]
mod tests;
