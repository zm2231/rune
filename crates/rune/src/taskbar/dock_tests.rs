use super::*;

#[test]
fn a_window_made_again_counts_once() {
    let first = DockCount::new(1);
    assert_eq!(set(1, 3), 3);
    let other = DockCount::new(2);
    assert_eq!(set(2, 1), 4);

    drop(first);
    assert_eq!(set(2, 1), 1);
    let _again = DockCount::new(3);
    assert_eq!(set(3, 3), 4);

    drop(other);
    assert_eq!(set(3, 3), 3);
}

#[test]
fn counts_saturate() {
    let _a = DockCount::new(1);
    let _b = DockCount::new(2);
    set(1, u32::MAX);
    assert_eq!(set(2, 1), u32::MAX);
}
