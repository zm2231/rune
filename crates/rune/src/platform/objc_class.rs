use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};

/// Whether key-value observing (or another runtime subclass that hides itself from `-class`)
/// sits on `object`. Placing a Rune per-instance subclass above KVO's crashes Foundation when
/// an observer is removed.
#[allow(unsafe_code)]
pub(crate) unsafe fn kvo_observed(object: &AnyObject) -> bool {
    // SAFETY: caller guarantees `object` is a live ObjC instance; `-class` takes no arguments.
    let reported: &AnyClass = unsafe { msg_send![object, class] };
    let actual = object.class();
    !std::ptr::eq(reported, actual) || actual.name().to_bytes().starts_with(b"NSKVONotifying_")
}

pub(crate) fn has_subclass(object: &AnyObject, prefix: &[u8]) -> bool {
    std::iter::successors(Some(object.class()), |c| c.superclass())
        .any(|c| c.name().to_bytes().starts_with(prefix))
}
