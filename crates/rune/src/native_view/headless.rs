#[cfg(test)]
use std::cell::{Cell, RefCell};

use winit::raw_window_handle::{HandleError, WindowHandle};

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Enter { back: bool },
    Reclaim,
}

#[derive(Default)]
pub(crate) struct Container {
    #[cfg(test)]
    pub(crate) built: Cell<bool>,
    #[cfg(test)]
    pub(crate) calls: RefCell<Vec<Call>>,
}

impl Container {
    pub(crate) fn new() -> Self { Self::default() }

    pub(crate) fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> { Err(HandleError::NotSupported) }

    pub(crate) fn in_window(&self) -> bool { false }

    pub(crate) fn focus(&self) -> bool { false }

    pub(crate) fn has_focus(&self) -> bool { false }

    pub(crate) fn can_enter(&self) -> bool {
        #[cfg(test)]
        { self.built.get() }
        #[cfg(not(test))]
        { false }
    }

    pub(crate) fn enter(&self, back: bool) -> bool {
        #[cfg(test)]
        { self.calls.borrow_mut().push(Call::Enter { back }); }
        let _ = back;
        self.can_enter()
    }

    pub(crate) fn reclaim(&self) {
        #[cfg(test)]
        self.calls.borrow_mut().push(Call::Reclaim);
    }
}
