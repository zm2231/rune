use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static COUNTS: RefCell<HashMap<u64, u32>> = RefCell::new(HashMap::new());
}

pub(crate) fn set(window: u64, count: u32) -> u32 {
    COUNTS.with_borrow_mut(|counts| {
        counts.insert(window, count);
        total(counts)
    })
}

pub(crate) struct DockCount(u64);

impl DockCount {
    pub(crate) fn new(window: u64) -> Self { Self(window) }
}

impl Drop for DockCount {
    fn drop(&mut self) {
        let left = COUNTS.try_with(|c| c.try_borrow_mut().ok().and_then(|mut counts| counts.remove(&self.0).map(|_| total(&counts))));
        if let Ok(Some(total)) = left {
            show(total);
        }
    }
}

fn total(counts: &HashMap<u64, u32>) -> u32 {
    counts.values().fold(0u32, |sum, &count| sum.saturating_add(count))
}

fn show(total: u32) {
    #[cfg(target_os = "macos")]
    crate::platform::macos::show_dock_badge(total);
    #[cfg(not(target_os = "macos"))]
    let _ = total;
}

#[cfg(test)]
#[path = "dock_tests.rs"]
mod tests;
