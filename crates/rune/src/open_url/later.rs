//! Links whose open the system answers later, as iOS's `openURL:options:completionHandler:`
//! does: each answer waits in a queue the host drains before its next frame, and wakes it.

use std::cell::RefCell;
use std::rc::Rc;

use super::{Link, OpenUrlError, Opened, OpenedFn};
use crate::runtime::Waker;

type Answer = (OpenedFn, Result<(), OpenUrlError>);

/// What the system says once it has tried a link.
pub(crate) type Done = Box<dyn FnOnce(Result<(), OpenUrlError>)>;

pub(crate) struct Later {
    answered: Rc<RefCell<Vec<Answer>>>,
    wake: Waker,
}

impl Later {
    pub(crate) fn new(wake: Waker) -> Self { Self { answered: Rc::default(), wake } }

    fn done(&self, link: &Link) -> Done {
        let Some(f) = link.opened.as_ref().and_then(Opened::take) else { return Box::new(|_| {}) };
        let (answered, wake) = (self.answered.clone(), self.wake.clone());
        Box::new(move |result| {
            answered.borrow_mut().push((f, result));
            wake();
        })
    }
}

impl crate::runtime::RuntimeCore {
    /// The system answers this runtime's links later: `Tx::open_url_then` hears its answer on
    /// the frame after it comes.
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub(crate) fn answer_links_later(&mut self, wake: Waker) { self.host.late_links = Some(Later::new(wake)); }

    /// Opens each link with `open`, which calls its `Done` when the system has answered.
    pub(crate) fn open_urls_later_with(&mut self, links: Vec<Link>, mut open: impl FnMut(&str, Done)) {
        let Some(later) = &self.host.late_links else { return };
        for link in links { open(&link.url, later.done(&link)); }
    }

    /// Queues what the system answered since the last frame for the frame that runs next.
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub(crate) fn take_late_link_answers(&mut self) {
        let Some(later) = &self.host.late_links else { return };
        let answered = std::mem::take(&mut *later.answered.borrow_mut());
        for (f, result) in answered { self.url_opened(f, result); }
    }
}

#[cfg(test)]
#[path = "later_tests.rs"]
mod tests;
