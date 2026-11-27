use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::Waker;
use crate::Tx;
use crate::machine::Effects;

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

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum State {
    #[default]
    Closed,
    Asked(QuickLook),
    Open(QuickLook),
    Closing,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    Show(QuickLook),
    Close,
    Taken,
    UserClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    OpenChanged,
}

impl State {
    fn open(&self) -> bool { matches!(self, Self::Asked(_) | Self::Open(_)) }

    fn look(&self) -> Option<&QuickLook> {
        match self {
            Self::Asked(look) | Self::Open(look) => Some(look),
            Self::Closed | Self::Closing => None,
        }
    }
}

pub(crate) fn step(state: &mut State, event: Event) -> Effects<Effect> {
    let event = match event {
        Event::Show(look) if look.files.is_empty() => Event::Close,
        event => event,
    };
    let was_open = state.open();
    *state = match (std::mem::take(state), event) {
        (_, Event::Show(look)) => State::Asked(look),
        (State::Asked(_) | State::Open(_) | State::Closing, Event::Close) => State::Closing,
        (State::Asked(look), Event::Taken) => State::Open(look),
        (State::Closing, Event::Taken) | (State::Open(_), Event::UserClosed) => State::Closed,
        (other, _) => other,
    };
    if state.open() != was_open { Effects::one(Effect::OpenChanged) } else { Effects::none() }
}

#[derive(Default)]
pub(crate) struct Previewing {
    state: State,
    closed: Rc<Cell<bool>>,
}

impl crate::runtime::RuntimeCore {
    pub(crate) fn request_quick_look(&mut self, look: Option<QuickLook>) {
        if self.host.native_overlay { return; }
        self.quick_look_step(look.map_or(Event::Close, Event::Show));
    }

    pub(crate) fn quick_look_open(&self) -> bool { self.host.quick_look.state.open() }

    pub(crate) fn quick_look(&self) -> Option<&QuickLook> { self.host.quick_look.state.look() }

    pub(crate) fn take_quick_look_closed(&mut self) {
        if self.host.quick_look.closed.take() { self.quick_look_step(Event::UserClosed); }
    }

    pub(crate) fn close_quick_look(&mut self) {
        self.quick_look_step(Event::Taken);
        self.quick_look_step(Event::UserClosed);
    }

    pub(crate) fn take_quick_look(&mut self) -> Option<Option<QuickLook>> {
        let take = match &self.host.quick_look.state {
            State::Asked(look) => Some(Some(look.clone())),
            State::Closing => Some(None),
            State::Closed | State::Open(_) => None,
        };
        if take.is_some() {
            self.host.quick_look.closed.set(false);
            self.quick_look_step(Event::Taken);
        }
        take
    }

    fn quick_look_step(&mut self, event: Event) {
        for Effect::OpenChanged in step(&mut self.host.quick_look.state, event) { self.commit.dirty = true; }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn show_in_window(rt: &mut crate::runtime::RuntimeCore, window: Option<&winit::window::Window>, wake: impl FnOnce() -> Waker) {
    #[cfg(target_os = "macos")]
    if let Some(window) = window && let Some(look) = rt.take_quick_look() {
        let closed = rt.host.quick_look.closed.clone();
        let wake = wake();
        macos::show(look, window, Box::new(move || { closed.set(true); wake(); }));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, wake);
        refuse(rt);
    }
}

pub(crate) fn refuse(rt: &mut crate::runtime::RuntimeCore) {
    if rt.take_quick_look().is_some() { rt.quick_look_step(Event::UserClosed); }
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
