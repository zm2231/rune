use super::classes;
use crate::{SizeClass, SizeClasses};

#[test]
fn uikit_classes_map_to_runes_and_unspecified_is_none() {
    assert_eq!(classes(1, 2), SizeClasses { horizontal: Some(SizeClass::Compact), vertical: Some(SizeClass::Regular) });
    assert_eq!(classes(2, 1), SizeClasses { horizontal: Some(SizeClass::Regular), vertical: Some(SizeClass::Compact) });
    assert_eq!(classes(0, 7), SizeClasses::default());
}
