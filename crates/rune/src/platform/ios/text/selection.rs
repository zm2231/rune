#![allow(unsafe_code)]

use crate::platform::objc_boundary::guarded;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, extern_class, msg_send};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::NSArray;

const WRITING_NATURAL: isize = -1;
const FILL_SUPERVIEW: usize = 2 | 16;
pub(crate) const MENU_CLEARANCE: f32 = 48.0;

extern_class!(
    // SAFETY: UIKit's class of this name is an `NSObject` subclass that UIKit keeps to the main
    // thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UITextSelectionRect"]
    struct UITextSelectionRect;
);

#[derive(Clone, Copy)]
struct Piece {
    rect: CGRect,
    start: bool,
    end: bool,
}

define_class!(
    // SAFETY: `UITextSelectionRect` is made to be subclassed with these overridden, and
    // `SelectionRect` has no `Drop`.
    #[unsafe(super(UITextSelectionRect, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTextSelectionRect"]
    #[ivars = Piece]
    struct SelectionRect;

    impl SelectionRect {
        #[unsafe(method(rect))]
        fn rect(&self) -> CGRect { guarded("rect", || { self.ivars().rect }) }

        #[unsafe(method(writingDirection))]
        fn writing_direction(&self) -> isize { guarded("writingDirection", || { WRITING_NATURAL }) }

        #[unsafe(method(containsStart))]
        fn contains_start(&self) -> bool { guarded("containsStart", || { self.ivars().start }) }

        #[unsafe(method(containsEnd))]
        fn contains_end(&self) -> bool { guarded("containsEnd", || { self.ivars().end }) }

        #[unsafe(method(isVertical))]
        fn is_vertical(&self) -> bool { guarded("isVertical", || { false }) }
    }
);

pub(crate) fn cg_rect([x, y, w, h]: [f32; 4]) -> CGRect {
    CGRect::new(CGPoint::new(x as CGFloat, y as CGFloat), CGSize::new(w as CGFloat, h as CGFloat))
}

pub(crate) fn rects(mtm: MainThreadMarker, rects: &[[f32; 4]]) -> Retained<NSArray> {
    let last = rects.len().saturating_sub(1);
    let pieces: Vec<Retained<AnyObject>> = rects.iter().enumerate().map(|(index, rect)| {
        let this = SelectionRect::alloc(mtm).set_ivars(Piece { rect: cg_rect(*rect), start: index == 0, end: index == last });
        // SAFETY: `init` is `UITextSelectionRect`'s initializer.
        let piece: Retained<SelectionRect> = unsafe { msg_send![super(this), init] };
        Retained::into_super(Retained::into_super(Retained::into_super(piece)))
    }).collect();
    NSArray::from_retained_slice(&pieces)
}

#[derive(Default)]
pub(crate) struct Display {
    interaction: Option<Retained<AnyObject>>,
    loupe: Option<Retained<AnyObject>>,
    shown: bool,
}

impl Display {
    pub(crate) fn shown(&self) -> bool { self.shown }

    pub(crate) fn show(&mut self, input: &AnyObject, host: &AnyObject, on: bool) {
        if on && self.interaction.is_none() {
            let Some(class) = AnyClass::get(c"UITextSelectionDisplayInteraction") else { return };
            // SAFETY: a plain view that fills the host and takes no touches, so nothing the
            // interaction adds to it can take a finger from the host. The interaction takes a
            // `UITextInput` and a delegate whose methods are all optional; both are the text
            // input, which the host keeps for as long as it keeps the container.
            self.interaction = Some(unsafe {
                let container: Retained<AnyObject> = msg_send![class!(UIView), new];
                let bounds: CGRect = msg_send![host, bounds];
                let _: () = msg_send![&*container, setFrame: bounds];
                let _: () = msg_send![&*container, setAutoresizingMask: FILL_SUPERVIEW];
                let _: () = msg_send![&*container, setUserInteractionEnabled: false];
                let _: () = msg_send![host, addSubview: &*container];
                let interaction: Allocated<AnyObject> = msg_send![class, alloc];
                let interaction: Retained<AnyObject> = msg_send![interaction, initWithTextInput: input, delegate: input];
                let _: () = msg_send![&*container, addInteraction: &*interaction];
                interaction
            });
        }
        let Some(interaction) = &self.interaction else { return };
        // SAFETY: `UITextSelectionDisplayInteraction`'s `activated` is a `BOOL`, its cursor and
        // highlight views are `UIView`s, and `setNeedsSelectionUpdate` takes nothing.
        unsafe {
            if on != self.shown { let _: () = msg_send![&**interaction, setActivated: on]; }
            if on {
                let cursor: Retained<AnyObject> = msg_send![&**interaction, cursorView];
                let highlight: Retained<AnyObject> = msg_send![&**interaction, highlightView];
                let _: () = msg_send![&*cursor, setHidden: true];
                let _: () = msg_send![&*highlight, setHidden: true];
                let _: () = msg_send![&**interaction, setNeedsSelectionUpdate];
            }
        }
        self.shown = on;
    }

    pub(crate) fn loupe_begin(&mut self, host: &AnyObject, (x, y): (f32, f32)) {
        self.loupe_end();
        let Some(class) = AnyClass::get(c"UITextLoupeSession") else { return };
        // SAFETY: a class method that takes a point, an optional widget view and the view the
        // point is in, and answers a session or nil.
        self.loupe = unsafe {
            msg_send![class, beginLoupeSessionAtPoint: CGPoint::new(x as CGFloat, y as CGFloat), fromSelectionWidgetView: None::<&AnyObject>, inView: host]
        };
    }

    pub(crate) fn loupe_move(&self, (x, y): (f32, f32), caret: CGRect) {
        let Some(loupe) = &self.loupe else { return };
        // SAFETY: a session's move takes a point, the caret's rect and a `BOOL`.
        unsafe { let _: () = msg_send![&**loupe, moveToPoint: CGPoint::new(x as CGFloat, y as CGFloat), withCaretRect: caret, trackingCaret: true]; }
    }

    pub(crate) fn loupe_end(&mut self) {
        let Some(loupe) = self.loupe.take() else { return };
        // SAFETY: `invalidate` takes nothing and hides the loupe.
        unsafe { let _: () = msg_send![&*loupe, invalidate]; }
    }
}
