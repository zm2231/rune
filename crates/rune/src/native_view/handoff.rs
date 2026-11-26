use crate::machine::Effects;

use super::NativeViewId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Keyboard {
    #[default]
    Rune,
    Hosted(NativeViewId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    TabInto { view: NativeViewId, back: bool },
    Pressed(NativeViewId),
    TabbedOut { view: NativeViewId, back: bool },
    Refused(NativeViewId),
    RuneFocused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Enter { view: NativeViewId, back: bool },
    Blur,
    Continue { from: NativeViewId, back: bool },
    Reclaim(NativeViewId),
}

pub(crate) fn step(state: &mut Keyboard, event: Event) -> Effects<Effect> {
    match event {
        Event::TabInto { view, back } => {
            *state = Keyboard::Hosted(view);
            Effects::one(Effect::Enter { view, back })
        }
        Event::Pressed(view) => {
            *state = Keyboard::Hosted(view);
            Effects::one(Effect::Blur)
        }
        Event::TabbedOut { view, back } => {
            *state = Keyboard::Hosted(view);
            Effects::one(Effect::Continue { from: view, back })
        }
        Event::Refused(view) => {
            *state = Keyboard::Rune;
            Effects::one(Effect::Reclaim(view))
        }
        Event::RuneFocused => match std::mem::take(state) {
            Keyboard::Hosted(view) => Effects::one(Effect::Reclaim(view)),
            Keyboard::Rune => Effects::none(),
        },
    }
}
