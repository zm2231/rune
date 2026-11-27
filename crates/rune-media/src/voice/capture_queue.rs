use std::collections::VecDeque;

pub(crate) struct Packet<'a> {
    pub(crate) position: u64,
    pub(crate) timed: bool,
    pub(crate) audio: Audio<'a>,
}

pub(crate) enum Audio<'a> {
    Samples(&'a [f32]),
    Silent(usize),
}

impl Audio<'_> {
    fn frames(&self) -> usize {
        match self {
            Self::Samples(samples) => samples.len(),
            Self::Silent(frames) => *frames,
        }
    }
}

pub(crate) struct CaptureQueue {
    rate: u64,
    next: Option<u64>,
    accounted: u64,
    pending: VecDeque<f32>,
    limit: usize,
    lost: u64,
}

impl CaptureQueue {
    pub(crate) fn new(rate: u32) -> Self {
        let limit = 2 * rate as usize;
        Self { rate: u64::from(rate), next: None, accounted: 0, pending: VecDeque::with_capacity(limit), limit, lost: 0 }
    }

    pub(crate) fn packet(&mut self, packet: &Packet<'_>) {
        let frames = packet.audio.frames() as u64;
        if packet.timed
            && let Some(next) = self.next
        {
            let gap = packet.position.saturating_sub(next);
            let fill = gap.min(self.rate);
            self.accounted += gap;
            self.lost += fill;
            self.push(std::iter::repeat_n(0.0, fill as usize));
        }
        self.accounted += frames;
        self.next = packet.timed.then(|| packet.position + frames);
        match packet.audio {
            Audio::Samples(samples) => self.push(samples.iter().copied()),
            Audio::Silent(frames) => self.push(std::iter::repeat_n(0.0, frames)),
        }
    }

    fn push(&mut self, samples: impl ExactSizeIterator<Item = f32>) {
        let overflow = (self.pending.len() + samples.len()).saturating_sub(self.limit);
        self.pending.drain(..overflow.min(self.pending.len()));
        self.pending.extend(samples);
    }

    pub(crate) fn read(&mut self, out: &mut [f32]) -> bool {
        if self.pending.len() < out.len() {
            return false;
        }
        let len = out.len();
        for (slot, sample) in out.iter_mut().zip(self.pending.drain(..len)) {
            *slot = sample;
        }
        true
    }

    pub(crate) fn take_lost(&mut self) -> u64 {
        std::mem::take(&mut self.lost)
    }

    pub(crate) fn ahead(&self) -> Option<i64> {
        let next = i64::try_from(self.next?).ok()?;
        Some(next.saturating_sub(i64::try_from(self.accounted).unwrap_or(i64::MAX)))
    }
}

#[cfg(test)]
#[path = "capture_queue_tests.rs"]
mod tests;
