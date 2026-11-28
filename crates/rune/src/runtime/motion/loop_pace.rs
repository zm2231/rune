use super::*;

#[derive(Default)]
pub(in crate::runtime) struct LoopPace {
    period: Option<f64>,
    steered_until: f64,
}

impl tracks::MotionState {
    pub(in crate::runtime) fn set_ambient_period(&mut self, period: Option<std::time::Duration>) {
        self.loop_pace.period = period.map(|p| p.as_secs_f64()).filter(|p| *p > 0.0);
    }

    pub(in crate::runtime) fn note_steering_input(&mut self, clock: f64) {
        self.loop_pace.steered_until = clock + crate::frame::clock::STEERED_IDLE.as_secs_f64();
    }

    pub(in crate::runtime) fn paced(&self, dt: f64, clock: f64) -> f64 {
        match self.loop_pace.period {
            Some(period) if clock > self.loop_pace.steered_until => dt.max(period),
            _ => dt,
        }
    }
}
