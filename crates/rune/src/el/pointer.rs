//! Pointer builders: cursor, window drag and resize regions, IME placement, hit targets,
//! pointer-move and hover handlers, and the handlers bound once a tree path is known.

use super::*;

impl El {
    /// The pointer's shape while it is over this element or a descendant that sets none.
    /// Applies while the pointer is over a hit target (a pressable, draggable, hoverable or
    /// text element) inside this one.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.handlers.cursor = Some(cursor);
        // The app's own cursor, which `El::selection_enabled(false)` leaves alone.
        if let Some(t) = &mut self.text && t.flags.has(crate::el::text_spec::TextFlags::SELECTABLE_CURSOR) { t.flags.set(crate::el::text_spec::TextFlags::SELECTABLE_CURSOR, false); }
        self
    }
    /// Pressing this element moves the window (or a window-backed overlay; pair with `.hit()`
    /// under `OverlayInput::Regions`), and on ordinary non-panel windows a double press zooms or
    /// minimises it as the system's titlebar setting says (overlays and macOS non-activating
    /// panels ignore the double press). Hit targets inside it keep their presses.
    pub fn window_drag(mut self) -> Self {
        self.flags.set_window_drag(WindowDrag::Drag);
        self
    }
    /// A part of a `.window_drag()` band that keeps its presses: a press, double press or
    /// secondary click on it, or on a bare child of it, neither moves, zooms nor opens the menu
    /// of the window. Controls (`on_press`, `on_drag`, focusable elements, text fields) already
    /// keep theirs; this is for the parts between them, such as the gaps of a toolbar group.
    /// Like any hit target, it takes presses from what lies under it.
    pub fn no_window_drag(mut self) -> Self {
        self.flags.set_window_drag(WindowDrag::NoDrag);
        self
    }
    /// Pressing this element starts an interactive OS/compositor-driven window resize from
    /// `edge` (Win32 sizing borders, X11 `_NET_WM_MOVERESIZE`, Wayland `xdg_toplevel.resize`),
    /// for borderless windows that draw their own frame.
    ///
    /// While hovered, the element shows the matching resize cursor (`Cursor::ResizeNs`,
    /// `Cursor::ResizeEw`, `Cursor::ResizeNwse`, or `Cursor::ResizeNesw`) unless an explicit
    /// [`El::cursor`] is set on it. Interactive children, [`El::no_window_drag`], inert
    /// subtrees, and active modals block the resize just as they block [`El::window_drag`].
    /// Ignored on macOS, on the web, in overlays, and in headless runs (custom hosts and
    /// headless tests observe the edge via
    /// [`Runtime::take_resize_action`](crate::Runtime::take_resize_action)).
    ///
    /// ```
    /// # use rune::{ResizeEdge, el};
    /// # let _ =
    /// el().pin(None, None, Some(0.0), Some(0.0))
    ///     .size(12.0, 12.0)
    ///     .window_resize(ResizeEdge::SouthEast)
    /// # ;
    /// ```
    pub fn window_resize(mut self, edge: crate::ResizeEdge) -> Self {
        self.flags.set_window_drag(WindowDrag::Resize(edge));
        self.cold_mut().window_resize = Some(edge);
        self
    }
    /// The caret of an editor the app draws itself rather than with `text_input`: while this
    /// element is drawn and no text field has focus, the window turns its input method on (dead
    /// keys, CJK, the emoji picker, dictation), places the candidate window at this element's
    /// drawn rect, and hands what it composes to [`App::on_ime`](crate::App::on_ime). Put it on
    /// the caret you draw, only while the editor has the keyboard. A focused field's caret wins,
    /// and a focused secure field keeps the input method off.
    pub fn ime_cursor(mut self) -> Self {
        self.flags.set(ElFlags::IME_CURSOR, true);
        self
    }
    /// With [`Self::ime_cursor`]: on macOS, while this caret has the input method, Option is the
    /// Alt key. A key pressed with it arrives as the key it's on with `mods.alt` (`Tx::key_base`
    /// its character) instead of the character the layout makes with Option, and the input method
    /// composes nothing for it (winit's `OptionAsAlt::Both`): a terminal's Option-as-Meta. It
    /// changes nothing elsewhere, nor while `on` is false.
    ///
    /// To treat only the left or only the right Option key as Alt while leaving the other free for
    /// layout characters, use [`Self::ime_option_as_alt_mode`].
    pub fn ime_option_as_alt(self, on: bool) -> Self {
        self.ime_option_as_alt_mode(crate::OptionAsAlt::from(on))
    }
    /// With [`Self::ime_cursor`]: on macOS, choose which Option key acts as Alt while this caret
    /// has the input method (winit's `OptionAsAlt`: `None`, `OnlyLeft`, `OnlyRight`, `Both`).
    ///
    /// Any key pressed with a selected Option key arrives as the key it is on with `mods.alt`
    /// (`Tx::key_base` its character) instead of the character the keyboard layout composes, and
    /// the input method composes nothing for it. An unselected Option key keeps composing the
    /// layout's characters (for example, right Option on a German layout still types `[` and `@`
    /// while [`OptionAsAlt::Left`] is active). Ignored off macOS, ignored while `mode` is
    /// [`OptionAsAlt::None`], and reset to `None` whenever a text field or another caret takes
    /// the input method.
    ///
    /// ```
    /// use rune::{OptionAsAlt, el};
    ///
    /// let caret = el().ime_cursor().ime_option_as_alt_mode(OptionAsAlt::Left);
    /// # let _ = caret;
    /// ```
    pub fn ime_option_as_alt_mode(mut self, mode: crate::OptionAsAlt) -> Self {
        if mode != crate::OptionAsAlt::None || !self.cold.is_empty() {
            self.cold_mut().option_as_alt = mode;
        }
        self
    }
    /// Mark the drawn, ancestor-clipped rectangle as native overlay input. Does not add a handler.
    pub fn hit(mut self) -> Self {
        self.vis_flags.set(VisualFlags::HIT, true);
        self
    }

    /// Observe the pointer moving over this element, in window points, without making a native
    /// overlay input region. [`El::on_pointer_move_local`] gives the element's own points.
    #[track_caller]
    pub fn on_pointer_move<S: 'static>(mut self, f: impl Fn(&mut S, PointerPress) + 'static) -> Self {
        self.handlers.on_pointer_move = Some(Rc::new(move |s, p, _| f(downcast::<S>(s), p.window())));
        #[cfg(debug_assertions)]
        { self.handlers.sites.pointer_move = Some(std::panic::Location::caller()); }
        self
    }
    /// [`El::on_pointer_move`] with the point in this element's own layout points as well as
    /// the window's (see [`PointerEvent`]), and a [`Tx`]: a canvas tracks the pointer in its own
    /// coordinates at any zoom, and a handler that only moves a [`Live`](crate::Live) value
    /// (a crosshair) says so with [`Tx::live_only`].
    ///
    /// ```
    /// # use rune::{Live, el};
    /// # struct Canvas { crosshair: Live<[f32; 2]> }
    /// # let _ = el()
    /// .on_pointer_move_local(|s: &mut Canvas, p, tx| {
    ///     s.crosshair.set([p.x, p.y]);
    ///     tx.live_only();
    /// })
    /// # ;
    /// ```
    #[track_caller]
    pub fn on_pointer_move_local<S: 'static>(mut self, f: impl Fn(&mut S, PointerEvent, &mut Tx) + 'static) -> Self {
        self.handlers.on_pointer_move = Some(Rc::new(move |s, p, tx| f(downcast::<S>(s), p, tx)));
        #[cfg(debug_assertions)]
        { self.handlers.sites.pointer_move = Some(std::panic::Location::caller()); }
        self
    }
    /// [`El::on_pointer_move_local`] for a component that doesn't read the app's state (a scroll
    /// bar that widens under the pointer), so it works in any app.
    #[track_caller]
    pub fn on_pointer_move_any(mut self, f: impl Fn(PointerEvent, &mut Tx) + 'static) -> Self {
        self.handlers.on_pointer_move = Some(Rc::new(move |_, p, tx| f(p, tx)));
        #[cfg(debug_assertions)]
        { self.handlers.sites.pointer_move = Some(std::panic::Location::caller()); }
        self
    }

    /// Called once when the pointer enters this hit target, not on animation frames.
    #[track_caller]
    pub fn on_hover<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.handlers.on_hover = Some(Rc::new(move |s, _| f(downcast::<S>(s))));
        #[cfg(debug_assertions)]
        { self.handlers.sites.hover = Some(std::panic::Location::caller()); }
        self
    }

    /// Called with `true` when the pointer enters this element's visible rect and `false` when
    /// it leaves, descendants included (moving onto a child button is not a leave). It is not a
    /// hit target, so it never takes presses or hover from its children. A held press keeps the
    /// pointer inside until it releases.
    #[track_caller]
    pub fn on_hover_change<S: 'static>(mut self, f: impl Fn(&mut S, bool, &mut Tx) + 'static) -> Self {
        self.handlers.on_hover_change = Some(Rc::new(move |s, inside, tx| f(downcast::<S>(s), inside, tx)));
        #[cfg(debug_assertions)]
        { self.handlers.sites.hover_change = Some(std::panic::Location::caller()); }
        self
    }
    /// [`El::on_hover_change`] for a component that doesn't read the app's state.
    #[track_caller]
    pub fn on_hover_change_any(mut self, f: impl Fn(bool, &mut Tx) + 'static) -> Self {
        self.handlers.on_hover_change = Some(Rc::new(move |_, inside, tx| f(inside, tx)));
        #[cfg(debug_assertions)]
        { self.handlers.sites.hover_change = Some(std::panic::Location::caller()); }
        self
    }
    /// [`El::on_hover_change_any`] after any hover handler the element already has, which keeps
    /// running: a component that follows the hover of an element it doesn't build. The change is
    /// live-only when both handlers say so.
    #[doc(hidden)]
    pub fn chain_hover_change_any(mut self, f: impl Fn(bool, &mut Tx) + 'static) -> Self {
        self.handlers.on_hover_change = Some(match self.handlers.get_mut_if_some().and_then(|h| h.on_hover_change.take()) {
            Some(prev) => Rc::new(move |s, inside, tx| {
                prev(s, inside, tx);
                let prev_live = std::mem::take(&mut tx.live_only);
                f(inside, tx);
                tx.live_only &= prev_live;
            }),
            None => Rc::new(move |_, inside, tx| f(inside, tx)),
        });
        self
    }

    /// Delay [`Self::on_hover_change`]: `true` arrives once the pointer has rested inside for
    /// `enter`, and `false` once it has stayed out for `leave`. A pass shorter than the delay
    /// reports nothing, and a held button never starts the wait (a press during it cancels it;
    /// the wait starts again when the pointer next moves). Losing window focus ends a shown hover
    /// and cancels a wait; the pointer's next move over the window, still unfocused, starts the
    /// wait again, so tooltips and hover cards open in a window behind another. A sidebar that
    /// peeks open when the pointer rests at the window's edge uses `hover_delays(40 ms, 0)`.
    pub fn hover_delays(mut self, enter: std::time::Duration, leave: std::time::Duration) -> Self {
        let d = [enter.as_secs_f32(), leave.as_secs_f32()];
        if d != [0.0; 2] || !self.handlers.is_empty() {
            self.handlers.hover_delay = d;
        }
        self
    }

    /// Track whether the pointer is still inside this element's hit bounds and inside the window
    /// during a held press, rather than holding [`Self::on_hover_change`] active until release.
    /// Moving a held press outside this element's clipped (and `press_scale`- /
    /// `press_offset`-compensated) bounds, or receiving a host pointer-leave while captured,
    /// fires `on_hover_change(false)` and retargets hover/press paint tracks off; moving back
    /// inside before release fires `on_hover_change(true)` and restores them without ending
    /// pointer capture or interrupting drags.
    ///
    /// Off by default so ancestors and drag containers keep Rune's drag-friendly retention.
    ///
    /// ```
    /// # use rune::{Cx, El, Env, Runtime, col, el};
    /// # #[derive(Default)] struct S { inside: Vec<bool> }
    /// # fn view(_: &S, _: &mut Cx<S>) -> El {
    /// col().size(200.0, 100.0).child(
    ///     el().key("btn").size(80.0, 40.0)
    ///         .hover_tracks_bounds()
    ///         .on_press(|_: &mut S| {})
    ///         .on_hover_change(|s: &mut S, inside, _| s.inside.push(inside)),
    /// )
    /// # }
    /// # let mut rt = Runtime::new(S::default(), view, (200.0, 100.0), Env::new(true, 1.0));
    /// # rt.frame(0.0);
    /// # rt.pointer_move(20.0, 20.0);
    /// # rt.pointer_down(20.0, 20.0);
    /// # rt.frame(0.0);
    /// # rt.pointer_move(150.0, 20.0);
    /// # rt.frame(0.0);
    /// # rt.pointer_move(20.0, 20.0);
    /// # rt.frame(0.0);
    /// # assert_eq!(rt.state.inside, [true, false, true]);
    /// ```
    pub fn hover_tracks_bounds(mut self) -> Self {
        self.handlers.hover_tracks_bounds = true;
        self
    }

    #[doc(hidden)]
    pub fn wrap_scroll_area(mut self, fallback_site: (&'static str, u32, u32)) -> (String, String, Self, Self) {
        let key = self.key.clone().unwrap_or_else(|| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            fallback_site.hash(&mut h);
            let v = h.finish();
            let k = format!("scroll-{:08x}", (v ^ (v >> 32)) as u32);
            self.key = Some(k.clone());
            k
        });
        let area = format!("{key}-scroll-area");
        let mut outer = col().key(area.clone());
        outer.style = self.style.take_box_placement();
        if let Some(c) = self.cold.get_mut_if_some() {
            let (fit, above, above_boundary, transition) = (std::mem::replace(&mut c.aspect_fit, 0.0), c.above.take(), c.above_boundary.take(), c.transition.take());
            if fit > 0.0 || above.is_some() || above_boundary.is_some() || transition.is_some() {
                let o = outer.cold_mut();
                (o.aspect_fit, o.above, o.above_boundary, o.transition) = (fit, above, above_boundary, transition);
            }
        }
        outer.vis.cold_mut().clip_to_content = true;
        (area, key, outer, self)
    }

    /// Runs `f` during flattening once this element's full tree path is known, merging any
    /// handlers and overlay children `f` adds into the flattened node. Multiple calls chain in
    /// order. For components like `rune-kit`'s `ScrollBar` whose state and targets scope by tree
    /// path even though elements build bottom-up before ancestors' keys attach.
    #[doc(hidden)]
    pub fn bind_path(mut self, f: impl Fn(&str, &mut PathBind) + 'static) -> Self {
        let cold = self.cold_mut();
        cold.on_path = Some(match cold.on_path.take() {
            Some(prev) => Rc::new(move |path, bind| {
                prev(path, bind);
                f(path, bind);
            }),
            None => Rc::new(f),
        });
        self
    }
}

