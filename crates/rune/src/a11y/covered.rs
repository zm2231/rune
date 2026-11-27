use crate::machine::Effects;

pub(crate) const COVERED_BUILD_INTERVAL: f64 = 0.25;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum CoveredBuild {
    #[default]
    Ready,
    Built { at: f64 },
    Owed { due: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CoveredBuildEvent {
    Wake { now: f64, stale: bool, urgent: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CoveredBuildEffect {
    Build,
    WakeAt(f64),
}

pub(crate) fn step(state: &mut CoveredBuild, event: CoveredBuildEvent) -> Effects<CoveredBuildEffect> {
    use {CoveredBuild::*, CoveredBuildEffect as E};
    let CoveredBuildEvent::Wake { now, stale, urgent } = event;
    match *state {
        Owed { due } if !stale => {
            *state = Built { at: due - COVERED_BUILD_INTERVAL };
            Effects::none()
        }
        _ if !stale => Effects::none(),
        Built { at } if !urgent && now < at + COVERED_BUILD_INTERVAL => {
            let due = at + COVERED_BUILD_INTERVAL;
            *state = Owed { due };
            Effects::one(E::WakeAt(due))
        }
        Owed { due } if !urgent && now < due => Effects::none(),
        Ready | Built { .. } | Owed { .. } => {
            *state = Built { at: now };
            Effects::one(E::Build)
        }
    }
}

impl super::tree::A11yTree {
    pub(crate) fn covered_step(&mut self, event: CoveredBuildEvent) -> Effects<CoveredBuildEffect> {
        step(&mut self.covered, event)
    }
}
