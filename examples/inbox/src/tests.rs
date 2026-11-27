use std::time::Duration;

use super::*;
use rune::{Env, RevealRequest, Runtime};

fn arrive(s: &Inbox, ids: std::ops::Range<u32>) {
    for id in ids {
        s.sender.send(Message::Arrived { id, subject: format!("Message {id}") }).unwrap();
    }
}

fn headless() -> Runtime<Inbox> {
    App::new(spec()).init(Inbox::new).headless_runtime((360.0, 320.0), Env::new(true, 1.0))
}

fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s)
}

#[test]
fn a_burst_drained_in_one_poll_rebuilds_once() {
    let mut s = Inbox::new();
    arrive(&s, 0..50);
    let mut tx = Tx::at(1.5);
    assert!(poll(&mut s, 1.5, &mut tx));
    assert_eq!(s.rebuilds, 1);
    assert_eq!(s.rows.len(), 50);
    assert_eq!(tx.requested().wakes().collect::<Vec<_>>(), [secs(0.5)], "only the next whole-second tick");
}

#[test]
fn arrivals_inside_the_rebuild_window_ask_one_wake_for_its_end() {
    let mut s = Inbox::new();
    arrive(&s, 0..1);
    poll(&mut s, 1.0, &mut Tx::at(1.0));

    arrive(&s, 1..10);
    let mut tx = Tx::at(1.05);
    assert!(poll(&mut s, 1.05, &mut tx));
    assert_eq!((s.rebuilds, s.rows.len()), (1, 1));
    assert_eq!(tx.requested().next_wake().map(|w| w.as_secs_f64()).map(|w| (w * 1e6).round()), Some(50_000.0));

    arrive(&s, 10..12);
    let mut tx = Tx::at(1.08);
    assert!(poll(&mut s, 1.08, &mut tx));
    assert_eq!(tx.requested().wakes().count(), 1, "the rebuild's wake is already armed; only the tick asks");

    let mut tx = Tx::at(1.1);
    poll(&mut s, 1.1, &mut tx);
    assert_eq!((s.rebuilds, s.rows.len()), (2, 12));
}

#[test]
fn a_selection_from_the_inbox_asks_to_reveal_its_row_once() {
    let mut s = Inbox::new();
    arrive(&s, 0..60);
    s.sender.send(Message::Select(55)).unwrap();
    let mut tx = Tx::at(0.0);
    poll(&mut s, 0.0, &mut tx);
    let reveal = RevealRequest { scope: Some("list"), key: "row-55", center: false, animated: false };
    assert_eq!(tx.requested().reveals().collect::<Vec<_>>(), [reveal]);

    let mut tx = Tx::at(0.25);
    poll(&mut s, 0.25, &mut tx);
    assert_eq!(tx.requested().reveals().count(), 0);
}

#[test]
fn headless_runtime_ticks_once_a_second_after_a_busy_stretch() {
    let mut rt = headless();
    rt.frame(0.0);
    assert_eq!(rt.next_wake(), Some(secs(1.0)));

    for id in 0..20 {
        arrive(&rt.state, id * 3..id * 3 + 3);
        rt.frame(0.03);
    }
    rt.frame(0.2);
    assert_eq!(rt.state.rows.len(), 60);
    assert!(!rt.state.stale);

    rt.frame(1.0 - rt.clock());
    let mut ticks = Vec::new();
    while rt.clock() < 6.0 {
        let wake = rt.next_wake().unwrap();
        rt.frame(wake.as_secs_f64());
        ticks.push((rt.clock() * 1000.0).round() / 1000.0);
    }
    assert_eq!(ticks, [2.0, 3.0, 4.0, 5.0, 6.0], "one wake a second, however busy it was before");
}

#[test]
fn headless_runtime_scrolls_the_selection_into_view() {
    let mut rt = headless();
    arrive(&rt.state, 0..70);
    rt.frame(0.0);
    let (_, row_y) = rt.find("row-55").unwrap();
    assert!(row_y > 320.0, "row-55 starts below the window, centred at {row_y}");
    rt.state.sender.send(Message::Select(55)).unwrap();
    rt.frame(0.0);
    rt.frame(0.0);
    let (_, row_y) = rt.find("row-55").unwrap();
    assert!((16.0..=320.0 - 16.0).contains(&row_y), "row-55 centred at {row_y}");
}
