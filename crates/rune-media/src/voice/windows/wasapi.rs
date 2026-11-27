use super::thread::Entered;
use crate::voice::capture_position::{CapturePosition, Packet};
use crate::voice::echo_path::{self, EchoPath, SystemAec};
use crate::voice::machine::Fault;
use crate::voice::streams::{MicLink, Opened, SpeakerLink};
use crate::voice::underrun::{self, Reading};
use std::collections::VecDeque;
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, E_ACCESSDENIED, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDCLNT_STREAMOPTIONS_NONE, AUDIO_EFFECT, AUDIO_EFFECT_STATE_ON, AudioCategory_Communications,
    AudioClientProperties, IAcousticEchoCancellationControl, IAudioCaptureClient, IAudioClient, IAudioClient2, IAudioClock, IAudioEffectsManager,
    IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, eCapture, eCommunications, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{GUID, Interface, PCWSTR};

const AEC_EFFECT: GUID = GUID::from_u128(0x6f64adbe_8211_11e2_8c70_2c27d7f001fa);
const MIC_BUFFER: Duration = Duration::from_millis(20);
const SPEAKER_BUFFER: Duration = Duration::from_millis(40);
const WAIT_MS: u32 = 100;
const STALLED_WAITS: u32 = 20;

fn fault(error: &windows::core::Error, what: &str) -> Fault {
    if error.code() == E_ACCESSDENIED {
        Fault::lasting("microphone access is off for this app; turn it on in Settings > Privacy & security > Microphone")
    } else {
        Fault::passing(format!("{what}: {}", error.message()))
    }
}

fn mono_float(rate: u32) -> WAVEFORMATEX {
    WAVEFORMATEX { wFormatTag: 3, nChannels: 1, nSamplesPerSec: rate, nAvgBytesPerSec: rate * 4, nBlockAlign: 4, wBitsPerSample: 32, cbSize: 0 }
}

struct Client {
    client: IAudioClient,
    event: HANDLE,
    frames: u32,
    waits: u32,
}

impl Client {
    fn open(device: &IMMDevice, rate: u32, buffer: Duration, what: &str) -> Result<Self, Fault> {
        // SAFETY: COM calls on interfaces this function owns; the format and properties outlive the calls that read them.
        unsafe {
            let client: IAudioClient2 = device.Activate(CLSCTX_ALL, None).map_err(|e| fault(&e, what))?;
            let properties = AudioClientProperties {
                cbSize: size_of::<AudioClientProperties>() as u32,
                bIsOffload: false.into(),
                eCategory: AudioCategory_Communications,
                Options: AUDCLNT_STREAMOPTIONS_NONE,
            };
            client.SetClientProperties(&properties).map_err(|e| fault(&e, what))?;
            let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            let period = i64::try_from(buffer.as_nanos() / 100).unwrap_or(i64::MAX);
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, period, 0, &mono_float(rate), None).map_err(|e| fault(&e, what))?;
            let event = CreateEventW(None, false, false, None).map_err(|e| fault(&e, what))?;
            let client: IAudioClient = client.cast().map_err(|e| fault(&e, what))?;
            let mut opened = Self { client, event, frames: 0, waits: 0 };
            opened.client.SetEventHandle(opened.event).map_err(|e| fault(&e, what))?;
            opened.frames = opened.client.GetBufferSize().map_err(|e| fault(&e, what))?;
            Ok(opened)
        }
    }

    fn wait(&mut self, what: &str) -> Result<(), Fault> {
        // SAFETY: a wait on this client's own event.
        if unsafe { WaitForSingleObject(self.event, WAIT_MS) } == WAIT_OBJECT_0 {
            self.waits = 0;
            return Ok(());
        }
        self.waits += 1;
        if self.waits >= STALLED_WAITS { Err(Fault::passing(format!("the {what} stopped delivering audio"))) } else { Ok(()) }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: stops this client and closes its own event once.
        unsafe {
            self.client.Stop().ok();
            CloseHandle(self.event).ok();
        }
    }
}

pub(super) struct Mic {
    client: Client,
    capture: IAudioCaptureClient,
    pending: VecDeque<f32>,
    position: CapturePosition,
    lost: u64,
    entered: Option<Entered>,
}

pub(super) struct Speaker {
    client: Client,
    render: IAudioRenderClient,
    clock: IAudioClock,
    frequency: u64,
    rate: u32,
    low: u32,
    given: u64,
    ahead: Option<i64>,
    underrun: underrun::State,
    entered: Option<Entered>,
}

