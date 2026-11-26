use super::*;
use crate::{Cx, El, Env, SizeClass, SizeClasses, col, el, memo};

fn view(_: &(), cx: &mut Cx<()>) -> El {
    let wide = cx.horizontal_size_class() == Some(SizeClass::Regular);
    let row = memo("row", (), move || el().key("row").w_full().h(if wide { 40.0 } else { 20.0 }));
    col().w_full().h_full().child(row)
}

fn runtime() -> Runtime<()> {
    let mut rt = Runtime::new((), view, (400.0, 800.0), Env::new(true, 3.0));
    rt.frame(0.0);
    rt
}

fn row(rt: &mut Runtime<()>) -> f32 {
    for _ in 0..600 { rt.frame(1.0 / 120.0); }
    rt.presented("row").unwrap().0[3]
}

const REGULAR: SizeClasses = SizeClasses { horizontal: Some(SizeClass::Regular), vertical: Some(SizeClass::Regular) };
const COMPACT: SizeClasses = SizeClasses { horizontal: Some(SizeClass::Compact), vertical: Some(SizeClass::Regular) };

#[test]
fn a_view_with_no_size_classes_lays_out_as_compact() {
    assert_eq!(row(&mut runtime()), 20.0);
}

#[test]
fn a_multitasking_resize_that_changes_the_class_re_runs_the_view_and_its_memos() {
    let mut rt = runtime();
    assert!(rt.set_size_classes(REGULAR));
    assert_eq!(row(&mut rt), 40.0);
    assert!(!rt.set_size_classes(REGULAR), "the same classes change nothing");
    assert!(rt.set_size_classes(COMPACT));
    assert_eq!(row(&mut rt), 20.0);
}
