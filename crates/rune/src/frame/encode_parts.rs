use std::cell::Cell;

use crate::time::Instant;

#[derive(Clone, Copy)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum Part { Prepare, Layers, Pass, Compositor, Arm, Kept, Copy }

pub(crate) const PARTS: usize = 7;

thread_local! {
    static NS: Cell<[u64; PARTS]> = const { Cell::new([0; PARTS]) };
}

pub(crate) struct Scope(Part, Instant);

impl Scope {
    pub(crate) fn new(part: Part) -> Self { Self(part, crate::time::now()) }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let ns = u64::try_from(self.1.elapsed().as_nanos()).unwrap_or(u64::MAX);
        NS.with(|parts| {
            let mut row = parts.get();
            row[self.0 as usize] = row[self.0 as usize].saturating_add(ns);
            parts.set(row);
        });
    }
}

pub(crate) fn take() -> [u64; PARTS] { NS.replace([0; PARTS]) }
