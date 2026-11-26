use crate::machine::Effects;

pub(crate) type Scene = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Scenes,
    Sheets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Hidden,
    Activating,
    Cancelled,
    Scene(Scene),
    Leaving { scene: Scene, close: bool },
    Away,
    Sheet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Launch {
    Connecting,
    Fresh,
    Restored,
    Settled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) key: String,
    pub(crate) starts_visible: bool,
    pub(crate) state: State,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Windows {
    mode: Mode,
    windows: Vec<Window>,
    sheets: Vec<usize>,
    launch: Launch,
    recent: Vec<usize>,
    front: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ask {
    Show,
    Focus,
    Hide,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event<'a> {
    Connected { scene: Scene, key: Option<&'a str> },
    Disconnected(Scene),
    Foreground { scene: Scene, key: Option<&'a str> },
    Background(Scene),
    Discarded(&'a str),
    Ask { window: usize, ask: Ask },
    Dismissed(usize),
    Drawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Attach { window: usize, scene: Scene },
    Activate(usize),
    Destroy(usize),
    Refuse(Scene),
    Present(usize),
    Dismiss(usize),
    Closed(usize),
    Refused(usize),
}

const PRIMARY: usize = 0;

impl Windows {
    pub(crate) fn new(mode: Mode, windows: Vec<Window>) -> Self {
        Windows { mode, windows, sheets: Vec::new(), launch: Launch::Connecting, recent: Vec::new(), front: Vec::new() }
    }

    pub(crate) fn len(&self) -> usize { self.windows.len() }

    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub(crate) fn key(&self, window: usize) -> &str { &self.windows[window].key }

    pub(crate) fn state(&self, window: usize) -> State { self.windows[window].state }

    pub(crate) fn find(&self, key: &str) -> Option<usize> { self.windows.iter().position(|w| w.key == key) }

    pub(crate) fn push(&mut self, key: String) -> usize {
        self.windows.push(Window { key, starts_visible: false, state: State::Hidden });
        self.windows.len() - 1
    }

    fn set(&mut self, window: usize, state: State) { self.windows[window].state = state; }

    fn in_scene(&self, scene: Scene) -> Option<usize> {
        self.windows.iter().position(|w| matches!(w.state, State::Scene(s) | State::Leaving { scene: s, .. } if s == scene))
    }

    fn in_front(&mut self, window: usize) {
        self.forget(window);
        self.recent.push(window);
        self.front.push(window);
    }

    fn rooted(&self) -> bool { matches!(self.state(PRIMARY), State::Scene(_)) }

    fn forget(&mut self, window: usize) {
        self.recent.retain(|&w| w != window);
        self.front.retain(|&w| w != window);
    }

    fn successor(&self, closing: usize) -> Option<usize> {
        if !self.front.contains(&closing) || self.front.iter().any(|&w| w != closing) { return None; }
        let session = |w: usize| matches!(self.state(w), State::Scene(_) | State::Away);
        self.recent.iter().rev().copied().find(|&w| w != closing && session(w))
            .or((closing != PRIMARY).then_some(PRIMARY))
    }
}

pub(crate) fn step(windows: &mut Windows, event: Event<'_>) -> Effects<Effect> {
    match event {
        Event::Connected { scene, key } => connected(windows, scene, key),
        Event::Disconnected(scene) => match windows.in_scene(scene) {
            Some(window) if matches!(windows.state(window), State::Leaving { .. }) => gone(windows, window),
            Some(window) => {
                windows.set(window, State::Away);
                windows.front.retain(|&w| w != window);
                Effects::none()
            }
            None => Effects::none(),
        },
        Event::Foreground { scene, key } => match windows.in_scene(scene) {
            Some(window) if matches!(windows.state(window), State::Leaving { .. }) => Effects::none(),
            Some(window) => {
                windows.in_front(window);
                Effects::none()
            }
            None => {
                let Some(key) = key else { return Effects::none() };
                let effects = connected(windows, scene, Some(key));
                if let Some(window) = windows.in_scene(scene) { windows.in_front(window); }
                effects
            }
        },
        Event::Background(scene) => match windows.in_scene(scene) {
            Some(window) if matches!(windows.state(window), State::Leaving { .. }) => gone(windows, window),
            Some(window) => {
                windows.front.retain(|&w| w != window);
                Effects::none()
            }
            None => Effects::none(),
        }
        Event::Discarded(key) => match windows.find(key) {
            Some(window) if windows.state(window) == State::Away => {
                windows.set(window, State::Hidden);
                windows.forget(window);
                Effects::one(Effect::Closed(window))
            }
            _ => Effects::none(),
        },
        Event::Ask { window, ask } => asked(windows, window, ask),
        Event::Dismissed(window) => {
            if windows.state(window) != State::Sheet { return Effects::none(); }
            windows.sheets.retain(|&w| w != window);
            windows.set(window, State::Hidden);
            Effects::one(Effect::Closed(window))
        }
        Event::Drawn => {
            if windows.launch != Launch::Fresh { return Effects::none(); }
            windows.launch = Launch::Settled;
            let mut effects = Effects::none();
            for window in 1..windows.len() {
                if windows.windows[window].starts_visible { effects.extend(asked(windows, window, Ask::Show)); }
            }
            effects
        }
    }
}

fn connected(windows: &mut Windows, scene: Scene, key: Option<&str>) -> Effects<Effect> {
    let key = if windows.mode == Mode::Sheets { None } else { key };
    if windows.launch == Launch::Connecting {
        windows.launch = if key.is_none() { Launch::Fresh } else { Launch::Restored };
    }
    let Some(window) = key.map_or(Some(PRIMARY), |key| windows.find(key)) else { return Effects::one(Effect::Refuse(scene)) };
    match windows.state(window) {
        State::Hidden | State::Activating | State::Away => {
            windows.set(window, State::Scene(scene));
            let mut effects = Effects::one(Effect::Attach { window, scene });
            if window == PRIMARY { effects.extend(windows.sheets.iter().map(|&sheet| Effect::Present(sheet))); }
            effects
        }
        State::Cancelled if key.is_none() => {
            windows.set(window, State::Scene(scene));
            Effects::one(Effect::Attach { window, scene })
        }
        State::Cancelled => {
            windows.set(window, State::Hidden);
            Effects::one(Effect::Refuse(scene))
        }
        State::Scene(_) | State::Leaving { .. } | State::Sheet => Effects::one(Effect::Refuse(scene)),
    }
}

fn asked(windows: &mut Windows, window: usize, ask: Ask) -> Effects<Effect> {
    match ask {
        Ask::Show | Ask::Focus => show(windows, window, ask == Ask::Focus),
        Ask::Hide | Ask::Close => hide(windows, window, ask == Ask::Close),
    }
}

fn show(windows: &mut Windows, window: usize, focus: bool) -> Effects<Effect> {
    match (windows.mode, windows.state(window)) {
        (Mode::Scenes, State::Hidden | State::Away | State::Cancelled) => {
            windows.set(window, State::Activating);
            Effects::one(Effect::Activate(window))
        }
        (Mode::Scenes, State::Scene(_)) if focus => Effects::one(Effect::Activate(window)),
        (Mode::Scenes, State::Leaving { scene, .. }) => {
            windows.set(window, State::Scene(scene));
            Effects::one(Effect::Activate(window))
        }
        (Mode::Sheets, State::Hidden) if window != PRIMARY => {
            windows.set(window, State::Sheet);
            windows.sheets.push(window);
            if windows.rooted() { Effects::one(Effect::Present(window)) } else { Effects::none() }
        }
        _ => Effects::none(),
    }
}

fn hide(windows: &mut Windows, window: usize, close: bool) -> Effects<Effect> {
    match windows.state(window) {
        _ if windows.mode == Mode::Sheets && window == PRIMARY => Effects::one(Effect::Refused(window)),
        State::Scene(scene) if let Some(next) = windows.successor(window) => {
            windows.set(window, State::Leaving { scene, close });
            windows.forget(window);
            show(windows, next, true)
        }
        State::Leaving { scene, close: pending } => {
            windows.set(window, State::Leaving { scene, close: pending || close });
            windows.forget(window);
            gone(windows, window)
        }
        State::Scene(_) | State::Away => {
            windows.set(window, State::Hidden);
            windows.forget(window);
            let mut effects = Effects::one(Effect::Destroy(window));
            if close { effects.push(Effect::Closed(window)); }
            effects
        }
        State::Activating => {
            windows.set(window, State::Cancelled);
            Effects::none()
        }
        State::Sheet => {
            windows.set(window, State::Hidden);
            let at = windows.sheets.iter().position(|&w| w == window).unwrap_or(windows.sheets.len());
            let above: Vec<usize> = windows.sheets.drain(at..).skip(1).collect();
            let mut effects = Effects::one(Effect::Dismiss(window));
            windows.sheets.extend(&above);
            effects.extend(above.into_iter().map(Effect::Present));
            if close { effects.push(Effect::Closed(window)); }
            effects
        }
        State::Hidden | State::Cancelled => Effects::none(),
    }
}

fn gone(windows: &mut Windows, window: usize) -> Effects<Effect> {
    let State::Leaving { close, .. } = windows.state(window) else { return Effects::none() };
    windows.set(window, State::Hidden);
    let mut effects = Effects::one(Effect::Destroy(window));
    if close { effects.push(Effect::Closed(window)); }
    effects
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
