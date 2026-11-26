#![allow(unsafe_code)]

use objc2::msg_send;
use objc2::DefinedClass;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect};

use super::super::doc::Event;
use super::super::handles::{self, Ends};
use super::super::selection::{self, cg_rect};
use super::RuneTextInput;
use crate::runtime::TouchPhase;

impl RuneTextInput {
    pub(crate) fn menu_anchor(&self) -> CGRect {
        let caret = self.ivars().caret.get();
        if !self.ivars().display.borrow().shown() { return caret; }
        let range = self.ivars().doc.borrow().selection();
        let rects = self.place(range.start).zip(self.ivars().doc.borrow().field_byte(range.end))
            .map(|((geometry, from), to)| geometry.selection_rects(from..to)).unwrap_or_default();
        let Some(bounds) = rects.into_iter().reduce(|[x, y, w, h], [x2, y2, w2, h2]| {
            let (left, top) = (x.min(x2), y.min(y2));
            [left, top, (x + w).max(x2 + w2) - left, (y + h).max(y2 + h2) - top]
        }) else { return caret };
        let [x, y, w, h] = bounds;
        cg_rect([x, y - selection::MENU_CLEARANCE, w, h + selection::MENU_CLEARANCE + handles::REACH])
    }


    pub(super) fn show_handles(&self, wanted: bool) {
        let Some(host) = self.ivars().host.load() else { return };
        let selection = self.ivars().doc.borrow().selection();
        // SAFETY: a `UIResponder` method of this signature.
        let first: bool = unsafe { msg_send![self, isFirstResponder] };
        let on = wanted && first && !selection.is_empty() && self.place(selection.start).is_some();
        self.ivars().display.borrow_mut().show(self, &host, on);
    }

    fn ends(&self) -> Option<Ends> {
        if !self.ivars().display.borrow().shown() { return None; }
        let selection = self.ivars().doc.borrow().selection();
        let start = self.place(selection.start).and_then(|(geometry, byte)| geometry.caret(byte))?;
        let end = self.place(selection.end).and_then(|(geometry, byte)| geometry.caret(byte))?;
        Some(Ends { selection, start, end })
    }

    pub(crate) fn dragging_handle(&self) -> bool {
        self.ivars().handles.borrow().dragging()
    }

    pub(crate) fn touch(&self, touch: u64, phase: TouchPhase, at: (f32, f32)) -> bool {
        let dragging = self.ivars().handles.borrow().dragging();
        let (event, caret) = match phase {
            TouchPhase::Started => (handles::Event::Down { touch, at, ends: self.ends() }, None),
            TouchPhase::Moved => {
                let Some((x, y)) = self.ivars().handles.borrow().aim(at) else { return false };
                let nearest = self.closest(CGPoint::new(x as CGFloat, y as CGFloat));
                let caret = self.place(nearest).and_then(|(geometry, byte)| geometry.caret(byte)).map(cg_rect);
                (handles::Event::Move { touch, at, nearest }, caret)
            }
            TouchPhase::Ended => (handles::Event::Up { touch, cancelled: false }, None),
            TouchPhase::Cancelled => (handles::Event::Up { touch, cancelled: true }, None),
        };
        self.handles(event, caret);
        dragging || self.ivars().handles.borrow().dragging()
    }

    pub(crate) fn hold(&self, phase: TouchPhase, at: (f32, f32)) -> bool {
        let point = CGPoint::new(at.0 as CGFloat, at.1 as CGFloat);
        let event = match phase {
            TouchPhase::Started => {
                let selection = self.ivars().doc.borrow().selection();
                if self.place(selection.end).is_none() { return false; }
                handles::Event::Hold { at, nearest: self.closest(point), selection }
            }
            TouchPhase::Moved => handles::Event::HoldMove { at, nearest: self.closest(point) },
            TouchPhase::Ended => handles::Event::HoldEnd { cancelled: false },
            TouchPhase::Cancelled => handles::Event::HoldEnd { cancelled: true },
        };
        let caret = match &event {
            handles::Event::Hold { nearest, .. } | handles::Event::HoldMove { nearest, .. } => {
                self.place(*nearest).and_then(|(geometry, byte)| geometry.caret(byte)).map(cg_rect)
            }
            _ => None,
        };
        self.handles(event, caret);
        self.ivars().handles.borrow().holding()
    }

    pub(super) fn handles(&self, event: handles::Event, caret: Option<CGRect>) {
        let effects = handles::step(&mut self.ivars().handles.borrow_mut(), event);
        let Some(host) = self.ivars().host.load() else { return };
        for effect in effects {
            match effect {
                handles::Effect::Select(range) => self.edit(Event::Select(range)),
                handles::Effect::LoupeBegin(at) => {
                    host.hide_menu();
                    self.ivars().display.borrow_mut().loupe_begin(&host, at);
                }
                handles::Effect::LoupeMove(at) => self.ivars().display.borrow().loupe_move(at, caret.unwrap_or(self.ivars().caret.get())),
                handles::Effect::LoupeEnd => self.ivars().display.borrow_mut().loupe_end(),
                handles::Effect::Menu => {
                    let range = self.ivars().doc.borrow().selection();
                    let rect = self.place(range.start).zip(self.ivars().doc.borrow().field_byte(range.end))
                        .and_then(|((geometry, from), to)| geometry.first_rect(from..to));
                    if let Some([x, y, w, h]) = rect { host.field_menu(x + w / 2.0, y + h / 2.0); }
                }
            }
        }
    }
}
