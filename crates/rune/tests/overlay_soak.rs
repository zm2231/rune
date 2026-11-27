//! Shows and hides two overlays that give their surface back while hidden (a HUD and a live-text
//! plate, as Hex shows on every dictation) a few thousand times, and checks that the process's
//! memory stays flat and that every show draws on the one GPU device the first show made
//! (hex#1268: memory grew by gigabytes over an hour of dictation on a hybrid laptop). Prints the
//! memory every few hundred cycles. Opens overlays, so it only runs with RUNE_GUI_TESTS=1 in a
//! desktop session; `RUNE_SOAK_CYCLES` sets the count (2000 by default), and `RUNE_SOAK_ONLY=hud`
//! or `=plate` cycles just that overlay. On macOS each line also splits the footprint into the
//! counted Rust heap and the rest, with threads and open files, and `RUNE_SOAK_CENSUS=1` prints
//! `vmmap -summary` and the `heap` tool's largest classes after the warm-up and at the end.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use rune::*;

static STARTS: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "macos")]
#[global_allocator]
static ALLOC: procwatch::alloc::Counting = procwatch::alloc::Counting::system();

/// Memory may grow this much from the warmed-up baseline to the last cycle.
const ALLOWED_GROWTH_MB: f64 = 48.0;
/// Cycles before the baseline: allocators, caches and the driver settle.
const WARM_UP: usize = 100;

fn hud(_: &(), _: &mut Cx<()>) -> El { el().size(200.0, 40.0).fill(oklcha(0.2, 0.0, 0.0, 0.6)).radius(20.0) }
fn plate(_: &(), _: &mut Cx<()>) -> El {
    el().w_full().h_full().fill(oklcha(0.25, 0.0, 0.0, 0.8)).radius(12.0).child(text("live words"))
}

fn wait(limit: Duration, f: impl Fn() -> bool) -> bool {
    let start = rune::time::now();
    while !f() {
        if start.elapsed() > limit { return false; }
        std::thread::sleep(Duration::from_micros(200));
    }
    true
}

/// Shows `overlay` until it has presented a frame.
fn show<T>(name: &str, overlay: &OverlayHandle<T>) {
    let presents = overlay.surface_stats().totals.presents;
    overlay.show();
    assert!(wait(Duration::from_secs(5), || overlay.surface_stats().totals.presents > presents && overlay.surface_description().is_some()),
        "{name} presented nothing: {:?}", overlay.failure());
}

/// Hides `overlay` until its surface is gone.
fn hide<T>(name: &str, overlay: &OverlayHandle<T>) {
    overlay.hide();
    assert!(wait(Duration::from_secs(5), || overlay.surface_description().is_none()), "{name} kept its surface");
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn memory_mb() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct Counters { cb: u32, faults: u32, peak_ws: usize, ws: usize, qpp: usize, qp: usize, qnpp: usize, qnp: usize, pagefile: usize, peak_pagefile: usize, private: usize }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
    // SAFETY: fills `c`, whose `cb` names PROCESS_MEMORY_COUNTERS_EX.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.private as f64 / 1e6
}

/// Resident memory, from `/proc/self/statm` (pages).
#[cfg(target_os = "linux")]
fn memory_mb() -> f64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: f64 = statm.split_whitespace().nth(1).and_then(|p| p.parse().ok()).unwrap_or(0.0);
    pages * 4096.0 / 1e6
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn memory_mb() -> f64 {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: `RUSAGE_INFO_V2` fills a `rusage_info_v2`, which `info` has room for.
    if unsafe { libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V2, info.as_mut_ptr().cast()) } != 0 { return 0.0; }
    // SAFETY: all-zero integers are a valid `rusage_info_v2`, and the call above filled it.
    unsafe { info.assume_init() }.ri_phys_footprint as f64 / 1e6
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn memory_mb() -> f64 { 0.0 }

#[cfg(target_os = "macos")]
fn breakdown() -> String {
    let Some(s) = procwatch::snapshot() else { return String::new() };
    let heap = s.heap.as_ref().map_or(0, |h| h.live);
    format!(" (Rust heap {:.1} MB, rest {:.1} MB, {} threads, {} open files)",
        heap as f64 / 1e6, s.unaccounted().unwrap_or(0) as f64 / 1e6, s.threads.len(), s.fds)
}

#[cfg(not(target_os = "macos"))]
fn breakdown() -> String { String::new() }

#[cfg(target_os = "macos")]
const TRACKED: [&str; 8] = ["Layer", "View", "Window", "Panel", "Drawable", "IOSurface", "KeyValue", "MTL"];

