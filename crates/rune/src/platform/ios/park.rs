#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum State {
    #[default]
    Live,
    Parked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    MemoryWarning { on_screen: bool },
    Drawing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Trim,
    Release,
    Remake,
}

pub(crate) fn step(state: &mut State, event: Event) -> crate::machine::Effects<Effect> {
    use crate::machine::Effects;
    match (*state, event) {
        (State::Live, Event::MemoryWarning { on_screen: true }) => Effects::one(Effect::Trim),
        (State::Live, Event::MemoryWarning { on_screen: false }) => {
            *state = State::Parked;
            Effects::two(Effect::Trim, Effect::Release)
        }
        (State::Parked, Event::Drawing) => {
            *state = State::Live;
            Effects::one(Effect::Remake)
        }
        (State::Parked, Event::MemoryWarning { .. }) | (State::Live, Event::Drawing) => Effects::none(),
    }
}

#[cfg(test)]
#[path = "park_tests.rs"]
mod tests;
