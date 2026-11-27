use super::VoiceEvent;
use super::io::Meters;
use super::machine::{Effect, Event, Fault, Opened, State, step};
use crate::video::WakerSlot;
use rune_core::machine::{Attempt, Seq};
use rune_core::time::Instant;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};

const MAX_EVENTS: usize = 64;

pub(crate) trait VoiceDevice {
    fn prepare(&mut self) -> Result<(), Fault>;
    fn run(&mut self) -> Result<(), Fault>;
    fn runs_from_first_audio(&self) -> bool {
        false
    }
    fn halt(&mut self);
    fn discard(&mut self);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    Start,
    Stop,
    Mute(bool),
    DevicesChanged,
    InterruptionBegan,
    InterruptionEnded { should_resume: bool },
    MediaReset,
    #[cfg_attr(any(target_os = "macos", target_os = "ios"), allow(dead_code))]
    Lost(Fault),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    AudioArrived,
    Backgrounded,
    Foregrounded,
    Close,
}

impl Signal {
    fn event(self, now: Instant) -> Event {
        match self {
            Self::Start => Event::Start,
            Self::Stop => Event::Stop,
            Self::Mute(muted) => Event::Mute(muted),
            Self::DevicesChanged => Event::DevicesChanged { now },
            Self::InterruptionBegan => Event::InterruptionBegan,
            Self::InterruptionEnded { should_resume } => Event::InterruptionEnded { should_resume },
            Self::MediaReset => Event::MediaReset { now },
            Self::Lost(fault) => Event::Lost { fault, now },
            Self::AudioArrived => Event::AudioArrived,
            Self::Backgrounded => Event::Backgrounded,
            Self::Foregrounded => Event::Foregrounded,
            Self::Close => Event::Close,
        }
    }
}

pub(crate) struct Published {
    state: State,
    events: VecDeque<VoiceEvent>,
    generation: u64,
}

pub(crate) struct Board {
    published: Mutex<Published>,
    pub(crate) waker: WakerSlot,
}

impl Board {
    pub(crate) fn new() -> Self {
        Self {
            published: Mutex::new(Published { state: State::Idle, events: VecDeque::new(), generation: 0 }),
            waker: WakerSlot::default(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Published> {
        self.published.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn state(&self) -> super::VoiceState {
        self.lock().state.public()
    }

    pub(crate) fn take_events(&self) -> Vec<VoiceEvent> {
        self.lock().events.drain(..).collect()
    }

    pub(crate) fn pending(&self) -> Option<u64> {
        let g = self.lock();
        (!g.events.is_empty()).then_some(g.generation)
    }

    fn publish(&self, state: &State, told: Vec<VoiceEvent>) {
        let generation = {
            let mut g = self.lock();
            if g.state == *state && told.is_empty() {
                return;
            }
            g.state = state.clone();
            for event in told {
                if g.events.len() >= MAX_EVENTS {
                    g.events.pop_front();
                }
                g.events.push_back(event);
            }
            g.generation = g.generation.wrapping_add(1).max(1);
            g.generation
        };
        self.waker.tell(generation);
    }
}

pub(crate) struct Control {
    device: Box<dyn VoiceDevice>,
    meters: Arc<Meters>,
    board: Arc<Board>,
    seq: Seq,
    state: State,
    wake: Option<(Attempt, Instant)>,
}

impl Control {
    pub(crate) fn new(device: Box<dyn VoiceDevice>, meters: Arc<Meters>, board: Arc<Board>) -> Self {
        Self { device, meters, board, seq: Seq::default(), state: State::Idle, wake: None }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn run(mut self, signals: &Receiver<Signal>) {
        self.device.prepare().ok();
        while self.state != State::Closed {
            let event = self.next_event(signals);
            self.feed(event);
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn prepare(&mut self) {
        self.device.prepare().ok();
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn drain(&mut self, signals: &Receiver<Signal>) -> Option<Instant> {
        while self.state != State::Closed {
            match signals.try_recv() {
                Ok(signal) => self.feed(signal.event(rune_core::time::now())),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.feed(Event::Close),
            }
        }
        let now = rune_core::time::now();
        if let Some((attempt, due)) = self.wake
            && now >= due
            && self.state != State::Closed
        {
            self.wake = None;
            self.feed(Event::RetryDue { attempt, now });
        }
        self.wake.map(|(_, due)| due).filter(|_| self.state != State::Closed)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn closed(&self) -> bool {
        self.state == State::Closed
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn next_event(&mut self, signals: &Receiver<Signal>) -> Event {
        let received = match self.wake {
            Some((attempt, due)) => {
                let wait = due.saturating_duration_since(rune_core::time::now());
                match signals.recv_timeout(wait) {
                    Err(RecvTimeoutError::Timeout) => {
                        self.wake = None;
                        return Event::RetryDue { attempt, now: rune_core::time::now() };
                    }
                    other => other.map_err(|_| ()),
                }
            }
            None => signals.recv().map_err(|_| ()),
        };
        received.unwrap_or(Signal::Close).event(rune_core::time::now())
    }

    pub(crate) fn feed(&mut self, event: Event) {
        let mut queue = VecDeque::from([event]);
        let mut told = Vec::new();
        while let Some(event) = queue.pop_front() {
            for effect in step(&mut self.seq, &mut self.state, event) {
                match effect {
                    Effect::Run => {
                        let later = self.device.runs_from_first_audio();
                        let result = self.device.run().map(|()| if later { Opened::AwaitingAudio } else { Opened::Live });
                        queue.push_back(Event::Ran { result, now: rune_core::time::now() });
                    }
                    Effect::Halt => self.device.halt(),
                    Effect::Discard => self.device.discard(),
                    Effect::Mute(muted) => self.meters.muted.store(muted, Ordering::Relaxed),
                    Effect::WakeAt { attempt, due } => self.wake = Some((attempt, due)),
                    Effect::Tell(event) => told.push(event),
                }
            }
        }
        self.board.publish(&self.state, told);
    }
}

pub(crate) type SignalSender = Sender<Signal>;
