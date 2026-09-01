use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::display_row::lisp_string::DisplayRowPrefixValues;
use crate::display_row::walk_state::{LineNumberFieldLayout, LineNumberRenderState};
use crate::display_row::width::DisplayRowCharWidthPolicy;
use crate::neovm_bridge::LayoutVar;
use crate::neovm_bridge::{
    LayoutBufferView, RustBufferAccess, buffer_local_bool, buffer_local_int, buffer_local_value,
};
#[cfg(test)]
use crate::types::LineWrapMode;
use crate::types::{DisplayLineNumbersMode, WindowKind, WindowParams};
use crate::window_layout::WindowLayoutBox;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferWindowGeometryRequest {
    text_x: f32,
    text_y: f32,
    text_width: f32,
    text_height: f32,
    vscroll: f32,
    kind: WindowKind,
    /// GUI (window-system) frame.  The GNU vscroll content-up shift is a
    /// graphical, sub-pixel concept; TTY frames are a char-cell grid and keep
    /// the historical behavior (vscroll shrinks the visible area, if set at all),
    /// so the shift is applied only when this is true.
    window_system: bool,
    top_chrome_rows: usize,
    bottom_chrome_rows: usize,
    char_width: f32,
    char_height: f32,
    /// Ceiling (in display rows) from `max-mini-window-height` for a
    /// minibuffer window.  `None` for ordinary windows; set via
    /// `with_max_mini_window_rows`.  GNU `resize_mini_window` measures the
    /// mini-window's content height with `move_it_to(ZV)` UNCLAMPED and only
    /// clips the result to this ceiling, so the walk must be allowed to emit
    /// up to this many rows even when the window is currently one row tall.
    max_mini_window_rows: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferWindowGeometry {
    pub(crate) text_x: f32,
    pub(crate) text_y: f32,
    pub(crate) text_width: f32,
    pub(crate) text_height: f32,
    /// Applied content-up shift (pixels) for the body row-walk origin. GNU
    /// `w->vscroll` scrolls the window's contents up by this many pixels: the
    /// first body row is drawn at `text_y - vscroll` (top-clipped) and one extra
    /// partially-visible row is exposed at the bottom, while the text area keeps
    /// its full height and the render is clipped to `text_y .. text_y +
    /// text_height`. Zero for minibuffer windows, where vscroll instead *shrinks*
    /// the visible area (vertico-posframe) via `text_height`.
    pub(crate) vscroll: f32,
    pub(crate) char_width: f32,
    pub(crate) char_height: f32,
    pub(crate) max_rows: usize,
    pub(crate) display_text_row_base: usize,
    pub(crate) display_text_rows: usize,
    pub(crate) bottom_chrome_rows: usize,
    pub(crate) mode_line_display_row: usize,
    /// Capacity of GNU's complete TEXT_AREA, including the line-number prefix.
    ///
    /// This is deliberately not a bare `usize`: buffer-content capacity is
    /// smaller when line numbers are enabled, and passing that value to the
    /// glyph matrix caused presentation to reset ordinary text onto the prefix.
    pub(crate) matrix_columns: GlyphMatrixColumnCapacity,
    pub(crate) line_number_pixel_width: f32,
    pub(crate) content_x: f32,
    /// Y at which a row stops being "visible" during the walk.  For ordinary
    /// windows this is `text_y + text_height` (the physical text area).  For a
    /// minibuffer it is lifted to span `max_rows` rows so the unclamped GNU
    /// `resize_mini_window` measurement can emit content rows beyond the
    /// window's current (often one-row) physical height; `max_rows` still hard
    /// caps the row count at the `max-mini-window-height` ceiling.
    pub(crate) visibility_bottom_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GlyphMatrixColumnCapacity(usize);

impl GlyphMatrixColumnCapacity {
    fn for_text_area(width_policy: DisplayRowCharWidthPolicy, text_width: f32) -> Self {
        Self(width_policy.columns_for_width(text_width))
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

pub(crate) struct BufferWindowGeometryPlan {
    pub(crate) geometry: BufferWindowGeometry,
    pub(crate) line_number_field: LineNumberFieldLayout,
}

impl BufferWindowGeometry {
    /// Absolute frame Y at which the body row-walk begins.  For an ordinary
    /// window with an active vscroll this is `text_y - vscroll` (GNU: the
    /// window's contents are scrolled up, so the first row starts above the text
    /// area and is top-clipped); it equals `text_y` otherwise.  The visible/clip
    /// area is always `text_y .. text_y + text_height`, independent of this.
    pub(crate) fn row_origin_y(&self) -> f32 {
        self.text_y - self.vscroll
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferWindowLocalDisplayPolicy {
    line_number_mode: DisplayLineNumbersMode,
    line_number_offset: i64,
    line_number_major_tick: i32,
    line_number_current_absolute: bool,
    line_number_widen: bool,
    line_number_min_width: i32,
    prefix_values: DisplayRowPrefixValues,
}

impl BufferWindowGeometryRequest {
    pub(crate) fn new(
        params: &WindowParams,
        layout_box: &WindowLayoutBox,
        char_width: f32,
        char_height: f32,
    ) -> Self {
        let body = layout_box.body();
        let chrome = layout_box.chrome();
        let text_x = body.x;
        let text_y = body.y;
        let text_width = body.width;
        let text_height = body.height;

        // In Emacs, w->vscroll is negative when content is shifted up.
        let vscroll = (-params.vscroll).max(0) as f32;
        // GNU vscroll content-up shift applies to ordinary GUI windows only.
        // Minibuffer windows repurpose vscroll to *hide* content by shrinking the
        // visible area (e.g. vertico-posframe), and TTY frames are a char-cell
        // grid; both keep the historical shrink. Ordinary GUI windows shift the
        // content UP by `vscroll` pixels while retaining the full text height (the
        // offset is applied to the row-walk origin in `into_geometry`).
        let uses_shift = params.window_system && !params.kind.is_minibuffer();
        let text_height = if uses_shift {
            text_height
        } else {
            (text_height - vscroll).max(0.0)
        };

        Self {
            text_x,
            text_y,
            text_width,
            text_height,
            vscroll,
            kind: params.kind,
            window_system: params.window_system,
            top_chrome_rows: usize::from(chrome.tab_line_height > 0.0)
                + usize::from(chrome.header_line_height > 0.0),
            bottom_chrome_rows: usize::from(chrome.mode_line_height > 0.0),
            char_width,
            char_height,
            max_mini_window_rows: None,
        }
    }

    /// Whether this window applies the GNU vscroll content-up shift (the fix for
    /// task #64): ordinary GUI windows with an active vscroll.  Minibuffers and
    /// TTY frames keep the historical shrink-based behavior.
    fn uses_vscroll_shift(self) -> bool {
        self.window_system && !self.kind.is_minibuffer() && self.vscroll > 0.0
    }

    /// Record the `max-mini-window-height` ceiling (in display rows) so the
    /// minibuffer walk measures its full content height up to that ceiling,
    /// the way GNU `resize_mini_window` does.  No-op for non-minibuffer
    /// windows.
    pub(crate) fn with_max_mini_window_rows(mut self, max_mini_window_rows: usize) -> Self {
        if self.kind.is_minibuffer() {
            self.max_mini_window_rows = Some(max_mini_window_rows.max(1));
        }
        self
    }

    pub(crate) fn line_number_row_capacity(self) -> usize {
        // GNU's `maybe_produce_line_number' reserves `lnum_width + 2`
        // columns: the right-aligned number plus one blank on each side.
        // `lnum_width` is wide enough for the largest line number that can
        // appear in the current window, so a tiny buffer in a tall window
        // still gets the same two-digit gutter GNU displays for visible rows
        // 1..N.
        self.base_max_rows()
    }

    pub(crate) fn into_geometry(
        self,
        line_number_field: LineNumberFieldLayout,
    ) -> BufferWindowGeometry {
        let max_rows = self.visible_max_rows();
        let width_policy = DisplayRowCharWidthPolicy::new(self.char_width);
        let line_number_pixel_width = line_number_field.extent().get();
        let display_text_row_base = self.top_chrome_rows;
        let display_text_rows = max_rows.max(1);
        let mode_line_display_row = display_text_row_base + display_text_rows;
        // GNU appends line-number glyphs to TEXT_AREA, then advances current_x
        // before producing buffer text.  The matrix therefore spans the whole
        // text area; only the buffer append surface subtracts the prefix width.
        let matrix_columns =
            GlyphMatrixColumnCapacity::for_text_area(width_policy, self.text_width);
        let content_x = self.text_x + line_number_pixel_width;

        // Content-up shift applied to the body row-walk origin. Minibuffers and
        // TTY frames keep vscroll baked into `text_height` (the shrink above), so
        // their origin is not shifted; ordinary GUI windows shift up by `vscroll`.
        let row_shift = if self.uses_vscroll_shift() {
            self.vscroll
        } else {
            0.0
        };

        // For a minibuffer measured with the GNU `move_it_to(ZV)` policy, lift
        // the visibility bottom to span `max_rows` so the walk can emit content
        // rows past the window's current physical height; `max_rows` (the
        // ceiling) still hard caps the count.  An ordinary window with an active
        // vscroll walks from the shifted origin (`text_y - vscroll`) and must be
        // allowed to emit the extra, partially-visible bottom row, so lift its
        // walk bottom to that shifted last-row edge; the render is still clipped
        // to the real text area (`text_y .. text_y + text_height`).  Otherwise the
        // physical text-area bottom is the limit.
        let physical_bottom_y = self.text_y + self.text_height;
        let visibility_bottom_y = if self.kind.is_minibuffer() {
            physical_bottom_y.max(self.text_y + max_rows as f32 * self.char_height)
        } else if row_shift > 0.0 {
            (self.text_y - row_shift) + max_rows as f32 * self.char_height
        } else {
            physical_bottom_y
        };

        BufferWindowGeometry {
            text_x: self.text_x,
            text_y: self.text_y,
            text_width: self.text_width,
            text_height: self.text_height,
            vscroll: row_shift,
            char_width: self.char_width,
            char_height: self.char_height,
            max_rows,
            display_text_row_base,
            display_text_rows,
            bottom_chrome_rows: self.bottom_chrome_rows,
            mode_line_display_row,
            matrix_columns,
            line_number_pixel_width,
            content_x,
            visibility_bottom_y,
        }
    }

    fn base_max_rows(self) -> usize {
        (self.text_height / self.char_height).floor() as usize
    }

    fn visible_max_rows(self) -> usize {
        // GNU `resize_mini_window` measures the mini-window's full content
        // height with an unclamped `move_it_to(ZV)` and clips it only to the
        // `max-mini-window-height` ceiling.  Mirror that: when the ceiling is
        // known, let the walk emit up to that many rows (not just the rows that
        // physically fit the current, often one-row, window).  vscroll != 0
        // means content is intentionally hidden (e.g. vertico-posframe), so
        // fall back to the physical row count there.
        if self.kind.is_minibuffer()
            && self.vscroll == 0.0
            && self.text_height > 0.0
            && let Some(ceiling) = self.max_mini_window_rows
        {
            return ceiling.max(1);
        }

        // Ordinary GUI window with an active vscroll: GNU shifts the content UP by
        // `vscroll` pixels, top-clipping the first row and exposing one more
        // partially-visible row at the bottom.  Walk enough rows to cover the
        // shifted span `[vscroll, vscroll + text_height]` — `base_max_rows + 1`
        // for a sub-line vscroll, more when vscroll exceeds a row.
        if self.uses_vscroll_shift() && self.text_height > 0.0 {
            return ((self.vscroll + self.text_height) / self.char_height).ceil() as usize;
        }

        let max_rows = self.base_max_rows();
        // The minibuffer must always render at least 1 row.  Its pixel
        // height may be fractionally smaller than char_height (e.g. 24px vs
        // 24.15 with line-spacing) causing floor() to yield 0.  Exception:
        // when vscroll is active, don't force 1 row -- vscroll is used (e.g.
        // by vertico-posframe) to intentionally hide content.
        if self.kind.is_minibuffer()
            && max_rows == 0
            && self.text_height > 0.0
            && self.vscroll == 0.0
        {
            1
        } else {
            max_rows
        }
    }

    pub(crate) fn into_window_plan<B: LayoutBufferView>(
        self,
        local_display_policy: &BufferWindowLocalDisplayPolicy,
        buffer_access: &RustBufferAccess<'_, B>,
        line_number_cell_width_px: f32,
    ) -> BufferWindowGeometryPlan {
        let line_number_columns = local_display_policy
            .line_number_columns(buffer_access, self.line_number_row_capacity());
        let line_number_field =
            LineNumberFieldLayout::new(line_number_columns, line_number_cell_width_px);
        let geometry = self.into_geometry(line_number_field);
        BufferWindowGeometryPlan {
            geometry,
            line_number_field,
        }
    }
}

impl BufferWindowLocalDisplayPolicy {
    /// Build buffer-local display policy under the window snapshot's semantic
    /// gates.
    ///
    /// The displayed buffer owns offsets, faces, and prefix variables, but the
    /// typed window snapshot is the sole authority for whether line numbers
    /// are allowed at all.  In particular, GNU forbids them in minibuffers even
    /// when an echo-area buffer has `display-line-numbers` enabled.
    pub(crate) fn from_window(buffer: &impl LayoutBufferView, params: &WindowParams) -> Self {
        Self {
            line_number_mode: params.display_line_numbers,
            line_number_offset: buffer_local_int(buffer, LayoutVar::DisplayLineNumbersOffset, 0),
            line_number_major_tick: buffer_local_int(
                buffer,
                LayoutVar::DisplayLineNumbersMajorTick,
                0,
            ) as i32,
            line_number_current_absolute: buffer_local_bool(
                buffer,
                LayoutVar::DisplayLineNumbersCurrentAbsolute,
            ),
            line_number_widen: buffer_local_bool(buffer, LayoutVar::DisplayLineNumbersWiden),
            line_number_min_width: buffer_local_int(buffer, LayoutVar::DisplayLineNumbersWidth, 0)
                as i32,
            prefix_values: DisplayRowPrefixValues::default_values(
                buffer_local_value(buffer, LayoutVar::LinePrefix),
                buffer_local_value(buffer, LayoutVar::WrapPrefix),
            ),
        }
    }

    pub(crate) fn line_number_columns<B: LayoutBufferView>(
        self,
        access: &RustBufferAccess<'_, B>,
        max_rows: usize,
    ) -> i32 {
        if !self.line_numbers_enabled() {
            return 0;
        }
        let total_lines = access.count_lines(0, access.zv()) + 1;
        let visible_lines = max_rows.max(1) as i64;
        let digit_count = total_lines.max(visible_lines).max(1).to_string().len() as i32;
        let min = self.line_number_min_width.max(1);
        digit_count.max(min) + 2
    }

    pub(crate) fn initial_line_numbers<B: LayoutBufferView>(
        self,
        access: &RustBufferAccess<'_, B>,
        window_start: i64,
        point_charpos: i64,
    ) -> LineNumberRenderState {
        let window_start_byte = access.charpos_to_bytepos(window_start);
        let begin_byte = if self.line_number_widen {
            0
        } else {
            access.begv()
        };
        let current_line = if self.line_numbers_enabled() {
            access.count_lines(begin_byte, window_start_byte) + 1
        } else {
            1
        };
        // `point_line` is needed for TWO things: relative line numbers
        // (`current_line - point_line`) AND the `line-number-current-line` face on
        // the current line (`is_current_line` == `current_line == point_line`).
        // GNU highlights the current line number in EVERY mode (xdisp.c
        // maybe_produce_line_number uses `lnum_face_id` when the row holds point),
        // so compute point_line whenever line numbers are on — not only in
        // relative/visual mode (>= 2). Gating it on `>= 2` left point_line at 0 in
        // absolute mode, so `is_current_line` was never true and the current line's
        // number rendered in the plain `line-number` face instead of
        // `line-number-current-line`.
        let point_line = if self.line_numbers_enabled() {
            let pt_byte = access.charpos_to_bytepos(point_charpos);
            access.count_lines(begin_byte, pt_byte) + 1
        } else {
            0
        };
        LineNumberRenderState::new(self.line_numbers_enabled(), current_line, point_line)
    }

    pub(crate) fn row_prelude_context(
        self,
        line_number_field: LineNumberFieldLayout,
        fallback_metrics: crate::display_row::metrics::DisplayRowFallbackMetrics,
    ) -> BufferSourceRowPreludeRequestContext {
        BufferSourceRowPreludeRequestContext::new(
            self.line_number_mode,
            self.line_number_current_absolute,
            self.line_number_offset,
            self.line_number_major_tick,
            line_number_field,
            self.prefix_values,
            fallback_metrics,
        )
    }

    pub(crate) fn has_prefix(self) -> bool {
        self.prefix_values.has_default_prefix()
    }

    pub(crate) fn has_line_default_prefix(self) -> bool {
        self.prefix_values.has_line_default_prefix()
    }

    fn line_numbers_enabled(self) -> bool {
        self.line_number_mode.enabled()
    }

    #[cfg(test)]
    pub(crate) fn from_parts(
        line_number_mode: DisplayLineNumbersMode,
        line_number_widen: bool,
        line_number_min_width: i32,
        prefix_values: DisplayRowPrefixValues,
    ) -> Self {
        Self {
            line_number_mode,
            line_number_offset: 0,
            line_number_major_tick: 0,
            line_number_current_absolute: false,
            line_number_widen,
            line_number_min_width,
            prefix_values,
        }
    }
}

#[cfg(test)]
#[path = "text_walk_test.rs"]
mod tests;
