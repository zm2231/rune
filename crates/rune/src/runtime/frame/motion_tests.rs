use crate::{Cx, El, Env, LoopSpec, Runtime, col, el};

fn view(spinners: &usize, _: &mut Cx<usize>) -> El {
    col().size(200.0, 200.0).children((0..*spinners).map(|i| el().key(format!("spin-{i}")).size(16.0, 16.0).looping(LoopSpec::rotation(1.0))))
}

#[test]
fn the_frame_log_lists_running_loops_only_when_they_change() {
    let mut rt = Runtime::new(1usize, view, (200.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    let listed = |rt: &Runtime<usize>, waking: bool| rt.core().fp_loops_since(waking)
        .map(|paths| paths.iter().filter_map(|p| p.rsplit('/').next().map(str::to_owned)).collect::<Vec<_>>());

    assert_eq!(listed(&rt, true), Some(vec!["spin-0".to_owned()]));
    rt.frame(0.016);
    assert_eq!(listed(&rt, true), None, "the same loops on the next frame aren't listed again");

    rt.update(|n, _| *n = 2);
    rt.frame(0.016);
    rt.frame(0.016);
    assert_eq!(listed(&rt, true), Some(vec!["spin-0".to_owned(), "spin-1".to_owned()]));

    assert_eq!(listed(&rt, false), Some(vec![]), "once no loop wakes the window, an empty list says so");
    assert_eq!(listed(&rt, false), None);

    rt.update(|n, _| *n = 0);
    for _ in 0..60 { rt.frame(0.016); }
    assert_eq!(listed(&rt, true), None, "with the loops gone the empty list stands");
    rt.update(|n, _| *n = 1);
    rt.frame(0.016);
    rt.frame(0.016);
    assert_eq!(listed(&rt, true), Some(vec!["spin-0".to_owned()]), "a loop mounted again is listed again");
}
