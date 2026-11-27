//! Full-duplex voice audio for calls: echo-cancelled microphone frames in, speaker frames out,
//! both as mono `f32` PCM at a rate the app picks.
//!
//! [`Voice::open`] returns a control handle and two lock-free stream ends. [`VoiceMic`] reads
//! what the microphone heard with the speaker's own output removed. [`VoiceSpeaker`] queues
//! what to play. The app's ends never block: the app and the audio side share only the two
//! rings and a few atomics, so neither waits on the other. On Apple platforms the device's audio
//! callback also never allocates; on Linux and Windows audio moves on Rune's own threads, and on
//! the web each 10 ms block crosses the page's main thread in freshly allocated buffers.
//! [`Voice::open_fake`] runs anywhere, on a [`FakeVoice`].
//!
//! # Platforms
//!
//! **macOS and iOS.** The stream runs through Apple's voice-processing I/O unit, which cancels
//! echo, suppresses noise and levels the microphone. On iOS the audio session is set to
//! play-and-record in voice-chat mode, using the loudspeaker unless headphones or a Bluetooth
//! headset are connected. When the default devices change (headphones plugged in or out) the
//! stream rebuilds itself on the new route. When a call or another app interrupts it, it pauses
//! and resumes afterwards.
//!
//! **Linux.** The stream opens a record and a playback stream on the default devices through
//! PulseAudio's asynchronous API, which PipeWire desktops serve through `pipewire-pulse`, and falls back
//! to ALSA's `default` device. Both libraries are loaded at run time, so a system without either
//! fails to open with [`VoiceError::OpenFailed`]. Echo cancellation and noise suppression run in
//! the app, with WebRTC's AEC3 and noise suppressor, using exactly the samples Rune plays as the
//! reference, so no system echo-cancel module is needed. Limits: only Rune's own output is
//! cancelled, not other apps' sound; steady pure tones are cancelled far less than speech or
//! noise; the microphone level isn't adjusted automatically; an echo that comes back more than
//! about 700 ms after it was played (a long virtual-cable or network loop) may not be found, and
//! then isn't cancelled; the sound server moves the streams when the default devices change, and a stream that fails is
//! rebuilt ([`VoiceEvent::DeviceChanged`]). If the default source is already PipeWire's or
//! PulseAudio's echo-cancel node, the echo is cancelled twice, which is harmless. When the
//! speaker runs dry and plays silence, the stream measures the gap (PulseAudio: from the two
//! streams' timing; ALSA: from the device's delay when it reports an underrun) and counts it in
//! [`VoiceStats::speaker_gap_frames`]; frames the microphone loses (a hole in PulseAudio's
//! record stream, or an ALSA overrun) are counted in [`VoiceStats::mic_gap_frames`] and filled
//! with silence, so later audio keeps its timing. Verified against a real PulseAudio server
//! whose microphone hears the speaker directly, including a speaker underrun on both
//! PulseAudio and ALSA; a microphone overrun and a desktop with a sound card and PipeWire
//! haven't been verified yet. Linux has no microphone permission, so
//! [`Voice::permission`] is [`MicPermission::Granted`].
//!
//! **Windows.** The stream opens shared-mode WASAPI clients on the default communications
//! microphone and speaker, tagged as communications audio so the system's voice processing
//! applies; on Windows 11 the speaker is named as the echo canceller's reference. When the
//! microphone's driver offers the system echo canceller (Windows 11 lists it among the
//! stream's audio effects, and turns it on when it can), the system cancels echo and Rune
//! leaves the microphone alone; otherwise, as on Windows 10 or with a driver that has none, the
//! same in-app canceller as on Linux runs instead. The two never both run. When the default
//! communications device changes the stream rebuilds on the new one ([`VoiceEvent::DeviceChanged`]).
//! [`Voice::permission`] reads the system's microphone privacy setting for desktop apps; when it
//! is off the stream fails with a message saying where to turn it on. Windows may lower other
//! apps' sound during a call, following the Communications setting in the Sound control panel.
//! When the speaker runs dry and plays silence, or the microphone drops frames (its position
//! skips, whether or not it flags the glitch), the stream measures the gap from the device's
//! clock and positions and counts it in
//! [`VoiceStats::speaker_gap_frames`] and [`VoiceStats::mic_gap_frames`]. A microphone gap is
//! filled with silence, up to a second at a time, so later audio keeps its timing after any gap
//! up to that long. Echo cancellation on real Windows
//! hardware hasn't been verified yet; the code is checked by compiling it and by tests that
//! run without audio devices.
//!
//! **The web.** The stream asks the browser for the microphone with its own echo cancellation,
//! noise suppression and gain control on, and runs both directions through one AudioWorklet in
//! an `AudioContext` at the audio device's own rate, so echo cancellation is the browser's. The
//! context keeps the device's rate because Firefox cancels only the sound played in the same
//! audio graph as the microphone, and a context at any other rate gets a graph of its own; Rune
//! converts between the device's rate and the stream's. Without threads in the page's wasm, the
//! stream's control and audio both pass through the page's main thread: the worklet posts each
//! 10 ms of microphone audio there and gets the same amount of speaker audio back, buffered
//! 20 ms ahead. A main thread busy for longer than the buffer (a long frame, heavy script) plays
//! silence or drops speaker audio, counted in [`VoiceStats::late_frames`]. The page needs HTTPS
//! or `localhost`. Browsers start audio only after the person interacts with the page, so call
//! [`Voice::start`] from a click or key handler; started earlier, audio begins at the next click
//! or key press. The stream stays [`VoiceState::Starting`] until the first microphone audio
//! arrives, so it isn't [`VoiceState::Running`] while the browser's permission prompt is open.
//! The first start asks for the microphone; a refusal fails the stream ([`VoiceState::Failed`])
//! and [`Voice::permission`] then reads [`MicPermission::Denied`]. With no microphone at all the
//! stream fails too, with a message that says so, rather than retrying. A change of audio devices
//! rebuilds the stream. Echo cancellation has been measured in headless Firefox and Chromium
//! against a PulseAudio server whose microphone hears the speaker, at device rates of 44.1 and
//! 48 kHz; Safari and real speakers haven't been verified yet.
//!
//! Elsewhere, [`Voice::open`] returns [`VoiceError::Unsupported`].
//!
//! Apps reach these types through `rune`, which re-exports them.

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod aec;
#[cfg(any(target_os = "windows", test))]
mod capture_queue;
mod control;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod duplex;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod echo_path;
mod fake;
mod io;
mod machine;
#[cfg(target_arch = "wasm32")]
mod rates;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod streams;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod underrun;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_arch = "wasm32")]
mod inline;
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux", target_os = "windows", target_arch = "wasm32")))]
mod unsupported;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use apple as backend;
#[cfg(target_os = "linux")]
use linux as backend;
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux", target_os = "windows", target_arch = "wasm32")))]
use unsupported as backend;
#[cfg(target_arch = "wasm32")]
use web as backend;
#[cfg(target_os = "windows")]
use windows as backend;

