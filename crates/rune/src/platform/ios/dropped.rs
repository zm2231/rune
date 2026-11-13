//! What an app asks for that a `RuneView` cannot do. A view is one app's one surface: it opens
//! no other windows and no overlays, and it is no window to show, move, minimise or quit. Such
//! a request is dropped, whether a handle made it from another thread or a handler from its
//! `Tx`, and the first of each kind a view drops says so on stderr, once, so an app ported
//! from the desktop hears why nothing happened.

use crate::Shell;
use crate::app::WindowRequests;

/// A kind of request a view drops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `App::window`'s windows.
    Windows,
    /// `App::overlay`'s overlays.
    Overlays,
    /// `rune::open_window`'s windows.
    Opened,
    /// `AppHandle::open`, `show`, `activate`, `hide` and `close`, and `Tx::close_window`.
    Window,
    /// `AppHandle::focus` and `minimize`, and `Tx::minimize`.
    Activation,
    /// `AppHandle::set_position` and `place`.
    Position,
    /// `AppHandle::set_dock_icon`.
    Dock,
    /// `AppHandle::quit` and `relaunch`, and `Tx::quit` and `relaunch`. A handle's call also
    /// marks its app as quitting (`Shared::quitting`); only the web's host reads that mark, so
    /// here it stays set and nothing acts on it.
    Quit,
}

impl Kind {
    #[cfg(test)]
    pub(crate) const ALL: [Kind; 8] = [Kind::Windows, Kind::Overlays, Kind::Opened, Kind::Window, Kind::Activation, Kind::Position, Kind::Dock, Kind::Quit];

    /// What stderr says of the first request of this kind, after `rune: `.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Kind::Windows => "App::window ignored: an iOS view shows one app, so its other windows never open",
            Kind::Overlays => "App::overlay ignored: an iOS view has no overlays, so they never show",
            Kind::Opened => "rune::open_window ignored: an iOS view shows one app, so the window never opens",
            Kind::Window => "AppHandle's open, show, activate, hide and close and Tx::close_window ignored: an iOS view is no window",
            Kind::Activation => "AppHandle's focus and minimize and Tx::minimize ignored: an iOS view is no window",
            Kind::Position => "AppHandle's set_position and place ignored: an iOS view is no window",
            Kind::Dock => "AppHandle::set_dock_icon ignored: iOS has no Dock",
            Kind::Quit => "quit and relaunch (AppHandle's and Tx's) ignored: an iOS app ends when the system ends it",
        }
    }
}

/// The kinds one view has reported already.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reported(u8);

impl Reported {
    /// Whether this is the first request of its kind, which is the one to report.
    pub(crate) fn first(&mut self, kind: Kind) -> bool {
        let bit = 1 << kind as u8;
        let first = self.0 & bit == 0;
        self.0 |= bit;
        first
    }
}

/// Each kind of request a handle makes, and whether `requests` holds one. Updates and wakes
/// are not among them: a view runs those.
pub(crate) fn asked(requests: &WindowRequests) -> [(Kind, bool); 5] {
    [
        (Kind::Window, requests.window.is_some()),
        (Kind::Activation, requests.activation.is_some()),
        (Kind::Position, requests.position.is_some() || requests.place.is_some()),
        (Kind::Dock, requests.dock.is_some()),
        (Kind::Quit, requests.quit.is_some()),
    ]
}

/// Each kind of request handlers make of their window, and whether a frame left one: the
/// runtime's latched close, quit or relaunch, a minimize, and how many windows
/// `rune::open_window` queued.
pub(crate) fn asked_by_handlers(shell: Option<Shell>, minimize: bool, opened: usize) -> [(Kind, bool); 4] {
    [
        (Kind::Opened, opened > 0),
        (Kind::Window, shell == Some(Shell::Close)),
        (Kind::Activation, minimize),
        (Kind::Quit, matches!(shell, Some(Shell::Quit | Shell::Relaunch))),
    ]
}

/// Takes the windows `rune::open_window` queued on this thread, which no event loop here
/// starts, and says how many there were: left queued, each would keep its state for good.
pub(crate) fn take_opened() -> usize {
    crate::windows::take_opened().len()
}

#[cfg(test)]
#[path = "dropped_tests.rs"]
mod tests;
