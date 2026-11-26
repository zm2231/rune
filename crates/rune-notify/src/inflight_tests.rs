use super::*;

#[test]
fn a_post_removed_while_it_waits_is_not_wanted_and_its_answer_withdraws_it() {
    let mut posts = Inflight::default();
    let ticket = posts.start("q-1");
    posts.withdraw("q-1");
    assert!(!posts.wanted("q-1", ticket));
    assert_eq!(posts.settle("q-1", ticket), Settled::Withdrawn);
    assert_eq!(posts.len(), 0);
}

#[test]
fn posting_again_after_a_remove_is_wanted_and_the_old_answer_leaves_it_alone() {
    let mut posts = Inflight::default();
    let old = posts.start("q-1");
    posts.withdraw("q-1");
    let new = posts.start("q-1");
    assert!(!posts.wanted("q-1", old));
    assert_eq!(posts.settle("q-1", old), Settled::Superseded);
    assert!(posts.wanted("q-1", new));
    assert_eq!(posts.settle("q-1", new), Settled::Wanted);
    assert_eq!(posts.len(), 0);
}

#[test]
fn removing_an_id_with_nothing_in_flight_leaves_others_wanted() {
    let mut posts = Inflight::default();
    let ticket = posts.start("q-1");
    posts.withdraw("q-2");
    assert!(posts.wanted("q-1", ticket));
    assert_eq!(posts.len(), 1);
}
