//! The accessibility tree as UIKit wants it. VoiceOver, Switch Control and Voice Control read
//! a view as a flat, ordered list of elements, each with a label, a value, traits and a frame,
//! where AccessKit's tree nests groups, controls and the text inside them. The mirror keeps
//! the runtime's tree as its updates arrive and flattens it: a control, a piece of text or an
//! image is an element with everything inside it folded into its name, and a container is only
//! walked through. It also turns what a reader does to an element (a double tap, a swipe up on
//! a slider, a three-finger scroll) into the tree's action, and finds the words a live region
//! newly holds.

use accesskit::{Action, ActionRequest, Live, Node, NodeId, Role, Toggled, TreeId, TreeUpdate};

use crate::runtime::HashMap;

/// What kind of thing an element is and what state it is in, as UIKit's traits: the view maps
/// each bit to its `UIAccessibilityTraits` constant.
pub(crate) mod traits {
    pub(crate) const BUTTON: u16 = 1 << 0;
    pub(crate) const LINK: u16 = 1 << 1;
    pub(crate) const IMAGE: u16 = 1 << 2;
    pub(crate) const SELECTED: u16 = 1 << 3;
    pub(crate) const STATIC_TEXT: u16 = 1 << 4;
    pub(crate) const DISABLED: u16 = 1 << 5;
    pub(crate) const ADJUSTABLE: u16 = 1 << 6;
    pub(crate) const HEADER: u16 = 1 << 7;
    pub(crate) const TOGGLE: u16 = 1 << 8;
    pub(crate) const SEARCH: u16 = 1 << 9;
    pub(crate) const UPDATES: u16 = 1 << 10;
    pub(crate) const TEXT_FIELD: u16 = 1 << 11;
    pub(crate) const SECURE_TEXT_FIELD: u16 = 1 << 12;

    const NAMES: [(u16, &str); 13] = [
        (BUTTON, "button"), (LINK, "link"), (IMAGE, "image"), (SELECTED, "selected"), (STATIC_TEXT, "static-text"),
        (DISABLED, "disabled"), (ADJUSTABLE, "adjustable"), (HEADER, "header"), (TOGGLE, "toggle"), (SEARCH, "search"),
        (UPDATES, "updates"), (TEXT_FIELD, "text-field"), (SECURE_TEXT_FIELD, "secure-text-field"),
    ];

    pub(crate) fn names(bits: u16) -> Vec<&'static str> {
        NAMES.iter().filter(|(bit, _)| bits & bit != 0).map(|(_, name)| *name).collect()
    }
}

/// One thing a reader can land on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Element {
    /// The tree node's id, which stays the same while the thing it stands for lives.
    pub(crate) id: u64,
    pub(crate) label: String,
    pub(crate) value: Option<String>,
    pub(crate) hint: Option<String>,
    pub(crate) traits: u16,
    /// Left, top, width and height in the app's points, which the view's zoom scales to its own.
    pub(crate) rect: [f32; 4],
    pub(crate) expanded: Option<bool>,
    pub(crate) menu: bool,
    pub(crate) editing: bool,
    /// The nearest scroll view around it, for a reader's scroll gesture.
    scrolls: Option<u64>,
    /// The `El::native_view` slot it hosts, which stands in the list in its place.
    pub(crate) hosted: Option<crate::NativeViewId>,
}

/// What a reader did to an element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Gesture {
    /// A double tap.
    Activate,
    /// A swipe up or down on an adjustable element.
    Increment,
    Decrement,
    /// The reader's cursor landed on it.
    #[cfg(any(target_os = "ios", test))]
    Focused,
    /// A scroll that shows the content further in this direction.
    Scroll(Toward),
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Toward { Up, Down, Left, Right }

/// What an update changed for the reader.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Changes {
    /// Elements came, went or moved in the order: the reader should look again.
    pub(crate) layout: bool,
    /// What live regions newly say, in tree order.
    pub(crate) announce: Vec<String>,
    pub(crate) focus: Option<u64>,
    pub(crate) page: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Axis { X, Y }

