use super::VoiceError;
use super::control::{SignalSender, VoiceDevice};
use super::io::{Capture, Playback};
use crate::recorder::MicPermission;

pub(super) fn check() -> Result<(), VoiceError> {
    Err(VoiceError::Unsupported)
}

pub(super) fn device(_rate: u32, _capture: Capture, _playback: Playback, _signals: SignalSender) -> Box<dyn VoiceDevice> {
    unreachable!("check refuses every open on this platform")
}

pub(super) fn permission() -> MicPermission {
    MicPermission::Restricted
}

pub(super) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    callback(MicPermission::Restricted);
}
