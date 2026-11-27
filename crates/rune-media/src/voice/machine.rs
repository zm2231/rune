use super::{VoiceEvent, VoiceState};
use rune_core::machine::{Attempt, Effects, Seq};
use rune_core::time::Instant;
use std::time::Duration;

pub(crate) const SETTLE: Duration = Duration::from_millis(150);
pub(crate) const MAX_TRIES: u32 = 8;
const FIRST_RETRY: Duration = Duration::from_millis(100);
const LAST_RETRY: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Start,
    Device,
    Resume,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Idle,
    Starting { tries: u32, cause: Cause },
    Running,
    Reconnecting { attempt: Attempt, due: Instant, tries: u32, cause: Cause },
    Interrupted { resume: bool },
    Failed(String),
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    pub(crate) message: String,
    pub(crate) lasting: bool,
}

impl Fault {
    pub(crate) fn passing(message: impl Into<String>) -> Self {
        Self { message: message.into(), lasting: false }
    }

    pub(crate) fn lasting(message: impl Into<String>) -> Self {
        Self { message: message.into(), lasting: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Start,
    Stop,
    Mute(bool),
    DevicesChanged { now: Instant },
    InterruptionBegan,
    InterruptionEnded { should_resume: bool },
    MediaReset { now: Instant },
    Lost { fault: Fault, now: Instant },
    Backgrounded,
    Foregrounded,
    Ran { result: Result<(), Fault>, now: Instant },
    RetryDue { attempt: Attempt, now: Instant },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Run,
    Halt,
    Discard,
    Mute(bool),
    WakeAt { attempt: Attempt, due: Instant },
    Tell(VoiceEvent),
}

impl State {
    pub(crate) fn public(&self) -> VoiceState {
        match self {
            Self::Idle | Self::Closed => VoiceState::Idle,
            Self::Starting { .. } => VoiceState::Starting,
            Self::Running => VoiceState::Running,
            Self::Reconnecting { .. } => VoiceState::Reconnecting,
            Self::Interrupted { .. } => VoiceState::Interrupted,
            Self::Failed(message) => VoiceState::Failed(message.clone()),
        }
    }
}

fn backoff(tries: u32) -> Duration {
    FIRST_RETRY.saturating_mul(1 << tries.min(5)).min(LAST_RETRY)
}

fn reconnect(seq: &mut Seq, state: &mut State, due: Instant, tries: u32, cause: Cause, first: Effect) -> Effects<Effect> {
    let attempt = seq.next();
    *state = State::Reconnecting { attempt, due, tries, cause };
    Effects::Two(first, Effect::WakeAt { attempt, due })
}

pub(crate) fn step(seq: &mut Seq, state: &mut State, event: Event) -> Effects<Effect> {
    use Effect::{Discard, Halt, Run, Tell};
    match (&*state, event) {
        (State::Closed, _) => Effects::none(),
        (_, Event::Close) => {
            *state = State::Closed;
            Effects::Two(Halt, Discard)
        }
        (_, Event::Mute(muted)) => Effects::one(Effect::Mute(muted)),

        (State::Idle, Event::Start) => {
            *state = State::Starting { tries: 0, cause: Cause::Start };
            Effects::one(Run)
        }
        (State::Failed(_), Event::Start) => {
            *state = State::Starting { tries: 0, cause: Cause::Start };
            Effects::Two(Discard, Run)
        }
        (State::Interrupted { .. }, Event::Start) => {
            *state = State::Interrupted { resume: true };
            Effects::none()
        }
        (_, Event::Start) => Effects::none(),

        (State::Running, Event::Stop) => {
            *state = State::Idle;
            Effects::Two(Halt, Tell(VoiceEvent::Stopped))
        }
        (State::Idle, Event::Stop) => Effects::none(),
        (_, Event::Stop) => {
            *state = State::Idle;
            Effects::one(Halt)
        }

        (State::Starting { tries, cause }, Event::Ran { result, now }) => match result {
            Ok(()) => {
                let told = match cause {
                    Cause::Start => VoiceEvent::Started,
                    Cause::Device => VoiceEvent::DeviceChanged,
                    Cause::Resume => VoiceEvent::Resumed,
                };
                *state = State::Running;
                Effects::one(Tell(told))
            }
            Err(fault) if fault.lasting || tries + 1 >= MAX_TRIES => {
                *state = State::Failed(fault.message.clone());
                Effects::Two(Discard, Tell(VoiceEvent::Failed(fault.message)))
            }
            Err(_) => {
                let (tries, cause) = (*tries + 1, *cause);
                reconnect(seq, state, now + backoff(tries - 1), tries, cause, Discard)
            }
        },
        (_, Event::Ran { .. }) => Effects::none(),

        (State::Reconnecting { attempt, due, tries, cause }, Event::RetryDue { attempt: a, now })
            if *attempt == a && now >= *due =>
        {
            *state = State::Starting { tries: *tries, cause: *cause };
            Effects::one(Run)
        }
        (_, Event::RetryDue { .. }) => Effects::none(),

        (State::Running, Event::DevicesChanged { now } | Event::MediaReset { now }) => {
            reconnect(seq, state, now + SETTLE, 0, Cause::Device, Discard)
        }
        (State::Reconnecting { cause, .. }, Event::DevicesChanged { now } | Event::MediaReset { now }) => {
            let cause = *cause;
            reconnect(seq, state, now + SETTLE, 0, cause, Discard)
        }
        (State::Failed(_), Event::DevicesChanged { now } | Event::MediaReset { now }) => {
            reconnect(seq, state, now + SETTLE, 0, Cause::Device, Discard)
        }
        (_, Event::DevicesChanged { .. } | Event::MediaReset { .. }) => Effects::one(Discard),

        (State::Running, Event::Lost { fault, .. }) if fault.lasting => {
            *state = State::Failed(fault.message.clone());
            Effects::Two(Discard, Tell(VoiceEvent::Failed(fault.message)))
        }
        (State::Running, Event::Lost { now, .. }) => reconnect(seq, state, now + SETTLE, 0, Cause::Device, Discard),
        (_, Event::Lost { .. }) => Effects::none(),

        (State::Running | State::Starting { .. } | State::Reconnecting { .. }, Event::InterruptionBegan) => {
            *state = State::Interrupted { resume: true };
            Effects::Two(Halt, Tell(VoiceEvent::Interrupted))
        }
        (_, Event::InterruptionBegan) => Effects::none(),

        (State::Interrupted { resume: true }, Event::InterruptionEnded { should_resume: true } | Event::Foregrounded) => {
            *state = State::Starting { tries: 0, cause: Cause::Resume };
            Effects::one(Run)
        }
        (State::Interrupted { resume: false }, Event::InterruptionEnded { .. }) => {
            *state = State::Idle;
            Effects::none()
        }
        (_, Event::InterruptionEnded { .. } | Event::Backgrounded | Event::Foregrounded) => Effects::none(),
    }
}