pub use fake::FakeVoice;
pub use io::{VoiceMic, VoiceSpeaker};

use crate::recorder::MicPermission;
use control::{Board, Control, Signal, SignalSender, VoiceDevice};
use io::{Capture, Meters, Playback};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use std::thread::JoinHandle;
use std::time::Duration;

const MIN_RATE: u32 = 8_000;
const MAX_RATE: u32 = 48_000;

/// How to open a [`Voice`]: the sample rate both streams use, and how much each end buffers.
///
/// ```
/// use rune_media::VoiceSpec;
/// use std::time::Duration;
///
/// let spec = VoiceSpec::new(24_000).mic_buffer(Duration::from_millis(300)).speaker_buffer(Duration::from_secs(5));
/// # let _ = spec;
/// ```
#[derive(Clone)]
pub struct VoiceSpec {
    sample_rate: u32,
    mic_buffer: Duration,
    speaker_buffer: Duration,
    waker: Option<rune_core::Waker>,
}

impl fmt::Debug for VoiceSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSpec")
            .field("sample_rate", &self.sample_rate)
            .field("mic_buffer", &self.mic_buffer)
            .field("speaker_buffer", &self.speaker_buffer)
            .finish()
    }
}

impl VoiceSpec {
    /// Mono `f32` at `sample_rate` Hz (8,000 to 48,000) both ways, with 200 ms of microphone
    /// and 2 s of speaker buffer.
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate, mic_buffer: Duration::from_millis(200), speaker_buffer: Duration::from_secs(2), waker: None }
    }

    /// How much captured audio waits for [`VoiceMic::read`] before new frames are dropped
    /// (20 ms to 2 s).
    pub fn mic_buffer(mut self, duration: Duration) -> Self {
        self.mic_buffer = duration;
        self
    }

    /// How far ahead of playback [`VoiceSpeaker::write`] can queue (100 ms to 10 s).
    pub fn speaker_buffer(mut self, duration: Duration) -> Self {
        self.speaker_buffer = duration;
        self
    }

    /// Called when the state changes or an event arrives, from another thread (on the web, from
    /// the page's main thread).
    pub fn waker(mut self, waker: rune_core::Waker) -> Self {
        self.waker = Some(waker);
        self
    }

    fn frames(&self, duration: Duration, min: Duration, max: Duration) -> usize {
        (duration.clamp(min, max).as_secs_f64() * f64::from(self.sample_rate)).ceil() as usize
    }

    fn validate(&self) -> Result<(), VoiceError> {
        if (MIN_RATE..=MAX_RATE).contains(&self.sample_rate) {
            Ok(())
        } else {
            Err(VoiceError::InvalidSpec(format!("sample rate {} Hz outside {MIN_RATE}..={MAX_RATE}", self.sample_rate)))
        }
    }
}

