//! Work deferred to the main run loop. AppKit UI that tracks in a nested loop (a pop-up menu,
//! the share picker) must not start inside a winit handler: winit panics if its handler
//! re-enters, and a tracking loop run there would hold every window's frames until it ended.
//! Started from a run-loop block instead, winit's observers (registered for the common modes,
//! which include the tracking mode) keep delivering redraws, display ticks and wakes.

#![allow(unsafe_code)]

use std::{cell::Cell, ffi::c_void};

use block2::{Block, RcBlock};

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopPerformBlock(run_loop: *mut c_void, mode: *const c_void, block: &Block<dyn Fn()>);
    fn CFRunLoopWakeUp(run_loop: *mut c_void);
}

/// Runs `f` on the main run loop after the current callout returns, in the default mode, as
/// winit queues its own deferred work. A panic in `f` is caught and reported as `what`'s.
pub(crate) fn run_later(what: &'static str, f: impl FnOnce() + 'static) {
    let f = Cell::new(Some(f));
    let block = RcBlock::new(move || {
        if let Some(f) = f.take() && std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
            eprintln!("rune: {what} panicked");
        }
    });
    // SAFETY: the main run loop is valid for the process, the mode is a CFStringRef, and
    // CFRunLoopPerformBlock copies the block.
    unsafe {
        let run_loop = CFRunLoopGetMain();
        CFRunLoopPerformBlock(run_loop, kCFRunLoopDefaultMode, &block);
        CFRunLoopWakeUp(run_loop);
    }
}
