use super::control::{Control, Signal};
use rune_core::time::Instant;
use std::cell::{Cell, RefCell};
use std::sync::mpsc::Receiver;
use wasm_bindgen::closure::Closure;

struct Host {
    control: Control,
    signals: Receiver<Signal>,
}

thread_local! {
    static HOSTS: RefCell<Vec<Host>> = const { RefCell::new(Vec::new()) };
    static ARRIVING: RefCell<Vec<Host>> = const { RefCell::new(Vec::new()) };
    static AGAIN: Cell<bool> = const { Cell::new(false) };
    static TIMER: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub(crate) fn add(mut control: Control, signals: Receiver<Signal>) {
    control.prepare();
    ARRIVING.with_borrow_mut(|arriving| arriving.push(Host { control, signals }));
    pump();
}

pub(crate) fn pump() {
    HOSTS.with(|hosts| {
        let Ok(mut hosts) = hosts.try_borrow_mut() else {
            AGAIN.set(true);
            return;
        };
        let mut next: Option<Instant>;
        loop {
            AGAIN.set(false);
            hosts.extend(ARRIVING.with_borrow_mut(std::mem::take));
            next = None;
            hosts.retain_mut(|host| {
                if let Some(due) = host.control.drain(&host.signals) {
                    next = Some(next.map_or(due, |n| n.min(due)));
                }
                !host.control.closed()
            });
            if !AGAIN.get() && ARRIVING.with_borrow(Vec::is_empty) {
                break;
            }
        }
        if let Some(due) = next {
            schedule(due);
        }
    });
}

pub(crate) fn soon() {
    schedule(rune_core::time::now());
}

fn schedule(due: Instant) {
    if TIMER.get().is_some_and(|pending| pending <= due) {
        return;
    }
    TIMER.set(Some(due));
    let wait = due.saturating_duration_since(rune_core::time::now());
    let fire = Closure::once_into_js(|| {
        TIMER.set(None);
        pump();
    });
    super::web::wake_after(wait.as_secs_f64() * 1_000.0, &fire);
}
