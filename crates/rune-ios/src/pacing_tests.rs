use rune::machine::Effects;

use super::{Pacer, Pacing, PacingEffect, PacingEvent, step};

const PERIOD: f64 = 1.0 / 120.0;

fn framed(animating: bool, wake_after: Option<f64>, now: f64) -> PacingEvent {
    PacingEvent::Framed { animating, wake_after, now, period: PERIOD }
}

fn effects(e: Effects<PacingEffect>) -> Vec<PacingEffect> {
    e.into_iter().collect()
}

#[test]
fn an_idle_frame_pauses_and_a_wake_resumes() {
    let mut p = Pacer::default();
    assert_eq!(effects(step(&mut p, framed(false, None, 1.0))), [PacingEffect::Pause]);
    assert_eq!(p.state, Pacing::Paused);
    assert_eq!(effects(step(&mut p, PacingEvent::Wake)), [PacingEffect::Resume]);
    assert_eq!(p.state, Pacing::Running);
    assert!(step(&mut p, PacingEvent::Wake).is_empty());
}

#[test]
fn an_animating_frame_keeps_ticking() {
    let mut p = Pacer::default();
    assert!(step(&mut p, framed(true, None, 1.0)).is_empty());
    assert_eq!(p.state, Pacing::Running);
}

#[test]
fn a_wake_within_one_refresh_keeps_ticking() {
    let mut p = Pacer::default();
    assert!(step(&mut p, framed(false, Some(PERIOD / 2.0), 1.0)).is_empty());
    assert_eq!(p.state, Pacing::Running);
}

#[test]
fn a_later_wake_sleeps_until_its_timer() {
    let mut p = Pacer::default();
    let out = effects(step(&mut p, framed(false, Some(0.5), 2.0)));
    let Pacing::Sleeping { attempt, due } = p.state else { panic!("{:?}", p.state) };
    assert_eq!(due, 2.5);
    assert_eq!(out, [PacingEffect::Pause, PacingEffect::ArmTimer { attempt, after: 0.5 }]);
    assert_eq!(effects(step(&mut p, PacingEvent::Timer { attempt })), [PacingEffect::Resume]);
    assert_eq!(p.state, Pacing::Running);
}

#[test]
fn a_timer_overtaken_by_a_wake_changes_nothing() {
    let mut p = Pacer::default();
    step(&mut p, framed(false, Some(0.5), 0.0));
    let Pacing::Sleeping { attempt: stale, .. } = p.state else { panic!() };
    step(&mut p, PacingEvent::Wake);
    step(&mut p, framed(false, Some(1.0), 0.1));
    let Pacing::Sleeping { attempt: fresh, .. } = p.state else { panic!() };
    assert_ne!(stale, fresh);
    assert!(step(&mut p, PacingEvent::Timer { attempt: stale }).is_empty());
    assert_eq!(p.state, Pacing::Sleeping { attempt: fresh, due: 1.1 });
}

#[test]
fn a_paused_link_that_sleeps_again_does_not_pause_twice() {
    let mut p = Pacer::default();
    step(&mut p, framed(false, None, 0.0));
    let out = effects(step(&mut p, framed(false, Some(1.0), 0.0)));
    assert!(matches!(out.as_slice(), [PacingEffect::ArmTimer { .. }]), "{out:?}");
}

#[test]
fn the_background_holds_the_link_until_the_foreground() {
    let mut p = Pacer::default();
    assert_eq!(effects(step(&mut p, PacingEvent::Background)), [PacingEffect::Pause]);
    assert!(step(&mut p, PacingEvent::Wake).is_empty());
    assert!(step(&mut p, framed(true, None, 0.0)).is_empty());
    assert_eq!(p.state, Pacing::Background);
    assert_eq!(effects(step(&mut p, PacingEvent::Foreground)), [PacingEffect::Resume]);
    assert_eq!(p.state, Pacing::Running);
}

#[test]
fn a_timer_armed_before_the_background_does_not_wake_it() {
    let mut p = Pacer::default();
    step(&mut p, framed(false, Some(0.5), 0.0));
    let Pacing::Sleeping { attempt, .. } = p.state else { panic!() };
    assert!(step(&mut p, PacingEvent::Background).is_empty());
    assert!(step(&mut p, PacingEvent::Timer { attempt }).is_empty());
    assert_eq!(p.state, Pacing::Background);
}
