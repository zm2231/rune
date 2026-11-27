use crate::machine::Effects;

/// Where pending `OverlayHandle::after_hidden` callbacks sit in the hide lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::overlay) enum Phase {
    /// No `after_hidden` callbacks are waiting.
    #[default]
    Idle,
    /// Callbacks are queued while the overlay is shown or fading out; waiting for it to hide.
    WaitingForHide,
    /// The overlay is hidden on Wayland and a `wl_display.sync` roundtrip is in flight.
    WaitingSync,
    /// The overlay hid on a non-Wayland host during the current turn; callbacks wait for the
    /// next event-loop turn.
    WaitingNextTurn,
    /// The next turn after a non-Wayland hide has begun; callbacks run at `about_to_wait`
    /// unless the overlay is shown first.
    ReadyToRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::overlay) enum Event {
    /// One or more callbacks arrived from `OverlayHandle::after_hidden`.
    Queued { hidden: bool, uses_sync: bool },
    /// The overlay became shown (`Presence::Shown`).
    Shown,
    /// The overlay finished hiding (`Presence::Hidden`, after native hide / surface destroy).
    /// `surface_destroyed` is true when a live Wayland surface was destroyed in this hide.
    Hidden { uses_sync: bool, surface_destroyed: bool },
    /// A new event-loop turn began (`AnyHost::new_events`).
    NewTurn,
    /// The in-flight Wayland `wl_display.sync` callback answered (`wl_callback.done`).
    #[cfg_attr(target_os = "ios", allow(dead_code))]
    SyncAnswered,
    /// Starting or polling the Wayland `wl_display.sync` failed (fatal display error).
    #[cfg_attr(target_os = "ios", allow(dead_code))]
    SyncFailed,
    /// The host reached `about_to_wait` while hidden (`Presence::Hidden`).
    AboutToWaitHidden,
    /// The overlay is closing, quitting, or entered terminal GPU loss: drop everything unrun.
    #[cfg_attr(target_os = "ios", allow(dead_code))]
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::overlay) enum Effect {
    /// Start (or replace) a Wayland `wl_display.sync` roundtrip on winit's display.
    StartSync,
    /// Drop any in-flight Wayland `wl_display.sync`.
    CancelSync,
    /// Wake the event loop so the next turn runs (`WaitingNextTurn`).
    Wake,
    /// Run all queued `after_hidden` callbacks once on the event thread.
    RunCallbacks,
    /// Drop all queued `after_hidden` callbacks unrun.
    DropCallbacks,
}

pub(in crate::overlay) fn step(phase: &mut Phase, event: Event) -> Effects<Effect> {
    match (*phase, event) {
        (Phase::Idle | Phase::WaitingForHide, Event::Queued { hidden: false, .. }) => {
            *phase = Phase::WaitingForHide;
            Effects::none()
        }
        (Phase::Idle | Phase::WaitingForHide, Event::Queued { hidden: true, uses_sync: true }) => {
            *phase = Phase::WaitingSync;
            Effects::one(Effect::StartSync)
        }
        (Phase::Idle | Phase::WaitingForHide, Event::Queued { hidden: true, uses_sync: false }) => {
            *phase = Phase::WaitingNextTurn;
            Effects::one(Effect::Wake)
        }
        (Phase::WaitingSync, Event::Queued { hidden: true, .. }) => Effects::none(),
        (Phase::WaitingSync, Event::Queued { hidden: false, .. }) => {
            *phase = Phase::WaitingForHide;
            Effects::one(Effect::CancelSync)
        }
        (Phase::WaitingNextTurn, Event::Queued { hidden: true, .. })
        | (Phase::ReadyToRun, Event::Queued { hidden: true, .. }) => Effects::none(),
        (Phase::WaitingNextTurn | Phase::ReadyToRun, Event::Queued { hidden: false, .. }) => {
            *phase = Phase::WaitingForHide;
            Effects::none()
        }

        (Phase::Idle, Event::Shown) => Effects::none(),
        (Phase::WaitingSync, Event::Shown) => {
            *phase = Phase::WaitingForHide;
            Effects::one(Effect::CancelSync)
        }
        (Phase::WaitingForHide | Phase::WaitingNextTurn | Phase::ReadyToRun, Event::Shown) => {
            *phase = Phase::WaitingForHide;
            Effects::none()
        }

        (Phase::Idle, Event::Hidden { .. }) => Effects::none(),
        (Phase::WaitingForHide, Event::Hidden { uses_sync: true, .. }) => {
            *phase = Phase::WaitingSync;
            Effects::one(Effect::StartSync)
        }
        (Phase::WaitingForHide, Event::Hidden { uses_sync: false, .. }) => {
            *phase = Phase::WaitingNextTurn;
            Effects::one(Effect::Wake)
        }
        (Phase::WaitingSync, Event::Hidden { uses_sync: true, surface_destroyed: true }) => {
            Effects::one(Effect::StartSync)
        }
        (Phase::WaitingSync, Event::Hidden { uses_sync: true, surface_destroyed: false }) => {
            Effects::none()
        }
        (Phase::WaitingSync, Event::Hidden { uses_sync: false, .. }) => {
            *phase = Phase::WaitingNextTurn;
            Effects::two(Effect::CancelSync, Effect::Wake)
        }
        (Phase::WaitingNextTurn, Event::Hidden { uses_sync: true, .. })
        | (Phase::ReadyToRun, Event::Hidden { uses_sync: true, .. }) => {
            *phase = Phase::WaitingSync;
            Effects::one(Effect::StartSync)
        }
        (Phase::WaitingNextTurn, Event::Hidden { uses_sync: false, .. }) => Effects::none(),
        (Phase::ReadyToRun, Event::Hidden { uses_sync: false, .. }) => {
            *phase = Phase::WaitingNextTurn;
            Effects::one(Effect::Wake)
        }

        (Phase::WaitingNextTurn, Event::NewTurn) => {
            *phase = Phase::ReadyToRun;
            Effects::none()
        }
        (_, Event::NewTurn) => Effects::none(),

        (Phase::WaitingSync, Event::SyncAnswered) => {
            *phase = Phase::Idle;
            Effects::one(Effect::RunCallbacks)
        }
        (_, Event::SyncAnswered) => Effects::none(),

        (Phase::WaitingSync, Event::SyncFailed) => {
            *phase = Phase::Idle;
            Effects::two(Effect::CancelSync, Effect::DropCallbacks)
        }
        (_, Event::SyncFailed) => Effects::none(),

        (Phase::ReadyToRun, Event::AboutToWaitHidden) => {
            *phase = Phase::Idle;
            Effects::one(Effect::RunCallbacks)
        }
        (_, Event::AboutToWaitHidden) => Effects::none(),

        (Phase::Idle, Event::Cancel) => Effects::none(),
        (Phase::WaitingSync, Event::Cancel) => {
            *phase = Phase::Idle;
            Effects::two(Effect::CancelSync, Effect::DropCallbacks)
        }
        (Phase::WaitingForHide | Phase::WaitingNextTurn | Phase::ReadyToRun, Event::Cancel) => {
            *phase = Phase::Idle;
            Effects::one(Effect::DropCallbacks)
        }
    }
}