// SAFETY: the interfaces live in the multithreaded apartment, which every thread using them joins in `begin`, and each link is used by one thread at a time.
unsafe impl Send for Mic {}
// SAFETY: as for `Mic`.
unsafe impl Send for Speaker {}

impl MicLink for Mic {
    fn begin(&mut self) {
        self.entered = Some(Entered::audio_thread());
    }

    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        while self.pending.len() < out.len() {
            self.client.wait("microphone")?;
            self.drain()?;
        }
        let len = out.len();
        for (slot, sample) in out.iter_mut().zip(self.pending.drain(..len)) {
            *slot = sample;
        }
        Ok(())
    }

    fn take_lost(&mut self) -> u64 {
        std::mem::take(&mut self.lost)
    }

    fn device_ahead(&self) -> Option<i64> {
        self.position.ahead()
    }
}

impl Mic {
    fn push(&mut self, samples: impl ExactSizeIterator<Item = f32>) {
        let room = self.pending.capacity() - self.pending.len();
        let overflow = samples.len().saturating_sub(room);
        self.pending.drain(..overflow.min(self.pending.len()));
        self.pending.extend(samples);
    }

    fn drain(&mut self) -> Result<(), Fault> {
        // SAFETY: capture calls on this stream; each buffer holds `frames` mono floats until it is released.
        unsafe {
            while self.capture.GetNextPacketSize().map_err(|e| fault(&e, "microphone"))? > 0 {
                let (mut data, mut frames, mut flags, mut position) = (std::ptr::null_mut(), 0u32, 0u32, 0u64);
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, Some(&mut position), None).map_err(|e| fault(&e, "microphone"))?;
                let gap = self.position.packet(&Packet {
                    position,
                    frames: u64::from(frames),
                    discontinuity: flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0,
                    timed: flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 == 0,
                });
                self.lost += gap;
                self.push(std::iter::repeat_n(0.0, gap as usize));
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    self.push(std::iter::repeat_n(0.0, frames as usize));
                } else {
                    self.push(std::slice::from_raw_parts(data.cast::<f32>(), frames as usize).iter().copied());
                }
                self.capture.ReleaseBuffer(frames).map_err(|e| fault(&e, "microphone"))?;
            }
        }
        Ok(())
    }
}

impl SpeakerLink for Speaker {
    fn begin(&mut self) {
        self.entered = Some(Entered::audio_thread());
    }

    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        let len = u32::try_from(frames.len()).unwrap_or(u32::MAX).min(self.client.frames);
        loop {
            // SAFETY: render calls on this stream; the buffer holds `len` mono floats until it is released.
            let written = unsafe {
                let padding = self.client.client.GetCurrentPadding().map_err(|e| fault(&e, "speaker"))?;
                if self.client.frames - padding >= len {
                    let data = self.render.GetBuffer(len).map_err(|e| fault(&e, "speaker"))?;
                    std::slice::from_raw_parts_mut(data.cast::<f32>(), len as usize).copy_from_slice(&frames[..len as usize]);
                    self.render.ReleaseBuffer(len, 0).map_err(|e| fault(&e, "speaker"))?;
                    self.given += u64::from(len);
                    true
                } else {
                    false
                }
            };
            if written {
                return Ok(());
            }
            self.client.wait("speaker")?;
        }
    }

    fn take_silence(&mut self) -> u64 {
        let mut position = 0u64;
        // SAFETY: reads of this stream's own clock and padding.
        let Ok(padding) = (unsafe { self.clock.GetPosition(&mut position, None).and_then(|()| self.client.client.GetCurrentPadding()) }) else {
            return 0;
        };
        if position == 0 || self.frequency == 0 {
            return 0;
        }
        let played = u128::from(position) * u128::from(self.rate) / u128::from(self.frequency);
        let taken = self.given.saturating_sub(u64::from(padding));
        let lead = i64::try_from(played).unwrap_or(i64::MAX).saturating_sub(i64::try_from(taken).unwrap_or(i64::MAX));
        self.ahead = Some(lead);
        underrun::step(&mut self.underrun, Reading { lead, low: padding < self.low })
    }

    fn device_ahead(&self) -> Option<i64> {
        self.ahead
    }
}

