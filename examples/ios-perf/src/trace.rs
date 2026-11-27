use std::sync::OnceLock;

use crate::probe;
use crate::stats::say;

/// Whether `IOS_PERF_TRACE` is set (to anything but `0`).
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("IOS_PERF_TRACE").is_some_and(|v| v != "0"))
}

/// With `IOS_PERF_TRACE` set, prints `PERF trace t_ms=… thread=… step=… …`: when a lifecycle step
/// ran, in milliseconds from the kernel's process start, and on which thread.
pub fn step(name: &str, detail: impl FnOnce() -> String) {
    if !on() { return; }
    let t = probe::since_start_ms(rune::time::system_now()).map_or_else(|| "nan".into(), |t| format!("{t:.1}"));
    let thread = std::thread::current();
    let detail = detail();
    let gap = if detail.is_empty() { "" } else { " " };
    say(format!("PERF trace t_ms={t} thread={} step={name}{gap}{detail}", thread.name().unwrap_or("other")));
}
