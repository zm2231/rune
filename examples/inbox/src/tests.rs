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

#[test]
fn a_burst_drained_in_one_poll_rebuilds_once() {
    let mut s = Inbox::new();
    arrive(&s, 0..50);
    let mut tx = Tx::at(1.0);
    assert!(poll(&mut s, 1.0, &mut tx));
    assert_eq!(s.rebuilds, 1);
    assert_eq!(s.rows.len(), 50);
    assert_eq!(tx.requested().next_wake(), Some(IDLE_WAKE));
}

#[test]
fn arrivals_inside_the_rebuild_window_wait_and_wake_fast() {
    let mut s = Inbox::new();
    arrive(&s, 0..1);
    poll(&mut s, 1.0, &mut Tx::at(1.0));

    arrive(&s, 1..10);
    let mut tx = Tx::at(1.05);
    assert!(poll(&mut s, 1.05, &mut tx));
    assert_eq!((s.rebuilds, s.rows.len()), (1, 1));
    assert_eq!(tx.requested().next_wake(), Some(BUSY_WAKE));

    let mut tx = Tx::at(1.1);
    assert!(!poll(&mut s, 1.1, &mut tx));
    assert_eq!((s.rebuilds, s.rows.len()), (2, 10));
    assert_eq!(tx.requested().next_wake(), Some(IDLE_WAKE));
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
fn headless_runtime_keeps_the_poll_cadence_and_scrolls_the_selection_into_view() {
    let mut rt = headless();
    rt.frame(0.0);
    assert_eq!(rt.next_wake(), Some(IDLE_WAKE));

    arrive(&rt.state, 0..60);
    rt.frame(0.25);
    assert_eq!((rt.state.rebuilds, rt.state.rows.len()), (1, 60));

    arrive(&rt.state, 60..70);
    rt.frame(0.05);
    assert_eq!(rt.state.rebuilds, 1);
    assert_eq!(rt.next_wake(), Some(BUSY_WAKE));
    rt.frame(0.1);
    assert_eq!((rt.state.rebuilds, rt.state.rows.len()), (2, 70));

    let (_, row_y) = rt.find("row-55").unwrap();
    assert!(row_y > 320.0, "row-55 starts below the window, centred at {row_y}");
    rt.state.sender.send(Message::Select(55)).unwrap();
    rt.frame(0.0);
    rt.frame(0.0);
    let (_, row_y) = rt.find("row-55").unwrap();
    assert!((16.0..=320.0 - 16.0).contains(&row_y), "row-55 centred at {row_y}");
}
