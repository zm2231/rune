use rtrb::{Consumer, Producer, RingBuffer};
use rune_core::time::Instant;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub(crate) struct Meters {
    origin: Instant,
    pub(crate) muted: AtomicBool,
    discard_until: AtomicU64,
    captured: AtomicU64,
    played: AtomicU64,
    mic_dropped: AtomicU64,
    oversized: AtomicU64,
    late: AtomicU64,
    speaker_gap: AtomicU64,
    mic_gap: AtomicU64,
    started_at: AtomicU64,
    first_audio_at: AtomicU64,
}

impl Meters {
    fn new() -> Self {
        Self {
            origin: rune_core::time::now(),
            muted: AtomicBool::new(false),
            discard_until: AtomicU64::new(0),
            captured: AtomicU64::new(0),
            played: AtomicU64::new(0),
            mic_dropped: AtomicU64::new(0),
            oversized: AtomicU64::new(0),
            late: AtomicU64::new(0),
            speaker_gap: AtomicU64::new(0),
            mic_gap: AtomicU64::new(0),
            started_at: AtomicU64::new(0),
            first_audio_at: AtomicU64::new(0),
        }
    }

    fn since_origin(&self) -> u64 {
        let nanos = rune_core::time::now().saturating_duration_since(self.origin).as_nanos();
        u64::try_from(nanos).unwrap_or(u64::MAX).max(1)
    }

    pub(crate) fn mark_start(&self) {
        self.first_audio_at.store(0, Ordering::Relaxed);
        self.started_at.store(self.since_origin(), Ordering::Release);
    }

    fn mark_audio(&self) {
        if self.first_audio_at.load(Ordering::Relaxed) == 0 && self.started_at.load(Ordering::Acquire) != 0 {
            let now = self.since_origin();
            self.first_audio_at.compare_exchange(0, now, Ordering::Release, Ordering::Relaxed).ok();
        }
    }

    pub(crate) fn count_oversized(&self) {
        self.oversized.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) fn count_late(&self, frames: u64) {
        if frames > 0 {
            self.late.fetch_add(frames, Ordering::Relaxed);
        }
    }

    pub(crate) fn count_speaker_gap(&self, frames: u64) {
        if frames > 0 {
            self.speaker_gap.fetch_add(frames, Ordering::Relaxed);
        }
    }

    pub(crate) fn count_mic_gap(&self, frames: u64) {
        if frames > 0 {
            self.mic_gap.fetch_add(frames, Ordering::Relaxed);
        }
    }

    pub(crate) fn stats(&self) -> super::VoiceStats {
        let started = self.started_at.load(Ordering::Acquire);
        let first = self.first_audio_at.load(Ordering::Acquire);
        super::VoiceStats {
            captured_frames: self.captured.load(Ordering::Relaxed),
            played_frames: self.played.load(Ordering::Relaxed),
            mic_dropped_frames: self.mic_dropped.load(Ordering::Relaxed),
            oversized_callbacks: self.oversized.load(Ordering::Relaxed),
            late_frames: self.late.load(Ordering::Relaxed),
            speaker_gap_frames: self.speaker_gap.load(Ordering::Relaxed),
            mic_gap_frames: self.mic_gap.load(Ordering::Relaxed),
            start_to_audio: (started != 0 && first >= started).then(|| Duration::from_nanos(first - started)),
        }
    }
}

pub(crate) struct Capture {
    mic: Producer<f32>,
    pub(crate) meters: Arc<Meters>,
}

impl Capture {
    pub(crate) fn capture(&mut self, samples: &[f32]) {
        let meters = &*self.meters;
        meters.mark_audio();
        let dropped = if meters.muted.load(Ordering::Relaxed) {
            let room = self.mic.slots().min(samples.len());
            if let Ok(chunk) = self.mic.write_chunk_uninit(room) {
                chunk.fill_from_iter(std::iter::repeat(0.0));
            }
            samples.len() - room
        } else {
            self.mic.push_partial_slice(samples).1.len()
        };
        meters.captured.fetch_add(samples.len() as u64, Ordering::Relaxed);
        if dropped > 0 {
            meters.mic_dropped.fetch_add(dropped as u64, Ordering::Relaxed);
        }
    }
}

