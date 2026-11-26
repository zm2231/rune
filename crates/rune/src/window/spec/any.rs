use super::WindowSpec;

/// A `WindowSpec` of any state type, for factories that mix them:
/// `vec![review.boxed(), settings.into()]`.
pub struct AnyWindowSpec(Box<dyn ErasedWindow>);

impl AnyWindowSpec {
    pub fn key(&self) -> &str { self.0.key() }

    /// A window `App::window` added: it closes by hiding and never keeps the loop running alone.
    pub(crate) fn secondary<T: 'static>(mut spec: WindowSpec<T>, handle: crate::WindowHandle) -> Self {
        spec.app.keep_running = true;
        Self(Box::new(Erased { spec, handle: Some(handle), secondary: true }))
    }

    pub(super) fn opened<T: 'static>(spec: WindowSpec<T>, handle: crate::WindowHandle) -> Self {
        Self(Box::new(Erased { spec, handle: Some(handle), secondary: false }))
    }

    pub(crate) fn resolve_args(&mut self, inherited: crate::tooling::Args) -> crate::tooling::Args { self.0.resolve_args(inherited) }

    /// Render this window headless; `scale` converts a physical size option to points.
    pub(crate) fn headless(self, args: crate::tooling::Args, scale: f32) -> Result<(), crate::tooling::StartError> { self.0.headless(args, scale) }

    /// This window as one member of a `--desktop` composite; `selected` takes `--scene`.
    pub(crate) fn desktop(self, args: &crate::tooling::Args, selected: bool, scale: f32) -> Box<dyn crate::desktop::Member> {
        self.0.desktop(args, selected, scale)
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn mount(self) -> crate::platform::ios::Mount { self.0.mount() }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn start(self, inherited: crate::tooling::Args, cx: crate::window::Start<'_>) -> Option<Box<dyn crate::window::AnyHost>> {
        self.0.start(inherited, cx)
    }
}

impl<S: 'static> From<WindowSpec<S>> for AnyWindowSpec {
    fn from(spec: WindowSpec<S>) -> Self { Self(Box::new(Erased { spec, handle: None, secondary: false })) }
}

trait ErasedWindow {
    fn key(&self) -> &str;
    fn take_windows(&mut self) -> Vec<AnyWindowSpec>;
    fn resolve_args(&mut self, inherited: crate::tooling::Args) -> crate::tooling::Args;
    fn headless(self: Box<Self>, args: crate::tooling::Args, scale: f32) -> Result<(), crate::tooling::StartError>;
    fn desktop(self: Box<Self>, args: &crate::tooling::Args, selected: bool, scale: f32) -> Box<dyn crate::desktop::Member>;
    #[cfg(not(target_arch = "wasm32"))]
    fn start(self: Box<Self>, inherited: crate::tooling::Args, cx: crate::window::Start<'_>) -> Option<Box<dyn crate::window::AnyHost>>;
    #[cfg(target_os = "ios")]
    fn mount(self: Box<Self>) -> crate::platform::ios::Mount;
}

struct Erased<S: 'static> {
    spec: WindowSpec<S>,
    handle: Option<crate::WindowHandle>,
    secondary: bool,
}

impl<S: 'static> ErasedWindow for Erased<S> {
    fn key(&self) -> &str { &self.spec.key }
    fn take_windows(&mut self) -> Vec<AnyWindowSpec> { std::mem::take(&mut self.spec.app.windows) }
    fn resolve_args(&mut self, inherited: crate::tooling::Args) -> crate::tooling::Args { self.spec.app.resolve_args(inherited) }
    // A trait object's methods are all linked with it: without the `tooling` feature these
    // bodies (never called then: `run` refuses a headless run first) keep headless rendering
    // out of a release app.
    #[cfg(not(any(feature = "tooling", debug_assertions)))]
    fn headless(self: Box<Self>, _: crate::tooling::Args, _: f32) -> Result<(), crate::tooling::StartError> { unreachable!("{}", crate::tooling::WITHOUT_TOOLING) }
    #[cfg(not(any(feature = "tooling", debug_assertions)))]
    fn desktop(self: Box<Self>, _: &crate::tooling::Args, _: bool, _: f32) -> Box<dyn crate::desktop::Member> { unreachable!("{}", crate::tooling::WITHOUT_TOOLING) }
    #[cfg(any(feature = "tooling", debug_assertions))]
    fn headless(self: Box<Self>, args: crate::tooling::Args, scale: f32) -> Result<(), crate::tooling::StartError> {
        crate::tooling::headless_window_spec(self.spec.app, self.spec.options, args, scale)
    }
    #[cfg(any(feature = "tooling", debug_assertions))]
    fn desktop(self: Box<Self>, args: &crate::tooling::Args, selected: bool, scale: f32) -> Box<dyn crate::desktop::Member> {
        let Erased { spec, handle, secondary } = *self;
        Box::new(crate::desktop::Window::new(spec, handle, args, selected, !secondary, scale))
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn start(self: Box<Self>, inherited: crate::tooling::Args, cx: crate::window::Start<'_>) -> Option<Box<dyn crate::window::AnyHost>> {
        let inherited = secondary_args(self.secondary, &self.spec.key, inherited);
        crate::window::start(self.spec, self.handle, !self.secondary, inherited, cx)
    }
    #[cfg(target_os = "ios")]
    fn mount(self: Box<Self>) -> crate::platform::ios::Mount { crate::platform::ios::mount(self.spec.app) }
}

#[cfg(not(target_arch = "wasm32"))]
fn secondary_args(secondary: bool, key: &str, mut inherited: crate::tooling::Args) -> crate::tooling::Args {
    // A secondary window is not what --scene or --size describe unless --window selects it.
    if secondary && inherited.window.as_deref() != Some(key) {
        inherited.scene = None;
        inherited.size = None;
    }
    inherited
}

/// Each window followed by those its app added with `App::window`, depth first.
pub(crate) fn flatten(windows: impl IntoIterator<Item = AnyWindowSpec>) -> Vec<AnyWindowSpec> {
    let mut out = Vec::new();
    for mut window in windows {
        let children = window.0.take_windows();
        out.push(window);
        out.extend(flatten(children));
    }
    out
}

pub(crate) fn validate(windows: &[AnyWindowSpec]) -> Result<(), String> {
    let mut keys = std::collections::HashSet::new();
    for window in windows {
        if window.key().is_empty() || !keys.insert(window.key()) { return Err(format!("empty or duplicate window key: {:?}", window.key())); }
    }
    Ok(())
}