#[derive(Default)]
pub(crate) struct Mirror {
    nodes: HashMap<u64, Node>,
    root: Option<NodeId>,
    elements: Vec<Element>,
    /// What each live region said last, by node.
    said: HashMap<u64, String>,
    focus: Option<u64>,
    paging: Option<(u64, Axis)>,
    masks: HashMap<u64, String>,
    start: Option<u64>,
    landing: Option<u64>,
}

impl Mirror {
    #[cfg(any(target_os = "ios", test))]
    pub(crate) fn elements(&self) -> &[Element] { &self.elements }

    pub(crate) fn set_masks(&mut self, masks: Vec<(u64, String)>) -> bool {
        let masks: HashMap<u64, String> = masks.into_iter().collect();
        if masks == self.masks { return false }
        self.masks = masks;
        true
    }

    pub(crate) fn set_start(&mut self, start: Option<u64>) { self.start = start; }

    #[cfg(any(target_os = "ios", test))]
    pub(crate) fn landing(&self) -> Option<u64> { self.landing }

    #[cfg(any(target_os = "ios", test))]
    pub(crate) fn refresh(&mut self) -> Changes {
        let Some(focus) = self.focus.filter(|_| self.root.is_some()) else { return Changes::default() };
        self.apply(TreeUpdate { nodes: Vec::new(), tree: None, tree_id: TreeId::ROOT, focus: NodeId(focus) })
    }

    /// Takes the tree's changes and flattens it again.
    pub(crate) fn apply(&mut self, update: TreeUpdate) -> Changes {
        // A whole tree after none (a reader just asked) says nothing: what a live region held
        // before anyone listened is old news.
        let first = self.root.is_none();
        let focus = update.focus.0;
        if let Some(tree) = &update.tree { self.root = Some(tree.root); }
        for (id, node) in update.nodes { self.nodes.insert(id.0, node); }
        let mut walk = Walk { nodes: &self.nodes, masks: &self.masks, focus, start: self.start, landing: None, elements: Vec::new(), live: Vec::new(), seen: Vec::new() };
        if let Some(root) = self.root { walk.node(root, [0.0, 0.0], true, None, None, None); }
        let Walk { elements, live, seen, landing, .. } = walk;
        self.landing = landing;
        let seen: crate::runtime::HashSet<u64> = seen.into_iter().collect();
        self.nodes.retain(|id, _| seen.contains(id));
        let mut announce = Vec::new();
        let mut said = HashMap::default();
        for (id, words) in live {
            if !first && self.said.get(&id) != Some(&words) && !words.trim().is_empty() { announce.push(words.clone()); }
            said.insert(id, words);
        }
        self.said = said;
        let order = |list: &[Element]| list.iter().map(|e| e.id).collect::<Vec<_>>();
        let layout = order(&elements) != order(&self.elements);
        let moved = !first && self.focus != Some(focus) && elements.iter().any(|e| e.id == focus);
        self.focus = Some(focus);
        self.elements = elements;
        let page = self.paging.take().and_then(|(id, axis)| page(self.nodes.get(&id)?, axis));
        Changes { layout, announce, focus: moved.then_some(focus), page }
    }

