//! What an iOS app's lifecycle hooks hear. A `RuneView` has no window of its own: what a
//! desktop window's occlusion and focus tell an app, the view's scene tells it here.
//!
//! | The system says | The app hears |
//! | --- | --- |
//! | the scene enters the foreground, or the view joins a window in front | `App::on_visibility(true)` |
//! | the scene enters the background, or the view leaves its window | `App::on_visibility(false)`, then the view rests and `App::on_rest` runs |
//! | the scene becomes active | `App::on_focus(true)` |
//! | the scene resigns active (Control Center, the app switcher, a call) | `App::on_focus(false)`, and what was held is released |
//! | the app will terminate | `App::on_focus(false)` and `App::on_visibility(false)` if it had not heard them, then `App::on_quit`, once; no rest |
//!
//! `App::on_close` and `App::on_reopen` are never called: a view is no window to close, and
//! iOS reopens no running app.
//!
//! A hook hears each value once. Coming forward the app is visible before it has the focus,
//! and going away it loses the focus before it hides, as UIKit orders its notifications; a
//! notification that arrives without the one before it brings that one's hook with it. A view
//! that joins a window hears where its scene is already: nothing in the background, visible in
//! the foreground, focused when active. At launch the view joins its window while the scene
//! is still connecting, which counts as the background, and the scene's first notifications
//! bring it forward.
//!
//! A view that goes out of sight rests as a window does that is hidden (`window::rest`): Rune's
//! caches go back to what the screen on show uses, `App::on_rest` runs, and the renderer gives
//! back everything it keeps between frames, which the first frame back in front makes again.
//! It rests once each time it hides, after `on_visibility(false)`, inside the notification:
//! an app in the background draws no frame to do it later, and may be suspended at any moment
//! after. The rest only drops what is held. It encodes and submits nothing, since the GPU
//! takes no work from an app in the background. An app that terminates does not rest: its
//! memory goes with it, and the time it has left is `on_quit`'s. A view on screen never rests
//! on a timer, as a desktop window does after 10 s without a frame.
//!
//! Input reaches only an active scene, so what was held is released once, whenever the scene
//! stops being active, by any way out: resigning, the background, the view leaving its window,
//! termination. Losing the focus does it, in a window's order (`Runtime::scene_focus`): a held
//! drag hears its cancelled `Phase::Up`, then `PhysicalInput::FocusLost` releases the keys, and
//! the view's key repeat stops. The system sends no key-up to an app it puts away. A
//! composition the keyboard has open stays open, as in a UIKit field.
//!
//! iOS sends the termination notice rarely: to an app that is running, in front or in the
//! background, when the system or the user ends it, and never to a suspended one. An app that
//! was in front first loses the focus and hides, so `on_quit` runs in an app that has heard
//! both. A desktop app that quits hears neither first: this order is iOS's own, so that what
//! was held is released before the app ends. `on_quit` is a last chance there, not the place
//! to save: save when `on_visibility` says false.

use crate::machine::Effects;

/// Where a scene, or an app without scenes, is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Off screen.
    Background,
    /// On screen without the user's input: launching, under Control Center or a system alert,
    /// in the app switcher.
    Inactive,
    /// On screen and taking input.
    Active,
}

impl Phase {
    /// The phase a `UISceneActivationState` names, or for a window with no scene the one the
    /// app's `UIApplicationState` names. Both count active as 0, inactive as 1 and background
    /// as 2. A scene still unattached (-1), as an app's is while it connects at launch, is in
    /// the background: its own notification brings it forward.
    pub(crate) fn from_raw(scene: Option<isize>, app: isize) -> Phase {
        match scene.unwrap_or(app) {
            0 => Phase::Active,
            1 => Phase::Inactive,
            _ => Phase::Background,
        }
    }
}

/// What the app was last told, as where its view is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum State {
    /// The view is in no window: hidden, without the focus.
    #[default]
    Detached,
    /// In a window of a scene in the background: hidden, without the focus.
    Background,
    /// On screen, without the focus.
    Inactive,
    /// On screen with the focus.
    Active,
    /// The app is ending: the hooks hear nothing more.
    Terminated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The view joined a window whose scene is in this phase.
    Attached(Phase),
    /// The view left its window.
    Detached,
    /// The scene is coming to the foreground.
    Foreground,
    /// The scene went to the background.
    Background,
    /// The scene became active.
    Active,
    /// The scene is about to stop being active.
    Inactive,
    /// The app is about to terminate.
    Terminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// `App::on_visibility`, and `AppHandle::is_visible`.
    Visibility(bool),
    /// `App::on_focus`. Losing it releases what was held: the pointer's drag is cancelled,
    /// then every key and modifier is up (`PhysicalInput::FocusLost`), and the view's key
    /// repeat stops.
    Focus(bool),
    /// The view went out of sight: Rune's caches and the renderer give back what the screen
    /// on show doesn't need, and `App::on_rest` runs. Always just after `Visibility(false)`,
    /// so once for each time the view hides.
    Rest,
    /// `App::on_quit`.
    Quit,
}

impl State {
    fn visible(self) -> bool { matches!(self, State::Inactive | State::Active) }

    fn focused(self) -> bool { self == State::Active }
}

/// Moves to `to`, with what changes for the app in UIKit's order: visible before focused on
/// the way in, the focus lost before hidden on the way out. A view that hides rests, unless
/// its app is ending.
fn go(state: &mut State, to: State) -> Effects<Effect> {
    let from = std::mem::replace(state, to);
    let mut effects = Effects::none();
    if from.focused() && !to.focused() { effects.push(Effect::Focus(false)); }
    if from.visible() != to.visible() {
        effects.push(Effect::Visibility(to.visible()));
        if !to.visible() && to != State::Terminated { effects.push(Effect::Rest); }
    }
    if !from.focused() && to.focused() { effects.push(Effect::Focus(true)); }
    effects
}

pub(crate) fn step(state: &mut State, event: Event) -> Effects<Effect> {
    match (*state, event) {
        (State::Terminated, _) => Effects::none(),
        // The app leaves the screen for good: it loses the focus and hides as a view leaving
        // its window does, without the rest, and then quits.
        (_, Event::Terminate) => {
            let mut effects = go(state, State::Terminated);
            effects.push(Effect::Quit);
            effects
        }
        // A view that moves from one window to another hears where the new one's scene is.
        (_, Event::Attached(Phase::Background)) => go(state, State::Background),
        (_, Event::Attached(Phase::Inactive)) => go(state, State::Inactive),
        (_, Event::Attached(Phase::Active)) => go(state, State::Active),
        (_, Event::Detached) => go(state, State::Detached),
        // A view in no window hears no scene: a notification that still arrives is not its own.
        (State::Detached, _) => Effects::none(),
        (State::Background, Event::Foreground) => go(state, State::Inactive),
        (_, Event::Foreground) => Effects::none(),
        (_, Event::Background) => go(state, State::Background),
        (_, Event::Active) => go(state, State::Active),
        // A scene in the background is inactive already.
        (State::Active, Event::Inactive) => go(state, State::Inactive),
        (_, Event::Inactive) => Effects::none(),
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
