use super::VoiceEvent;
use super::machine::{Cause, Effect, Event, Fault, MAX_TRIES, SETTLE, State, step};
use rune_core::machine::{Attempt, Effects, Seq};
use rune_core::time::Instant;
use std::time::Duration;

struct Rig {
    seq: Seq,
    state: State,
    t0: Instant,
}

impl Rig {
    fn new() -> Self {
        Self { seq: Seq::default(), state: State::Idle, t0: rune_core::time::now() }
    }

    fn at(&self, ms: u64) -> Instant {
        self.t0 + Duration::from_millis(ms)
    }

    fn step(&mut self, event: Event) -> Vec<Effect> {
        step(&mut self.seq, &mut self.state, event).into_iter().collect()
    }

    fn ran(&mut self, ms: u64, result: Result<(), Fault>) -> Vec<Effect> {
        let now = self.at(ms);
        self.step(Event::Ran { result, now })
    }

    fn running(mut self) -> Self {
        assert_eq!(self.step(Event::Start), vec![Effect::Run]);
        assert_eq!(self.ran(0, Ok(())), vec![Effect::Tell(VoiceEvent::Started)]);
        self
    }

    fn wake(&self) -> (Attempt, Instant) {
        match self.state {
            State::Reconnecting { attempt, due, .. } => (attempt, due),
            ref other => panic!("not reconnecting: {other:?}"),
        }
    }
}

#[test]
fn start_runs_the_device_and_tells_started() {
    let rig = Rig::new().running();
    assert_eq!(rig.state, State::Running);
}

#[test]
fn a_passing_fault_retries_after_a_backoff_and_ignores_stale_timers() {
    let mut rig = Rig::new();
    rig.step(Event::Start);
    let effects = rig.ran(0, Err(Fault::passing("busy")));
    let (attempt, due) = rig.wake();
    assert_eq!(effects, vec![Effect::Discard, Effect::WakeAt { attempt, due }]);
    assert_eq!(due, rig.at(100));

    let early = rig.at(50);
    assert!(rig.step(Event::RetryDue { attempt, now: early }).is_empty());
    let stale = Attempt(attempt.0.wrapping_sub(1));
    assert!(rig.step(Event::RetryDue { attempt: stale, now: due }).is_empty());
    assert_eq!(rig.step(Event::RetryDue { attempt, now: due }), vec![Effect::Run]);
    assert_eq!(rig.state, State::Starting { tries: 1, cause: Cause::Start });

    rig.ran(100, Err(Fault::passing("busy")));
    let (_, second) = rig.wake();
    assert_eq!(second, rig.at(300));
}

#[test]
fn a_lasting_fault_fails_at_once() {
    let mut rig = Rig::new();
    rig.step(Event::Start);
    let effects = rig.ran(0, Err(Fault::lasting("no permission")));
    assert_eq!(effects, vec![Effect::Discard, Effect::Tell(VoiceEvent::Failed("no permission".into()))]);
    assert_eq!(rig.state, State::Failed("no permission".into()));
    assert_eq!(rig.step(Event::Start), vec![Effect::Discard, Effect::Run]);
}

#[test]
fn retries_give_up_after_the_last_try() {
    let mut rig = Rig::new();
    rig.step(Event::Start);
    let mut now = 0;
    for _ in 1..MAX_TRIES {
        rig.ran(now, Err(Fault::passing("busy")));
        let (attempt, due) = rig.wake();
        now = due.duration_since(rig.t0).as_millis() as u64;
        assert_eq!(rig.step(Event::RetryDue { attempt, now: due }), vec![Effect::Run]);
    }
    rig.ran(now, Err(Fault::passing("busy")));
    assert_eq!(rig.state, State::Failed("busy".into()));
}

#[test]
fn device_changes_settle_into_one_rebuild_and_tell_device_changed() {
    let mut rig = Rig::new().running();
    let first = rig.step(Event::DevicesChanged { now: rig.at(10) });
    let (stale, _) = rig.wake();
    assert_eq!(first[0], Effect::Discard);
    let second = rig.step(Event::DevicesChanged { now: rig.at(40) });
    let (attempt, due) = rig.wake();
    assert_eq!(second, vec![Effect::Discard, Effect::WakeAt { attempt, due }]);
    assert_eq!(due, rig.at(40) + SETTLE);

    assert!(rig.step(Event::RetryDue { attempt: stale, now: due }).is_empty());
    assert_eq!(rig.step(Event::RetryDue { attempt, now: due }), vec![Effect::Run]);
    assert_eq!(rig.ran(200, Ok(())), vec![Effect::Tell(VoiceEvent::DeviceChanged)]);
    assert_eq!(rig.state, State::Running);
}

#[test]
fn media_reset_while_running_rebuilds_like_a_device_change() {
    let mut rig = Rig::new().running();
    let effects = rig.step(Event::MediaReset { now: rig.at(0) });
    assert_eq!(effects[0], Effect::Discard);
    assert!(matches!(rig.state, State::Reconnecting { cause: Cause::Device, .. }));
}

#[test]
fn a_device_change_while_idle_only_drops_the_unit() {
    let mut rig = Rig::new();
    assert_eq!(rig.step(Event::DevicesChanged { now: rig.at(0) }), vec![Effect::Discard]);
    assert_eq!(rig.state, State::Idle);
}

