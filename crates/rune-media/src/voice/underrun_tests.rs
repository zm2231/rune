use super::{Reading, State, step};

fn feed(state: &mut State, readings: &[(i64, bool)]) -> u64 {
    readings.iter().map(|&(lead, low)| step(state, Reading { lead, low })).sum()
}

#[test]
fn clock_jitter_while_the_buffer_is_full_is_never_silence() {
    let mut state = State::Waiting;
    assert_eq!(feed(&mut state, &[(-1_920, false), (-1_900, false), (-1_950, false), (-1_870, false), (-1_920, false)]), 0);
}

#[test]
fn an_underrun_counts_the_silence_even_when_it_lands_across_readings() {
    let mut state = State::Waiting;
    feed(&mut state, &[(-1_920, false), (-1_900, false)]);
    assert_eq!(feed(&mut state, &[(-1_420, true), (-1_180, false), (-1_180, false)]), 720);
}

#[test]
fn after_the_buffer_refills_jitter_is_not_counted_again() {
    let mut state = State::Waiting;
    feed(&mut state, &[(-1_900, false), (-1_420, true), (-1_420, false), (-1_420, false), (-1_420, false)]);
    assert_eq!(feed(&mut state, &[(-1_380, false), (-1_300, false), (-1_440, false)]), 0);
}

#[test]
fn a_buffer_that_runs_low_without_starving_counts_nothing() {
    let mut state = State::Waiting;
    feed(&mut state, &[(-1_900, false), (-1_880, false)]);
    assert_eq!(feed(&mut state, &[(-1_890, true), (-1_885, false), (-1_900, false), (-1_880, false)]), 0);
}
