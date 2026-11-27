#![allow(unsafe_code)]

mod alsa;
mod pulse;

use super::VoiceError;
use super::control::{Signal, SignalSender, VoiceDevice};
use super::duplex::{self, Heard, Render};
use super::io::{Capture, Playback};
use super::machine::Fault;
use crate::recorder::MicPermission;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

pub(crate) trait MicLink: Send {
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault>;
}

pub(crate) trait SpeakerLink: Send {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault>;
}

pub(crate) type Links = (Box<dyn MicLink>, Box<dyn SpeakerLink>);
pub(crate) type Opener = Box<dyn FnMut(u32, usize) -> Result<Links, Fault> + Send>;

pub(super) fn check() -> Result<(), VoiceError> {
    if pulse::available() || alsa::available() {
        Ok(())
    } else {
        Err(VoiceError::OpenFailed("neither libpulse-simple.so.0 nor libasound.so.2 could be loaded".into()))
    }
}

pub(super) fn permission() -> MicPermission {
    MicPermission::Granted
}

pub(super) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    callback(MicPermission::Granted);
}

fn open_system(rate: u32, block: usize) -> Result<Links, Fault> {
    let pulse = match pulse::open(rate, block) {
        Ok(links) => return Ok(links),
        Err(e) => e,
    };
    alsa::open(rate).map_err(|alsa| Fault::passing(format!("PulseAudio: {pulse}; ALSA: {alsa}")))
}

pub(super) fn device(rate: u32, capture: Capture, playback: Playback, signals: SignalSender) -> Box<dyn VoiceDevice> {
    with_opener(rate, capture, playback, signals, Box::new(open_system))
}

pub(crate) fn with_opener(rate: u32, capture: Capture, playback: Playback, signals: SignalSender, open: Opener) -> Box<dyn VoiceDevice> {
    Box::new(Streams { rate, open, signals, ends: Some((capture, playback)), pumps: None })
}

struct Pumps {
    stop: Arc<AtomicBool>,
    mic: JoinHandle<Heard>,
    speaker: JoinHandle<Render>,
}

struct Streams {
    rate: u32,
    open: Opener,
    signals: SignalSender,
    ends: Option<(Capture, Playback)>,
    pumps: Option<Pumps>,
}

impl Streams {
    fn stop(&mut self) {
        if let Some(pumps) = self.pumps.take() {
            pumps.stop.store(true, Ordering::Relaxed);
            if let (Ok(heard), Ok(render)) = (pumps.mic.join(), pumps.speaker.join()) {
                self.ends = Some(duplex::join(render, heard));
            }
        }
    }
}

fn spawn<T: Send + 'static>(name: &str, work: impl FnOnce() -> T + Send + 'static) -> Result<JoinHandle<T>, Fault> {
    std::thread::Builder::new().name(name.into()).spawn(work).map_err(|e| Fault::passing(e.to_string()))
}

impl VoiceDevice for Streams {
    fn prepare(&mut self) -> Result<(), Fault> {
        Ok(())
    }

    fn run(&mut self) -> Result<(), Fault> {
        if self.pumps.is_some() {
            return Ok(());
        }
        let Some((capture, playback)) = self.ends.take() else {
            return Err(Fault::lasting("the voice stream's audio threads stopped unexpectedly"));
        };
        let block = (self.rate / 100) as usize;
        let (mic_link, speaker_link) = match (self.open)(self.rate, block) {
            Ok(links) => links,
            Err(fault) => {
                self.ends = Some((capture, playback));
                return Err(fault);
            }
        };
        let (render, heard) = duplex::cancelling(self.rate, capture, playback);
        let stop = Arc::new(AtomicBool::new(false));
        let (mic_stop, mic_signals) = (stop.clone(), self.signals.clone());
        let mic = spawn("rune-voice-mic", move || listen(mic_link, heard, block, &mic_stop, &mic_signals))?;
        let (speaker_stop, speaker_signals) = (stop.clone(), self.signals.clone());
        let speaker = spawn("rune-voice-speaker", move || play(speaker_link, render, block, &speaker_stop, &speaker_signals));
        let speaker = match speaker {
            Ok(speaker) => speaker,
            Err(fault) => {
                stop.store(true, Ordering::Relaxed);
                drop(mic.join());
                return Err(Fault::lasting(fault.message));
            }
        };
        self.pumps = Some(Pumps { stop, mic, speaker });
        Ok(())
    }

    fn halt(&mut self) {
        self.stop();
    }

    fn discard(&mut self) {
        self.stop();
    }
}

impl Drop for Streams {
    fn drop(&mut self) {
        self.stop();
    }
}

fn report(result: Result<(), Fault>, stop: &AtomicBool, signals: &SignalSender) {
    if let Err(fault) = result
        && !stop.swap(true, Ordering::Relaxed)
    {
        signals.send(Signal::Lost(fault)).ok();
    }
}

fn listen(mut link: Box<dyn MicLink>, mut heard: Heard, block: usize, stop: &AtomicBool, signals: &SignalSender) -> Heard {
    let mut frames = vec![0.0; block];
    let result = (|| {
        while !stop.load(Ordering::Relaxed) {
            link.read(&mut frames)?;
            heard.capture(&frames);
        }
        Ok(())
    })();
    report(result, stop, signals);
    heard
}

fn play(mut link: Box<dyn SpeakerLink>, mut render: Render, block: usize, stop: &AtomicBool, signals: &SignalSender) -> Render {
    let mut frames = vec![0.0; block];
    let result = (|| {
        while !stop.load(Ordering::Relaxed) {
            render.render(&mut frames);
            link.write(&frames)?;
        }
        Ok(())
    })();
    report(result, stop, signals);
    render
}

#[cfg(test)]
mod tests;
