use std::cell::Cell;
use std::sync::Mutex;
use std::time::Duration;

use super::heap_trim::{
    Allocator, PROBE_MISSES, PROBE_SIZE, TRIM_MIN_GAP, TrimEffect, TrimEvent, TrimState,
    env_allows_trim, probe_glibc_used, record_probe, set_trim_enabled, step, trim_enabled,
    uses_glibc_allocator,
};
use super::rest::{self, REST_AFTER, Rest, RestEffect, RestEvent};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::heap_trim::glibc::Trimmer;

static OPT_OUT_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn in_own_process(test: &str, body: impl FnOnce()) {
    const CHILD: &str = "RUNE_HEAP_TRIM_CHILD";
    if std::env::var_os(CHILD).is_some() {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([&format!("window::state::heap_trim_tests::{test}"), "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{test} failed in its own process:\n{stdout}\n{stderr}"
    );
}

#[test]
fn trim_state_first_rest_trims_concurrent_coalesces_and_respects_30s_cooldown() {
    let t0 = crate::time::now();
    let mut state = TrimState::default();

    // First rest triggers one trim.
    let eff: Vec<_> = step(&mut state, TrimEvent::Rested { now: t0 }).into_iter().collect();
    assert_eq!(eff, vec![TrimEffect::Trim]);
    assert_eq!(state, TrimState::Running);

    // Multiple windows / overlays resting while already running coalesce into zero extra trims.
    let eff2 = step(&mut state, TrimEvent::Rested { now: t0 + Duration::from_millis(5) });
    assert!(eff2.is_empty());
    let eff3 = step(&mut state, TrimEvent::Rested { now: t0 + Duration::from_secs(60) });
    assert!(eff3.is_empty());

    // Completion returns to Idle with last_done stamped.
    let t_done = t0 + Duration::from_millis(10);
    let eff_fin = step(&mut state, TrimEvent::Finished { now: t_done });
    assert!(eff_fin.is_empty());
    assert_eq!(state, TrimState::Idle { last_done: Some(t_done) });

    // Straggler Finished while Idle is ignored.
    assert!(step(&mut state, TrimEvent::Finished { now: t_done + Duration::from_secs(1) }).is_empty());

    // Repeat rest inside 30 s cooldown skips.
    let just_before = t_done + TRIM_MIN_GAP - Duration::from_millis(1);
    assert!(step(&mut state, TrimEvent::Rested { now: just_before }).is_empty());
    assert_eq!(state, TrimState::Idle { last_done: Some(t_done) });

    // Repeat rest at/after 30 s trims again.
    let at_boundary = t_done + TRIM_MIN_GAP;
    assert_eq!(
        step(&mut state, TrimEvent::Rested { now: at_boundary }).into_iter().collect::<Vec<_>>(),
        vec![TrimEffect::Trim]
    );
    assert_eq!(state, TrimState::Running);
}

#[test]
fn rest_triggers_trim_once_and_arms_no_periodic_idle_timer() {
    let t0 = crate::time::now();
    let mut rest_state = Rest::default();
    let mut trim_state = TrimState::default();

    // Frame drawn -> deadline set to t0 + REST_AFTER.
    assert!(rest::step(&mut rest_state, RestEvent::Drew { now: t0, low: false }).is_empty());
    assert_eq!(rest_state.deadline(), Some(t0 + REST_AFTER));

    // At REST_AFTER, Rest fires GiveBackUnused once -> TrimState triggers Trim once.
    let rest_eff: Vec<_> = rest::step(&mut rest_state, RestEvent::Woke { now: t0 + REST_AFTER }).into_iter().collect();
    assert_eq!(rest_eff, vec![RestEffect::GiveBackUnused]);
    assert_eq!(rest_state.deadline(), None, "idle window must arm no periodic timer");

    let mut trims = 0usize;
    for _ in rest_eff {
        if !step(&mut trim_state, TrimEvent::Rested { now: t0 + REST_AFTER }).is_empty() {
            trims += 1;
            let _ = step(
                &mut trim_state,
                TrimEvent::Finished { now: t0 + REST_AFTER + Duration::from_millis(4) },
            );
        }
    }
    assert_eq!(trims, 1);

    // Even minutes later while idle, waking Rest does nothing and triggers zero additional trims.
    for mins in 1..=5 {
        let later = t0 + REST_AFTER + Duration::from_secs(mins * 60);
        let eff = rest::step(&mut rest_state, RestEvent::Woke { now: later });
        assert!(eff.is_empty());
        assert_eq!(rest_state.deadline(), None);
    }
    assert_eq!(trims, 1);
}

#[test]
fn env_override_and_app_builder_opt_out_toggle_trim_enabled() {
    assert!(env_allows_trim(None));
    assert!(env_allows_trim(Some("")));
    assert!(env_allows_trim(Some("1")));
    assert!(env_allows_trim(Some("true")));
    assert!(!env_allows_trim(Some("0")));
    assert!(!env_allows_trim(Some(" 0 ")));
    assert!(!env_allows_trim(Some("false")));
    assert!(!env_allows_trim(Some("OFF")));
    assert!(!env_allows_trim(Some("no")));

    let _guard = OPT_OUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let spec = crate::AppSpec::new("TrimOptOut", (320.0, 200.0), || (), |_, _| crate::el());
    let _app = crate::App::new(spec).trim_heap_at_rest(false);
    assert!(!trim_enabled());

    let spec2 = crate::AppSpec::new("TrimOptIn", (320.0, 200.0), || (), |_, _| crate::el());
    let _app2 = crate::App::new(spec2).trim_heap_at_rest(true);
    assert!(trim_enabled());
    set_trim_enabled(true);
}

#[test]
fn allocator_probe_detects_glibc_rejects_foreign_allocators_and_handles_concurrent_free_and_wrap() {
    let mut backing = vec![0u8; PROBE_SIZE + 64];
    let used = Cell::new(1024usize);

    // 1. Glibc-backed allocator: used rises by PROBE_SIZE on alloc and drops on dealloc.
    let ptr = backing.as_mut_ptr();
    assert!(probe_glibc_used(
        usize::MAX,
        || used.get(),
        |l| {
            used.set(used.get() + l.size() + 16);
            ptr
        },
        |_, l| {
            used.set(used.get() - (l.size() + 16));
        },
    ));

    // 2. Foreign / custom / LD_PRELOAD allocator: glibc used bytes never move -> false.
    let foreign_used = Cell::new(4096usize);
    assert!(!probe_glibc_used(
        usize::MAX,
        || foreign_used.get(),
        |_| ptr,
        |_, _| {},
    ));

    // 3. Concurrent C free spoils attempt 1, attempt 2 succeeds.
    let attempt = Cell::new(0usize);
    let noisy_used = Cell::new(200_000usize);
    assert!(probe_glibc_used(
        usize::MAX,
        || noisy_used.get(),
        |l| {
            let n = attempt.get();
            attempt.set(n + 1);
            if n == 0 {
                // Concurrent free larger than PROBE_SIZE hides the growth on attempt 1.
                noisy_used.set(noisy_used.get() - 65_536);
            } else {
                noisy_used.set(noisy_used.get() + l.size() + 16);
            }
            ptr
        },
        |_, l| {
            noisy_used.set(noisy_used.get().saturating_sub(l.size() + 16));
        },
    ));

    // 4. Legacy 32-bit mallinfo wrap across u32::MAX boundary still detects exact delta.
    let wrap_used = Cell::new((u32::MAX as usize) - 1024);
    assert!(probe_glibc_used(
        u32::MAX as usize,
        || wrap_used.get(),
        |l| {
            wrap_used.set(wrap_used.get().wrapping_add(l.size() + 16) & (u32::MAX as usize));
            ptr
        },
        |_, l| {
            wrap_used.set(wrap_used.get().wrapping_sub(l.size() + 16) & (u32::MAX as usize));
        },
    ));
}

#[test]
fn platform_guard_matches_target_abi_and_coalesces_live_trim() {
    let _guard = OPT_OUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_trim_enabled(true);

    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        use super::heap_trim::{maybe_trim_heap, test_support};
        assert!(!uses_glibc_allocator());
        maybe_trim_heap();
        assert_eq!(test_support::trim_calls(), 0);
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    in_own_process("platform_guard_matches_target_abi_and_coalesces_live_trim", || {
        assert!(
            (0..PROBE_MISSES).any(|_| uses_glibc_allocator()),
            "default Linux/glibc unit test binary should detect glibc allocator"
        );
        let trimmer: &'static Trimmer = Box::leak(Box::new(Trimmer::new()));

        // Trigger two rests back-to-back: must coalesce into a single background trim.
        trimmer.rest();
        trimmer.rest();
        trimmer.wait_idle();
        assert_eq!(trimmer.calls(), 1);

        // Another rest immediately inside the 30 s cooldown must be skipped.
        trimmer.rest();
        trimmer.wait_idle();
        assert_eq!(trimmer.calls(), 1);

        // Opting out prevents trimming even after resetting cooldown.
        set_trim_enabled(false);
        trimmer.reset_cooldown();
        trimmer.rest();
        trimmer.wait_idle();
        assert_eq!(trimmer.calls(), 1);
        set_trim_enabled(true);
    });
}

#[test]
fn one_hit_settles_glibc_and_only_consecutive_missed_rests_settle_foreign() {
    let mut state = Allocator::Unknown { misses: 0 };
    for n in 1..PROBE_MISSES {
        record_probe(&mut state, false);
        assert_eq!(state, Allocator::Unknown { misses: n });
    }
    record_probe(&mut state, true);
    assert_eq!(state, Allocator::Glibc, "a hit after misses still settles glibc");
    record_probe(&mut state, false);
    assert_eq!(state, Allocator::Glibc);

    let mut state = Allocator::Unknown { misses: 0 };
    for _ in 0..PROBE_MISSES {
        record_probe(&mut state, false);
    }
    assert_eq!(state, Allocator::Foreign);
    record_probe(&mut state, true);
    assert_eq!(state, Allocator::Foreign);
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn another_threads_free_inside_every_attempt_misses_a_rest_and_the_next_rest_hits() {
    in_own_process("another_threads_free_inside_every_attempt_misses_a_rest_and_the_next_rest_hits", another_threads_free_body);
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(unsafe_code)]
fn another_threads_free_body() {
    use std::alloc::Layout;
    use std::sync::mpsc;

    let symbols = super::heap_trim::glibc::symbols().expect("glibc exports malloc_trim and mallinfo");
    let big = 2 * PROBE_SIZE;
    let (cue, cued) = mpsc::channel::<()>();
    let (freed, wait_freed) = mpsc::channel::<()>();
    let attempts = super::heap_trim::PROBE_ATTEMPTS;
    let other = std::thread::spawn(move || {
        let held: Vec<Vec<u8>> = (0..attempts).map(|_| vec![1u8; big]).collect();
        freed.send(()).unwrap();
        for block in held {
            if cued.recv().is_err() {
                return;
            }
            drop(block);
            freed.send(()).unwrap();
        }
    });
    wait_freed.recv().unwrap();

    let alloc = |l: Layout| {
        // SAFETY: the probe layout has non-zero size and valid alignment.
        unsafe { std::alloc::alloc(l) }
    };
    let dealloc = |p: *mut u8, l: Layout| {
        // SAFETY: `p` came from `alloc(l)`.
        unsafe { std::alloc::dealloc(p, l) }
    };
    let samples = Cell::new(0usize);
    let spoiled = probe_glibc_used(
        symbols.mask(),
        || {
            let n = samples.get();
            samples.set(n + 1);
            if n % 3 == 1 {
                cue.send(()).unwrap();
                wait_freed.recv().unwrap();
            }
            symbols.used()
        },
        alloc,
        dealloc,
    );
    drop(cue);
    other.join().unwrap();
    assert!(!spoiled, "a {} KiB free on another thread hides the probe's growth", big / 1024);
    assert_eq!(samples.get(), 3 * attempts);

    let mut state = Allocator::Unknown { misses: 0 };
    record_probe(&mut state, spoiled);
    assert_eq!(state, Allocator::Unknown { misses: 1 }, "a missed rest settles nothing");
    for _ in 1..PROBE_MISSES {
        if state == Allocator::Glibc {
            break;
        }
        record_probe(&mut state, probe_glibc_used(symbols.mask(), || symbols.used(), alloc, dealloc));
    }
    assert_eq!(state, Allocator::Glibc, "a later rest without the other thread's free settles glibc");
}