/// What [`El::bind_path`] can attach once an element's resolved tree path is known: chained
/// pointer, hover and scroll-offset handlers, and overlay children.
#[doc(hidden)]
#[derive(Default)]
pub struct PathBind {
    pub(crate) on_pointer_move: Option<Rc<dyn Fn(PointerEvent, &mut Tx)>>,
    pub(crate) on_hover_change: Option<Rc<dyn Fn(bool, &mut Tx)>>,
    pub(crate) on_scroll_offset: Option<ScrollOffsetFn>,
    #[cfg(debug_assertions)]
    pub(crate) pointer_move_site: Option<&'static std::panic::Location<'static>>,
    #[cfg(debug_assertions)]
    pub(crate) hover_change_site: Option<&'static std::panic::Location<'static>>,
    pub(crate) children: Vec<El>,
}

impl PathBind {
    #[track_caller]
    pub fn on_pointer_move_any(&mut self, f: impl Fn(PointerEvent, &mut Tx) + 'static) -> &mut Self {
        self.on_pointer_move = Some(match self.on_pointer_move.take() {
            Some(prev) => Rc::new(move |p, tx| {
                prev(p, tx);
                let prev_live = std::mem::take(&mut tx.live_only);
                f(p, tx);
                tx.live_only &= prev_live;
            }),
            None => Rc::new(f),
        });
        #[cfg(debug_assertions)]
        { self.pointer_move_site = Some(std::panic::Location::caller()); }
        self
    }

    #[track_caller]
    pub fn on_hover_change_any(&mut self, f: impl Fn(bool, &mut Tx) + 'static) -> &mut Self {
        self.on_hover_change = Some(match self.on_hover_change.take() {
            Some(prev) => Rc::new(move |inside, tx| {
                prev(inside, tx);
                let prev_live = std::mem::take(&mut tx.live_only);
                f(inside, tx);
                tx.live_only &= prev_live;
            }),
            None => Rc::new(f),
        });
        #[cfg(debug_assertions)]
        { self.hover_change_site = Some(std::panic::Location::caller()); }
        self
    }

    pub fn on_scroll_offset(&mut self, f: impl Fn(crate::ScrollPos) + 'static) -> &mut Self {
        self.on_scroll_offset = Some(match self.on_scroll_offset.take() {
            Some(prev) => Rc::new(move |at| {
                prev(at);
                f(at);
            }),
            None => Rc::new(f),
        });
        self
    }

    pub fn child(&mut self, child: El) -> &mut Self {
        self.children.push(child);
        self
    }
}
