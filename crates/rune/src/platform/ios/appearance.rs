use crate::ColorScheme;

#[cfg(test)]
#[path = "appearance_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Appearance {
    window: ColorScheme,
    app: ColorScheme,
    applied: ColorScheme,
}

impl Appearance {
    pub(crate) fn new(window: ColorScheme) -> Self {
        Self { window, app: ColorScheme::System, applied: ColorScheme::System }
    }

    pub(crate) fn set_window(&mut self, scheme: ColorScheme) { self.window = scheme; }

    pub(crate) fn set_app(&mut self, scheme: ColorScheme) { self.app = scheme; }

    pub(crate) fn changed(&mut self) -> Option<isize> {
        let effective = self.window.effective_for_window(self.app);
        if effective == self.applied { return None; }
        self.applied = effective;
        Some(style(effective))
    }
}

pub(crate) fn status_bar_style(interface_style: isize) -> isize {
    match interface_style {
        2 => 1,
        1 => 3,
        _ => 0,
    }
}

pub(crate) fn style(scheme: ColorScheme) -> isize {
    match scheme {
        ColorScheme::System => 0,
        ColorScheme::Light => 1,
        ColorScheme::Dark => 2,
    }
}
