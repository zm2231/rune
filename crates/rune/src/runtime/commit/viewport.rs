use super::*;
use crate::el::ScrollAxes;

#[derive(Clone, Copy)]
pub(super) enum ScrollXChild {
    Single { vertical: bool },
    Many { vertical: bool },
}

pub(super) fn scroll_x_viewport(style: &mut CompactStyle, axes: ScrollAxes, children: usize) -> Option<ScrollXChild> {
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

pub(super) fn apply_scroll_x_child(style: &mut CompactStyle, sx: ScrollXChild) {
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
