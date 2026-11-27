use std::cell::RefCell;
use std::rc::Rc;

use rune::{Color, Cx, Provided};

use super::kit::Kit;
use super::text_scale::{Scale, TextScale};

const BUILT: usize = 4;

/// What a [`KitTheme`] builds a kit for: the window's appearance where the view reads
/// [`KitCx::kit`]. Read it from a view with [`KitEnv::of`], or make one with [`KitEnv::new`] for a
/// test or a picture of the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct KitEnv {
    /// [`rune::Cx::system_dark`]: the window's appearance, after any scheme the app forces.
    pub dark: bool,
    /// [`rune::theme::blends_srgb`]: the blend mode translucent tokens resolve for.
    pub srgb_blend: bool,
    /// [`rune::Cx::system_increased_contrast`].
    pub increased_contrast: bool,
    /// [`rune::Cx::accent`]: the system accent for this appearance.
    pub accent: Color,
    /// The [`TextScale`](crate::TextScale) in scope ([`KitCx::with_text_scale`]), 1 outside one.
    /// [`KitTheme`] applies it to the kits it builds ([`Kit::with_text_scale`]).
    pub text_scale: f32,
}

impl KitEnv {
    /// `dark`, in the blend mode of the window building the view ([`rune::theme::blends_srgb`],
    /// sRGB outside one), without increased contrast, with the default system accent and the
    /// theme's own text size.
    pub fn new(dark: bool) -> Self {
        Self { dark, srgb_blend: rune::theme::blends_srgb(), increased_contrast: false, accent: rune::SystemAccent::default().accent(dark), text_scale: 1.0 }
    }

    /// The environment of the window `cx` builds.
    pub fn of<S>(cx: &Cx<S>) -> Self {
        Self {
            dark: cx.system_dark(),
            srgb_blend: rune::theme::blends_srgb(),
            increased_contrast: cx.system_increased_contrast(),
            accent: cx.accent(),
            text_scale: cx.provided::<Scale>().map_or(1.0, |s| s.0),
        }
    }

    /// This environment in another appearance, for a view that shows one the window isn't in.
    pub fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    /// This environment blending in sRGB when `on`, in linear light otherwise.
    pub fn srgb_blend(mut self, on: bool) -> Self {
        self.srgb_blend = on;
        self
    }

    /// This environment with or without the system's increased contrast.
    pub fn increased_contrast(mut self, on: bool) -> Self {
        self.increased_contrast = on;
        self
    }

    /// This environment with type at `scale` times the theme's.
    pub fn text_scale(mut self, scale: f32) -> Self {
        self.text_scale = scale;
        self
    }

    /// This environment with another system accent.
    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }
}

struct Source {
    build: Box<dyn Fn(&KitEnv) -> Rc<Kit>>,
    built: RefCell<Vec<(KitEnv, Rc<Kit>)>>,
}

/// A kit that follows the window: a function from [`KitEnv`] to [`Kit`], built on first use for
/// each environment and kept for the four most recent ones. Hold one in app state and hand it to
/// [`KitCx::with_kit`] around the part of the view it styles (the whole window, or a sidebar
/// that takes its own kit); views inside read it with [`KitCx::kit`].
///
/// A theme is cheap to clone, and clones share their kits. Dropping the last clone frees every
/// kit it built, so replacing it when the app's appearance changes leaks nothing. A new theme,
/// even an identical one, rebuilds the memos built under it.
#[derive(Clone)]
pub struct KitTheme(Provided<Source>);

impl KitTheme {
    /// The kit `build` makes for each environment. `build` runs with colours resolving for the
    /// environment's blend mode ([`rune::theme::with_blend`]).
    pub fn new(build: impl Fn(&KitEnv) -> Kit + 'static) -> Self {
        Self::from_source(Box::new(move |env| Rc::new(build(env))))
    }

    /// The same kit in every environment.
    pub fn fixed(kit: Kit) -> Self {
        let kit = Rc::new(kit);
        Self::from_source(Box::new(move |_| kit.clone()))
    }

    fn from_source(build: Box<dyn Fn(&KitEnv) -> Rc<Kit>>) -> Self {
        Self(Provided::new(Source { build, built: RefCell::new(Vec::new()) }))
    }

    /// Rune's neutral kit for the window's appearance, with [`Kit::increased_contrast`] when
    /// the system asks for more contrast. [`KitCx::kit`] uses it where no theme is in scope.
    pub fn neutral() -> Self {
        Self::new(|env| {
            let k = Kit::neutral(env.dark);
            if env.increased_contrast { k.increased_contrast() } else { k }
        })
    }

    /// The kit for `env`, built now if none of the last four environments was `env`.
    pub fn kit(&self, env: &KitEnv) -> Rc<Kit> {
        let mut built = self.0.built.borrow_mut();
        if let Some(i) = built.iter().position(|(e, _)| e == env) {
            let hit = built.remove(i);
            let kit = hit.1.clone();
            built.insert(0, hit);
            return kit;
        }
        drop(built);
        let built = rune::theme::with_blend(env.srgb_blend, || (self.0.build)(env));
        let kit = if (built.text_scale - env.text_scale).abs() < 1e-6 { built } else { Rc::new(built.with_text_scale(env.text_scale)) };
        let mut built = self.0.built.borrow_mut();
        built.insert(0, (*env, kit.clone()));
        built.truncate(BUILT);
        kit
    }
}

thread_local! {
    static NEUTRAL: KitTheme = KitTheme::neutral();
}

/// Reads and scopes the [`KitTheme`] a view builds with.
pub trait KitCx {
    /// The kit of the innermost [`Self::with_kit`] around this build, for this window's
    /// environment, or the neutral kit ([`KitTheme::neutral`]) outside any.
    fn kit(&self) -> Rc<Kit>;

    /// Runs `build` with `theme` as the kit [`Self::kit`] returns inside it.
    fn with_kit<R>(&mut self, theme: &KitTheme, build: impl FnOnce(&mut Self) -> R) -> R;

    /// Runs `build` with kits read inside it at `scale`'s text size, whichever theme they come
    /// from.
    fn with_text_scale<R>(&mut self, scale: &TextScale, build: impl FnOnce(&mut Self) -> R) -> R;
}

impl<S> KitCx for Cx<S> {
    fn kit(&self) -> Rc<Kit> {
        let env = KitEnv::of(self);
        match self.provided::<Source>() {
            Some(source) => KitTheme(source).kit(&env),
            None => NEUTRAL.with(|t| t.kit(&env)),
        }
    }

    fn with_kit<R>(&mut self, theme: &KitTheme, build: impl FnOnce(&mut Self) -> R) -> R {
        self.provide(&theme.0, build)
    }

    fn with_text_scale<R>(&mut self, scale: &TextScale, build: impl FnOnce(&mut Self) -> R) -> R {
        self.provide(&scale.0, build)
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
