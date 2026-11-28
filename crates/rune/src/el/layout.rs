//! Layout builders: size, padding, gaps, flex and grid placement, and absolute pins.

use super::*;

pub(super) mod anchor;
pub(super) mod direction;
mod growth;
mod pixel_snap;
mod ride_trailing;
pub(crate) mod watch;

impl El {
    #[inline]
    pub fn w(mut self, w: f32) -> Self {
        self.set_w(w);
        self
    }
    /// [`El::w`] in place.
    #[inline]
    pub fn set_w(&mut self, w: f32) -> &mut Self {
        self.style.set_width(length(w));
        self
    }
    #[inline]
    pub fn h(mut self, h: f32) -> Self {
        self.set_h(h);
        self
    }
    /// [`El::h`] in place.
    #[inline]
    pub fn set_h(&mut self, h: f32) -> &mut Self {
        self.style.set_height(length(h));
        self
    }
    #[inline]
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.set_size(w, h);
        self
    }
    /// [`El::size`] in place.
    #[inline]
    pub fn set_size(&mut self, w: f32, h: f32) -> &mut Self {
        self.set_w(w).set_h(h)
    }
    /// Width over height, as CSS `aspect-ratio`: with one side set (or stretched by the parent)
    /// the other follows; a box with both sides set ignores it. A picture that sets only one
    /// side already takes its image's ratio ([`pixels`](Self::pixels)); use this for video or
    /// any other box. Anything not positive removes it.
    #[inline]
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.set_aspect_ratio(ratio);
        self
    }
    /// [`El::aspect_ratio`] in place.
    #[inline]
    pub fn set_aspect_ratio(&mut self, ratio: f32) -> &mut Self {
        self.style.set_aspect_ratio((ratio.is_finite() && ratio > 0.0).then_some(ratio));
        self
    }
    /// The largest box at `ratio` (width over height) that fits the room the element is given,
    /// as SwiftUI's `.aspectRatio(ratio, contentMode: .fit)` or CSS `object-fit: contain` for
    /// the element itself. The room is the free space along the parent's main axis and its
    /// whole cross side, within `max_w` and `max_h`; siblings keep theirs, so a caption under
    /// a picture sits under the picture. The parent's alignment places the fitted box. Use it
    /// for a picture or video in a bounded cell: `el().aspect_fit(w / h).pixels_fit(p, Fill)`.
    /// In a column with no bounded height, bound it with `max_h`. It sets both sides, so `w`
    /// and `h` don't apply; anything not positive removes it.
    pub fn aspect_fit(mut self, ratio: f32) -> Self {
        let ratio = if ratio.is_finite() && ratio > 0.0 { ratio } else { 0.0 };
        if ratio > 0.0 || !self.cold.is_empty() {
            self.cold_mut().aspect_fit = ratio;
        }
        self
    }
    #[inline]
    pub fn w_full(mut self) -> Self {
        self.set_w_full();
        self
    }
    /// [`El::w_full`] in place.
    #[inline]
    pub fn set_w_full(&mut self) -> &mut Self {
        self.style.set_width(percent(1.0));
        self
    }
    /// Width as a fraction of the parent's (CSS `width: 40%` is `.w_frac(0.4)`).
    #[inline]
    pub fn w_frac(mut self, f: f32) -> Self {
        self.set_w_frac(f);
        self
    }
    /// [`El::w_frac`] in place.
    #[inline]
    pub fn set_w_frac(&mut self, f: f32) -> &mut Self {
        self.style.set_width(percent(f));
        self
    }
    /// Fit width to content without stretching across a column (CSS `width: fit-content`).
    ///
    /// Keeps `align-self` at `auto`, so a control hugs its content inside a stretching `col()`
    /// while still deferring to a parent's cross-axis alignment (such as `col().items_center()`
    /// or `row().center_y()`).
    ///
    /// ```
    /// # use rune::{row, text};
    /// # let _ =
    /// row().w_fit().px(12.0).child(text("Badge"))
    /// # ;
    /// ```
    #[inline]
    pub fn w_fit(mut self) -> Self {
        self.set_w_fit();
        self
    }
    /// [`El::w_fit`] in place.
    ///
    /// ```
    /// # use rune::row;
    /// let mut e = row();
    /// e.set_w_fit();
    /// ```
    #[inline]
    pub fn set_w_fit(&mut self) -> &mut Self {
        self.style.set_width(Dimension::fit_content());
        self
    }
    #[inline]
    pub fn h_full(mut self) -> Self {
        self.set_h_full();
        self
    }
    /// [`El::h_full`] in place.
    #[inline]
    pub fn set_h_full(&mut self) -> &mut Self {
        self.style.set_height(percent(1.0));
        self
    }
    /// Height as a fraction of the parent's (CSS `height: 40%` is `.h_frac(0.4)`).
    ///
    /// ```
    /// # use rune::{col, el};
    /// # let _ =
    /// col().h(200.0).child(el().h_frac(0.4))
    /// # ;
    /// ```
    #[inline]
    pub fn h_frac(mut self, f: f32) -> Self {
        self.set_h_frac(f);
        self
    }
    /// [`El::h_frac`] in place.
    ///
    /// ```
    /// # use rune::el;
    /// let mut e = el();
    /// e.set_h_frac(0.4);
    /// ```
    #[inline]
    pub fn set_h_frac(&mut self, f: f32) -> &mut Self {
        self.style.set_height(percent(f));
        self
    }
    /// Fit height to content without stretching across a row (CSS `height: fit-content`).
    ///
    /// Keeps `align-self` at `auto`, so a child hugs its content height inside a stretching
    /// `row()` while still deferring to a parent's cross-axis alignment (such as `row().center_y()`).
    ///
    /// ```
    /// # use rune::{col, text};
    /// # let _ =
    /// col().h_fit().py(6.0).child(text("Badge"))
    /// # ;
    /// ```
    #[inline]
    pub fn h_fit(mut self) -> Self {
        self.set_h_fit();
        self
    }
    /// [`El::h_fit`] in place.
    ///
    /// ```
    /// # use rune::col;
    /// let mut e = col();
    /// e.set_h_fit();
    /// ```
    #[inline]
    pub fn set_h_fit(&mut self) -> &mut Self {
        self.style.set_height(Dimension::fit_content());
        self
    }
    /// Take a `g` share of the free space along the parent's main axis (CSS `flex: g 1 0`).
    ///
    /// This also sets flex-basis to 0 and allows shrinking, so the element's own content does
    /// not size it: it is only as big as the space the parent has left over. Inside a parent
    /// with no bounded height (an auto-height column) there is no free space, and a grown
    /// `.scroll(..)` viewport collapses to 0 pt. Bound an ancestor's height (`.h(..)`,
    /// `.h_full()`, or a grown child of a bounded parent), or use `.min_h(..)` for a floor.
    /// A grown column that holds its own flex content also needs `.min_h(0.0)` (see
    /// [`El::min_h`]).
    #[inline]
    pub fn grow(mut self, g: f32) -> Self {
        self.set_grow(g);
        self
    }
    /// [`El::grow`] in place.
    #[inline]
    pub fn set_grow(&mut self, g: f32) -> &mut Self {
        self.style.set_grow(g);
        self
    }
    #[inline]
    pub fn padding(mut self, p: f32) -> Self {
        self.set_padding(p);
        self
    }
    /// [`El::padding`] in place.
    #[inline]
    pub fn set_padding(&mut self, p: f32) -> &mut Self {
        self.style.set_padding_all(p);
        self
    }
    #[inline]
    pub fn px(mut self, p: f32) -> Self {
        self.set_px(p);
        self
    }
    /// [`El::px`] in place.
    #[inline]
    pub fn set_px(&mut self, p: f32) -> &mut Self {
        self.style.set_pad_left(p);
        self.style.set_pad_right(p);
        self
    }
    #[inline]
    pub fn py(mut self, p: f32) -> Self {
        self.set_py(p);
        self
    }
    /// [`El::py`] in place.
    #[inline]
    pub fn set_py(&mut self, p: f32) -> &mut Self {
        self.style.set_pad_top(p);
        self.style.set_pad_bottom(p);
        self
    }
    #[inline]
    pub fn pt(mut self, p: f32) -> Self {
        self.set_pt(p);
        self
    }
    /// [`El::pt`] in place.
    #[inline]
    pub fn set_pt(&mut self, p: f32) -> &mut Self {
        self.style.set_pad_top(p);
        self
    }
    #[inline]
    pub fn pb(mut self, p: f32) -> Self {
        self.set_pb(p);
        self
    }
    /// [`El::pb`] in place.
    #[inline]
    pub fn set_pb(&mut self, p: f32) -> &mut Self {
        self.style.set_pad_bottom(p);
        self
    }
    #[inline]
    pub fn pl(mut self, p: f32) -> Self {
        self.set_pl(p);
        self
    }
    /// [`El::pl`] in place.
    #[inline]
    pub fn set_pl(&mut self, p: f32) -> &mut Self {
        self.style.set_pad_left(p);
        self
    }
    #[inline]
    pub fn ml(mut self, m: f32) -> Self {
        self.set_ml(m);
        self
    }
    /// [`El::ml`] in place.
    #[inline]
    pub fn set_ml(&mut self, m: f32) -> &mut Self {
        self.style.set_margin_left(m);
        self
    }
    #[inline]
    pub fn mr(mut self, m: f32) -> Self {
        self.set_mr(m);
        self
    }
    /// [`El::mr`] in place.
    #[inline]
    pub fn set_mr(&mut self, m: f32) -> &mut Self {
        self.style.set_margin_right(m);
        self
    }
    #[inline]
    pub fn gap(mut self, g: f32) -> Self {
        self.set_gap(g);
        self
    }
    /// [`El::gap`] in place.
    #[inline]
    pub fn set_gap(&mut self, g: f32) -> &mut Self {
        self.style.set_gap(g, g);
        self
    }
    /// Centre children on the cross axis.
    #[inline]
    pub fn center_y(mut self) -> Self {
        self.set_center_y();
        self
    }
    /// [`El::center_y`] in place.
    #[inline]
    pub fn set_center_y(&mut self) -> &mut Self {
        self.style.set_align_items(AlignItems::CENTER);
        self
    }
    #[inline]
    pub fn center(mut self) -> Self {
        self.set_center();
        self
    }
    /// [`El::center`] in place.
    #[inline]
    pub fn set_center(&mut self) -> &mut Self {
        self.style.set_align_items(AlignItems::CENTER);
        self.style.set_justify_content(JustifyContent::CENTER);
        self
    }
    #[inline]
    pub fn justify_between(mut self) -> Self {
        self.set_justify_between();
        self
    }
    /// [`El::justify_between`] in place.
    #[inline]
    pub fn set_justify_between(&mut self) -> &mut Self {
        self.style.set_justify_content(JustifyContent::SPACE_BETWEEN);
        self
    }
    #[inline]
    pub fn justify_end(mut self) -> Self {
        self.set_justify_end();
        self
    }
    /// [`El::justify_end`] in place.
    #[inline]
    pub fn set_justify_end(&mut self) -> &mut Self {
        self.style.set_justify_content(JustifyContent::FLEX_END);
        self
    }
    /// Wrap children onto new lines (CSS `flex-wrap: wrap`).
    #[inline]
    pub fn flex_wrap(mut self) -> Self {
        self.set_flex_wrap();
        self
    }
    /// [`El::flex_wrap`] in place.
    #[inline]
    pub fn set_flex_wrap(&mut self) -> &mut Self {
        self.style.set_flex_wrap(FlexWrap::Wrap);
        self
    }
    /// Align children to the start of the cross axis (CSS `align-items: flex-start`).
    #[inline]
    pub fn items_start(mut self) -> Self {
        self.set_items_start();
        self
    }
    /// [`El::items_start`] in place.
    #[inline]
    pub fn set_items_start(&mut self) -> &mut Self {
        self.style.set_align_items(AlignItems::FLEX_START);
        self
    }
    /// Align children to the end of the cross axis (CSS `align-items: flex-end`).
    #[inline]
    pub fn items_end(mut self) -> Self {
        self.set_items_end();
        self
    }
    /// [`El::items_end`] in place.
    #[inline]
    pub fn set_items_end(&mut self) -> &mut Self {
        self.style.set_align_items(AlignItems::FLEX_END);
        self
    }
    /// Centre children on the cross axis (CSS `align-items: center`; same as `center_y`).
    #[inline]
    pub fn items_center(mut self) -> Self {
        self.set_items_center();
        self
    }
    /// [`El::items_center`] in place.
    #[inline]
    pub fn set_items_center(&mut self) -> &mut Self {
        self.style.set_align_items(AlignItems::CENTER);
        self
    }
    /// Stretch children across the cross axis (CSS `align-items: stretch`).
    #[inline]
    pub fn items_stretch(mut self) -> Self {
        self.set_items_stretch();
        self
    }
    /// [`El::items_stretch`] in place.
    #[inline]
    pub fn set_items_stretch(&mut self) -> &mut Self {
        self.style.set_align_items(AlignItems::STRETCH);
        self
    }
    /// This element's own cross-axis alignment (CSS `align-self`).
    #[inline]
    pub fn self_start(mut self) -> Self {
        self.set_self_start();
        self
    }
    /// [`El::self_start`] in place.
    #[inline]
    pub fn set_self_start(&mut self) -> &mut Self {
        self.style.set_align_self(Some(AlignSelf::FLEX_START));
        self
    }
    #[inline]
    pub fn self_center(mut self) -> Self {
        self.set_self_center();
        self
    }
    /// [`El::self_center`] in place.
    #[inline]
    pub fn set_self_center(&mut self) -> &mut Self {
        self.style.set_align_self(Some(AlignSelf::CENTER));
        self
    }
    #[inline]
    pub fn self_end(mut self) -> Self {
        self.set_self_end();
        self
    }
    /// [`El::self_end`] in place.
    #[inline]
    pub fn set_self_end(&mut self) -> &mut Self {
        self.style.set_align_self(Some(AlignSelf::FLEX_END));
        self
    }
    #[inline]
    pub fn self_stretch(mut self) -> Self {
        self.set_self_stretch();
        self
    }
    /// [`El::self_stretch`] in place.
    #[inline]
    pub fn set_self_stretch(&mut self) -> &mut Self {
        self.style.set_align_self(Some(AlignSelf::STRETCH));
        self
    }
    /// Reset this element's cross-axis alignment to defer to the parent's `align-items`
    /// (CSS `align-self: auto`).
    ///
    /// ```
    /// # use rune::el;
    /// # let control = el();
    /// # let _ =
    /// control.self_start().self_auto()
    /// # ;
    /// ```
    #[inline]
    pub fn self_auto(mut self) -> Self {
        self.set_self_auto();
        self
    }
    /// [`El::self_auto`] in place.
    ///
    /// ```
    /// # use rune::el;
    /// # let mut control = el();
    /// control.set_self_auto();
    /// ```
    #[inline]
    pub fn set_self_auto(&mut self) -> &mut Self {
        self.style.set_align_self(None);
        self
    }
    /// Pack children at the start of the main axis (CSS `justify-content: flex-start`).
    #[inline]
    pub fn justify_start(mut self) -> Self {
        self.set_justify_start();
        self
    }
    /// [`El::justify_start`] in place.
    #[inline]
    pub fn set_justify_start(&mut self) -> &mut Self {
        self.style.set_justify_content(JustifyContent::FLEX_START);
        self
    }
    /// Centre children on the main axis (CSS `justify-content: center`).
    #[inline]
    pub fn justify_center(mut self) -> Self {
        self.set_justify_center();
        self
    }
    /// [`El::justify_center`] in place.
    #[inline]
    pub fn set_justify_center(&mut self) -> &mut Self {
        self.style.set_justify_content(JustifyContent::CENTER);
        self
    }
    /// CSS `justify-content: space-around`.
    pub fn justify_around(mut self) -> Self {
        self.style.set_justify_content(JustifyContent::SPACE_AROUND);
        self
    }
    /// CSS `justify-content: space-evenly`.
    pub fn justify_evenly(mut self) -> Self {
        self.style.set_justify_content(JustifyContent::SPACE_EVENLY);
        self
    }
    /// Minimum width in points. Preserved by [`El::shrink`] and the single-line truncation
    /// builders in either call order; pass `.min_w(0.0)` after an earlier explicit minimum to
    /// override it down to zero.
    #[inline]
    pub fn min_w(mut self, w: f32) -> Self {
        self.set_min_w(w);
        self
    }
    /// [`El::min_w`] in place.
    #[inline]
    pub fn set_min_w(&mut self, w: f32) -> &mut Self {
        self.style.set_min_width(length(w));
        self
    }
    #[inline]
    pub fn max_h(mut self, h: f32) -> Self {
        self.set_max_h(h);
        self
    }
    /// [`El::max_h`] in place.
    #[inline]
    pub fn set_max_h(&mut self, h: f32) -> &mut Self {
        self.style.set_max_height(length(h));
        self
    }
    /// Separate row and column gaps (CSS `row-gap`, `column-gap`).
    #[inline]
    pub fn gaps(mut self, row: f32, column: f32) -> Self {
        self.set_gaps(row, column);
        self
    }
    /// [`El::gaps`] in place.
    #[inline]
    pub fn set_gaps(&mut self, row: f32, column: f32) -> &mut Self {
        self.style.set_gap(column, row);
        self
    }
    #[inline]
    pub fn max_w(mut self, w: f32) -> Self {
        self.set_max_w(w);
        self
    }
    /// [`El::max_w`] in place.
    #[inline]
    pub fn set_max_w(&mut self, w: f32) -> &mut Self {
        self.style.set_max_width(length(w));
        self
    }
    /// Minimum height in points.
    ///
    /// Flex items default to a minimum of their content height (CSS `min-height: auto`), so a
    /// nested flex column with tall content overflows its parent instead of shrinking, and a
    /// scroll viewport inside it never gets to scroll. Put `.min_h(0.0)` on each flex column
    /// between the bounded ancestor and the viewport:
    ///
    /// ```
    /// # use rune::{ScrollSpec, col, el};
    /// # let (header, list) = (el(), el());
    /// # let _ =
    /// col().h_full().child(header).child(
    ///     col().grow(1.0).min_h(0.0).child(col().scroll(ScrollSpec { stick_bottom: false }).grow(1.0).child(list)),
    /// )
    /// # ;
    /// ```
    #[inline]
    pub fn min_h(mut self, h: f32) -> Self {
        self.set_min_h(h);
        self
    }
    /// [`El::min_h`] in place.
    #[inline]
    pub fn set_min_h(&mut self, h: f32) -> &mut Self {
        self.style.set_min_height(length(h));
        self
    }
    /// Left and right margin together (CSS `margin-inline`).
    #[inline]
    pub fn mx(mut self, m: f32) -> Self {
        self.set_mx(m);
        self
    }
    /// [`El::mx`] in place.
    #[inline]
    pub fn set_mx(&mut self, m: f32) -> &mut Self {
        self.style.set_margin_x(m);
        self
    }
    /// Top and bottom margin together (CSS `margin-block`).
    #[inline]
    pub fn my(mut self, m: f32) -> Self {
        self.set_my(m);
        self
    }
    /// [`El::my`] in place.
    #[inline]
    pub fn set_my(&mut self, m: f32) -> &mut Self {
        self.style.set_margin_y(m);
        self
    }
    #[inline]
    pub fn mt(mut self, m: f32) -> Self {
        self.set_mt(m);
        self
    }
    /// [`El::mt`] in place.
    #[inline]
    pub fn set_mt(&mut self, m: f32) -> &mut Self {
        self.style.set_margin_top(m);
        self
    }
    #[inline]
    pub fn mb(mut self, m: f32) -> Self {
        self.set_mb(m);
        self
    }
    /// [`El::mb`] in place.
    #[inline]
    pub fn set_mb(&mut self, m: f32) -> &mut Self {
        self.style.set_margin_bottom(m);
        self
    }
    #[inline]
    pub fn pr(mut self, p: f32) -> Self {
        self.set_pr(p);
        self
    }
    /// [`El::pr`] in place.
    #[inline]
    pub fn set_pr(&mut self, p: f32) -> &mut Self {
        self.style.set_pad_right(p);
        self
    }
    /// Sets `flex-shrink: 1` and lowers an automatic `min-width` to `0`, preserving any explicit
    /// [`El::min_w`] regardless of call order and leaving `min-height` untouched (nested vertical
    /// flex columns still need explicit [`El::min_h`]`(0.0)`). To override an earlier explicit
    /// minimum down to zero, call `.min_w(0.0)` after.
    ///
    /// ```
    /// use rune::{Env, Runtime, el, row};
    ///
    /// let mut rt = Runtime::new(
    ///     (),
    ///     |_: &(), _| row().w(100.0)
    ///         .child(el().key("a").w(300.0).min_w(160.0).shrink())
    ///         .child(el().key("b").w(300.0).shrink().min_w(160.0)),
    ///     (100.0, 100.0),
    ///     Env::new(true, 1.0),
    /// );
    /// rt.frame(0.0);
    /// assert_eq!(rt.presented("a").unwrap().0[2], 160.0);
    /// assert_eq!(rt.presented("b").unwrap().0[2], 160.0);
    /// ```
    #[inline]
    pub fn shrink(mut self) -> Self {
        self.set_shrink();
        self
    }
    /// [`El::shrink`] in place: sets `flex-shrink: 1` and lowers an automatic `min-width` to `0`
    /// while preserving any explicit [`El::min_w`] and leaving `min-height` untouched.
    #[inline]
    pub fn set_shrink(&mut self) -> &mut Self {
        self.set_shrink_by(1.0)
    }
    /// [`El::shrink`] with `flex-shrink: factor`. Siblings give way in proportion to their
    /// factor times their size, so a sibling with a tiny factor keeps its size until the others
    /// reach their minimums.
    ///
    /// ```
    /// use rune::{Env, Runtime, el, row};
    ///
    /// let mut rt = Runtime::new(
    ///     (),
    ///     |_: &(), _| row().w(300.0)
    ///         .child(el().key("title").w(300.0).min_w(80.0).shrink())
    ///         .child(el().key("tools").w(100.0).shrink_by(0.001)),
    ///     (300.0, 100.0),
    ///     Env::new(true, 1.0),
    /// );
    /// rt.frame(0.0);
    /// assert!((rt.presented("title").unwrap().0[2] - 200.0).abs() < 0.5);
    /// assert!((rt.presented("tools").unwrap().0[2] - 100.0).abs() < 0.5);
    /// ```
    #[inline]
    pub fn shrink_by(mut self, factor: f32) -> Self {
        self.set_shrink_by(factor);
        self
    }
    /// [`El::shrink_by`] in place.
    #[inline]
    pub fn set_shrink_by(&mut self, factor: f32) -> &mut Self {
        self.style.set_flex_shrink(factor.max(0.0));
        if self.style.min_size().width.is_auto() {
            self.style.set_min_width(length(0.0));
        }
        self
    }
    /// Lay children out in `n` equal columns, each as wide as the widest child needs (CSS
    /// `grid-template-columns: repeat(n, 1fr)`), or sharing the width the element is given.
    /// Children fill the cells in order unless placed with `grid_cell`.
    pub fn equal_columns(mut self, n: usize) -> Self {
        self.style.set_equal_columns(n);
        self
    }
    /// Place this child of an `equal_columns` element in `column` (0-based) of the first row.
    /// Children in the same cell overlap in order, so a moving highlight can sit under a label.
    pub fn grid_cell(mut self, column: usize) -> Self {
        self.style.set_grid_cell(column);
        self
    }
    /// Out of flow, positioned at `left, top` inside the parent's padding box.
    #[inline]
    pub fn absolute(mut self, left: f32, top: f32) -> Self {
        self.set_absolute(left, top);
        self
    }
    /// [`El::absolute`] in place.
    #[inline]
    pub fn set_absolute(&mut self, left: f32, top: f32) -> &mut Self {
        self.style.set_absolute_inset(Rect { left: length(left), top: length(top), right: auto(), bottom: auto() });
        self
    }
    /// Out of flow, pinned to the given edges of the parent's padding box (`None` leaves an
    /// edge free); pinning both opposite edges stretches the element between them.
    #[inline]
    pub fn pin(mut self, left: Option<f32>, top: Option<f32>, right: Option<f32>, bottom: Option<f32>) -> Self {
        self.set_pin(left, top, right, bottom);
        self
    }
    /// [`El::pin`] in place.
    #[inline]
    pub fn set_pin(&mut self, left: Option<f32>, top: Option<f32>, right: Option<f32>, bottom: Option<f32>) -> &mut Self {
        let edge = |v: Option<f32>| v.map_or(auto(), length);
        self.style.set_absolute_inset(Rect { left: edge(left), top: edge(top), right: edge(right), bottom: edge(bottom) });
        self
    }
}
