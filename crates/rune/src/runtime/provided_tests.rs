use std::cell::Cell;

use crate::{Cx, El, Env, GrowthSpec, Provided, Runtime, ScrollSpec, col, el, memo};

thread_local! {
    static ROW_BUILDS: Cell<usize> = const { Cell::new(0) };
}

struct S {
    shade: Provided<u32>,
    inner: Provided<u32>,
}

fn view(s: &S, cx: &mut Cx<S>) -> El {
    let outside = cx.provided::<u32>().map(|p| *p);
    assert_eq!(outside, None);
    cx.provide(&s.shade, |cx| {
        assert_eq!(cx.provided::<u32>().map(|p| *p), Some(*s.shade));
        let inner = cx.provide(&s.inner, |cx| cx.provided::<u32>().map(|p| *p));
        assert_eq!(inner, Some(*s.inner));
        assert_eq!(cx.provided::<u32>().map(|p| *p), Some(*s.shade));
        col().key("scroll").size(200.0, 200.0).scroll(ScrollSpec { stick_bottom: false }).child(col().key("rows").w_full().growth(GrowthSpec::default()).virtual_rows(
            3,
            20.0,
            |_: &S, i| i.to_string().into(),
            |_: &S, i| {
                memo(format!("row{i}"), 0, || {
                    ROW_BUILDS.set(ROW_BUILDS.get() + 1);
                    el().h(20.0)
                })
            },
        ))
    })
}

#[test]
fn the_innermost_value_is_in_scope_and_a_new_one_rebuilds_memos_in_virtual_rows() {
    ROW_BUILDS.set(0);
    let mut rt = Runtime::new(S { shade: Provided::new(1), inner: Provided::new(2) }, view, (200.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert_eq!(ROW_BUILDS.get(), 3);
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    assert_eq!(ROW_BUILDS.get(), 3, "the same values reuse the rows");
    rt.update(|s, _| s.shade = Provided::new(1));
    rt.frame(2.0 / 60.0);
    assert_eq!(ROW_BUILDS.get(), 6, "an equal but new value rebuilds the rows");
}

fn catching(_: &(), cx: &mut Cx<()>) -> El {
    let value = Provided::new(7u32);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cx.provide(&value, |_| panic!("a view part fails"))));
    assert!(caught.is_err());
    assert!(cx.provided::<u32>().is_none(), "the failed scope is gone");
    let outer = Provided::new(1u32);
    let inner = cx.provide(&outer, |cx| cx.provided::<u32>().map(|p| *p));
    assert_eq!(inner, Some(1));
    el()
}

#[test]
fn a_panic_caught_inside_a_scope_leaves_no_value_in_scope() {
    let mut rt = Runtime::new((), catching, (100.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);
}
