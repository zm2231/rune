//! A trait collection's size classes. Plain arithmetic, testable on every platform.

use crate::{SizeClass, SizeClasses};

/// `UIUserInterfaceSizeClass`: 1 compact, 2 regular, anything else unspecified.
fn class(raw: isize) -> Option<SizeClass> {
    match raw {
        1 => Some(SizeClass::Compact),
        2 => Some(SizeClass::Regular),
        _ => None,
    }
}

/// The classes of a trait collection's `horizontalSizeClass` and `verticalSizeClass`.
pub(crate) fn classes(horizontal: isize, vertical: isize) -> SizeClasses {
    SizeClasses { horizontal: class(horizontal), vertical: class(vertical) }
}

#[cfg(test)]
#[path = "size_class_tests.rs"]
mod tests;
