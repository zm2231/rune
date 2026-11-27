//! A background thread posts messages over a channel; `poll` drains them, rebuilds the list
//! at most once per [`REBUILD_SECS`] however many arrive, scrolls a selection a message names
//! into view, and once a second says how long ago the list was built. `src/tests.rs` drives
//! this loop with no window: once by calling `poll` with a `Tx::at`, and once through a
//! headless runtime.
//!
//! Its wakes show the two ways to repeat one without piling them up: the rebuild asks
//! `wake_after` only when an arrival arms it, and the age tick asks `wake_at` on a fixed
//! schedule every run, which a repeat of the same deadline doesn't add to.
//!
//! ```sh
//! cargo run -p inbox
//! cargo test -p inbox
//! ```

#![forbid(unsafe_code)]

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use rune::tok::{self, space, type_};
use rune::{App, AppSpec, Cx, El, ScrollSpec, Tx, col, el, text};

const REBUILD_SECS: f64 = 0.1;
const TICK_SECS: f64 = 1.0;

enum Message {
    Arrived { id: u32, subject: String },
    Select(u32),
}

struct Inbox {
    sender: Sender<Message>,
    inbox: Receiver<Message>,
    arrived: Vec<(u32, String)>,
    rows: Vec<(u32, String)>,
    stale: bool,
    built_at: f64,
    rebuilds: u32,
    next_tick: f64,
    age: Option<u32>,
    selected: Option<u32>,
    revealed: Option<u32>,
}

impl Inbox {
    fn new() -> Inbox {
        let (sender, inbox) = channel();
        Inbox {
            sender,
            inbox,
            arrived: Vec::new(),
            rows: Vec::new(),
            stale: false,
            built_at: f64::NEG_INFINITY,
            rebuilds: 0,
            next_tick: 0.0,
            age: None,
            selected: None,
            revealed: None,
        }
    }
}

fn row_key(id: u32) -> String {
    format!("row-{id}")
}

fn rebuild(s: &mut Inbox, clock: f64) {
    s.rows.clone_from(&s.arrived);
    s.stale = false;
    s.built_at = clock;
    s.rebuilds += 1;
    s.age = Some(0);
}

fn poll(s: &mut Inbox, clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    let was_stale = s.stale;
    while let Ok(message) = s.inbox.try_recv() {
        changed = true;
        match message {
            Message::Arrived { id, subject } => {
                s.arrived.push((id, subject));
                s.stale = true;
            }
            Message::Select(id) => s.selected = Some(id),
        }
    }
    if s.stale && clock - s.built_at >= REBUILD_SECS {
        rebuild(s, clock);
    } else if s.stale && !was_stale {
        tx.wake_after(Duration::from_secs_f64(s.built_at + REBUILD_SECS - clock));
    }
    if clock >= s.next_tick {
        while s.next_tick <= clock {
            s.next_tick += TICK_SECS;
        }
        let age = s.built_at.is_finite().then(|| (clock - s.built_at) as u32);
        changed |= age != s.age;
        s.age = age;
    }
    tx.wake_at(s.next_tick);
    if s.revealed != s.selected
        && let Some(id) = s.selected
        && s.rows.iter().any(|(row, _)| *row == id)
    {
        tx.scroll_into_view_in("list", row_key(id));
        s.revealed = Some(id);
    }
    changed
}

fn view(s: &Inbox, cx: &mut Cx<Inbox>) -> El {
    let t = tok::theme(cx.system_dark());
    let rows = s.rows.iter().map(|(id, subject)| {
        let selected = s.selected == Some(*id);
        el().key(row_key(*id))
            .h(32.0)
            .px(space::S3)
            .center_y()
            .fill(if selected { t.press } else { t.bg })
            .child(text(subject.clone()).font(type_::BODY).color(t.text))
    });
    col().w_full().h_full().fill(t.bg)
        .child(text(match s.age {
            Some(age) => format!("{} messages, updated {age} s ago", s.rows.len()),
            None => format!("{} messages", s.rows.len()),
        }).key("count").font(type_::LABEL).color(t.text_secondary).padding(space::S3))
        .child(col().key("list").w_full().shrink().min_h(0.0).scroll(ScrollSpec::default()).children(rows))
}

fn feed(s: &mut Inbox, waker: rune::Waker) {
    let sender = s.sender.clone();
    std::thread::spawn(move || {
        for id in 0..200u32 {
            std::thread::sleep(Duration::from_millis(if id % 20 == 0 { 600 } else { 5 }));
            if sender.send(Message::Arrived { id, subject: format!("Message {id}") }).is_err() {
                return;
            }
            if id % 40 == 39 && sender.send(Message::Select(id / 2)).is_err() {
                return;
            }
            waker();
        }
    });
}

fn spec() -> AppSpec<Inbox> {
    AppSpec { poll: Some(poll), ..AppSpec::new("Inbox", (360.0, 320.0), Inbox::new, view) }
}

fn main() {
    App::new(AppSpec { setup: Some(feed), ..spec() }).run();
}

#[cfg(test)]
mod tests;
