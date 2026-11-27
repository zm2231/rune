use super::io::{Capture, Playback};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use super::aec::Aec;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use rtrb::{Consumer, Producer, RingBuffer};

pub(crate) struct Render {
    playback: Playback,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    reference: Option<Producer<f32>>,
}

pub(crate) struct Heard {
    capture: Capture,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    cancel: Option<Box<Cancel>>,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
struct Cancel {
    aec: Aec,
    reference: Consumer<f32>,
    scratch: Box<[f32]>,
}

pub(crate) fn passing(capture: Capture, playback: Playback) -> (Render, Heard) {
    (
        Render {
            playback,
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            reference: None,
        },
        Heard {
            capture,
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            cancel: None,
        },
    )
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn cancelling(rate: u32, capture: Capture, playback: Playback) -> (Render, Heard) {
    let (producer, consumer) = RingBuffer::new(rate as usize);
    let cancel = Cancel { aec: Aec::new(rate, capture.meters.clone()), reference: consumer, scratch: vec![0.0; rate as usize / 100].into_boxed_slice() };
    (Render { playback, reference: Some(producer) }, Heard { capture, cancel: Some(Box::new(cancel)) })
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn for_path(path: super::echo_path::EchoPath, rate: u32, capture: Capture, playback: Playback) -> (Render, Heard) {
    use super::echo_path::EchoPath;
    match path {
        EchoPath::System | EchoPath::TurnOnSystem => passing(capture, playback),
        EchoPath::InProcess => cancelling(rate, capture, playback),
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
pub(crate) fn join(render: Render, heard: Heard) -> (Capture, Playback) {
    (heard.capture, render.playback)
}

impl Render {
    #[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
    pub(crate) fn silence(&mut self, frames: u64) {
        self.playback.meters.count_speaker_gap(frames);
    }

    #[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
    pub(crate) fn device(&mut self, ahead: Option<i64>) {
        self.playback.meters.mark_speaker_device(ahead);
    }

    pub(crate) fn render(&mut self, out: &mut [f32]) {
        self.playback.render(out);
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if let Some(reference) = &mut self.reference {
            let (_played, _dropped) = reference.push_partial_slice(out);
        }
    }
}

impl Heard {
    #[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
    pub(crate) fn lost(&mut self, frames: u64) {
        self.capture.meters.count_mic_gap(frames);
    }

    #[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
    pub(crate) fn device(&mut self, ahead: Option<i64>) {
        self.capture.meters.mark_mic_device(ahead);
    }

    pub(crate) fn capture(&mut self, heard: &[f32]) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if let Some(cancel) = &mut self.cancel {
            loop {
                let n = cancel.reference.pop_partial_slice(&mut cancel.scratch).0.len();
                if n == 0 {
                    break;
                }
                cancel.aec.render(&cancel.scratch[..n]);
            }
            let capture = &mut self.capture;
            cancel.aec.capture(heard, |cleaned| capture.capture(cleaned));
            return;
        }
        self.capture.capture(heard);
    }
}
