use crate::{Cx, El, Env, Runtime, SEARCH_INDEX_PENDING, SearchItem, Tx, el};

use super::{IndexOp, Queue};

#[test]
fn a_headless_run_keeps_what_handlers_asked_to_index_in_order() {
    fn view(_: &(), _: &mut Cx<()>) -> El {
        el().key("index").size(40.0, 40.0).on_press_tx(|_: &mut (), tx: &mut Tx| {
            tx.index_search_items([SearchItem::new("note-42").with_title("Groceries")]);
            tx.remove_search_items(["note-7"]);
            tx.remove_search_domain("archive");
            tx.index_search_items(Vec::new());
        })
    }
    let mut rt = Runtime::new((), view, (80.0, 60.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt.pointer_down(10.0, 10.0);
    rt.pointer_up(10.0, 10.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.core_mut().host.take_search_index_requests(), [
        IndexOp::Index(vec![SearchItem::new("note-42").with_title("Groceries")]),
        IndexOp::Remove(vec!["note-7".into()]),
        IndexOp::RemoveDomain("archive".into()),
    ]);
}

#[test]
fn the_indexer_queue_refuses_what_would_pass_its_bound_and_counts_it() {
    let mut queue = Queue::default();
    let items = |n: usize| IndexOp::Index((0..n).map(|i| SearchItem::new(i.to_string())).collect());
    assert_eq!(queue.admit(vec![items(SEARCH_INDEX_PENDING - 1), items(5), IndexOp::RemoveDomain("old".into())]), 5);
    assert!(matches!(queue.next(), Some(IndexOp::Index(v)) if v.len() == SEARCH_INDEX_PENDING - 1));
    assert_eq!(queue.next(), Some(IndexOp::RemoveDomain("old".into())));
    assert_eq!(queue.admit(vec![items(5)]), 0, "room again once the indexer took them");
}
