use super::*;
use crate::el::ScrollAxes;
use taffy::FlexboxContainerStyle;

#[derive(Clone, Copy)]
enum ScrollXChild {
    Single { vertical: bool },
    Many { vertical: bool },
}

#[derive(Clone, Copy, Default)]
pub(super) struct ViewportChild {
    sx: Option<ScrollXChild>,
    safe_self: bool,
}

impl ViewportChild {
    pub(super) fn restyles(self, style: &CompactStyle) -> bool {
        self.sx.is_some() || (self.safe_self && style.align_self().is_some_and(|a| safe_items(a) != a))
    }

    pub(super) fn apply(self, style: &mut CompactStyle) {
        if let Some(sx) = self.sx { apply_scroll_x_child(style, sx); }
        if self.safe_self && let Some(a) = style.align_self() { style.set_align_self(Some(safe_items(a))); }
    }
}

pub(super) fn viewport(style: &mut CompactStyle, axes: ScrollAxes, children: usize) -> ViewportChild {
    let sx = scroll_x_viewport(style, axes, children);
    let row = matches!(style.flex_direction(), FlexDirection::Row | FlexDirection::RowReverse);
    let (main, cross) = if row { (axes.x(), axes.y()) } else { (axes.y(), axes.x()) };
    if main { style.set_justify_content(safe_content(style.justify_content())); }
    if cross { style.set_align_items(safe_items(style.align_items())); }
    ViewportChild { sx, safe_self: cross }
}

fn safe_items(a: AlignItems) -> AlignItems {
    let shifts = matches!(a.keyword, AlignItemsKeyword::Center | AlignItemsKeyword::End | AlignItemsKeyword::FlexEnd | AlignItemsKeyword::SelfEnd);
    if shifts && a.safety == AlignmentSafety::Default { a.with_safety(AlignmentSafety::Safe) } else { a }
}

fn safe_content(j: JustifyContent) -> JustifyContent {
    let shifts = matches!(j.keyword, AlignContentKeyword::Center | AlignContentKeyword::End | AlignContentKeyword::FlexEnd);
    if shifts && j.safety == AlignmentSafety::Default { j.with_safety(AlignmentSafety::Safe) } else { j }
}

fn scroll_x_viewport(style: &mut CompactStyle, axes: ScrollAxes, children: usize) -> Option<ScrollXChild> {
    if !axes.x() {
        return None;
    }
    if children == 1 {
        style.set_flex_direction(FlexDirection::Row);
        Some(ScrollXChild::Single { vertical: axes.y() })
    } else {
        if !axes.y() { style.set_flex_direction(FlexDirection::Row); }
        Some(ScrollXChild::Many { vertical: axes.y() })
    }
}

fn apply_scroll_x_child(style: &mut CompactStyle, sx: ScrollXChild) {
    match sx {
        ScrollXChild::Single { vertical } => unwrapped_width(style, vertical),
        ScrollXChild::Many { vertical } => {
            let w = style.size().width;
            let w_full = w == Dimension::percent(1.0);
            if w.is_auto() || w_full {
                style.set_width(Dimension::max_content());
                if w_full && style.min_size().width.is_auto() {
                    style.set_min_width(percent(1.0));
                }
            }
            style.set_flex_shrink(0.0);
            if vertical && style.align_self().is_none() {
                style.set_align_self(Some(AlignSelf::FLEX_START));
            }
        }
    }
}

fn unwrapped_width(style: &mut CompactStyle, vertical: bool) {
    let w = style.size().width;
    if w.is_auto() || w == Dimension::percent(1.0) {
        style.set_width(Dimension::max_content());
        if style.min_size().width.is_auto() { style.set_min_width(percent(1.0)); }
    }
    style.set_flex_shrink(0.0);
    if vertical && style.align_self().is_none() { style.set_align_self(Some(AlignSelf::FLEX_START)); }
}

#[cfg(test)]
#[path = "viewport_tests.rs"]
mod tests;
