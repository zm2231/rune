use super::*;

impl OverlayCore {
    pub(super) fn theme_changed(&mut self, rt: &mut RuntimeCore, system_dark: bool) {
        self.system_dark = system_dark;
        self.restyle(rt);
    }

    pub(super) fn set_color_scheme(&mut self, rt: &mut RuntimeCore, scheme: crate::ColorScheme) {
        self.color_scheme = scheme;
        self.restyle(rt);
    }

    pub(super) fn app_scheme_changed(&mut self, rt: &mut RuntimeCore, app: crate::ColorScheme) {
        self.app_scheme = app;
        self.restyle(rt);
    }

    pub(super) fn adopt_system_dark(&mut self, rt: &mut RuntimeCore, system_dark: bool) {
        self.system_dark = system_dark;
        self.dark = self.color_scheme.effective_dark(self.app_scheme, system_dark);
        rt.set_system_dark(self.dark);
        self.sync_native_scheme();
    }

    fn restyle(&mut self, rt: &mut RuntimeCore) {
        self.dark = self.color_scheme.effective_dark(self.app_scheme, self.system_dark);
        rt.set_system_dark(self.dark);
        self.sync_native_scheme();
        self.sync_window_backdrop(rt, true);
        self.dirty = true;
    }

    pub(super) fn sync_native_scheme(&self) {
        #[cfg(target_os = "macos")]
        if let Some(panel) = &self.panel { panel.set_color_scheme(self.color_scheme.effective_for_window(self.app_scheme)); }
    }
}

#[cfg(test)]
#[path = "scheme_tests.rs"]
mod tests;
