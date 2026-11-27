use super::control::{Signal, SignalSender, VoiceDevice};
use super::duplex::{self, Heard, Render};
use super::echo_path::EchoPath;
use super::io::{Capture, Playback};
use super::machine::Fault;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_millis(500);

pub(crate) trait MicLink: Send {
    fn begin(&mut self) {}
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault>;
    fn take_lost(&mut self) -> u64 {
        0
    }
}

pub(crate) trait SpeakerLink: Send {
    fn begin(&mut self) {}
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault>;
    fn take_silence(&mut self) -> u64 {
        0
    }
}

pub(crate) struct Opened {
    pub(crate) mic: Box<dyn MicLink>,
    pub(crate) speaker: Box<dyn SpeakerLink>,
    pub(crate) echo: EchoPath,
}

pub(crate) type Opener = Box<dyn FnMut(u32, usize) -> Result<Opened, Fault> + Send>;
pub(crate) type Work = Box<dyn FnOnce() + Send>;
pub(crate) type Spawner = Box<dyn FnMut(&str, Work) -> std::io::Result<()> + Send>;

pub(crate) fn device(rate: u32, capture: Capture, playback: Playback, signals: SignalSender, open: Opener) -> Box<dyn VoiceDevice> {
    with_spawner(rate, capture, playback, signals, open, Box::new(spawn_thread))
}

pub(crate) fn with_spawner(
    rate: u32,
    capture: Capture,
    playback: Playback,
    signals: SignalSender,
    open: Opener,
    spawn: Spawner,
) -> Box<dyn VoiceDevice> {
    Box::new(Streams { rate, open, spawn, signals, ends: Some((capture, playback)), pumps: None })
}

fn spawn_thread(name: &str, work: Work) -> std::io::Result<()> {
    std::thread::Builder::new().name(name.into()).spawn(work).map(drop)
}

type Slot<T> = Arc<Mutex<Option<T>>>;

fn lock<T>(slot: &Mutex<Option<T>>) -> std::sync::MutexGuard<'_, Option<T>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Pumps {
    stop: Arc<AtomicBool>,
    heard: Slot<Heard>,
    render: Slot<Render>,
    finished: Receiver<()>,
    running: usize,
}

impl Pumps {
    fn end(self) -> Option<(Capture, Playback)> {
        self.stop.store(true, Ordering::Relaxed);
        let deadline = rune_core::time::now() + PATIENCE;
        for _ in 0..self.running {
            let wait = deadline.saturating_duration_since(rune_core::time::now());
            if self.finished.recv_timeout(wait).is_err() {
                break;
            }
        }
        let (heard, render) = (lock(&self.heard).take(), lock(&self.render).take());
        heard.zip(render).map(|(heard, render)| duplex::join(render, heard))
    }
}

struct Streams {
    rate: u32,
    open: Opener,
    spawn: Spawner,
    signals: SignalSender,
    ends: Option<(Capture, Playback)>,
    pumps: Option<Pumps>,
}

impl Streams {
    fn stop(&mut self) {
        if let Some(pumps) = self.pumps.take()
            && let Some(ends) = pumps.end()
        {
            self.ends = Some(ends);
        }
    }
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
            return Err(Fault::lasting("the voice stream's audio buffers were lost"));
        };
        let block = (self.rate / 100) as usize;
        let opened = match (self.open)(self.rate, block) {
            Ok(opened) => opened,
            Err(fault) => {
                self.ends = Some((capture, playback));
                return Err(fault);
            }
        };
        let (render, heard) = duplex::for_path(opened.echo, self.rate, capture, playback);
        let (done, finished) = mpsc::channel();
        let mut pumps = Pumps {
            stop: Arc::new(AtomicBool::new(false)),
            heard: Arc::new(Mutex::new(Some(heard))),
            render: Arc::new(Mutex::new(Some(render))),
            finished,
            running: 0,
        };
        let mic = {
            let (stop, slot, signals, done) = (pumps.stop.clone(), pumps.heard.clone(), self.signals.clone(), done.clone());
            let link = opened.mic;
            Box::new(move || {
                listen(link, &slot, block, &stop, &signals);
                done.send(()).ok();
            }) as Work
        };
        let speaker = {
            let (stop, slot, signals) = (pumps.stop.clone(), pumps.render.clone(), self.signals.clone());
            let link = opened.speaker;
            Box::new(move || {
                play(link, &slot, block, &stop, &signals);
                done.send(()).ok();
            }) as Work
        };
        for (name, work) in [("rune-voice-mic", mic), ("rune-voice-speaker", speaker)] {
            if let Err(e) = (self.spawn)(name, work) {
                self.ends = pumps.end();
                return Err(Fault::passing(format!("couldn't start the {name} thread: {e}")));
            }
            pumps.running += 1;
        }
        self.pumps = Some(pumps);
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

fn listen(mut link: Box<dyn MicLink>, slot: &Mutex<Option<Heard>>, block: usize, stop: &AtomicBool, signals: &SignalSender) {
    link.begin();
    let mut frames = vec![0.0; block];
    let result = (|| {
        while !stop.load(Ordering::Relaxed) {
            link.read(&mut frames)?;
            let lost = link.take_lost();
            match lock(slot).as_mut() {
                Some(heard) if !stop.load(Ordering::Relaxed) => {
                    heard.lost(lost);
                    heard.capture(&frames);
                }
                _ => break,
            }
        }
        Ok(())
    })();
    report(result, stop, signals);
}

fn play(mut link: Box<dyn SpeakerLink>, slot: &Mutex<Option<Render>>, block: usize, stop: &AtomicBool, signals: &SignalSender) {
    link.begin();
    let mut frames = vec![0.0; block];
    let result = (|| {
        while !stop.load(Ordering::Relaxed) {
            let silence = link.take_silence();
            match lock(slot).as_mut() {
                Some(render) if !stop.load(Ordering::Relaxed) => {
                    render.silence(silence);
                    render.render(&mut frames);
                }
                _ => break,
            }
            link.write(&frames)?;
        }
        Ok(())
    })();
    report(result, stop, signals);
}
