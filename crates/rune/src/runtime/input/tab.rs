use super::*;
use crate::runtime::focus::LIST_ROLES;

impl RuntimeCore {
    pub(in crate::runtime) fn tab_order(&self) -> Vec<usize> {
        let all: Vec<_> = (0..self.tree.nodes.len()).filter(|&i| self.can_focus(i) && self.tree.nodes[i].focus.order >= 0).collect();
        let mut order: Vec<_> = all.iter().copied().filter(|&i| {
            let Some(g) = self.focus_group(i) else { return true; };
            let chosen = all.iter().copied().filter(|&j| self.focus_group(j) == Some(g))
                .find(|&j| self.kbd.focus.is(&self.tree.nodes[j].path))
                .or_else(|| all.iter().copied().find(|&j| self.focus_group(j) == Some(g) && self.tree.nodes[j].focus.selected))
                .or_else(|| all.iter().copied().find(|&j| self.focus_group(j) == Some(g)));
            chosen == Some(i)
        }).collect();
        order.sort_by_key(|&i| { let n = self.tree.nodes[i].focus.order; (if n > 0 { n } else { i32::MAX }, i) });
        order
    }

    fn tab_stops(&self) -> Vec<usize> {
        let mut order = self.tab_order();
        if !self.kbd.tab_visits_all() {
            // A control that owns its keyboard (`El::on_key`) is a list of its own.
            order.retain(|&i| self.tree.nodes[i].input.is_some() || self.tree.nodes[i].focus.key.is_some()
                || self.is_caret_browsing_node(&self.tree.nodes[i])
                || self.focus_group(i).is_some_and(|g| LIST_ROLES.contains(&self.tree.nodes[g].role)));
        }
        let hosts = self.host_stops();
        if !hosts.is_empty() {
            order.extend(hosts.into_iter().filter(|i| !order.contains(i)).collect::<Vec<_>>());
            order.sort_by_key(|&i| { let n = self.tree.nodes[i].focus.order; (if n > 0 { n } else { i32::MAX }, i) });
        }
        order
    }

    pub(super) fn tab_key(&mut self, app: &mut dyn AppDriver, ev: &KeyEvent, tx: &mut Tx) -> bool {
        if self.focused_index().is_some_and(|i| self.edit.link_tab(&self.tree.nodes[i], ev.mods.shift)) { return true; }
        let order = self.tab_stops();
        if order.is_empty() { return self.active_modal().is_some(); }
        let current = order.iter().position(|&i| self.kbd.focus.is(&self.tree.nodes[i].path));
        // With nothing focused, Tab goes on from where focus was or the pointer pressed last;
        // from an element outside the Tab order (`tab_index(-1)`, a region a request focused),
        // it goes on from that element's place, as a browser's does.
        let from = self.kbd.focus.tab_start().filter(|_| self.commit.defaults.web_focus).and_then(|p| self.tree.nodes.iter().position(|n| n.path == p))
            .or_else(|| self.focused_index().filter(|_| current.is_none()));
        let target = order[neighbour(&order, current, from, ev.mods.shift)];
        self.tab_to(app, target, ev.mods.shift, tx);
        true
    }

    pub(in crate::runtime) fn tab_past_host(&mut self, app: &mut dyn AppDriver, view: crate::NativeViewId, back: bool, tx: &mut Tx) -> bool {
        let Some(host) = self.host_of(view) else { return false };
        let order = self.tab_stops();
        if order.is_empty() { return false; }
        let current = order.iter().position(|&i| i == host);
        let target = order[neighbour(&order, current, Some(host), back)];
        self.tab_to(app, target, back, tx)
    }

    fn tab_to(&mut self, app: &mut dyn AppDriver, target: usize, back: bool, tx: &mut Tx) -> bool {
        if let Some(view) = self.native_view_of(target) {
            return self.native_view_event(app, crate::native_view::handoff::Event::TabInto { view, back }, tx);
        }
        let path = self.tree.nodes[target].path.clone();
        let reveal_caret = self.tree.nodes[target].input.as_ref().is_some_and(|i| i.reveal_caret_on_focus);
        let changed = self.set_focus(app, Some(path.clone()), true);
        if !reveal_caret {
            tx.scroll_to.push(crate::runtime::ScrollReveal::into_view(None, path));
        }
        self.edit.enter_links(&self.tree.nodes[target], back);
        changed
    }
}

fn neighbour(order: &[usize], current: Option<usize>, from: Option<usize>, back: bool) -> usize {
    match (current, from) {
        (Some(i), _) if back => (i + order.len() - 1) % order.len(),
        (Some(i), _) => (i + 1) % order.len(),
        (None, Some(k)) if back => order.iter().rposition(|&j| j < k).unwrap_or(order.len() - 1),
        (None, Some(k)) => order.iter().position(|&j| j > k).unwrap_or(0),
        (None, None) if back => order.len() - 1,
        (None, None) => 0,
    }
}
