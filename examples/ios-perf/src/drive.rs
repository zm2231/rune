pub const ROW_HEIGHT: f32 = 60.0;

const DECAY_PER_SECOND: f64 = 2.002;
const FLING_SPEED: f64 = 6000.0;
const STOP_SPEED: f64 = 30.0;
const DIRECTIONS: [f64; 3] = [1.0, 1.0, -1.0];

pub fn fling_seconds() -> f64 { (FLING_SPEED / STOP_SPEED).ln() / DECAY_PER_SECOND }

pub fn total_seconds() -> f64 { fling_seconds() * DIRECTIONS.len() as f64 }

fn fling_distance(t: f64) -> f64 { FLING_SPEED / DECAY_PER_SECOND * (1.0 - (-DECAY_PER_SECOND * t).exp()) }

pub fn offset_at(t: f64) -> f64 {
    let each = fling_seconds();
    let t = t.clamp(0.0, total_seconds());
    let done = ((t / each) as usize).min(DIRECTIONS.len());
    let whole: f64 = DIRECTIONS[..done].iter().map(|d| d * fling_distance(each)).sum();
    let part = DIRECTIONS.get(done).map_or(0.0, |d| d * fling_distance(t - done as f64 * each));
    whole + part
}

pub fn glide_targets() -> Vec<f64> {
    let each = fling_seconds();
    (1..=DIRECTIONS.len()).map(|i| offset_at(i as f64 * each)).collect()
}