#[cfg(target_os = "macos")]
fn census(when: &str) {
    if std::env::var_os("RUNE_SOAK_CENSUS").is_none() { return; }
    let pid = std::process::id().to_string();
    for (tool, args, lines) in [("vmmap", vec!["-summary", pid.as_str()], usize::MAX), ("heap", vec![pid.as_str()], 80)] {
        match std::process::Command::new(tool).args(&args).output() {
            Ok(out) => {
                println!("--- {tool} {when} ---");
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines().take(lines) { println!("{line}"); }
                if tool == "heap" {
                    println!("--- heap {when}: layers, views, windows, drawables, observers ---");
                    for line in text.lines().filter(|line| TRACKED.iter().any(|class| line.contains(class))) { println!("{line}"); }
                }
            }
            Err(error) => println!("--- {tool} {when}: {error} ---"),
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn census(_: &str) {}

fn main() {
    if std::env::var_os("RUNE_GUI_TESTS").is_none() {
        println!("overlay_soak: skipped (set RUNE_GUI_TESTS=1 in a desktop session)");
        return;
    }
    let only = rune::env::var("RUNE_SOAK_ONLY");
    let only: Option<&'static str> = match only.as_deref() {
        None => None,
        Some("hud") => Some("hud"),
        Some("plate") => Some("plate"),
        Some(other) => panic!("RUNE_SOAK_ONLY is hud or plate, not {other}"),
    };
    let cycles: usize = rune::env::var("RUNE_SOAK_CYCLES").and_then(|n| n.parse().ok()).unwrap_or(2000).max(WARM_UP + 1);
    std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(3600)); eprintln!("timeout"); std::process::exit(2) });
    rune::orderly_exit();
    on_gpu(|event| if event == GpuEvent::Starting { STARTS.fetch_add(1, Ordering::SeqCst); });
    let mut app = App::new(AppSpec::new("overlay soak", (100.0, 100.0), || (), hud)).args(Vec::new()).windowless();
    let hud = app.overlay(OverlaySpec::new("soak hud", (200.0, 40.0), hud)
        .options(OverlayOptions { prewarm: true, ..Default::default() })
        .release_gpu_when_hidden(), ());
    let plate = app.overlay(OverlaySpec::new("soak live text", (420.0, 44.0), plate)
        .options(OverlayOptions { placement: Some(OverlayPlacement::Edge { edge: ScreenEdge::Top, margin: 60.0, align: Align::Center, inset: 0.0 }), ..Default::default() })
        .release_gpu_when_hidden(), ());
    let handle = app.handle();
    let result = Arc::new(Mutex::new(None));
    let out = result.clone();
    std::thread::spawn(move || {
        let run = std::panic::catch_unwind(move || {
            let started = rune::time::now();
            let mut baseline = 0.0;
            for cycle in 0..cycles {
                if only != Some("plate") { show("hud", &hud); }
                if only != Some("hud") { show("live text", &plate); }
                if only != Some("hud") { hide("live text", &plate); }
                if only != Some("plate") { hide("hud", &hud); }
                if cycle + 1 == WARM_UP {
                    baseline = memory_mb();
                    census("after the warm-up");
                }
                if (cycle + 1) % 250 == 0 {
                    println!("cycle {:>5}: {:.0} MB{}, {} GPU device(s) made, {:.0} s", cycle + 1, memory_mb(), breakdown(), STARTS.load(Ordering::SeqCst), started.elapsed().as_secs_f64());
                }
            }
            census("at the end");
            let end = memory_mb();
            let starts = STARTS.load(Ordering::SeqCst);
            println!("overlay_soak: {cycles} cycles, {baseline:.0} MB after {WARM_UP}, {end:.0} MB at the end, {starts} GPU device(s) made");
            assert!(hud.failure().is_none() && plate.failure().is_none(), "{:?} {:?}", hud.failure(), plate.failure());
            assert_eq!(starts, 1, "every show draws on the first show's device");
            assert!(end - baseline < ALLOWED_GROWTH_MB, "memory grew {:.0} MB over {} cycles", end - baseline, cycles - WARM_UP);
        });
        if run.is_err() { eprintln!("overlay_soak: failed"); std::process::exit(1); }
        *out.lock().unwrap() = Some(run.is_ok());
        handle.quit();
    });
    app.run();
    assert_eq!(*result.lock().unwrap(), Some(true), "overlay_soak failed");
    println!("overlay_soak: ok");
}
