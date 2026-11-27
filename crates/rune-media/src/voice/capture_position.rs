pub(crate) struct Packet {
    pub(crate) position: u64,
    pub(crate) frames: u64,
    pub(crate) discontinuity: bool,
    pub(crate) timed: bool,
}

pub(crate) struct CapturePosition {
    rate: u64,
    next: Option<u64>,
    accounted: u64,
}

impl CapturePosition {
    pub(crate) fn new(rate: u32) -> Self {
        Self { rate: u64::from(rate), next: None, accounted: 0 }
    }

    pub(crate) fn packet(&mut self, packet: &Packet) -> u64 {
        let mut fill = 0;
        if packet.discontinuity
            && packet.timed
            && let Some(next) = self.next
        {
            let gap = packet.position.saturating_sub(next);
            fill = gap.min(self.rate);
            self.accounted += gap;
        }
        self.accounted += packet.frames;
        self.next = packet.timed.then(|| packet.position + packet.frames);
        fill
    }

    pub(crate) fn ahead(&self) -> Option<i64> {
        let next = i64::try_from(self.next?).ok()?;
        Some(next.saturating_sub(i64::try_from(self.accounted).unwrap_or(i64::MAX)))
    }
}

#[cfg(test)]
#[path = "capture_position_tests.rs"]
mod tests;
