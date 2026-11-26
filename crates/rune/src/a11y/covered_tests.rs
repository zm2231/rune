use super::covered::{COVERED_BUILD_INTERVAL as I, CoveredBuild, CoveredBuildEffect as E, CoveredBuildEvent, step};

fn wake(state: &mut CoveredBuild, now: f64, stale: bool, urgent: bool) -> Vec<E> {
    step(state, CoveredBuildEvent::Wake { now, stale, urgent }).into_iter().collect()
}

#[test]
fn a_stream_of_changes_builds_once_per_interval_and_its_last_change_lands() {
    let mut state = CoveredBuild::default();
    let mut builds = Vec::new();
    let mut wakes = Vec::new();
    let mut now = 0.0;
    let mut built_through = -1.0;
    let mut changed_at = Vec::new();
    while now < 1.0 {
        changed_at.push(now);
        for effect in wake(&mut state, now, true, false) {
            match effect {
                E::Build => { builds.push(now); built_through = now; }
                E::WakeAt(due) => wakes.push(due),
            }
        }
        wakes.retain(|&due| {
            if due > now + 0.02 { return true; }
            if wake(&mut state, due, true, false) == [E::Build] { builds.push(due); built_through = due; }
            false
        });
        now += 0.02;
    }
    for due in std::mem::take(&mut wakes) {
        if wake(&mut state, due, true, false) == [E::Build] { builds.push(due); built_through = due; }
    }
    assert!(builds.windows(2).all(|w| w[1] - w[0] >= I - 1e-9), "at most one build per interval: {builds:?}");
    assert!(built_through >= *changed_at.last().unwrap(), "the last change builds: {builds:?}");
    for at in changed_at {
        assert!(builds.iter().any(|&b| b >= at && b <= at + I + 1e-9), "a change at {at} builds within the interval: {builds:?}");
    }
}

#[test]
fn a_change_after_a_quiet_interval_builds_at_once() {
    let mut state = CoveredBuild::default();
    assert_eq!(wake(&mut state, 0.0, true, false), [E::Build]);
    assert_eq!(wake(&mut state, 0.1, false, false), []);
    assert_eq!(wake(&mut state, 0.3, true, false), [E::Build]);
}

#[test]
fn an_owed_build_that_something_else_made_is_dropped_and_still_throttles() {
    let mut state = CoveredBuild::default();
    assert_eq!(wake(&mut state, 0.0, true, false), [E::Build]);
    assert_eq!(wake(&mut state, 0.1, true, false), [E::WakeAt(I)]);
    assert_eq!(wake(&mut state, 0.15, false, false), [], "an assistive action built it");
    assert_eq!(wake(&mut state, 0.2, true, false), [E::WakeAt(I)], "a change before the interval ends still waits");
    assert_eq!(wake(&mut state, I, true, false), [E::Build]);
}

#[test]
fn activation_or_focus_builds_at_once_inside_the_interval() {
    let mut state = CoveredBuild::default();
    assert_eq!(wake(&mut state, 0.0, true, false), [E::Build]);
    assert_eq!(wake(&mut state, 0.05, true, true), [E::Build]);
    assert_eq!(wake(&mut state, 0.1, true, false), [E::WakeAt(0.05 + I)]);
    assert_eq!(wake(&mut state, 0.12, true, true), [E::Build]);
    assert_eq!(wake(&mut state, 0.12, false, true), [], "nothing to build");
}
