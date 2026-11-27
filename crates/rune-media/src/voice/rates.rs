use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction};

pub(crate) struct Rates {
    block: usize,
    mic: Option<Stage>,
    speaker: Option<Stage>,
}

struct Stage {
    resampler: Async<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
}

impl Stage {
    fn new(from: u32, to: u32, block: usize, fixed: FixedAsync) -> Option<Self> {
        let parameters =
            SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2).interpolation(SincInterpolationType::Linear);
        let resampler = Async::new_sinc(f64::from(to) / f64::from(from), 1.0, &parameters, block, 1, fixed).ok()?;
        let (input, output) = (vec![0.0; resampler.input_frames_max()], vec![0.0; resampler.output_frames_max()]);
        Some(Self { resampler, input, output })
    }

    fn run(&mut self, frames: usize) -> usize {
        let wanted = self.resampler.output_frames_next();
        let (Ok(input), Ok(mut output)) = (
            InterleavedSlice::new(&self.input[..frames], 1, frames),
            InterleavedSlice::new_mut(&mut self.output[..wanted], 1, wanted),
        ) else {
            return 0;
        };
        self.resampler.process_into_buffer(&input, &mut output, None).map_or(0, |(_, written)| written)
    }
}

impl Rates {
    pub(crate) fn new(device: u32, voice: u32, block: usize) -> Self {
        let (mic, speaker) = if device == voice {
            (None, None)
        } else {
            (Stage::new(device, voice, block, FixedAsync::Input), Stage::new(voice, device, block, FixedAsync::Output))
        };
        Self { block, mic, speaker }
    }

    pub(crate) fn block(&self) -> usize {
        self.block
    }

    pub(crate) fn hear(&mut self, heard: &[f32], deliver: impl FnOnce(&[f32])) {
        let Some(stage) = &mut self.mic else {
            deliver(heard);
            return;
        };
        stage.input[..heard.len()].copy_from_slice(heard);
        let written = stage.run(heard.len());
        deliver(&stage.output[..written]);
    }

    pub(crate) fn play(&mut self, out: &mut [f32], render: impl FnOnce(&mut [f32])) {
        let Some(stage) = &mut self.speaker else {
            render(out);
            return;
        };
        let needed = stage.resampler.input_frames_next();
        render(&mut stage.input[..needed]);
        let written = stage.run(needed);
        out[..written].copy_from_slice(&stage.output[..written]);
        out[written..].fill(0.0);
    }
}