pub(crate) struct Playback {
    speaker: Consumer<f32>,
    read_total: u64,
    pub(crate) meters: Arc<Meters>,
}

impl Playback {
    pub(crate) fn render(&mut self, out: &mut [f32]) {
        let meters = &*self.meters;
        meters.mark_audio();
        let until = meters.discard_until.load(Ordering::Acquire);
        if until > self.read_total {
            let skip = usize::try_from(until - self.read_total).unwrap_or(usize::MAX).min(self.speaker.slots());
            if let Ok(chunk) = self.speaker.read_chunk(skip) {
                chunk.commit_all();
            }
            self.read_total += skip as u64;
        }
        let (played, silent) = self.speaker.pop_partial_slice(out);
        let n = played.len();
        silent.fill(0.0);
        self.read_total += n as u64;
        meters.played.fetch_add(n as u64, Ordering::Relaxed);
    }
}

/// The app's end of the microphone: echo-cancelled mono `f32` frames at the stream's rate.
/// One reader; it is `Send`, so it can move to the thread that encodes or sends audio.
pub struct VoiceMic {
    mic: Consumer<f32>,
    meters: Arc<Meters>,
}

impl VoiceMic {
    /// Moves up to `out.len()` captured frames into `out`, oldest first, and returns how many.
    /// Never blocks.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        self.mic.pop_partial_slice(out).0.len()
    }

    /// Frames captured and not yet read.
    pub fn available(&self) -> usize {
        self.mic.slots()
    }

    /// Frames the microphone captured that were dropped because the app didn't read them in
    /// time (the mic buffer was full).
    pub fn dropped_frames(&self) -> u64 {
        self.meters.mic_dropped.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for VoiceMic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceMic").field("available", &self.available()).finish()
    }
}

/// The app's end of the speaker: mono `f32` frames at the stream's rate, played in order. Audio
/// written here is what echo cancellation removes from the microphone. One writer; it is `Send`.
pub struct VoiceSpeaker {
    speaker: Producer<f32>,
    written_total: u64,
    meters: Arc<Meters>,
}

impl VoiceSpeaker {
    /// Queues as many of `frames` as fit and returns how many. Never blocks; when it returns less
    /// than `frames.len()`, write the rest once [`free`](Self::free) has room.
    pub fn write(&mut self, frames: &[f32]) -> usize {
        let n = self.speaker.push_partial_slice(frames).0.len();
        self.written_total += n as u64;
        n
    }

    /// Frames that can be written now.
    pub fn free(&self) -> usize {
        self.speaker.slots()
    }

    /// Frames written and not yet played.
    pub fn queued(&self) -> usize {
        self.speaker.buffer().capacity() - self.speaker.slots()
    }

    /// Drops everything written so far and not yet played, on the next audio callback. Frames
    /// written after `clear` play as usual. For barge-in: when the person talks over a reply,
    /// clear it and read [`played_frames`](Self::played_frames) to know how much was heard.
    pub fn clear(&mut self) {
        self.meters.discard_until.store(self.written_total, Ordering::Release);
    }

    /// Frames sent to the speaker since the stream opened, cleared frames excluded.
    pub fn played_frames(&self) -> u64 {
        self.meters.played.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for VoiceSpeaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceSpeaker").field("queued", &self.queued()).finish()
    }
}

pub(crate) struct Ends {
    pub(crate) capture: Capture,
    pub(crate) playback: Playback,
    pub(crate) mic: VoiceMic,
    pub(crate) speaker: VoiceSpeaker,
    pub(crate) meters: Arc<Meters>,
}

pub(crate) fn ends(mic_frames: usize, speaker_frames: usize) -> Ends {
    let meters = Arc::new(Meters::new());
    let (mic_tx, mic_rx) = RingBuffer::new(mic_frames.max(1));
    let (speaker_tx, speaker_rx) = RingBuffer::new(speaker_frames.max(1));
    Ends {
        capture: Capture { mic: mic_tx, meters: meters.clone() },
        playback: Playback { speaker: speaker_rx, read_total: 0, meters: meters.clone() },
        mic: VoiceMic { mic: mic_rx, meters: meters.clone() },
        speaker: VoiceSpeaker { speaker: speaker_tx, written_total: 0, meters: meters.clone() },
        meters,
    }
}
