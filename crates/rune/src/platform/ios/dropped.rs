//! What an app asks for that iOS cannot do. No window moves, minimises, has a Dock icon or
//! quits, and a `RuneView` a UIKit app embeds is no window at all: it opens no other windows
//! and has none to show, hide or close. Such a request is dropped, whether a handle made it
//! from another thread or a handler from its `Tx`, and the first of each kind a view drops says
//! so on stderr, once, so an app ported from the desktop hears why nothing happened.

use crate::Shell;
use crate::app::WindowRequests;

/// A kind of request a view drops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `App::window`'s windows, in an embedded view.
    Windows,
    /// `rune::open_window`'s windows, in an embedded view.
    Opened,
    /// `AppHandle::open`, `show`, `activate`, `hide` and `close`, and `Tx::close_window`, in an
    /// embedded view.
    Window,
    /// `AppHandle::minimize` and `Tx::minimize`, and `AppHandle::focus` in an embedded view.
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
    pub(crate) const ALL: [Kind; 7] = [Kind::Windows, Kind::Opened, Kind::Window, Kind::Activation, Kind::Position, Kind::Dock, Kind::Quit];

    /// What stderr says of the first request of this kind, after `rune: `.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Kind::Windows => "App::window ignored: an embedded iOS view shows one app, so its other windows never open",
            Kind::Opened => "rune::open_window ignored: an embedded iOS view shows one app, so the window never opens",
            Kind::Window => "AppHandle's open, show, activate, hide and close and Tx::close_window ignored: an embedded iOS view is no window",
            Kind::Activation => "AppHandle's minimize and Tx::minimize ignored: an iOS window doesn't minimise, and an embedded view takes no focus requests",
            Kind::Position => "AppHandle's set_position and place ignored: iOS places its windows itself",
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

/// Each kind of request a handle makes, and whether `requests` holds one the view drops. Updates
/// and wakes are not among them: a view runs those.
pub(crate) fn asked(requests: &WindowRequests, managed: bool) -> [(Kind, bool); 5] {
    let focus = requests.activation == Some(crate::app::Activation::Focus);
    [
        (Kind::Window, !managed && requests.window.is_some()),
        (Kind::Activation, requests.activation.is_some() && !(managed && focus)),
        (Kind::Position, requests.position.is_some() || requests.place.is_some()),
        (Kind::Dock, requests.dock.is_some()),
        (Kind::Quit, requests.quit.is_some()),
    ]
}

/// Each kind of request handlers make of their window, and whether a frame left one the view
/// drops: the runtime's latched close, quit or relaunch, a minimize, and how many windows
/// `rune::open_window` queued.
pub(crate) fn asked_by_handlers(shell: Option<Shell>, minimize: bool, opened: usize, managed: bool) -> [(Kind, bool); 4] {
    [
        (Kind::Opened, opened > 0),
        (Kind::Window, !managed && shell == Some(Shell::Close)),
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