fn system_aec(client: &IAudioClient) -> Option<(IAudioEffectsManager, SystemAec)> {
    // SAFETY: COM calls on this client; the effect array is read within its length and freed once.
    unsafe {
        let manager: IAudioEffectsManager = client.GetService().ok()?;
        let (mut effects, mut count): (*mut AUDIO_EFFECT, u32) = (std::ptr::null_mut(), 0);
        manager.GetAudioEffects(&mut effects, &mut count).ok()?;
        let found = (!effects.is_null())
            .then(|| std::slice::from_raw_parts(effects, count as usize).iter().find(|e| e.id == AEC_EFFECT).copied())
            .flatten();
        CoTaskMemFree(Some(effects.cast_const().cast()));
        let effect = found?;
        Some((manager, SystemAec { on: effect.state == AUDIO_EFFECT_STATE_ON, can_set: effect.canSetState.as_bool() }))
    }
}

fn reference(mic: &IAudioClient, speaker: &IMMDevice) {
    // SAFETY: COM calls on live interfaces; the endpoint id is freed once after use.
    unsafe {
        let Ok(control) = mic.GetService::<IAcousticEchoCancellationControl>() else { return };
        let Ok(id) = speaker.GetId() else { return };
        control.SetEchoCancellationRenderEndpoint(PCWSTR(id.0)).ok();
        CoTaskMemFree(Some(id.0.cast_const().cast()));
    }
}

fn echo(mic: &IAudioClient) -> EchoPath {
    let found = system_aec(mic);
    match echo_path::choose(found.as_ref().map(|(_, aec)| *aec)) {
        EchoPath::TurnOnSystem => {
            // SAFETY: a COM call on the manager `system_aec` returned for this client.
            let on = found.is_some_and(|(manager, _)| unsafe { manager.SetAudioEffectState(AEC_EFFECT, AUDIO_EFFECT_STATE_ON) }.is_ok());
            if on { EchoPath::System } else { EchoPath::InProcess }
        }
        path => path,
    }
}

pub(super) fn open(rate: u32, _block: usize) -> Result<Opened, Fault> {
    // SAFETY: COM calls on this thread, which joined the multithreaded apartment when the device was made.
    unsafe {
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| fault(&e, "listing audio devices"))?;
        let speaker_device =
            devices.GetDefaultAudioEndpoint(eRender, eCommunications).map_err(|_| Fault::passing("there's no speaker or headset to play to"))?;
        let mic_device =
            devices.GetDefaultAudioEndpoint(eCapture, eCommunications).map_err(|_| Fault::passing("there's no microphone to record from"))?;
        let speaker = Client::open(&speaker_device, rate, SPEAKER_BUFFER, "speaker")?;
        let render: IAudioRenderClient = speaker.client.GetService().map_err(|e| fault(&e, "speaker"))?;
        render.GetBuffer(speaker.frames).map_err(|e| fault(&e, "speaker"))?;
        render.ReleaseBuffer(speaker.frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32).map_err(|e| fault(&e, "speaker"))?;
        let clock: IAudioClock = speaker.client.GetService().map_err(|e| fault(&e, "speaker"))?;
        let frequency = clock.GetFrequency().map_err(|e| fault(&e, "speaker"))?;
        let mut period = 0i64;
        speaker.client.GetDevicePeriod(Some(&mut period), None).map_err(|e| fault(&e, "speaker"))?;
        let low = u32::try_from(i64::from(rate) * period / 10_000_000).unwrap_or(0);
        let given = u64::from(speaker.frames);
        let mic = Client::open(&mic_device, rate, MIC_BUFFER, "microphone")?;
        let capture: IAudioCaptureClient = mic.client.GetService().map_err(|e| fault(&e, "microphone"))?;
        reference(&mic.client, &speaker_device);
        let echo = echo(&mic.client);
        speaker.client.Start().map_err(|e| fault(&e, "speaker"))?;
        mic.client.Start().map_err(|e| fault(&e, "microphone"))?;
        let pending = VecDeque::with_capacity(rate as usize);
        Ok(Opened {
            mic: Box::new(Mic { client: mic, capture, pending, position: CapturePosition::new(rate), lost: 0, entered: None }),
            speaker: Box::new(Speaker {
                client: speaker,
                render,
                clock,
                frequency,
                rate,
                low,
                given,
                ahead: None,
                underrun: underrun::State::Waiting,
                entered: None,
            }),
            echo,
        })
    }
}