    /// The tree's action for what a reader did to element `id`, if the element takes it.
    #[cfg(any(target_os = "ios", test))]
    pub(crate) fn request(&mut self, id: u64, gesture: Gesture) -> Option<ActionRequest> {
        let (request, paging) = self.resolve(id, gesture)?;
        if paging.is_some() { self.paging = paging; }
        Some(request)
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        const GESTURES: [(Gesture, &str); 8] = [
            (Gesture::Activate, "activate"), (Gesture::Increment, "increment"), (Gesture::Decrement, "decrement"),
            (Gesture::Menu, "menu"), (Gesture::Scroll(Toward::Up), "scroll-up"), (Gesture::Scroll(Toward::Down), "scroll-down"),
            (Gesture::Scroll(Toward::Left), "scroll-left"), (Gesture::Scroll(Toward::Right), "scroll-right"),
        ];
        let elements = self.elements.iter().map(|e| {
            let node = &self.nodes[&e.id];
            let mut v = serde_json::json!({ "label": e.label, "traits": traits::names(e.traits), "frame": e.rect.map(|x| (x * 10.0).round() / 10.0) });
            if let Some(key) = node.author_id() { v["key"] = key.into(); }
            if let Some(value) = &e.value { v["value"] = value.as_str().into(); }
            if let Some(hint) = &e.hint { v["hint"] = hint.as_str().into(); }
            if let Some(expanded) = e.expanded { v["expanded"] = expanded.into(); }
            if self.landing == Some(e.id) { v["first"] = true.into(); }
            let actions: Vec<&str> = GESTURES.iter().filter(|(g, _)| self.resolve(e.id, *g).is_some()).map(|(_, name)| *name).collect();
            if !actions.is_empty() { v["actions"] = actions.into(); }
            v
        });
        serde_json::Value::Array(elements.collect())
    }

    fn resolve(&self, id: u64, gesture: Gesture) -> Option<(ActionRequest, Option<(u64, Axis)>)> {
        let node = self.nodes.get(&id)?;
        let mut paging = None;
        let first = |actions: &[Action]| actions.iter().copied().find(|a| node.supports_action(*a));
        let (target, action) = match gesture {
            Gesture::Activate => (id, first(&[Action::Click, Action::Focus])?),
            Gesture::Increment => (id, first(&[Action::Increment])?),
            Gesture::Decrement => (id, first(&[Action::Decrement])?),
            #[cfg(any(target_os = "ios", test))]
            Gesture::Focused => (id, first(&[Action::ScrollIntoView])?),
            Gesture::Menu => (id, first(&[Action::ShowContextMenu])?),
            Gesture::Scroll(toward) => {
                let scrolls = self.elements.iter().find(|e| e.id == id)?.scrolls?;
                let view = self.nodes.get(&scrolls)?;
                let (action, axis) = match toward {
                    Toward::Up => (Action::ScrollUp, Axis::Y),
                    Toward::Down => (Action::ScrollDown, Axis::Y),
                    Toward::Left => (Action::ScrollLeft, Axis::X),
                    Toward::Right => (Action::ScrollRight, Axis::X),
                };
                let (offset, max) = offsets(view, axis);
                let ahead = match toward { Toward::Up | Toward::Left => offset > EDGE, Toward::Down | Toward::Right => offset < max - EDGE };
                if !view.supports_action(action) || !ahead { return None; }
                paging = Some((scrolls, axis));
                (scrolls, action)
            }
        };
        Some((ActionRequest { action, target_tree: TreeId::ROOT, target_node: NodeId(target), data: None }, paging))
    }
}

struct Walk<'a> {
    focus: u64,
    start: Option<u64>,
    landing: Option<u64>,
    nodes: &'a HashMap<u64, Node>,
    masks: &'a HashMap<u64, String>,
    elements: Vec<Element>,
    live: Vec<(u64, String)>,
    seen: Vec<u64>,
}

