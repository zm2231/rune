use super::*;
use crate::el::{Align, Side};
use crate::{Cx, El, Env, Transition, col, el, memo};

struct S {
    ids: Vec<u32>,
}

fn row(id: u32) -> El {
    col().key(format!("row-{id}")).w(200.0).h(20.0).transition(Transition::opacity())
        .child(el().key(format!("dot-{id}")).w(8.0).h(8.0))
        .child(el().key(format!("badge-{id}")).w(4.0).h(4.0).anchored(format!("dot-{id}"), Side::Right, Align::Center, 2.0))
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let tip = el().key("tip").w(6.0).h(6.0).anchored("dot-1", Side::Bottom, Align::Start, 1.0);
    let rows = col().key("rows").children(s.ids.iter().map(|&id| if id % 2 == 0 { memo(format!("row-{id}"), id, move || row(id)) } else { row(id) }));
    col().size(300.0, 400.0).pl(40.0).pt(40.0).child(tip).child(rows)
}

fn assert_beside_their_dots(rt: &Runtime<S>, step: &str) {
    for (i, &id) in rt.state.ids.iter().enumerate() {
        let y = 40.0 + 20.0 * i as f32;
        assert_eq!(rt.presented(&format!("dot-{id}")).unwrap().0, [40.0, y, 8.0, 8.0], "{step}: dot-{id}");
        assert_eq!(rt.presented(&format!("badge-{id}")).unwrap().0, [50.0, y + 2.0, 4.0, 4.0], "{step}: badge-{id}");
    }
    if let Some(i) = rt.state.ids.iter().position(|&id| id == 1) {
        assert_eq!(rt.presented("tip").unwrap().0, [40.0, 40.0 + 20.0 * i as f32 + 9.0, 6.0, 6.0], "{step}: tip");
    }
}

#[test]
fn anchored_overlays_follow_their_own_anchor_through_inserts_removes_and_moves() {
    let mut rt = Runtime::new(S { ids: (1..=6).collect() }, view, (300.0, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert_beside_their_dots(&rt, "mount");

    let steps: [(&str, fn(&mut Vec<u32>)); 5] = [
        ("insert at the front", |ids| ids.insert(0, 7)),
        ("insert in the middle", |ids| ids.insert(3, 8)),
        ("remove one", |ids| ids.retain(|&id| id != 3)),
        ("move the last to the front", |ids| ids.rotate_right(1)),
        ("swap two", |ids| ids.swap(1, 4)),
    ];
    for (step, change) in steps {
        rt.update(|s, _| change(&mut s.ids));
        settle(&mut rt);
        assert_beside_their_dots(&rt, step);
    }

    rt.update(|s, _| s.ids.retain(|&id| id != 4));
    rt.frame(0.016);
    rt.update(|s, _| s.ids.insert(0, 4));
    settle(&mut rt);
    assert_beside_their_dots(&rt, "bring back a row still leaving");

    rt.update(|s, _| s.ids.retain(|&id| id != 1));
    settle(&mut rt);
    assert_beside_their_dots(&rt, "remove the tip's anchor");
    assert_eq!(rt.presented("tip").map(|p| p.0), Some([0.0; 4]), "an overlay whose anchor is gone isn't placed");
}

fn settle(rt: &mut Runtime<S>) {
    for _ in 0..90 { rt.frame(0.016); }
}
