use super::super::VoiceError;
use super::super::control::{Signal, SignalSender};
use super::super::machine::Fault;
use crate::recorder::{MicPermission, macos as mic};
use objc2_core_audio::{
    AudioObjectAddPropertyListener, AudioObjectID, AudioObjectPropertyAddress, AudioObjectRemovePropertyListener,
    kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};
use std::ffi::c_void;
use std::ptr::NonNull;

const WATCHED: [u32; 2] = [kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDefaultOutputDevice];

pub(crate) fn check() -> Result<(), VoiceError> {
    mic::preflight().map_err(VoiceError::OpenFailed)
}

pub(crate) fn permission() -> MicPermission {
    mic::permission()
}

pub(crate) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    mic::request_permission(callback);
}

pub(super) struct Platform {
    signals: Box<SignalSender>,
}

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

impl Platform {
    pub(super) fn new(signals: SignalSender) -> Self {
        let mut platform = Self { signals: Box::new(signals) };
        let data = platform.data();
        for selector in WATCHED {
            let address = address(selector);
            // SAFETY: the address is valid for the call, and `data` points to the boxed sender, which outlives the listener because `Drop` removes it first.
            unsafe { AudioObjectAddPropertyListener(kAudioObjectSystemObject as AudioObjectID, NonNull::from(&address), Some(on_change), data) };
        }
        platform
    }

    fn data(&mut self) -> *mut c_void {
        (&raw mut *self.signals).cast()
    }

    pub(super) fn prepare(&mut self) -> Result<(), Fault> {
        Ok(())
    }

    pub(super) fn activate(&mut self) -> Result<(), Fault> {
        Ok(())
    }

    pub(super) fn deactivate(&mut self) {}
}

impl Drop for Platform {
    fn drop(&mut self) {
        let data = self.data();
        for selector in WATCHED {
            let address = address(selector);
            // SAFETY: removes exactly the listener `new` added, with the same address, proc and data.
            unsafe { AudioObjectRemovePropertyListener(kAudioObjectSystemObject as AudioObjectID, NonNull::from(&address), Some(on_change), data) };
        }
    }
}

unsafe extern "C-unwind" fn on_change(
    _object: AudioObjectID,
    _count: u32,
    _addresses: NonNull<AudioObjectPropertyAddress>,
    data: *mut c_void,
) -> i32 {
    // SAFETY: `data` is the boxed sender `Platform::new` registered, alive until the listener is removed.
    if let Some(signals) = unsafe { data.cast::<SignalSender>().as_ref() } {
        signals.send(Signal::DevicesChanged).ok();
    }
    0
}
