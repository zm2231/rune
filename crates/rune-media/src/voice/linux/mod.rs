#![allow(unsafe_code)]

mod alsa;
mod pulse;

use super::VoiceError;
use super::control::{SignalSender, VoiceDevice};
use super::io::{Capture, Playback};
use super::machine::Fault;
use super::streams::{self, Opened};
use crate::recorder::MicPermission;

pub(super) fn check() -> Result<(), VoiceError> {
    if pulse::available() || alsa::available() {
        Ok(())
    } else {
        Err(VoiceError::OpenFailed("neither libpulse.so.0 nor libasound.so.2 could be loaded".into()))
    }
}

pub(super) fn permission() -> MicPermission {
    MicPermission::Granted
}

pub(super) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    callback(MicPermission::Granted);
}

fn open(rate: u32, block: usize) -> Result<Opened, Fault> {
    let pulse = match pulse::open(rate, block) {
        Ok(opened) => return Ok(opened),
        Err(e) => e,
    };
    alsa::open(rate).map_err(|alsa| Fault::passing(format!("PulseAudio: {pulse}; ALSA: {alsa}")))
}

pub(super) fn device(rate: u32, capture: Capture, playback: Playback, signals: SignalSender) -> Box<dyn VoiceDevice> {
    streams::device(rate, capture, playback, signals, Box::new(open))
}

#[cfg(test)]
mod tests;
