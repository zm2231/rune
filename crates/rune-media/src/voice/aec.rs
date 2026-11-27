use sonora::config::{EchoCanceller, NoiseSuppression};
use sonora::config::EchoCanceller3Config;
use sonora::{AudioProcessing, Config, StreamConfig};
use super::io::{CancellerReading, Meters};
use std::sync::Arc;

const DELAY_FILTERS: usize = 10;
const BLOCKS_PER_REPORT: u32 = 100;

pub(crate) struct Aec {
    apm: AudioProcessing,
    render: Block,
    capture: Block,
    out: Box<[f32]>,
    lead: Lead,
    meters: Arc<Meters>,
}

#[derive(Default)]
struct Lead {
    rendered: u64,
    captured: u64,
    low: i64,
    high: i64,
    blocks: u32,
}

impl Lead {
    fn capture(&mut self, frames: usize) -> Option<(i64, i64)> {
        self.captured += frames as u64;
        let lead = i64::try_from(self.rendered).unwrap_or(i64::MAX) - i64::try_from(self.captured).unwrap_or(i64::MAX);
        if self.blocks == 0 {
            (self.low, self.high) = (lead, lead);
        }
        (self.low, self.high) = (self.low.min(lead), self.high.max(lead));
        self.blocks += 1;
        if self.blocks < BLOCKS_PER_REPORT {
            return None;
        }
        self.blocks = 0;
        Some((self.low, self.high))
    }
}

struct Block {
    frames: Box<[f32]>,
    len: usize,
}

impl Block {
    fn new(len: usize) -> Self {
        Self { frames: vec![0.0; len].into_boxed_slice(), len: 0 }
    }

    fn fill<'a>(&mut self, samples: &'a [f32]) -> (bool, &'a [f32]) {
        let take = (self.frames.len() - self.len).min(samples.len());
        self.frames[self.len..self.len + take].copy_from_slice(&samples[..take]);
        self.len += take;
        (self.len == self.frames.len(), &samples[take..])
    }
}

fn canceller() -> EchoCanceller3Config {
    let mut config = EchoCanceller3Config::default();
    config.delay.num_filters = DELAY_FILTERS;
    config
}

impl Aec {
    pub(crate) fn new(rate: u32, meters: Arc<Meters>) -> Self {
        let config = Config {
            echo_canceller: Some(EchoCanceller::default()),
            noise_suppression: Some(NoiseSuppression::default()),
            gain_controller2: None,
            ..Default::default()
        };
        let stream = StreamConfig::new(rate, 1);
        let frames = stream.num_frames();
        Self {
            apm: AudioProcessing::builder().config(config).echo_canceller3_config(canceller()).capture_config(stream).render_config(stream).build(),
            render: Block::new(frames),
            capture: Block::new(frames),
            out: vec![0.0; frames].into_boxed_slice(),
            lead: Lead::default(),
            meters,
        }
    }

    pub(crate) fn render(&mut self, mut played: &[f32]) {
        self.lead.rendered += played.len() as u64;
        while !played.is_empty() {
            let (full, rest) = self.render.fill(played);
            played = rest;
            if full {
                self.apm.process_render_f32(&[&self.render.frames], &mut [&mut self.out]).ok();
                self.render.len = 0;
            }
        }
    }

    pub(crate) fn capture(&mut self, mut heard: &[f32], mut cleaned: impl FnMut(&[f32])) {
        while !heard.is_empty() {
            let (full, rest) = self.capture.fill(heard);
            heard = rest;
            if full {
                if self.apm.process_capture_f32(&[&self.capture.frames], &mut [&mut self.out]).is_err() {
                    self.out.fill(0.0);
                }
                self.capture.len = 0;
                if let Some(lead) = self.lead.capture(self.capture.frames.len()) {
                    self.report(lead);
                }
                cleaned(&self.out);
            }
        }
    }

    fn report(&self, lead: (i64, i64)) {
        let stats = self.apm.statistics();
        self.meters.canceller.publish(CancellerReading {
            erle_db: stats.echo_return_loss_enhancement.unwrap_or(0.0) as f32,
            erl_db: stats.echo_return_loss.unwrap_or(0.0) as f32,
            alignment_ms: stats.delay_ms.and_then(|ms| u32::try_from(ms).ok()).unwrap_or(0),
            lead,
        });
    }
}
