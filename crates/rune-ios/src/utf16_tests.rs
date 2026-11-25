use super::*;

const MIXED: &str = "aé漢👍🏽b";

#[test]
fn offsets_cross_between_bytes_and_units() {
    // a(1 byte, 1 unit) é(2, 1) 漢(3, 1) 👍(4, 2) 🏽(4, 2) b(1, 1)
    assert_eq!(len(MIXED), 8);
    assert_eq!([0, 1, 3, 6, 10, 14, 15].map(|b| units(MIXED, b)), [0, 1, 2, 3, 5, 7, 8]);
    assert_eq!([0, 1, 2, 3, 5, 7, 8].map(|u| byte(MIXED, u)), [0, 1, 3, 6, 10, 14, 15]);
}

#[test]
fn an_offset_inside_a_character_rounds_down_to_it() {
    assert_eq!(byte(MIXED, 4), 6, "the middle of 👍's surrogate pair is its start");
    assert_eq!(units(MIXED, 8), 3, "a byte inside 👍 is its start");
    assert_eq!(byte(MIXED, 99), MIXED.len());
}

#[test]
fn ranges_and_slices_follow_units() {
    assert_eq!(slice(MIXED, 2..5), "漢👍");
    assert_eq!(byte_range(MIXED, 5..2), 3..10, "a reversed range is ordered");
    assert_eq!(unit_range(MIXED, 6..14), 3..7);
    assert_eq!(slice("", 0..3), "");
}

#[test]
fn steps_never_land_inside_a_character() {
    assert_eq!(step(MIXED, 3, 1), Some(5), "one unit right from 👍's start is past it");
    assert_eq!(step(MIXED, 5, -1), Some(3));
    assert_eq!(step(MIXED, 0, -1), None);
    assert_eq!(step(MIXED, 8, 1), None);
    assert_eq!(step(MIXED, 8, 0), Some(8));
}