impl Walk<'_> {
    /// `clip` is what the scroll views around the node show of it: left, top, right, bottom.
    fn node(&mut self, id: NodeId, origin: [f64; 2], root: bool, scrolls: Option<u64>, clip: Option<[f64; 4]>, owner: Option<u64>) {
        let Some(n) = self.nodes.get(&id.0) else { return };
        self.seen.push(id.0);
        let origin = super::read::origin(n, origin, root);
        let bounds = n.bounds().map(|b| [b.x0 + origin[0], b.y0 + origin[1], b.x1 + origin[0], b.y1 + origin[1]]);
        let hidden = n.is_hidden() || n.role() == Role::TextRun;
        let name = super::read::name(n, |id| self.nodes.get(&id.0));
        if matches!(n.live(), Some(Live::Polite | Live::Assertive)) {
            self.live.push((id.0, name.clone().unwrap_or_default()));
        }
        let scrolls = if SCROLLS.iter().any(|a| n.supports_action(*a)) { Some(id.0) } else { scrolls };
        let shown = bounds.map(|b| clip.map_or(b, |clip| within(b, clip)));
        let element = if hidden { None } else { shown.and_then(|shown| element(id.0, n, name, self.masks.get(&id.0).map(String::as_str), shown, scrolls, self.focus)) };
        let clip = if n.clips_children() && !root { shown.or(clip) } else { clip };
        let leaf = element.is_some();
        self.elements.extend(element);
        // An element's name already holds the text inside it; its children are still walked so
        // they stay in the mirror, but add nothing a reader lands on.
        let mark = self.elements.len();
        let owner = owner.or(leaf.then_some(id.0));
        for child in n.children() { self.node(*child, origin, false, scrolls, clip, owner); }
        if self.start == Some(id.0) && !hidden {
            self.landing = owner.or_else(|| self.elements.get(mark).map(|e| e.id));
        }
        if leaf || hidden { self.elements.truncate(mark); }
    }
}

const SCROLLS: [Action; 4] = [Action::ScrollUp, Action::ScrollDown, Action::ScrollLeft, Action::ScrollRight];

const EDGE: f64 = 0.5;

const PAGE: f64 = 0.9;

fn offsets(view: &Node, axis: Axis) -> (f64, f64) {
    match axis {
        Axis::Y => (view.scroll_y().unwrap_or(0.0), view.scroll_y_max().unwrap_or(0.0)),
        Axis::X => (view.scroll_x().unwrap_or(0.0), view.scroll_x_max().unwrap_or(0.0)),
    }
}

fn page(view: &Node, axis: Axis) -> Option<String> {
    let bounds = view.bounds()?;
    let (offset, max) = offsets(view, axis);
    let step = PAGE * match axis { Axis::Y => bounds.height(), Axis::X => bounds.width() };
    if step <= 0.0 { return None; }
    let total = (max / step).ceil().max(0.0) as u32 + 1;
    let at = if offset >= max - EDGE { total } else { ((offset / step).round().max(0.0) as u32 + 1).min(total) };
    Some(format!("Page {at} of {total}"))
}

/// The part of `b` inside `clip`. Something scrolled out of view keeps its place in the order
/// and collapses onto the edge it left by, where no finger finds it; a reader that steps onto
/// it scrolls it back.
fn within(b: [f64; 4], clip: [f64; 4]) -> [f64; 4] {
    let hold = |v: f64, low: f64, high: f64| v.max(low).min(high.max(low));
    [hold(b[0], clip[0], clip[2]), hold(b[1], clip[1], clip[3]), hold(b[2], clip[0], clip[2]), hold(b[3], clip[1], clip[3])]
}