/// Why a [`Voice`] didn't open.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoiceError {
    InvalidSpec(String),
    /// This platform has no echo-cancelled voice I/O in Rune yet.
    Unsupported,
    OpenFailed(String),
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpec(message) => write!(f, "invalid voice spec: {message}"),
            Self::Unsupported => write!(f, "voice audio isn't supported on this platform yet"),
            Self::OpenFailed(message) => write!(f, "couldn't open voice audio: {message}"),
        }
    }
}

impl std::error::Error for VoiceError {}

/// Where a [`Voice`] is.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoiceState {
    Idle,
    Starting,
    Running,
    /// The devices changed or the device failed; the stream is being rebuilt.
    Reconnecting,
    /// A call or another app has the audio; the stream resumes when it ends.
    Interrupted,
    /// The stream couldn't start or come back. [`Voice::start`] tries again.
    Failed(String),
}

/// Something that happened to a [`Voice`], from [`Voice::take_events`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoiceEvent {
    Started,
    Stopped,
    /// The stream moved to new devices and is running again.
    DeviceChanged,
    Interrupted,
    Resumed,
    Failed(String),
}

/// Counters from a [`Voice`]'s audio thread.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct VoiceStats {
    pub captured_frames: u64,
    pub played_frames: u64,
    pub mic_dropped_frames: u64,
    /// Device callbacks larger than the stream's buffers, which got silence.
    pub oversized_callbacks: u64,
    /// Speaker frames played as silence, or dropped, because audio didn't cross between the
    /// app's thread and the audio thread in time. Only the web counts these: there the audio
    /// passes through the page's main thread, so a long frame or a busy script shows up here.
    pub late_frames: u64,
    /// Silence the speaker device played because the audio thread fed it late (an underrun),
    /// as the device reports it. Linux and Windows count these.
    pub speaker_gap_frames: u64,
    /// Frames the microphone device lost in a glitch, as the device reports it (on Windows, as
    /// its position shows, flagged or not). They are replaced with silence, up to a second at a
    /// time, so later audio keeps its timing. Linux and Windows count these.
    pub mic_gap_frames: u64,
    /// The microphone device's own position minus the frames it delivered (skips included),
    /// taken together on the audio thread. It holds steady; a step means the device's position
    /// went back, repeating audio. Windows reports it; `None` elsewhere.
    pub mic_device_ahead_frames: Option<i64>,
    /// The speaker device's own clock minus the frames it has taken from its buffer, taken
    /// together on the audio thread. It holds steady near zero; a step means the device played
    /// more or less than it was given. Windows reports it; `None` elsewhere.
    pub speaker_device_ahead_frames: Option<i64>,
    /// From the last [`Voice::start`] to the device's first callback.
    pub start_to_audio: Option<Duration>,
    /// What Rune's own echo canceller reports, refreshed once a second while it runs (Linux, and
    /// Windows without the system's canceller); `None` when the system cancels the echo.
    pub echo_canceller: Option<EchoCancellerStats>,
}