#[test]
fn an_interruption_pauses_and_resumes_when_the_system_says_so() {
    let mut rig = Rig::new().running();
    assert_eq!(rig.step(Event::InterruptionBegan), vec![Effect::Halt, Effect::Tell(VoiceEvent::Interrupted)]);
    assert_eq!(rig.state, State::Interrupted { resume: true });
    assert_eq!(rig.step(Event::InterruptionEnded { should_resume: true }), vec![Effect::Run]);
    assert_eq!(rig.ran(0, Ok(())), vec![Effect::Tell(VoiceEvent::Resumed)]);
    assert_eq!(rig.state, State::Running);
}

#[test]
fn an_interruption_without_a_resume_hint_waits_for_the_foreground() {
    let mut rig = Rig::new().running();
    rig.step(Event::InterruptionBegan);
    assert!(rig.step(Event::InterruptionEnded { should_resume: false }).is_empty());
    assert_eq!(rig.state, State::Interrupted { resume: true });
    assert_eq!(rig.step(Event::Foregrounded), vec![Effect::Run]);
}

#[test]
fn stopping_during_an_interruption_stays_stopped_after_it() {
    let mut rig = Rig::new().running();
    rig.step(Event::InterruptionBegan);
    assert_eq!(rig.step(Event::Stop), vec![Effect::Halt]);
    assert!(rig.step(Event::InterruptionEnded { should_resume: true }).is_empty());
    assert_eq!(rig.state, State::Idle);
}

#[test]
fn starting_during_an_interruption_starts_when_it_ends() {
    let mut rig = Rig::new();
    rig.state = State::Interrupted { resume: false };
    assert!(rig.step(Event::Start).is_empty());
    assert_eq!(rig.state, State::Interrupted { resume: true });
    assert_eq!(rig.step(Event::InterruptionEnded { should_resume: true }), vec![Effect::Run]);
}

#[test]
fn going_to_the_background_keeps_a_call_running() {
    let mut rig = Rig::new().running();
    assert!(rig.step(Event::Backgrounded).is_empty());
    assert_eq!(rig.state, State::Running);
    assert!(rig.step(Event::Foregrounded).is_empty());
    assert_eq!(rig.state, State::Running);
}

#[test]
fn mute_applies_in_every_state_without_changing_it() {
    for state in [State::Idle, State::Running, State::Interrupted { resume: true }, State::Failed("x".into())] {
        let mut rig = Rig::new();
        rig.state = state.clone();
        assert_eq!(rig.step(Event::Mute(true)), vec![Effect::Mute(true)]);
        assert_eq!(rig.state, state);
    }
}

#[test]
fn stop_halts_and_tells_stopped() {
    let mut rig = Rig::new().running();
    assert_eq!(rig.step(Event::Stop), vec![Effect::Halt, Effect::Tell(VoiceEvent::Stopped)]);
    assert_eq!(rig.state, State::Idle);
    assert!(rig.step(Event::Stop).is_empty());
}

#[test]
fn close_releases_the_device_and_ignores_everything_after() {
    let mut rig = Rig::new().running();
    assert_eq!(rig.step(Event::Close), vec![Effect::Halt, Effect::Discard]);
    assert_eq!(rig.state, State::Closed);
    assert!(rig.step(Event::Start).is_empty());
    assert!(rig.step(Event::Mute(true)).is_empty());
    assert_eq!(Effects::<Effect>::none(), step(&mut rig.seq, &mut rig.state, Event::DevicesChanged { now: rig.t0 }));
}

#[test]
fn a_failed_stream_comes_back_after_a_device_change_or_media_reset() {
    for event in [Event::DevicesChanged { now: rune_core::time::now() }, Event::MediaReset { now: rune_core::time::now() }] {
        let mut rig = Rig::new();
        rig.step(Event::Start);
        rig.ran(0, Err(Fault::lasting("no device")));
        let effects = rig.step(event);
        let (attempt, due) = rig.wake();
        assert_eq!(effects, vec![Effect::Discard, Effect::WakeAt { attempt, due }]);
        assert_eq!(rig.step(Event::RetryDue { attempt, now: due }), vec![Effect::Run]);
        assert_eq!(rig.ran(0, Ok(())), vec![Effect::Tell(VoiceEvent::DeviceChanged)]);
        assert_eq!(rig.state, State::Running);
    }
}

#[test]
fn a_stream_lost_while_running_rebuilds_after_settling() {
    let mut rig = Rig::new().running();
    let effects = rig.step(Event::Lost { fault: Fault::passing("server restarted"), now: rig.at(5) });
    let (attempt, due) = rig.wake();
    assert_eq!(effects, vec![Effect::Discard, Effect::WakeAt { attempt, due }]);
    assert_eq!(due, rig.at(5) + SETTLE);
    assert_eq!(rig.step(Event::RetryDue { attempt, now: due }), vec![Effect::Run]);
    assert_eq!(rig.ran(200, Ok(())), vec![Effect::Tell(VoiceEvent::DeviceChanged)]);
}

#[test]
fn a_lasting_loss_while_running_fails_and_start_tries_again() {
    let mut rig = Rig::new().running();
    let effects = rig.step(Event::Lost { fault: Fault::lasting("microphone blocked"), now: rig.at(5) });
    assert_eq!(effects, vec![Effect::Discard, Effect::Tell(VoiceEvent::Failed("microphone blocked".into()))]);
    assert_eq!(rig.state, State::Failed("microphone blocked".into()));
    assert_eq!(rig.step(Event::Start), vec![Effect::Discard, Effect::Run]);
}

#[test]
fn a_loss_reported_after_stopping_changes_nothing() {
    let mut rig = Rig::new().running();
    rig.step(Event::Stop);
    assert!(rig.step(Event::Lost { fault: Fault::lasting("late"), now: rig.at(5) }).is_empty());
    assert_eq!(rig.state, State::Idle);
}