/// The element `n` is, or none for a container and for text with nothing to say. `shown` is
/// the part of it in view: left, top, right, bottom in the app's points.
fn element(id: u64, n: &Node, name: Option<String>, mask: Option<&str>, shown: [f64; 4], scrolls: Option<u64>, focus: u64) -> Option<Element> {
    use traits::*;
    let rect = [shown[0] as f32, shown[1] as f32, (shown[2] - shown[0]) as f32, (shown[3] - shown[1]) as f32];
    if let Some(view) = n.class_name().and_then(crate::native_view::from_a11y_class) {
        return Some(Element { id, label: String::new(), value: None, hint: None, traits: 0, rect, expanded: None, menu: false, editing: false, scrolls, hosted: Some(view) });
    }
    let role = n.role();
    let control = match role {
        Role::Button | Role::DefaultButton | Role::MenuItem | Role::Tab | Role::ListBoxOption | Role::TreeItem | Role::ComboBox
        | Role::RadioButton | Role::MenuItemRadio | Role::Splitter => BUTTON,
        Role::CheckBox | Role::Switch | Role::MenuItemCheckBox => BUTTON | TOGGLE,
        Role::Link => LINK,
        Role::Slider | Role::SpinButton => ADJUSTABLE,
        Role::SearchInput => SEARCH | TEXT_FIELD,
        Role::TextInput | Role::MultilineTextInput => TEXT_FIELD,
        Role::PasswordInput => SECURE_TEXT_FIELD,
        _ if n.supports_action(Action::Click) => BUTTON,
        _ => {
            let kind = match role {
                Role::Label | Role::Paragraph | Role::Status | Role::Alert | Role::Tooltip | Role::Note => STATIC_TEXT,
                Role::Heading => STATIC_TEXT | HEADER,
                Role::Image => IMAGE,
                Role::ProgressIndicator | Role::Meter | Role::Timer => UPDATES,
                _ => return None,
            };
            // Text, an image or a gauge with nothing to say is not worth a stop.
            if name.as_deref().is_none_or(|s| s.trim().is_empty()) && n.numeric_value().is_none() { return None; }
            kind
        }
    };
    let mut traits = control;
    if n.is_disabled() { traits |= DISABLED; }
    if n.is_selected() == Some(true) { traits |= SELECTED; }
    let text_field = matches!(role, Role::TextInput | Role::MultilineTextInput | Role::SearchInput);
    let value = match n.toggled() {
        // What `UISwitch` reports, which a reader speaks as on, off or mixed.
        Some(Toggled::True) => Some("1".to_owned()),
        Some(Toggled::False) => Some("0".to_owned()),
        Some(Toggled::Mixed) => Some("2".to_owned()),
        None if role == Role::PasswordInput => mask.map(str::to_owned),
        None if text_field || traits & ADJUSTABLE != 0 || traits & UPDATES != 0 => {
            n.value().map(str::to_owned).or_else(|| n.numeric_value().map(|x| format!("{x}")))
        }
        None => None,
    };
    // An empty field is read by its placeholder, as UIKit's own are.
    let (label, value) = match (name, n.placeholder()) {
        (None, Some(placeholder)) => (placeholder.to_owned(), value),
        (Some(name), Some(placeholder)) if name == placeholder && value.as_deref().is_none_or(str::is_empty) => (name, None),
        (name, Some(placeholder)) if value.as_deref().is_none_or(str::is_empty) => (name.unwrap_or_default(), Some(placeholder.to_owned())),
        (name, _) => (name.unwrap_or_default(), value),
    };
    let value = value.filter(|v| !v.is_empty());
    let menu = n.supports_action(Action::ShowContextMenu);
    Some(Element { id, label, value, hint: n.description().map(str::to_owned), traits, rect, expanded: n.is_expanded(), menu, editing: id == focus && traits & TEXT_FIELD != 0, scrolls, hosted: None })
}

impl crate::runtime::RuntimeCore {
    pub(crate) fn a11y_first(&self) -> Option<u64> {
        let nodes = &self.tree.nodes;
        let marked = nodes.iter().position(|n| n.a11y.more.as_ref().is_some_and(|more| more.first))?;
        let inside = (marked..self.subtree_end(marked)).find(|&j| self.a11y_in_tree(j));
        let unseen = |j: usize| {
            let n = &nodes[j];
            n.is_moved_stub() || n.a11y.hidden || n.vis.opacity <= 0.01 || self.is_exiting(n.id) || !self.scene.contains(n.id)
        };
        let around = || {
            for j in std::iter::successors(Some(marked), |&p| nodes[p].parent) {
                if unseen(j) { return None }
                if j != marked && self.a11y_in_tree(j) { return Some(j) }
            }
            None
        };
        inside.or_else(around).map(|j| super::tree::element_id(nodes[j].id).0)
    }

    pub fn ios_accessibility_json(&self) -> serde_json::Value {
        let mut mirror = Mirror::default();
        mirror.set_masks(self.a11y_masks());
        mirror.set_start(self.a11y_first());
        mirror.apply(self.accessibility_tree());
        mirror.json()
    }
}

#[cfg(test)]
#[path = "ios_tests.rs"]
mod tests;
