const SETTLED_READINGS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Waiting,
    Steady { lead: i64 },
    Refilling { lead: i64, settled: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) lead: i64,
    pub(crate) low: bool,
}

pub(crate) fn step(state: &mut State, reading: Reading) -> u64 {
    let Reading { lead: now, low } = reading;
    let (next, silence) = match *state {
        State::Waiting => (State::Steady { lead: now }, 0),
        State::Steady { lead } if low => (State::Refilling { lead: lead.max(now), settled: 0 }, now - lead),
        State::Steady { .. } => (State::Steady { lead: now }, 0),
        State::Refilling { lead, settled } => {
            let settled = if low { 0 } else { settled + 1 };
            let grown = now - lead;
            let lead = lead.max(now);
            let next = if settled >= SETTLED_READINGS { State::Steady { lead } } else { State::Refilling { lead, settled } };
            (next, grown)
        }
    };
    *state = next;
    u64::try_from(silence).unwrap_or(0)
}

#[cfg(test)]
#[path = "underrun_tests.rs"]
mod tests;