/// The in-app echo canceller's view of the echo, from [`VoiceStats::echo_canceller`]. For
/// diagnosing a leak: a fall in `erle_db` with a jump in `alignment` means the canceller lost and
/// re-found the echo; a steady trend in `reference_lead_frames` means the speaker's and
/// microphone's clocks drift apart; a wider range than usual means speaker and microphone audio
/// reached the canceller unevenly that second. It doesn't say why delivery was uneven (device
/// packet sizes or thread scheduling).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct EchoCancellerStats {
    /// How much the canceller estimates it removes from the echo, in dB (echo return loss
    /// enhancement).
    pub erle_db: f32,
    /// How much quieter than the speaker the canceller estimates the echo is, in dB (echo
    /// return loss).
    pub erl_db: f32,
    /// How far back in the speaker's signal the canceller lines up the echo. It holds a fixed
    /// starting value until the canceller has found the echo.
    pub alignment: Duration,
    /// Speaker frames given to the canceller minus microphone frames, lowest and highest over the
    /// last second.
    pub reference_lead_frames: (i64, i64),
}

/// An echo-cancelled, full-duplex voice stream. Dropping it stops the stream and releases the
/// device; [`VoiceMic`] and [`VoiceSpeaker`] stay valid but carry no more audio.
///
/// ```no_run
/// use rune_media::{Voice, VoiceSpec};
///
/// let (voice, mut mic, mut speaker) = Voice::open(VoiceSpec::new(24_000))?;
/// voice.start();
/// let mut frames = [0.0f32; 480];
/// let n = mic.read(&mut frames);
/// speaker.write(&frames[..n]);
/// # Ok::<(), rune_media::VoiceError>(())
/// ```
pub struct Voice {
    signals: SignalSender,
    board: Arc<Board>,
    meters: Arc<Meters>,
    sample_rate: u32,
    #[cfg(not(target_arch = "wasm32"))]
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Voice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Voice").field("sample_rate", &self.sample_rate).field("state", &self.state()).finish()
    }
}

impl Voice {
    /// Opens the system's echo-cancelled voice I/O on the default microphone and speaker. The
    /// device is prepared on a thread of its own (on the web, asynchronously on the page's main
    /// thread), so this returns at once; call [`start`](Self::start) to begin. A bundled Apple
    /// app needs `NSMicrophoneUsageDescription` in its Info.plist, and without one this fails
    /// instead of letting the system end the app.
    pub fn open(spec: VoiceSpec) -> Result<(Voice, VoiceMic, VoiceSpeaker), VoiceError> {
        spec.validate()?;
        backend::check()?;
        let rate = spec.sample_rate;
        Self::launch(spec, move |capture, playback, signals| backend::device(rate, capture, playback, signals))
    }

    /// Opens a stream on `fake` instead of a device, on any platform.
    pub fn open_fake(spec: VoiceSpec, fake: &FakeVoice) -> Result<(Voice, VoiceMic, VoiceSpeaker), VoiceError> {
        spec.validate()?;
        let (rate, fake) = (spec.sample_rate, fake.clone());
        Self::launch(spec, move |capture, playback, signals| fake.device(rate, capture, playback, signals))
    }

