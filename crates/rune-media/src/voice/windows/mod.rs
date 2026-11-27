#![allow(unsafe_code)]

mod thread;
mod wasapi;

use super::VoiceError;
use super::control::{Signal, SignalSender, VoiceDevice};
use super::io::{Capture, Playback};
use super::machine::Fault;
use super::streams;
use crate::recorder::MicPermission;
use thread::Entered;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    eCommunications,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{HSTRING, PCWSTR, implement, w};

const CONSENT: [&str; 2] = [
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone",
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged",
];

pub(super) fn check() -> Result<(), VoiceError> {
    Ok(())
}

fn denied(key: &str) -> bool {
    let mut text = [0u16; 16];
    let mut bytes = size_of_val(&text) as u32;
    // SAFETY: reads a string value into `text`, whose size in bytes is passed alongside it.
    let read = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(key), w!("Value"), RRF_RT_REG_SZ, None, Some(text.as_mut_ptr().cast()), Some(&mut bytes))
    };
    let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    read.is_ok() && String::from_utf16_lossy(&text[..len]).eq_ignore_ascii_case("Deny")
}

pub(super) fn permission() -> MicPermission {
    if CONSENT.iter().any(|key| denied(key)) { MicPermission::Denied } else { MicPermission::Granted }
}

pub(super) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    callback(permission());
}

#[implement(IMMNotificationClient)]
struct Watcher {
    signals: SignalSender,
}

impl IMMNotificationClient_Impl for Watcher_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, _: EDataFlow, role: ERole, _: &PCWSTR) -> windows::core::Result<()> {
        if role == eCommunications {
            self.signals.send(Signal::DevicesChanged).ok();
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

struct Watch {
    devices: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

impl Watch {
    fn new(signals: SignalSender) -> Option<Self> {
        let client: IMMNotificationClient = Watcher { signals }.into();
        // SAFETY: COM calls on this thread, which joined the multithreaded apartment first.
        unsafe {
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            devices.RegisterEndpointNotificationCallback(&client).ok()?;
            Some(Self { devices, client })
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // SAFETY: removes exactly the callback `new` registered.
        unsafe { self.devices.UnregisterEndpointNotificationCallback(&self.client).ok() };
    }
}

struct Device {
    streams: Box<dyn VoiceDevice>,
    _watch: Option<Watch>,
    _entered: Entered,
}

impl VoiceDevice for Device {
    fn prepare(&mut self) -> Result<(), Fault> {
        self.streams.prepare()
    }

    fn run(&mut self) -> Result<(), Fault> {
        if permission() == MicPermission::Denied {
            return Err(Fault::lasting("microphone access is off for this app; turn it on in Settings > Privacy & security > Microphone"));
        }
        self.streams.run()
    }

    fn halt(&mut self) {
        self.streams.halt();
    }

    fn discard(&mut self) {
        self.streams.discard();
    }
}

pub(super) fn device(rate: u32, capture: Capture, playback: Playback, signals: SignalSender) -> Box<dyn VoiceDevice> {
    let entered = Entered::apartment();
    let watch = Watch::new(signals.clone());
    let streams = streams::device(rate, capture, playback, signals, Box::new(wasapi::open));
    Box::new(Device { streams, _watch: watch, _entered: entered })
}
