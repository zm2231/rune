use std::any::Any;
use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

thread_local! {
    static ENTRY: Cell<Option<&'static str>> = const { Cell::new(None) };
    static REPORTED: Cell<bool> = const { Cell::new(false) };
}

#[cfg_attr(not(any(target_os = "ios", target_os = "macos")), allow(dead_code))]
pub(crate) fn guarded<R>(entry: &'static str, body: impl FnOnce() -> R) -> R {
    report_panics();
    let outer = ENTRY.replace(Some(entry));
    let reported = REPORTED.replace(false);
    let result = panic::catch_unwind(AssertUnwindSafe(body));
    ENTRY.set(outer);
    match result {
        Ok(value) => {
            REPORTED.set(reported);
            value
        }
        Err(payload) => {
            if !REPORTED.get() {
                eprintln!("{}", report(entry, payload.as_ref(), None));
            }
            eprintln!("rune: ending the app, since a panic can't unwind through Objective-C");
            std::process::abort()
        }
    }
}

fn report_panics() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if let Some(entry) = ENTRY.get() {
                let location = info.location().map(ToString::to_string);
                eprintln!("{}", report(entry, info.payload(), location.as_deref()));
                REPORTED.set(true);
            }
            previous(info);
        }));
    });
}

fn report(entry: &str, payload: &(dyn Any + Send), location: Option<&str>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(a panic without a message)");
    format!("rune: a panic inside `{entry}` at {}: {message}", location.unwrap_or("an unknown location"))
}

#[cfg(test)]
#[path = "objc_boundary_tests.rs"]
mod tests;
