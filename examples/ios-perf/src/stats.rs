pub const PERIOD_MS: f64 = 1000.0 / 120.0;
pub const MISS_MS: f64 = PERIOD_MS * 1.5;

pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return f64::NAN; }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

pub fn sorted(values: &[f64]) -> Vec<f64> {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v
}

pub fn intervals_ms(times_s: &[f64]) -> Vec<f64> {
    times_s.windows(2).map(|w| (w[1] - w[0]) * 1000.0).collect()
}

pub fn scroll_line(mode: &str, source: &str, times_s: &[f64], extra: &str) -> String {
    let gaps = intervals_ms(times_s);
    let missed = gaps.iter().filter(|g| **g > MISS_MS).count();
    let dropped: u64 = gaps.iter().filter(|g| **g > MISS_MS).map(|g| ((g / PERIOD_MS).round() as u64).saturating_sub(1).max(1)).sum();
    let s = sorted(&gaps);
    format!(
        "PERF scroll mode={mode} source={source} frames={} missed={missed} dropped={dropped} p50_ms={:.2} p99_ms={:.2} max_ms={:.2}{extra}",
        times_s.len(), percentile(&s, 50.0), percentile(&s, 99.0), s.last().copied().unwrap_or(f64::NAN),
    )
}

pub fn spread(name: &str, values: &[f64]) -> String {
    let s = sorted(values);
    format!(" {name}_p50={:.2} {name}_p99={:.2} {name}_max={:.2}", percentile(&s, 50.0), percentile(&s, 99.0), s.last().copied().unwrap_or(f64::NAN))
}

pub fn say(line: impl AsRef<str>) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", line.as_ref()).and_then(|()| out.flush()).ok();
}