    fn launch(
        spec: VoiceSpec,
        device: impl FnOnce(Capture, Playback, SignalSender) -> Box<dyn VoiceDevice> + Send + 'static,
    ) -> Result<(Voice, VoiceMic, VoiceSpeaker), VoiceError> {
        let mic_frames = spec.frames(spec.mic_buffer, Duration::from_millis(20), Duration::from_secs(2));
        let speaker_frames = spec.frames(spec.speaker_buffer, Duration::from_millis(100), Duration::from_secs(10));
        let ends = io::ends(mic_frames, speaker_frames);
        let board = Arc::new(Board::new());
        if let Some(waker) = spec.waker {
            board.waker.set(waker);
        }
        let (signals, received) = mpsc::channel();
        let (capture, playback, meters) = (ends.capture, ends.playback, ends.meters.clone());
        #[cfg(not(target_arch = "wasm32"))]
        let voice = {
            let (thread_board, thread_signals) = (board.clone(), signals.clone());
            let thread = std::thread::Builder::new()
                .name("rune-voice".into())
                .spawn(move || {
                    let device = device(capture, playback, thread_signals);
                    Control::new(device, meters, thread_board).run(&received);
                })
                .map_err(|e| VoiceError::OpenFailed(e.to_string()))?;
            Voice { signals, board, meters: ends.meters, sample_rate: spec.sample_rate, thread: Some(thread) }
        };
        #[cfg(target_arch = "wasm32")]
        let voice = {
            let device = device(capture, playback, signals.clone());
            inline::add(Control::new(device, meters, board.clone()), received);
            Voice { signals, board, meters: ends.meters, sample_rate: spec.sample_rate }
        };
        Ok((voice, ends.mic, ends.speaker))
    }

    /// The rate both streams run at, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Starts the microphone and speaker. Without microphone permission the system asks first,
    /// and the microphone hears silence until it is granted.
    pub fn start(&self) {
        self.meters.mark_start();
        self.send(Signal::Start);
    }

    /// Stops both streams. Queued speaker audio stays queued.
    pub fn stop(&self) {
        self.send(Signal::Stop);
    }

    /// Mutes the microphone: [`VoiceMic`] reads silence. The speaker keeps playing.
    pub fn set_muted(&self, muted: bool) {
        self.send(Signal::Mute(muted));
    }

    pub fn muted(&self) -> bool {
        self.meters.muted.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> VoiceState {
        self.board.state()
    }

    /// Everything that happened since the last call, oldest first.
    pub fn take_events(&self) -> Vec<VoiceEvent> {
        self.board.take_events()
    }

    /// Installs a waker called when the state changes or an event arrives, from another thread (on
    /// the web, from the page's main thread).
    pub fn set_waker(&self, waker: rune_core::Waker) {
        self.board.waker.set(waker);
        if let Some(generation) = self.board.pending() {
            self.board.waker.tell(generation);
        }
    }

    pub fn stats(&self) -> VoiceStats {
        self.meters.stats()
    }

    /// Microphone access now. Never asks.
    pub fn permission() -> MicPermission {
        backend::permission()
    }

    /// Asks for microphone access if it hasn't been decided, then calls `callback` with the
    /// answer, from any thread.
    pub fn request_permission(callback: impl FnOnce(MicPermission) + Send + 'static) {
        backend::request_permission(Box::new(callback));
    }

    fn send(&self, signal: Signal) {
        self.signals.send(signal).ok();
        #[cfg(target_arch = "wasm32")]
        inline::pump();
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.send(Signal::Close);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

#[cfg(test)]
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod duplex_tests;
#[cfg(test)]
mod io_tests;
#[cfg(test)]
mod machine_tests;
#[cfg(test)]
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod streams_tests;
#[cfg(test)]
mod tests;
