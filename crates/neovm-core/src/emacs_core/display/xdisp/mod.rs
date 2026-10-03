//! Display engine builtins for the Elisp interpreter.
//!
//! Implements display-related functions from Emacs `xdisp.c`:
//! - `format-mode-line` — format a mode line string
//! - `invisible-p` — check if a position or property is invisible
//! - `line-pixel-height` — get line height in pixels
//! - `window-text-pixel-size` — calculate text pixel dimensions
//! - `pos-visible-in-window-p` — check if position is visible
//! - `move-point-visually` — move point in visual order
//! - `lookup-image-map` — lookup image map coordinates
//! - `current-bidi-paragraph-direction` — get bidi paragraph direction
//! - `move-to-window-line` — move to a specific window line
//! - `tool-bar-height` — get tool bar height
//! - `tab-bar-height` — get tab bar height
//! - `line-number-display-width` — get line number display width
//! - `long-line-optimizations-p` — check if long-line optimizations are enabled

#[path = "mode_line_flow.rs"]
mod mode_line_flow_policy;

#[inline]
pub(crate) fn mode_line_flow_enabled() -> bool {
    mode_line_flow_policy::enabled()
}
mod mode_line_gc;
pub(crate) mod motion;

use self::motion::MotionEngine;
use super::buffer::resolve_buffer_designator_allow_nil_current_in_manager;
use super::chartable::{make_char_table_value, make_char_table_with_extra_slots};
use super::display_spec;
use super::error::{EvalResult, Flow, signal};
use super::hook_runtime;
use super::intern::intern;
pub(crate) use super::invisibility::{Invisibility, text_prop_means_invisible};
use super::symbol::LispVariableLocality;
use super::value::*;
use crate::buffer::{
    AccessibleEmacsByteRange, Buffer, BufferId, CharLen, CharPos0, CharRange, EmacsBytePos,
    EmacsByteRange, LispCharPos1, TextPropertyTable,
};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range};
use crate::window::{
    DisplayRowSnapshot, FrameId, FrameManager, Window, WindowDisplaySnapshot, WindowId,
};
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use strum::{EnumString, IntoStaticStr};

impl super::eval::Context {
    /// Choose a redisplay start by moving backward from point in display rows.
    ///
    /// GNU's `recenter:` branch initializes its display iterator at point and
    /// calls `move_it_vertically_backward` (src/xdisp.c:21191-21212).  Keep
    /// that operation behind the core display-motion seam so layout callers
    /// never substitute raw buffer-newline arithmetic for display rows that
    /// can wrap, fold, or be replaced. `None` leaves the caller's semantic
    /// viewport unchanged; a failed motion must never turn into a jump to
    /// `point-min`.
    ///
    /// GNU reaches `recenter:` inside `redisplay_window`, after
    /// `set_buffer_internal_1 (XBUFFER (w->contents))` (xdisp.c:20532-20535,
    /// emacs-31.0.90), so the iterator and every text-property probe it makes
    /// read the window's buffer. Redisplay here is entered with whatever
    /// buffer Lisp left current -- the active minibuffer while a completion
    /// UI is up -- so this helper selects `buffer_id` for the scan and hands
    /// the caller's buffer back afterwards, as `with_frame_display_context`
    /// does for mode-line Lisp.
    pub fn redisplay_start_before_point_by_display_rows(
        &mut self,
        buffer_id: BufferId,
        window_id: WindowId,
        point: CharPos0,
        rows: i64,
    ) -> Option<CharPos0> {
        let point_byte = self
            .buffers
            .get(buffer_id)
            .map(|buffer| buffer.char_pos_to_emacs_byte_pos_clamped(point))?;
        let saved_buffer_id = self.buffers.current_buffer_id();
        if saved_buffer_id != Some(buffer_id)
            && let Err(flow) = self.set_current_buffer_unrecorded(buffer_id)
        {
            tracing::debug!(
                "display-row viewport placement cannot select the window buffer: {flow:?}"
            );
            if let Some(saved_buffer_id) = saved_buffer_id {
                self.restore_current_buffer_if_live(saved_buffer_id);
            }
            return None;
        }
        let motion = super::indent::scan_screen_line_motion_target(
            self,
            buffer_id,
            point_byte,
            Some(Value::make_window(window_id.0)),
            -rows.max(0),
        );
        if let Some(saved_buffer_id) = saved_buffer_id {
            self.restore_current_buffer_if_live(saved_buffer_id);
        }
        let start_byte = match motion {
            Ok(motion) => motion.target,
            Err(flow) => {
                tracing::debug!("display-row viewport placement failed: {flow:?}");
                return None;
            }
        };
        self.buffers
            .get(buffer_id)
            .map(|buffer| buffer.emacs_byte_pos_to_char_pos_clamped(start_byte))
    }

    /// Whether redisplay of `window_id` can enter Lisp through
    /// `window-scroll-functions`.
    ///
    /// The hook can be buffer-local. Inspect the displayed buffer directly so
    /// layout can distinguish a real suspension point from GNU's nil-hook
    /// fast path without changing the selected window or current buffer.
    pub fn window_scroll_functions_may_run(&self, window_id: WindowId) -> bool {
        let Some(buffer_id) = self
            .frames
            .find_window_frame_id(window_id)
            .and_then(|frame_id| self.frames.get(frame_id))
            .and_then(|frame| frame.find_window(window_id))
            .and_then(Window::buffer_id)
        else {
            return false;
        };
        self.buffers
            .get(buffer_id)
            .and_then(|buffer| buffer.buffer_local_value("window-scroll-functions"))
            .or_else(|| {
                self.obarray
                    .symbol_value("window-scroll-functions")
                    .copied()
            })
            .is_some_and(|hook| !hook.is_nil())
    }

    /// GNU `run_window_scroll_functions` (src/xdisp.c:19222) for a start
    /// redisplay just committed.
    ///
    /// GNU sets `w->start` from the candidate, runs the hook, then re-reads
    /// `w->start` so a hook that moves the start wins. We publish the start
    /// first for the same reason, so the hook's own `set-window-start` is the
    /// value used when the redisplay runtime resumes layout.
    ///
    /// `inhibit-redisplay` is bound like every other Lisp seam redisplay
    /// already runs (`pre-redisplay-function`, the window-change hooks),
    /// because this remains part of the same logical redisplay even though the
    /// physical layout attempt has released its borrow. Errors are demoted,
    /// mirroring GNU's `safe_run_hooks_2`.
    pub fn run_window_scroll_functions_for_committed_start(&mut self, window_id: WindowId) {
        // No global-value early-out: `window-scroll-functions` may be
        // buffer-local, and the builtin enters the displayed buffer before it
        // reads the hook (GNU `run_window_scroll_functions` runs with the
        // window's buffer current).
        let window = Value::make_window(window_id.0);
        let specpdl_count = self.specpdl.len();
        if let Err(flow) =
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-redisplay"), Value::T)
        {
            tracing::debug!("window-scroll binding signalled (ignored): {flow:?}");
            return;
        }
        let result = super::window_cmds::builtin_run_window_scroll_functions(self, vec![window]);
        let result = self.unbind_to_with_result(specpdl_count, result);
        if let Err(flow) = result {
            tracing::debug!("window-scroll-functions signalled (ignored): {flow:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_integer_or_marker(arg: &Value) -> Result<(), Flow> {
    if arg.is_marker() {
        return Ok(());
    }
    match arg.kind() {
        ValueKind::Fixnum(_) | ValueKind::Veclike(VecLikeType::Bignum) => Ok(()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *arg],
        )),
    }
}

fn integer_or_marker_value_in_buffers(
    buffers: &crate::buffer::BufferManager,
    arg: Value,
) -> Result<i64, Flow> {
    crate::emacs_core::position::fix_position_with_buffers(buffers, &arg)
}

fn expect_fixnum_arg(name: &str, arg: &Value) -> Result<(), Flow> {
    match arg.kind() {
        ValueKind::Fixnum(_) => Ok(()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(name), *arg],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn validate_window_text_pixel_size_from_arg(from: Value) -> Result<(), Flow> {
    if from.is_nil() || from.is_t() {
        return Ok(());
    }
    if from.is_cons() {
        expect_integer_or_marker(&from.cons_car())?;
        expect_fixnum_arg("fixnump", &from.cons_cdr())?;
        return Ok(());
    }
    expect_integer_or_marker(&from)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn validate_window_text_pixel_size_to_arg(to: Value) -> Result<(), Flow> {
    if to.is_nil() || to.is_t() {
        return Ok(());
    }
    expect_integer_or_marker(&to)
}

fn emacs_char_count(bytes: &[u8], multibyte: bool) -> usize {
    if multibyte {
        crate::emacs_core::emacs_char::chars_in_multibyte(bytes)
    } else {
        bytes.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineColumn {
    line: usize,
    column: usize,
}

/// What a measured buffer region occupies on screen.
///
/// The horizontal and vertical extents are LOGICAL PIXELS, not character
/// columns and rows.  `window-text-pixel-size` is a pixel measurement (GNU
/// walks the display iterator, whose `it.pixel_width` and `it.ascent` /
/// `it.descent` are pixels), and a `display` property can carry an element
/// whose size is not a whole number of cells -- an image.  A column count
/// cannot represent a 200-pixel image in a 9-pixel cell at all: that is why
/// the unit is part of the type rather than a multiplier applied by each
/// caller.  Text still uses the monospace approximation, contributing
/// [`TextCellPixels::width`] per column.
///
/// `max_width` is the value GNU's `window_text_pixel_size` returns -- `x -
/// start_x` over the walked display lines, which [`MeasuredRange`] defines --
/// and not a raw widest-line measurement.  `height` shares the same origin:
/// both count from FROM's display line, never from the start of FROM's
/// logical line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RegionTextMetrics {
    /// Screen lines the region spans.
    pub(crate) lines: usize,
    /// GNU's returned width, in logical pixels.
    pub(crate) max_width: f32,
    /// The region's height in logical pixels: the sum of the lines' own
    /// heights, each of them the tallest thing displayed on that line.
    pub(crate) height: f32,
}

impl RegionTextMetrics {
    pub(crate) const EMPTY: Self = Self {
        lines: 0,
        max_width: 0.0,
        height: 0.0,
    };

    /// The metrics of a region whose iterator landed on an occupied row but
    /// traversed no text: one row of the frame's cell, no width.
    pub(crate) fn empty_row(cell_height: f32) -> Self {
        Self {
            lines: 1,
            max_width: 0.0,
            height: cell_height.max(0.0),
        }
    }
}

/// A frame's character cell, in logical pixels.
///
/// The scanner's text approximation: one column costs exactly `width` pixels
/// and a line's vertical extent starts at the cell's own split.  The split is
/// GNU's `FONT_BASE` / `FONT_DESCENT` for the frame's default font, because a
/// row is as tall as `max (ascent) + max (descent)` over the elements on it
/// (`move_it_in_display_line_to`, src/xdisp.c:11203-11207) and an image with
/// `:ascent 100` rises above the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextCellPixels {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
}

impl TextCellPixels {
    pub(crate) fn new(width: f32, height: f32, ascent: f32) -> Self {
        let width = if width.is_finite() && width > 0.0 {
            width
        } else {
            1.0
        };
        let height = if height.is_finite() && height > 0.0 {
            height
        } else {
            1.0
        };
        let ascent = if ascent.is_finite() {
            ascent.clamp(0.0, height)
        } else {
            height
        };
        Self {
            width,
            height,
            ascent,
        }
    }

    pub(crate) fn descent(self) -> f32 {
        (self.height - self.ascent).max(0.0)
    }
}

/// One display element's extent in logical pixels: how far it advances the
/// text cursor, and how far it reaches above and below the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ElementExtent {
    pub(crate) advance: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
}

/// What a `display` spec contributes to the layout of the run it covers.
///
/// The unit is part of the value because the two cases are NOT interchangeable:
/// a stretch is a whole number of text cells (so a long run stays exact), while
/// an image's advance is an absolute pixel count that has no cell count at all.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DisplayElement {
    /// A `(space ...)` stretch, in text columns.
    Cells(f32),
    /// An element with its own pixel metrics.
    Pixels(ElementExtent),
}

/// One display line's horizontal extent.
///
/// Text is counted in EXACT columns and multiplied by the cell width once, at
/// the point of use, so a long line cannot accumulate float error the way a
/// running pixel sum would; an element with its own size contributes absolute
/// pixels.  The two are not interchangeable, which is why they are separate
/// fields with a single accessor.
#[derive(Clone, Copy, Debug, Default)]
struct LineAdvance {
    columns: f32,
    pixels: f32,
    /// Set when the line ended at the X-LIMIT edge (GNU
    /// `MOVE_LINE_TRUNCATED`): its width is then exactly that edge.
    truncated_at: Option<f32>,
}

impl LineAdvance {
    fn width(self, cell_width: f32) -> f32 {
        match self.truncated_at {
            Some(edge) => edge,
            None => self.columns * cell_width + self.pixels,
        }
    }

    /// Forget everything the row has advanced so far.
    fn restart(&mut self) {
        *self = Self::default();
    }
}

// Whether the mode-line expansion currently running consumed `%c` / `%C`.
//
// GNU keeps the equivalent as `w->column_number_displayed` and consults it in
// `mode_line_update_needed` (xdisp.c:13831-13837), which sets
// `w->update_mode_line` and so DISQUALIFIES the one-line optimization whenever
// the displayed column has changed. Redisplay needs the same answer, because a
// column is the one point-dependent mode-line construct that a
// same-screen-row precondition does NOT pin: moving point left or right within
// a row changes `%c` while the row is unchanged.
//
// We record only WHETHER the spec was consumed, not its value. Refusing the
// chrome skip outright whenever `%c`/`%C` is displayed is strictly more
// conservative than GNU (which compares the value and still skips when the
// column happens to be unchanged), and it costs nothing in the default
// configuration because `column-number-mode` is off (lisp/simple.el:9387).
// Comparing values is the later relaxation.
//
// Thread-local for the same reason as the layout engine's eval counter: the
// mode line is expanded on the evaluator's thread.
thread_local! {
    static COLUMN_SPEC_CONSUMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn note_column_spec_consumed() {
    COLUMN_SPEC_CONSUMED.with(|consumed| consumed.set(true));
}

/// Arm the `%c`/`%C` detector for one mode-line expansion.
pub fn reset_column_spec_consumed() {
    COLUMN_SPEC_CONSUMED.with(|consumed| consumed.set(false));
}

/// Whether the expansion since [`reset_column_spec_consumed`] displayed a
/// column. See the `COLUMN_SPEC_CONSUMED` comment for why redisplay asks.
pub fn column_spec_consumed() -> bool {
    COLUMN_SPEC_CONSUMED.with(std::cell::Cell::get)
}

fn prefix_line_and_column(
    buf: &Buffer,
    accessible: AccessibleEmacsByteRange,
    end_byte: EmacsBytePos,
) -> LineColumn {
    let origin = accessible.start();
    let end = accessible.clamp(end_byte);
    // Column only needs the current line's prefix: find its start with a
    // backward newline scan (O(column) via memrchr, not O(point)) and count
    // chars over just that span.
    let bol = buf
        .prev_newline_emacs_byte(end, origin)
        .map(|nl| nl.add_len(crate::buffer::EmacsByteLen::new(1)))
        .unwrap_or(origin);
    // Line number: GNU keeps a base_line_pos/base_line_number anchor so the
    // newline count runs from a recently displayed line, not from the buffer
    // start (xdisp.c:29486-29620) — O(distance moved), not O(point). The
    // anchor (held per buffer) is valid only when no edit landed at or
    // before it since the last accepted redisplay: the unchanged-region
    // accumulator is the BEG_UNCHANGED analog of GNU's
    // BASE_LINE_NUMBER_VALID_P (xdisp.c:19351). An invalid or missing
    // anchor falls back to the full prefix scan (SIMD over the gap buffer).
    let anchor = buf.line_number_anchor.get();
    let anchor_valid = anchor.is_some_and(|anchor| {
        anchor.accessible_start == origin
            && anchor.line_start <= end
            && buf.changed_char_range().is_none_or(|(dirty_start, _)| {
                buf.char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(
                    dirty_start.max(0) as usize,
                )) > anchor.line_start
            })
    });
    let line = if anchor_valid {
        let anchor = anchor.expect("validated mode-line number anchor");
        anchor.line_number + buf.count_newlines_emacs_byte(anchor.line_start, end)
    } else {
        buf.count_newlines_emacs_byte(origin, end) + 1
    };
    // Re-seat the anchor at this line's start whenever it was invalid or
    // point moved far from it (GNU re-seats near the window when the count
    // drifts past a few window heights, xdisp.c:29544-29552).
    const RESEAT_DISTANCE_BYTES: usize = 32 * 1024;
    if !anchor_valid
        || end.get().abs_diff(
            anchor
                .expect("validated mode-line number anchor")
                .line_start
                .get(),
        ) > RESEAT_DISTANCE_BYTES
    {
        buf.line_number_anchor
            .set(Some(crate::buffer::buffer::ModeLineNumberAnchor {
                accessible_start: origin,
                line_start: bol,
                line_number: line,
            }));
    }
    let mut tail = Vec::new();
    buf.copy_emacs_byte_range_to(EmacsByteRange::new(bol, end), &mut tail);
    let col = emacs_char_count(&tail, buf.get_multibyte());
    LineColumn { line, column: col }
}

/// Resolve a `(space :align-to SPEC)` / `(space :width SPEC)` value to a count of
/// canonical character columns, mirroring GNU's `calc_pixel_width_or_height`
/// (src/xdisp.c) but in *column* units rather than pixels.  GNU works in pixels
/// where a bare number is multiplied by `FRAME_COLUMN_WIDTH`; since our caller
/// multiplies the final column count by `char_width`, here one number == one
/// column and the window-relative edge keywords resolve to column offsets.
///
/// `align_to` selects the GNU "first pass" semantics where bare window symbols
/// (`left`, `text`, …) stand for the *position* of the element's left edge;
/// when false they stand for their *width*.  Returns the resolved column count
/// (may be 0), or `None` for spec forms we do not model (pixel `(N)` lists,
/// physical units like `in`/`cm`, images, fringe/scroll-bar/right edges that
/// depend on window box geometry we do not track here).
fn calc_space_columns(spec: Value, align_to: bool) -> Option<f64> {
    if spec.is_nil() {
        return Some(0.0);
    }
    // A bare number stands for that many columns (GNU: NUM * FRAME_COLUMN_WIDTH).
    if let Some(n) = spec.as_fixnum() {
        return Some(n as f64);
    }
    if let Some(f) = spec.as_float() {
        return Some(f);
    }
    if let Some(name) = spec.as_symbol_name() {
        // Window-relative edge keywords.  We model the text area as starting at
        // column 0 (we do not subtract the line-number gutter / margins / fringe
        // here — see the function doc TODO).  `left`/`text` => column 0; the
        // others depend on window box geometry we do not have, so bail.
        return match name {
            "left" => Some(0.0),
            // `text` as a *width* is the text-area width, which we do not know;
            // as an align-to *position* it is the left edge of the text area = 0.
            "text" if align_to => Some(0.0),
            _ => None,
        };
    }
    if spec.is_cons() {
        let car = spec.cons_car();
        // `(+ EXPR...)` / `(- EXPR...)`: GNU sums recursively-resolved values.
        if car.is_symbol_named("+") || car.is_symbol_named("-") {
            let minus = car.is_symbol_named("-");
            let mut cdr = spec.cons_cdr();
            let mut acc = 0.0;
            let mut first = true;
            while cdr.is_cons() {
                let part = calc_space_columns(cdr.cons_car(), align_to)?;
                if first {
                    acc = if minus { -part } else { part };
                    first = false;
                } else {
                    acc += part;
                }
                cdr = cdr.cons_cdr();
            }
            if minus {
                acc = -acc;
            }
            return Some(acc);
        }
        // `(NUM)` absolute-pixel and image/slice specs are not modeled here.
        return None;
    }
    None
}

/// Resolve a `(space ...)` plist to the number of columns the space occupies,
/// honoring `:width`/`:align-to` (the column-affecting subset GNU's display
/// iterator resolves via `calc_pixel_width_or_height`).  `cur_col` is the column
/// at the spec, needed for `:align-to` (which is an absolute target column).
/// Returns `None` when the spec carries no column-bearing keyword we model.
fn space_spec_advance_columns(plist: Value, cur_col: usize) -> Option<usize> {
    let qcwidth = Value::symbol(":width");
    let qcalign = Value::symbol(":align-to");

    // `:width N` => advance N columns from the current position.
    if let Some(width) = super::plist::plist_get(plist, &qcwidth)
        && !width.is_nil()
        && let Some(cols) = calc_space_columns(width, false)
    {
        return Some(cols.max(0.0).round() as usize);
    }

    // `:align-to COL` => advance so the running column reaches COL (never go
    // backwards, matching GNU which never produces a negative-width space).
    if let Some(align) = super::plist::plist_get(plist, &qcalign)
        && !align.is_nil()
        && let Some(target) = calc_space_columns(align, true)
    {
        let target = target.max(0.0).round() as usize;
        return Some(target.saturating_sub(cur_col));
    }

    None
}

/// How a display line ends in one window -- GNU `enum line_wrap_method`
/// (src/dispextern.h), the `it->line_wrap` that `init_iterator` resolves once
/// per window+buffer pair (src/xdisp.c:3413-3426):
///
/// ```c
///   if (TRUNCATE != 0)
///     it->line_wrap = TRUNCATE;
///   if (base_face_id == DEFAULT_FACE_ID
///       && !it->w->hscroll
///       && (WINDOW_FULL_WIDTH_P (it->w)
///           || NILP (Vtruncate_partial_width_windows)
///           || (FIXNUMP (Vtruncate_partial_width_windows)
///               && (XFIXNUM (Vtruncate_partial_width_windows)
///                   <= WINDOW_TOTAL_COLS (it->w))))
///       && NILP (BVAR (current_buffer, truncate_lines)))
///     it->line_wrap = NILP (BVAR (current_buffer, word_wrap))
///       ? WINDOW_WRAP : WORD_WRAP;
/// ```
///
/// Every screen-line question downstream -- `vertical-motion`,
/// `beginning-of-visual-line`, `end-of-visual-line`, `next-line` /
/// `previous-line` under `line-move-visual`, `move-to-window-line`,
/// `count-screen-lines` -- is answered against this one value.
///
/// It is an enum rather than a `truncate: bool` on purpose.  A boolean can
/// spell only two of GNU's three methods, so a scanner taking one had no way
/// to represent "break at the last word boundary" and silently answered
/// `WindowWrap` instead; and "not truncated" read as a single bit made it easy
/// for the truncation half of the decision to be computed from the GLOBAL
/// `truncate-partial-width-windows` while its sibling `truncate-lines` was
/// read buffer-locally.  With the three methods in the type, a new consumer
/// must say what it does for `WordWrap`, and the value can only be produced by
/// the one function that reads every input the way GNU does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LineWrap {
    /// GNU `TRUNCATE`: the display line is the whole logical line; text past
    /// the right edge is not shown and never starts a new screen line.
    Truncate,
    /// GNU `WINDOW_WRAP`: continuation at the right edge, mid-word.
    WindowWrap,
    /// GNU `WORD_WRAP`: continuation at the last position on the row where a
    /// wrap is allowed (`char_can_wrap_before` after `char_can_wrap_after`,
    /// src/xdisp.c:577-617), falling back to `WindowWrap` when the row offers
    /// no such position.
    WordWrap,
}

/// Where a measured region's pixel x-axis is anchored.
///
/// GNU's `window_text_pixel_size` does not start measuring at FROM.  It starts
/// the display iterator at FROM, rewinds it to the beginning of FROM's
/// *display line* (`move_it_by_lines (&it, 0)` then `it.current_x = it.hpos =
/// it.wrap_prefix_width = 0`, src/xdisp.c:11833-11899), walks forward to FROM
/// and keeps the x it reached as `start_x`; the width it returns is `x -
/// start_x`.  So the prefix of FROM's display line is inside the measurement:
/// `abcd\n` reports 36 from every one of FROM 1..4, not 36/27/18/9.
///
/// The subtraction is dropped when the walk crosses a display-line boundary
/// (`if (it.current_y > start_y) start_x = 0;`, src/xdisp.c:12004): a region
/// spanning several rows is as wide as its widest row, each measured from that
/// row's own left edge.  Both conventions live here because the scanner has to
/// know where the prefix is while it is walking; a caller that passed only
/// FROM could not tell it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MeasuredRange {
    /// First byte of FROM's display line: where the walk starts, and what the
    /// returned width subtracts.  GNU rewinds to the *screen* line, so for a
    /// soft-wrapped line this may be earlier than the logical line start; the
    /// scanner re-derives the display-line boundary from [`RowEdge::wrap`] and
    /// discards whatever it measured before reaching [`Self::from`].
    pub(crate) line_start: EmacsBytePos,
    /// GNU's FROM: the position the returned width and height are measured
    /// from.
    pub(crate) from: EmacsBytePos,
    /// GNU's TO, exclusive.
    pub(crate) to: EmacsBytePos,
}

impl MeasuredRange {
    /// A range that starts measuring at `from` -- the convention for callers
    /// with no display line to rewind to (`buffer-text-pixel-size`, whose GNU
    /// implementation measures the accessible portion of the buffer).
    pub(crate) fn starting_at(from: EmacsBytePos, to: EmacsBytePos) -> Self {
        Self {
            line_start: from,
            from,
            to,
        }
    }
}

/// GNU's `it.last_visible_x` together with `it.line_wrap`: the pixel x at
/// which a display row stops, and what the window does there.
///
/// One quantity, not two.  GNU has a single row edge -- X-LIMIT when the
/// caller supplied one, otherwise the window's body width (`it.last_visible_x
/// = it.first_visible_x + body_width`, src/xdisp.c:3507, which an explicit
/// X-LIMIT replaces whole, src/xdisp.c:11924) -- and `line_wrap` alone decides
/// whether the row truncates at it or continues on the next row.
///
/// Keeping the edge and the wrap budget as separate, differently-united
/// quantities is what let two divergences sit here at once: a GUI row had no
/// edge at all, so a truncated long line measured wider than GNU reports; and
/// a terminal row wrapped at a budget the caller's X-LIMIT did not move, so
/// an X-LIMIT narrower than the window truncated where GNU continues.  With
/// one value the two questions cannot be answered from different inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RowEdge {
    /// The pixel x at which the row stops, measured from the row's left edge.
    pub(crate) x: f32,
    /// What the window does at that x (GNU `it.line_wrap`).
    pub(crate) wrap: LineWrap,
}

impl RowEdge {
    pub(crate) fn new(x: f32, wrap: LineWrap) -> Self {
        Self {
            x: x.max(0.0),
            wrap,
        }
    }

    /// A terminal window's row edge, in the pixel unit the scanner uses.
    ///
    /// GNU reserves the final TTY column: the continuation or truncation glyph
    /// occupies one, so the row stops one column short of the body
    /// (`it->last_visible_x -= it->truncation_pixel_width` /
    /// `continuation_pixel_width`, src/xdisp.c:3508-3516, done "only if the
    /// window has no right fringe", which a terminal never has).  Word
    /// boundary selection remains the display scanner's concern.
    pub(crate) fn tty(body_columns: usize, cell_width: f32, wrap: LineWrap) -> Self {
        let usable = NonZeroUsize::new(body_columns.max(1))
            .expect("positive body width")
            .get()
            .saturating_sub(1)
            .max(1);
        Self::new(usable as f32 * cell_width, wrap)
    }
}

#[cfg(test)]
mod row_edge_tests {
    use super::*;

    #[test]
    fn tty_row_edge_reserves_one_column_for_the_edge_glyph() {
        assert_eq!(RowEdge::tty(80, 9.0, LineWrap::Truncate).x, 711.0);
        assert_eq!(RowEdge::tty(80, 9.0, LineWrap::WindowWrap).x, 711.0);
        // A one-column body cannot go below one usable column.
        assert_eq!(RowEdge::tty(1, 9.0, LineWrap::Truncate).x, 9.0);
    }
}

impl LineWrap {
    /// Whether a display line is a whole logical line.
    pub(crate) fn truncates(self) -> bool {
        matches!(self, Self::Truncate)
    }

    /// Whether a `(COLS . LINES)` goal past the row's content may stop at the
    /// row's own right EDGE -- the column a truncation `$` or continuation
    /// `\` covers -- or must stop at the last glyph the row drew.
    ///
    /// GNU decides this in `move_it_in_display_line`, which is the function
    /// `Fvertical_motion` calls for the goal (`src/indent.c:2540`) and is NOT
    /// the same function as the walk it wraps:
    ///
    /// ```c
    ///   if (it->line_wrap == WORD_WRAP && (op & MOVE_TO_X))
    ///     {
    ///       SAVE_IT (save_it, *it, save_data);
    ///       skip = move_it_in_display_line_to (it, to_charpos, to_x, op);
    ///       /* When word-wrap is on, TO_X may lie past the end of a wrapped
    ///          line.  Then it->current is the character on the next line, so
    ///          backtrack to the space before the wrap point.  */
    ///       if (skip == MOVE_LINE_CONTINUED)
    ///         {
    ///           int prev_x = max (it->current_x - 1, 0);
    ///           RESTORE_IT (it, &save_it, save_data);
    ///           move_it_in_display_line_to (it, -1, prev_x, MOVE_TO_X);
    ///         }
    ///     }
    ///   else
    ///     move_it_in_display_line_to (it, to_charpos, to_x, op);
    /// ```
    ///
    /// (`src/xdisp.c:10859-10888`).  So the WORD_WRAP difference is a
    /// backtrack in the CALLER, taken whenever the goal ran off the end of a
    /// continued row, and it does not depend on `wrap_it` at all.  Ledger 212
    /// residual 1 recorded a reading of `move_it_in_display_line_to`'s
    /// `it->line_wrap != WORD_WRAP || wrap_it.sp < 0` branch that its own
    /// measurement contradicted, and asked the next reader to start from the
    /// contradiction; this is where it goes.  A TRUNCATE row cannot reach the
    /// backtrack (it returns `MOVE_LINE_TRUNCATED`, and the wrapper is not
    /// entered anyway), and a row that ends at a newline or at ZV returns
    /// `MOVE_NEWLINE_OR_CR` / `MOVE_POS_MATCH_OR_ZV`, so only a CONTINUED row
    /// backs off -- which is exactly a row that carries a marker column.
    ///
    /// Measured, GNU Emacs 31.0.90, 80x24 pty, `truncate-lines' nil, ONE
    /// 300-character line of `x' with no wrap opportunity anywhere in it
    /// (`tmp/l216/wrapgoal-gnu-*.txt`), identical COLD and WARM:
    ///
    /// ```text
    ///   word-wrap nil   goal 78 -> 79   goal 79 -> 80   goal 80 -> 80
    ///   word-wrap t     goal 78 -> 79   goal 79 -> 80   goal 80 -> 79
    /// ```
    ///
    /// The two agree until the goal passes the row's edge, and only then does
    /// WORD_WRAP step back -- non-monotonically, which is the signature of a
    /// backtrack rather than of a different stop set.
    pub(crate) fn goal_stops_at_row_edge(self) -> bool {
        match self {
            Self::Truncate | Self::WindowWrap => true,
            Self::WordWrap => false,
        }
    }
}

/// Per-character column width policy for [`region_text_metrics_with_display`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CharColumnWidth {
    /// One column per character (matches `window-text-pixel-size`).
    One,
    /// The character's display width (1 or 2 for wide chars); matches the
    /// `crate::encoding::char_width` accounting of `buffer-text-pixel-size`.
    DisplayWidth,
}

/// The one display-aware region scanner.
///
/// Scans the buffer's byte range `[from, to)` honoring the `display` text
/// property / overlay at each position, and returns the region's extent in
/// logical pixels.  Every element is measured the way GNU's display iterator
/// measures it:
///
///   * a `(space :align-to N)` / `(space :width N)` spec contributes its real
///     column width (GNU resolves these through `calc_pixel_width_or_height`),
///     instead of being counted as a single character column;
///   * an image spec contributes its own pixel advance and its own baseline
///     split, resolved through the same image catalog redisplay uses
///     (`produce_image_glyph`, src/xdisp.c:32384-32520);
///   * plain text, tabs and newlines contribute one cell column each, exactly
///     as before.
///
/// `apply_trim` requests the GNU `TO == t` trailing-blank-line trimming,
/// applied to the *byte* range before scanning.
///
/// `cell` is the frame's character cell (see [`TextCellPixels`]) and `frame` is
/// the frame the region is measured for: image specs are resolved against it
/// and a terminal frame displays no image at all (GNU `valid_image_p` is false
/// without a window system).  `char_width` selects the per-char column
/// accounting (see [`CharColumnWidth`]).
///
/// `range` carries GNU's FROM together with the start of FROM's display line,
/// which is where the walk begins and what the returned width subtracts (see
/// [`MeasuredRange`]).  `edge` is GNU's one row edge and what the window does
/// at it; `y_limit` is GNU's Y-LIMIT in PIXELS, and it both stops the walk and
/// clamps the returned height.
#[allow(clippy::too_many_arguments)] // measurement bounds and display policy are independent inputs
pub(crate) fn region_text_metrics_with_display(
    eval: &super::eval::Context,
    frame: FrameId,
    buffer_id: BufferId,
    range: MeasuredRange,
    apply_trim: bool,
    cell: TextCellPixels,
    char_width: CharColumnWidth,
    edge: Option<RowEdge>,
    y_limit: Option<f32>,
) -> RegionTextMetrics {
    let Some(buf) = eval.buffers.get(buffer_id) else {
        return RegionTextMetrics::EMPTY;
    };

    // The GNU `TO == t` semantics measure through the line ending the last
    // non-empty line, not through trailing blank lines.  We reuse the existing
    // byte-level trimmer to find the trimmed end, then scan with display props.
    let scan_end = if apply_trim {
        let mut bytes = Vec::new();
        buf.copy_emacs_byte_range_to(EmacsByteRange::new(range.from, range.to), &mut bytes);
        let trimmed_len = trim_window_text_to_non_empty_line_end(&bytes).len();
        EmacsBytePos::new(range.from.get() + trimmed_len)
    } else {
        range.to
    };

    let display_sym = Value::symbol("display");
    // GNU's `FETCH_BYTE (start_bpos) == '\n'`: a newline at FROM takes no place
    // on display, so the walk pretends to start at the next line's origin.
    let origin_is_newline = buf.emacs_byte_at_pos(range.from) == Some(b'\n');
    let mut state = ScanState::new(
        cell,
        char_width,
        edge,
        y_limit,
        range.from,
        origin_is_newline,
    );

    // GNU rewinds to the beginning of FROM's display line and walks forward to
    // FROM, so the prefix is walked even when the measured range is empty:
    // FROM == TO still reports the row FROM sits on.
    let mut scan = range.line_start.get().min(range.from.get());
    let end = scan_end.get().max(range.from.get());
    while scan < end {
        if state.y_limit_reached() {
            break;
        }

        if state.before_origin() && scan >= range.from.get() {
            state.reach_origin();
            // The origin's own character is still to be processed.
        }

        // GNU's display iterator processes overlay strings anchored at a
        // position *before* the buffer character there: before-strings of
        // overlays starting here, plus after-strings of overlays ending here
        // (see `load_overlay_strings`/`get_overlay_strings` in xdisp.c).  Each
        // contributes its own laid-out columns to the running line width.
        process_overlay_strings_at(eval, frame, buf, scan, &display_sym, &mut state);

        // A `display` property (text property or overlay) whose value replaces
        // the text it covers -- a `(space ...)` stretch or an image -- defines
        // the whole run's extent.  Resolve it once and advance over the run
        // atomically.
        if let Some((element, run_end)) = display_element_run(
            eval,
            frame,
            buf,
            scan,
            state.line_columns(),
            &display_sym,
            end,
        ) && run_end > scan
        {
            state.push_display_element(element);
            state.last_code = None;
            scan = run_end.min(end);
            continue;
        }

        let scan_pos = EmacsBytePos::new(scan);
        let Some(code) = buf.char_code_after_emacs_byte_pos(scan_pos) else {
            break;
        };
        let char_len = buf
            .char_after_emacs_byte_len(scan_pos)
            .map(|len| len.get().max(1))
            .unwrap_or(1);

        state.push_char(code);
        scan += char_len;
    }

    // The overlay strings anchored at the scan end (`point-max`) are appended
    // after the last buffer char: the *before-string* of an overlay STARTING at
    // `end` (e.g. the zero-length `vertico--candidates-ov` whose before-string
    // holds the whole candidate list — see vertico.el
    // `(make-overlay (point-max) (point-max) ...)` + `'before-string`), and the
    // *after-string* of an overlay ENDING at `end`.  GNU's display iterator
    // reaches `point-max` and runs `handle_stop` -> `get_overlay_strings` there,
    // laying out both kinds before stopping, so we must too.  The main loop only
    // visits positions `scan < end`, so `end` itself is processed exactly once
    // here with the same full `before = true` collection used in the loop body.
    if !state.y_limit_reached() {
        process_overlay_strings_at(eval, frame, buf, end, &display_sym, &mut state);
    }

    // A range that ends where it starts never entered the loop, and one whose
    // last byte is the origin's own character left the origin unreached; both
    // still measure FROM's display line, one of GNU's rewound positions.
    if state.before_origin() {
        state.reach_origin();
    }

    state.finish()
}

/// Mutable accounting state shared between the buffer-text scan and the
/// overlay-string walk in [`region_text_metrics_with_display`].  Tracks the
/// running column on the current line, the widest line seen, the line count,
/// and the row-edge / `y_limit` caps, so overlay strings contribute their
/// columns and embedded newlines exactly like buffer text.
struct ScanState {
    cell: TextCellPixels,
    char_width: CharColumnWidth,
    /// GNU's row edge (`it.last_visible_x`) and what the window does there.
    /// `None` means an unbounded row, which only `buffer-text-pixel-size`
    /// reaches when the window has no geometry at all.
    edge: Option<RowEdge>,
    /// GNU's Y-LIMIT in pixels.  It stops the walk at the first row whose
    /// pixel span contains it and clamps the returned height to it.
    y_limit: Option<f32>,
    /// The height to return once the y-limit stopped the walk: the limit
    /// itself.  `Some` means the scan is over.
    y_limit_stop: Option<f32>,
    /// GNU's `start_x`: the x of FROM inside FROM's display line.  Subtracted
    /// from the width unless the walk crossed a display-line boundary.
    origin_x: f32,
    /// Byte position of GNU's FROM, or `None` once the walk has passed it.
    /// Positions before it are FROM's display-line prefix: they build
    /// [`Self::origin_x`] and are otherwise discarded, because GNU resets the
    /// iterator's y there (`it.current_y = start_y;`, src/xdisp.c:11906) and
    /// measures from FROM's row.
    origin_pos: Option<usize>,
    /// Whether the character AT FROM is a newline (GNU's `FETCH_BYTE
    /// (start_bpos) == '\n'`).
    origin_is_newline: bool,
    /// Whether a row break happened after the walk passed FROM.  GNU drops
    /// `start_x` then (`if (it.current_y > start_y) start_x = 0;`,
    /// src/xdisp.c:12004).
    crossed_display_line: bool,
    /// Widest display line so far, in logical pixels.
    max_width: f32,
    /// Sum of the finished lines' heights, in logical pixels.
    height: f32,
    lines: usize,
    line: LineAdvance,
    /// Tallest reach above / below the baseline on the current line, as the
    /// display iterator accumulates them: from ZERO, so a row that produced no
    /// element is zero pixels tall and a row of text is one cell.  GNU reads
    /// `it.max_ascent + it.max_descent`, which starts at 0 at every row break
    /// (`it->max_ascent = it->max_descent = 0;`, src/xdisp.c:11199) and only
    /// grows as `PRODUCE_GLYPHS` runs -- seeding them with the cell's own split
    /// would make an empty row one line tall where GNU reports none.
    line_ascent: f32,
    line_descent: f32,
    last_code: Option<u32>,
}

impl ScanState {
    fn new(
        cell: TextCellPixels,
        char_width: CharColumnWidth,
        edge: Option<RowEdge>,
        y_limit: Option<f32>,
        origin: EmacsBytePos,
        origin_is_newline: bool,
    ) -> Self {
        let cell = TextCellPixels::new(cell.width, cell.height, cell.ascent);
        Self {
            cell,
            char_width,
            edge: edge.filter(|edge| edge.x.is_finite()),
            y_limit: y_limit.filter(|limit| limit.is_finite() && *limit >= 0.0),
            y_limit_stop: None,
            origin_x: 0.0,
            origin_pos: Some(origin.get()),
            origin_is_newline,
            crossed_display_line: false,
            max_width: 0.0,
            height: 0.0,
            lines: 1,
            line: LineAdvance::default(),
            line_ascent: 0.0,
            line_descent: 0.0,
            last_code: None,
        }
    }

    /// Whether the walk has passed GNU's FROM yet.
    fn before_origin(&self) -> bool {
        self.origin_pos.is_some()
    }

    /// The walk reached GNU's FROM.  Everything measured so far is FROM's
    /// display-line prefix, which the returned width subtracts and which must
    /// not contribute rows or height.
    ///
    /// The prefix's baseline split is kept, as GNU keeps `it.max_ascent` /
    /// `it.max_descent` across the rewind: a range that ends where it starts
    /// (`FROM` == `TO`) still reports the row it sits on, one cell tall.
    fn reach_origin(&mut self) {
        if !self.before_origin() {
            return;
        }
        // GNU rewinds to the start of FROM's SCREEN line, so a row break that
        // is already due at FROM belongs to the prefix: without this, FROM at
        // the first position of a wrapped row reports the row the prefix had
        // just filled, and counts it in the height.  The break is taken while
        // `before_origin` still holds, so it cannot mark a crossing.
        if let Some(edge) = self.wrapping_edge()
            && self.line_width() >= edge
        {
            self.soft_wrap();
        }
        self.origin_pos = None;
        self.origin_x = self.line_width();
        self.max_width = 0.0;
        self.height = 0.0;
        self.lines = 1;
        self.last_code = None;
        // GNU's "If FROM is on a newline, pretend that we start at the
        // beginning of the next line, because the newline takes no place on
        // display" (src/xdisp.c:11920) zeroes the iterator's x for the walk --
        // but not `start_x`, which was read from the iterator just before.
        if self.origin_is_newline {
            self.line.restart();
        }
    }

    /// True once the y-limit stopped the walk; the caller stops scanning.
    fn y_limit_reached(&self) -> bool {
        self.y_limit_stop.is_some()
    }

    fn line_width(&self) -> f32 {
        self.line.width(self.cell.width)
    }

    /// The running row's width in whole text columns, for `(space :align-to N)`
    /// (an absolute target column).  A row carrying an image has no exact
    /// column count; the nearest one keeps the target monotonic.
    fn line_columns(&self) -> usize {
        (self.line_width() / self.cell.width).round().max(0.0) as usize
    }

    fn line_height(&self) -> f32 {
        self.line_ascent + self.line_descent
    }

    /// Whether the current line has reached the row edge, in which case no
    /// further element is produced on it (`MOVE_LINE_TRUNCATED`).
    fn line_at_limit(&self) -> bool {
        self.line.truncated_at.is_some()
            || self
                .edge
                .is_some_and(|edge| edge.wrap.truncates() && self.line_width() >= edge.x)
    }

    /// How much room is left on a truncating row before the edge.
    fn room_to_limit(&self) -> Option<f32> {
        self.edge
            .filter(|edge| edge.wrap.truncates())
            .map(|edge| (edge.x - self.line_width()).max(0.0))
    }

    /// End the row at the edge, as GNU's `MOVE_LINE_TRUNCATED` does.
    fn truncate_at_limit(&mut self) {
        self.line.truncated_at = self.edge.map(|edge| edge.x);
    }

    /// Place a `display` spec's element on the current line, each in its own
    /// unit: a stretch counts text cells (so a long line stays exact), an
    /// image counts its own pixels.
    fn push_display_element(&mut self, element: DisplayElement) {
        match element {
            DisplayElement::Cells(columns) => self.advance_columns(columns),
            DisplayElement::Pixels(extent) => self.push_element(extent),
        }
    }

    /// Fold an element's reach above and below the baseline into the line.
    fn fold_vertical(&mut self, extent: ElementExtent) {
        self.line_ascent = self.line_ascent.max(extent.ascent.max(0.0));
        self.line_descent = self.line_descent.max(extent.descent.max(0.0));
    }

    /// Place a display element that carries its own PIXEL size -- an image --
    /// on the current line.
    ///
    /// Enforces GNU's two horizontal rules: an element that would start at or
    /// past the X-LIMIT edge is never produced (`move_it_in_display_line_to`
    /// returns `MOVE_LINE_TRUNCATED` before `PRODUCE_GLYPHS`, so it contributes
    /// neither advance nor height), and one that straddles the edge is cropped
    /// to it (`produce_image_glyph`, src/xdisp.c:32500-32514) while still
    /// contributing its height: the row is as tall as what it draws.
    fn push_element(&mut self, extent: ElementExtent) {
        if self.line.truncated_at.is_some() {
            return;
        }
        if let Some(room) = self.room_to_limit() {
            if room <= 0.0 {
                self.truncate_at_limit();
                return;
            }
            if extent.advance > room {
                self.line.pixels += room;
                self.truncate_at_limit();
                self.fold_vertical(extent);
                return;
            }
        }
        // An element that does not fit the wrap edge moves to the next row
        // before it is produced (GNU wraps in `move_it_in_display_line_to`
        // before `PRODUCE_GLYPHS`).
        if let Some(edge) = self.wrapping_edge()
            && self.line_width() + extent.advance > edge
            && self.line_width() > 0.0
        {
            self.soft_wrap();
        }
        self.note_text_baseline();
        self.line.pixels += extent.advance.max(0.0);
        self.fold_vertical(extent);
        if self.line_at_limit() {
            self.truncate_at_limit();
        }
    }

    /// The row edge when the window continues past it instead of truncating.
    fn wrapping_edge(&self) -> Option<f32> {
        self.edge
            .filter(|edge| !edge.wrap.truncates())
            .map(|edge| edge.x)
    }

    /// Fold the frame's own character cell into the running row's baseline
    /// split, as producing a text glyph does.
    fn note_text_baseline(&mut self) {
        self.line_ascent = self.line_ascent.max(self.cell.ascent);
        self.line_descent = self.line_descent.max(self.cell.descent());
    }

    /// Advance the running line by `columns` text cells.
    fn advance_columns(&mut self, columns: f32) {
        if self.line.truncated_at.is_some() {
            return;
        }
        // A run that straddles the truncation edge is displayed up to the edge,
        // and the row's width is then exactly the edge.
        if let Some(room) = self.room_to_limit()
            && columns * self.cell.width > room
        {
            self.note_text_baseline();
            self.line.pixels += room;
            self.truncate_at_limit();
            return;
        }
        let mut remaining = columns;
        // A run of text is split across rows at the wrap edge, which is what
        // the continuation glyph marks.
        while remaining > 0.0 {
            self.note_text_baseline();
            match self.wrapping_edge() {
                Some(wrap_at) => {
                    if self.line_width() >= wrap_at {
                        self.soft_wrap();
                        self.note_text_baseline();
                    }
                    let available = wrap_at - self.line_width();
                    let advanced = (available / self.cell.width).min(remaining);
                    self.line.columns += advanced;
                    remaining -= advanced;
                    if remaining > 0.0 {
                        self.soft_wrap();
                        self.note_text_baseline();
                    }
                }
                None => {
                    self.line.columns += remaining;
                    remaining = 0.0;
                }
            }
            if self.line_at_limit() {
                self.truncate_at_limit();
                break;
            }
        }
    }

    /// Finish the current row and start the next one.
    ///
    /// GNU's `move_it_to` stops at the first row whose pixel span CONTAINS
    /// Y-LIMIT, restores the iterator to that row's start -- so the row
    /// contributes no width to `max_current_x` -- and then clamps the height
    /// to the limit (`if (y > max_y) y = max_y`, src/xdisp.c:12012).  A row
    /// that TO is reached inside is not rolled back: reaching TO breaks the
    /// walk before the y test runs.
    ///
    /// Rolling the row back here, at the one place a row can end, is what
    /// keeps the retracted width and the retracted height from disagreeing:
    /// there is no second counter to decrement.
    fn soft_wrap(&mut self) {
        if self.roll_back_if_y_limit_lands_inside() {
            return;
        }
        if !self.before_origin() {
            // The walk from GNU's FROM crossed a display-line boundary, so the
            // returned width is measured from each row's own left edge rather
            // than from FROM.
            self.crossed_display_line = true;
        }
        self.lines += 1;
        self.max_width = self.max_width.max(self.line_width());
        self.height += self.line_height();
        self.line = LineAdvance::default();
        self.line_ascent = 0.0;
        self.line_descent = 0.0;
    }

    /// Retract the current row and stop the walk when Y-LIMIT lands strictly
    /// inside it.  Returns whether the walk stopped.
    ///
    /// Rows before FROM are not walked by GNU at all -- the iterator is
    /// rewound to FROM's display line and reset there -- so a limit that lands
    /// inside one of them must not end the walk.
    fn roll_back_if_y_limit_lands_inside(&mut self) -> bool {
        if self.y_limit_stop.is_some() {
            return true;
        }
        if self.before_origin() {
            return false;
        }
        let Some(limit) = self.y_limit else {
            return false;
        };
        let row_top = self.height;
        if limit >= row_top && limit < row_top + self.line_height() {
            self.y_limit_stop = Some(limit);
            return true;
        }
        false
    }

    /// GNU's returned width: the widest display line walked, minus `start_x`
    /// unless the walk crossed a display-line boundary.
    fn measurement_width(&self) -> f32 {
        let widest = if self.y_limit_stop.is_some() {
            // The row Y-LIMIT landed in was retracted: GNU restored the
            // iterator to that row's start before recording `max_current_x`,
            // so whatever width the row had reached is not part of the answer.
            self.max_width
        } else {
            self.max_width.max(self.line_width())
        };
        let from_origin = if self.crossed_display_line {
            0.0
        } else {
            self.origin_x
        };
        let width = widest - from_origin;
        // GNU: `if (x > max_x) x = max_x;` (src/xdisp.c:12000), the row edge
        // when the caller supplied X-LIMIT and the body width otherwise.
        match self.edge {
            Some(edge) => width.min(edge.x),
            None => width,
        }
    }

    /// End the current line, record its width, and reset for the next line.
    fn newline(&mut self) {
        // The newline is a produced element like any other: GNU's iterator
        // produces it, so `it.max_ascent + it.max_descent` holds the font's
        // split by the time the row closes and an empty display line is one
        // cell tall (`abcd\n\nefgh\n` is 60 pixels, not 40), while a row the
        // walk never entered stays zero.
        self.note_text_baseline();
        self.soft_wrap();
    }

    /// Account for one character `code` (newline, tab, or normal), tracking it
    /// as the last code seen so a trailing newline can be rolled back.
    fn push_char(&mut self, code: u32) {
        if code == '\n' as u32 {
            self.newline();
        } else if self.line.truncated_at.is_none() {
            if code == '\t' as u32 {
                // GNU's tab stops are pixel multiples of
                // `tab_width * FRAME_COLUMN_WIDTH` from the row's left edge
                // (`gui_produce_glyphs`, src/xdisp.c), so text after an image
                // lands on the stop GNU picks.  A row that is all text keeps
                // whole columns, which is the same stop set.
                let tab_width = 8.0 * self.cell.width;
                let x = self.line_width();
                let next_tab_x = (1.0 + (x / tab_width).floor()) * tab_width;
                let advance = (next_tab_x - x).max(0.0);
                if self.line.pixels == 0.0 {
                    // A text-only row's stops are whole columns; rounding the
                    // division keeps that exact for a fractional cell width.
                    self.advance_columns((advance / self.cell.width).round());
                } else {
                    self.push_element(ElementExtent {
                        advance,
                        ascent: self.cell.ascent,
                        descent: self.cell.descent(),
                    });
                }
            } else {
                let columns = match self.char_width {
                    CharColumnWidth::One => 1.0,
                    CharColumnWidth::DisplayWidth => char::from_u32(code)
                        .map(crate::encoding::char_width)
                        .unwrap_or(1) as f32,
                };
                self.advance_columns(columns);
            }
        }
        self.last_code = Some(code);
    }

    fn finish(mut self) -> RegionTextMetrics {
        if let Some(limit) = self.y_limit_stop {
            // GNU stopped before producing the row Y-LIMIT landed in, and
            // clamped the height to the limit.
            return RegionTextMetrics {
                lines: self.lines,
                max_width: self.measurement_width(),
                height: limit,
            };
        }
        // A trailing newline does not open a line, and that empty (never
        // displayed) line contributes neither width nor height.  The final row
        // is closed here, where it can still be retracted by Y-LIMIT.
        if self.last_code == Some('\n' as u32) {
            self.lines = self.lines.saturating_sub(1);
        } else if !self.roll_back_if_y_limit_lands_inside() {
            self.height += self.line_height();
        } else {
            let limit = self.y_limit_stop.expect("rollback records the limit");
            return RegionTextMetrics {
                lines: self.lines,
                max_width: self.measurement_width(),
                height: limit,
            };
        }
        RegionTextMetrics {
            lines: self.lines,
            max_width: self.measurement_width(),
            height: self.height,
        }
    }
}

/// The extent of the `display` property at buffer byte `pos`, if its value
/// replaces the covered text with an element that has its own size.
///
/// Returns `(element, run_end_byte)`.  Mirrors the `display`-spec branch of
/// GNU's display iterator: the covered run is laid out as ONE element -- a
/// stretch glyph for `(space ...)`, an image glyph for `(image ...)` -- and
/// the iterator jumps to the end of the run.  `cur_column` is the running row
/// width in text columns, which `(space :align-to N)` needs (an absolute
/// target column).
///
/// Returns `None` when there is no `display` property, or its value is not a
/// spec this measurement models.  Deliberately unmodelled: `(slice ...)`,
/// `(when ...)`, `((margin ...) ...)`, fringe bitmaps and the neomacs media
/// heads (`video` / `webkit` / `surface`), all of which also replace text on a
/// window-system frame.
fn display_element_run(
    eval: &super::eval::Context,
    frame: FrameId,
    buf: &Buffer,
    pos: usize,
    cur_column: usize,
    display_sym: &Value,
    region_end: usize,
) -> Option<(DisplayElement, usize)> {
    let charpos0 = buf.emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(pos));
    let charpos1 = charpos0.get() as i64 + 1;

    // GNU consults overlays and text properties (get_char_property_and_overlay).
    let (display, overlay) = super::textprop::buffer_overlay_property_at_byte_pos(
        &eval.obarray,
        &eval.buffers,
        buf,
        pos,
        *display_sym,
        None,
    )
    .map(|(v, ov)| (v, Some(ov)))
    .or_else(|| {
        let v = super::textprop::builtin_get_text_property_in_state(
            &eval.obarray,
            &eval.buffers,
            &[Value::fixnum(charpos1), *display_sym],
        )
        .ok()?;
        if v.is_nil() { None } else { Some((v, None)) }
    })?;

    let element = match super::display_spec::display_spec_kind(display) {
        super::display_spec::DisplaySpecKind::Space => {
            let width = space_spec_advance_columns(display.cons_cdr(), cur_column)?;
            DisplayElement::Cells(width as f32)
        }
        // An image replaces its text only on a frame that can display one:
        // GNU's `valid_image_p` is false without a window system, and the
        // covered characters are then displayed normally.
        super::display_spec::DisplaySpecKind::Image
            if eval
                .frames
                .get(frame)
                .is_some_and(|frame| frame.effective_window_system().is_some()) =>
        {
            DisplayElement::Pixels(image_element_extent(eval, frame, display)?)
        }
        _ => return None,
    };

    // End of the run: overlay-end for an overlay `display`, else the
    // text-property range end (GNU `OVERLAY_END` vs `get_property_and_range`).
    let run_end = if let Some(ov) = overlay {
        buf.overlays
            .overlay_end_emacs_byte_pos(ov)
            .map(|p| p.get())
            .unwrap_or(region_end)
    } else {
        let run_end_char1 = super::textprop::builtin_next_single_property_change_in_state(
            &eval.obarray,
            &eval.buffers,
            &[Value::fixnum(charpos1), *display_sym],
        )
        .ok()
        .and_then(|v| match v.kind() {
            ValueKind::Fixnum(n) => Some(n),
            _ => None,
        })
        .unwrap_or_else(|| buf.accessible_char_region().end().get() as i64 + 1);
        buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new((run_end_char1 - 1).max(0) as usize))
            .get()
    };

    Some((element, run_end.min(region_end)))
}

/// The display extent of an image spec, in logical pixels.
///
/// GNU's `produce_image_glyph` + `image_ascent` (src/xdisp.c:32384-32520,
/// src/image.c:1887-1924): the glyph advances by the image's width plus one
/// horizontal margin on each side it reaches, rises `image_ascent` above the
/// baseline and descends by the rest (plus a vertical margin where the image
/// touches the cell's top and bottom).  The size itself comes from the image
/// catalog -- the same lookup redisplay uses -- so a measurement and a
/// redisplay of the same buffer cannot disagree about how big the image is.
fn image_element_extent(
    eval: &super::eval::Context,
    frame: FrameId,
    spec: Value,
) -> Option<ElementExtent> {
    let live_frame = eval.frames.get(frame)?;
    let cell = TextCellPixels::new(
        live_frame.char_width,
        live_frame.char_height,
        live_frame.font_cell_ascent(),
    );
    let environment = super::image_catalog::image_scale_environment(live_frame, &eval.obarray);
    let request = super::image::image_resolve_request_from_spec(
        &spec,
        environment,
        eval.face_table().default_face_colors(),
    )?;
    let placement = eval
        .display_host
        .as_ref()?
        .image_catalog()?
        .lookup(request, environment.size_limit())
        .placement();

    // GNU's glyph arithmetic, for a whole (unsliced) image:
    //   it->pixel_width = img->width + 2 * img->hmargin
    //   it->ascent      = image_ascent (img->height + img->vmargin)
    //   it->descent     = img->height - ascent + 2 * img->vmargin
    // (`produce_image_glyph`, src/xdisp.c:32447-32463; `image_ascent`,
    // src/image.c:1887-1924.)
    let (hmargin, vmargin) = super::image::image_spec_margins(&spec);
    let (hmargin, vmargin) = (hmargin.max(0.0), vmargin.max(0.0));
    let image_width = placement.width().max(1) as f32;
    let image_height = placement.height().max(1) as f32;
    let width = image_width + 2.0 * hmargin;
    let height = image_height + 2.0 * vmargin;
    let ascent = super::image::image_spec_ascent(&spec)
        .resolve(image_height + vmargin, cell.ascent, cell.descent())
        .clamp(0.0, height);
    Some(ElementExtent {
        advance: width,
        ascent,
        descent: (height - ascent).max(0.0),
    })
}

/// One overlay string anchored at the scan position, with the metadata GNU's
/// `compare_overlay_entries` needs to interleave it among the other strings.
struct OverlayStringEntry {
    string: Value,
    overlay: Value,
    /// True for an `after-string`, false for a `before-string`.
    after_string_p: bool,
    priority: i64,
}

fn overlay_string_priority(overlay: Value) -> i64 {
    overlay
        .as_overlay_data()
        .and_then(|data| {
            super::plist::plist_get(data.plist, &Value::symbol("priority"))
                .and_then(|p| p.as_fixnum())
        })
        .unwrap_or(0)
}

/// Rust port of GNU `compare_overlay_entries` (`src/xdisp.c`), mirroring the
/// layout engine's `neovm_bridge::compare_overlay_entries`.  Orders the strings
/// into one visual sequence: different kinds → after-string in front of
/// before-string for *different* overlays but before-string in front of
/// after-string for the *same* overlay; same kind → before-strings sort by
/// increasing priority, after-strings by decreasing priority.
fn compare_overlay_string_entries(
    e1: &OverlayStringEntry,
    e2: &OverlayStringEntry,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if e1.after_string_p != e2.after_string_p {
        if eq_value(&e1.overlay, &e2.overlay) {
            if e1.after_string_p {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        } else if e1.after_string_p {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else if e1.priority != e2.priority {
        if e1.after_string_p {
            e2.priority.cmp(&e1.priority)
        } else {
            e1.priority.cmp(&e2.priority)
        }
    } else {
        Ordering::Equal
    }
}

/// Collect the overlay strings anchored at buffer byte `pos`: `before-string`
/// of every overlay *starting* at `pos`, and `after-string` of every overlay
/// *ending* at `pos`.  Mirrors GNU `load_overlay_strings` (which scans overlays
/// starting or ending at the iterator position).  The entries are ordered by
/// `compare_overlay_string_entries`.  When `before` is false, only after-strings
/// are gathered (used at the scan end, where no buffer char follows).
fn collect_overlay_strings_at(buf: &Buffer, pos: usize, before: bool) -> Vec<OverlayStringEntry> {
    let bytepos = EmacsBytePos::new(pos);
    // Overlays starting or ending at `pos` may be zero-length (e.g. the vertico
    // completion overlay at point-max), so scan the [pos-1, pos+1) neighborhood
    // — exactly as the layout bridge's `overlay_strings_at` does — and then
    // filter by exact start/end below.
    let scan_range = EmacsByteRange::new(
        EmacsBytePos::new(pos.saturating_sub(1)),
        EmacsBytePos::new(pos + 1),
    );
    let mut overlay_ids = buf.overlays.overlays_in_emacs_byte_range(scan_range);
    overlay_ids.sort();
    overlay_ids.dedup();

    let mut entries = Vec::new();
    for oid in overlay_ids {
        let priority = overlay_string_priority(oid);

        if before
            && buf.overlays.overlay_start_emacs_byte_pos(oid) == Some(bytepos)
            && let Some(val) = buf
                .overlays
                .overlay_get_named(oid, Value::symbol("before-string"))
            && val.is_string()
        {
            entries.push(OverlayStringEntry {
                string: val,
                overlay: oid,
                after_string_p: false,
                priority,
            });
        }

        if buf.overlays.overlay_end_emacs_byte_pos(oid) == Some(bytepos)
            && let Some(val) = buf
                .overlays
                .overlay_get_named(oid, Value::symbol("after-string"))
            && val.is_string()
        {
            entries.push(OverlayStringEntry {
                string: val,
                overlay: oid,
                after_string_p: true,
                priority,
            });
        }
    }

    // Stable insertion sort by `compare_overlay_string_entries`.  A manual sort
    // is used (not `sort_by`) because the comparator is NOT a total order — a
    // zero-length overlay carrying both a before- and an after-string can form a
    // comparison cycle that GNU's qsort tolerates but Rust's `sort_by` may
    // panic on.  Overlay-string counts at a position are tiny, so O(n^2) is fine.
    for i in 1..entries.len() {
        let mut j = i;
        while j > 0
            && compare_overlay_string_entries(&entries[j], &entries[j - 1])
                == std::cmp::Ordering::Less
        {
            entries.swap(j, j - 1);
            j -= 1;
        }
    }
    entries
}

/// Process the overlay before- and after-strings anchored at buffer byte `pos`,
/// folding each one's laid-out columns (and embedded newlines) into `state`.
fn process_overlay_strings_at(
    eval: &super::eval::Context,
    frame: FrameId,
    buf: &Buffer,
    pos: usize,
    display_sym: &Value,
    state: &mut ScanState,
) {
    if buf.overlays.is_empty() {
        return;
    }
    for entry in collect_overlay_strings_at(buf, pos, true) {
        walk_overlay_string(eval, frame, entry.string, display_sym, state);
    }
}

/// Walk an overlay string character by character, folding it into `state`.
///
/// The string carries its OWN `display` text properties: a `(space :align-to N)`
/// / `(space :width N)` advances/jumps the running column exactly like the same
/// spec in buffer text (resolved via the shared [`space_spec_advance_columns`]),
/// replacing the covered string chars.  Embedded newlines end the current line
/// (updating the max width) and reset the column to 0 — critical for the
/// multi-line vertico candidate after-string, whose widest line determines the
/// posframe width.  Other chars count as their display width.
fn walk_overlay_string(
    eval: &super::eval::Context,
    frame: FrameId,
    string: Value,
    display_sym: &Value,
    state: &mut ScanState,
) {
    let Some(s) = string.as_lisp_string() else {
        return;
    };
    let schars = s.schars();
    if schars == 0 {
        return;
    }
    let bytes = s.as_bytes();
    let multibyte = s.is_multibyte();

    let mut char_index = 0usize; // 0-based char position into the string
    let mut byte_off = 0usize;
    while char_index < schars && byte_off < bytes.len() {
        if state.y_limit_reached() {
            return;
        }

        // Resolve this string's own `display` property at the char position.  A
        // spec that replaces its text -- `(space ...)`, an image -- covers the
        // run up to the next `display` change and defines its extent.
        if let Some((element, run_end_char)) = string_display_element_run(
            eval,
            frame,
            string,
            char_index,
            state.line_columns(),
            display_sym,
        ) && run_end_char > char_index
        {
            state.push_display_element(element);
            // Skip the covered chars (advance both char and byte cursors).
            let mut skip = run_end_char.min(schars) - char_index;
            while skip > 0 && byte_off < bytes.len() {
                let len = if multibyte {
                    super::emacs_char::string_char(&bytes[byte_off..]).1.max(1)
                } else {
                    1
                };
                byte_off += len;
                char_index += 1;
                skip -= 1;
            }
            state.last_code = None;
            continue;
        }

        let (code, len) = if multibyte {
            let (code, len) = super::emacs_char::string_char(&bytes[byte_off..]);
            (code, len.max(1))
        } else {
            (bytes[byte_off] as u32, 1)
        };
        state.push_char(code);
        byte_off += len;
        char_index += 1;
    }
}

/// String-object analogue of [`display_element_run`]: the element an overlay
/// string's own `display` text property contributes at char position
/// `char_index`, with the position its run ends at.
///
/// `cur_col` is the running column at the spec, needed for `:align-to` (an
/// absolute target).  `run_end_char` is the next `display`-property change in
/// the string (the end of the covered run), defaulting to the string length.
fn string_display_element_run(
    eval: &super::eval::Context,
    frame: FrameId,
    string: Value,
    char_index: usize,
    cur_col: usize,
    display_sym: &Value,
) -> Option<(DisplayElement, usize)> {
    // `get-text-property` on a string takes a 0-based char position.
    let display = super::textprop::builtin_get_text_property_in_state(
        &eval.obarray,
        &eval.buffers,
        &[Value::fixnum(char_index as i64), *display_sym, string],
    )
    .ok()?;

    let element = match super::display_spec::display_spec_kind(display) {
        super::display_spec::DisplaySpecKind::Space => {
            DisplayElement::Cells(space_spec_advance_columns(display.cons_cdr(), cur_col)? as f32)
        }
        super::display_spec::DisplaySpecKind::Image
            if eval
                .frames
                .get(frame)
                .is_some_and(|frame| frame.effective_window_system().is_some()) =>
        {
            DisplayElement::Pixels(image_element_extent(eval, frame, display)?)
        }
        _ => return None,
    };

    let schars = string.as_lisp_string().map(|s| s.schars()).unwrap_or(0);
    let run_end_char = super::textprop::builtin_next_single_property_change_in_state(
        &eval.obarray,
        &eval.buffers,
        &[Value::fixnum(char_index as i64), *display_sym, string],
    )
    .ok()
    .and_then(|v| match v.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Some(n as usize),
        _ => None,
    })
    .unwrap_or(schars)
    .min(schars);

    Some((element, run_end_char))
}

fn trim_window_text_to_non_empty_line_end(bytes: &[u8]) -> &[u8] {
    let mut last_nonblank = bytes.len();
    while last_nonblank > 0 && matches!(bytes[last_nonblank - 1], b' ' | b'\t' | b'\n' | b'\r') {
        last_nonblank -= 1;
    }
    if last_nonblank == 0 {
        return &bytes[..0];
    }

    let mut end = last_nonblank;
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if end < bytes.len() && matches!(bytes[end], b'\n' | b'\r') {
        end += 1;
    }
    &bytes[..end]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
// Variant names intentionally map to the Lisp symbols `mode-line`,
// `header-line`, and `tab-line`.
#[allow(clippy::enum_variant_names)]
enum WindowLineSelector {
    ModeLine,
    HeaderLine,
    TabLine,
}

impl WindowLineSelector {
    fn from_lisp_value(value: Value) -> Option<Self> {
        value.as_symbol_name().and_then(|name| name.parse().ok())
    }
}

fn window_text_pixel_size_includes_mode_line(mode_lines: Option<&Value>) -> bool {
    mode_lines.is_some_and(|mode| {
        mode.is_t()
            || mode.is_symbol_named("t")
            || WindowLineSelector::from_lisp_value(*mode).is_some()
    })
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (format-mode-line &optional FORMAT FACE WINDOW BUFFER) -> string
///
/// Batch-compatible behavior: accepts 1..4 args and returns an empty string.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_format_mode_line(args: Vec<Value>) -> EvalResult {
    expect_args_range("format-mode-line", &args, 1, 4)?;
    if let Some(window) = args.get(2)
        && !window.is_nil()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("windowp"), *window],
        ));
    }
    if let Some(buffer) = args.get(3)
        && !buffer.is_nil()
        && !buffer.is_buffer()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *buffer],
        ));
    }
    Ok(Value::string(""))
}

/// `(format-mode-line &optional FORMAT FACE WINDOW BUFFER)` evaluator-backed variant.
///
/// Handles string formats with %-construct expansion and list-based format
/// specs by recursively processing elements (symbols, strings, :eval, :propertize,
/// and conditional cons cells).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn format_mode_line_from_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    frames: &crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
    args: Vec<Value>,
) -> Result<Option<Value>, Flow> {
    expect_args_range("format-mode-line", &args, 1, 4)?;
    validate_optional_window_designator_in_state(
        frames,
        args.get(2),
        crate::emacs_core::window_cmds::WindowDomain::Any,
    )?;
    validate_optional_buffer_designator_in_state(buffers, args.get(3))?;

    let target_buffer = resolve_mode_line_buffer_in_state(frames, args.get(2), args.get(3));
    let saved_buffer = buffers.current_buffer_id();
    if let Some(buffer_id) = target_buffer {
        buffers.switch_current_unrecorded(buffer_id);
    }

    if args[0].is_nil() {
        if let Some(buffer_id) = saved_buffer {
            buffers.switch_current_unrecorded(buffer_id);
        }
        return Ok(Some(Value::string("")));
    }

    let format_val = args[0];
    let face_spec = resolve_mode_line_face_spec(&args);
    let pctx = build_mode_line_percent_context(frames, &*buffers, None, obarray, args.get(2));
    let mut result = ModeLineRendered::default();
    let needs_eval = format_mode_line_recursive_in_state(
        obarray,
        dynamic,
        &*buffers,
        processes,
        &pctx,
        &format_val,
        &mut result,
        0,
        false,
    );

    if let Some(buffer_id) = saved_buffer {
        buffers.switch_current_unrecorded(buffer_id);
    }

    if needs_eval {
        Ok(None)
    } else {
        Ok(Some(result.into_value(face_spec)))
    }
}

pub(crate) fn builtin_format_mode_line_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    finish_format_mode_line_in_eval(eval, &args)
}

/// Render a mode-line format in GNU's `MODE_LINE_DISPLAY` mode.
///
/// This mirrors GNU's `display_mode_line` (xdisp.c:27911-27935) rather
/// than its `Fformat_mode_line` string API: the walker runs with
/// `mode_line_target = MODE_LINE_DISPLAY`, which makes `%-` expand to
/// dashes that fill the remaining row width (as opposed to the literal
/// `"--"` that string mode returns). The layout engine calls this
/// entry point directly (bypassing the Lisp-facing
/// `format-mode-line` builtin) when it needs a fully rendered TTY/GUI
/// mode-line row.
///
/// Arguments:
///
/// - `eval`: evaluator context (for risky-local lookup and `:eval` evaluation).
/// - `format_val`: the mode-line format expression — the buffer's
///   `mode-line-format` slot value, already resolved.
/// - `window`: target window (for %-spec position info).
/// - `buffer`: target buffer (for buffer-local percent specs).
/// - `target_cols`: the row width in character cells. `%-` fills to this width.
///
/// Returns the rendered string. The compatibility entry defers nonlocal exits
/// to Context's redisplay driver and returns an empty string for this row.
/// Use [`try_format_mode_line_for_display_with_sources`] to handle Flow directly.
pub fn format_mode_line_for_display(
    eval: &mut super::eval::Context,
    format_val: Value,
    window: Value,
    buffer: Value,
    target_cols: usize,
) -> Value {
    format_mode_line_for_display_with_sources(eval, format_val, window, buffer, target_cols)
        .into_value()
}

/// [`format_mode_line_for_display`] with GNU's per-glyph Lisp string source
/// identity retained for the layout/input pipeline.
pub fn format_mode_line_for_display_with_sources(
    eval: &mut super::eval::Context,
    format_val: Value,
    window: Value,
    buffer: Value,
    target_cols: usize,
) -> ModeLineDisplayOutput {
    if eval.has_mode_line_display_flow() {
        return ModeLineDisplayOutput::from_root_string(Value::string(""));
    }
    match try_format_mode_line_for_display_with_sources(
        eval,
        format_val,
        window,
        buffer,
        target_cols,
    ) {
        Ok(output) => output,
        Err(flow) => {
            if !flow.is_signal() && mode_line_flow_policy::enabled() {
                eval.defer_mode_line_display_flow(flow);
            } else {
                tracing::debug!("mode-line display failed: {flow:?}");
            }
            ModeLineDisplayOutput::from_root_string(Value::string(""))
        }
    }
}

/// Fallible redisplay seam. Signals in `:eval` are logged and contribute nil;
/// throws and other nonlocal exits return only after all display scopes restore.
/// The compatibility adapter defers these exits to Context's redisplay driver.
pub fn try_format_mode_line_for_display_with_sources(
    eval: &mut super::eval::Context,
    format_val: Value,
    window: Value,
    buffer: Value,
    target_cols: usize,
) -> Result<ModeLineDisplayOutput, Flow> {
    try_format_mode_line_display(eval, format_val, window, buffer, target_cols, true)
}

/// Frame-title evaluation uses the same safe evaluator, but unlike actual
/// mode/header/tab lines GNU does not save match data around the title walker.
pub fn try_format_frame_title_for_display(
    eval: &mut super::eval::Context,
    format_val: Value,
    window: Value,
    buffer: Value,
    target_cols: usize,
) -> EvalResult {
    try_format_mode_line_display(eval, format_val, window, buffer, target_cols, false)
        .map(ModeLineDisplayOutput::into_value)
}

fn try_format_mode_line_display(
    eval: &mut super::eval::Context,
    format_val: Value,
    window: Value,
    buffer: Value,
    target_cols: usize,
    save_match_data: bool,
) -> Result<ModeLineDisplayOutput, Flow> {
    let args = [format_val, Value::NIL, window, buffer];
    validate_optional_window_designator(
        eval,
        args.get(2),
        crate::emacs_core::window_cmds::WindowDomain::Any,
    )?;
    validate_optional_buffer_designator(eval, args.get(3))?;
    let saved_buffer = eval.buffers.current_buffer_id();
    let mut title_selection = if !save_match_data && mode_line_flow_policy::enabled() {
        Some(mode_line_flow_policy::FormatSelection::enter(
            eval,
            Some(&window),
        )?)
    } else {
        None
    };
    let target_buffer = resolve_mode_line_buffer(eval, args.get(2), args.get(3));
    if let Some(buffer_id) = target_buffer {
        if let Err(flow) = eval.set_current_buffer_unrecorded(buffer_id) {
            if let Some(selection) = title_selection {
                selection.restore(eval);
            }
            if let Some(buffer_id) = saved_buffer {
                eval.restore_current_buffer_if_live(buffer_id);
            }
            return Err(flow);
        }
    }
    let saved_match_data =
        (save_match_data && mode_line_flow_policy::enabled()).then(|| eval.match_data.clone());
    let match_roots = mode_line_gc::ScratchRoots::new();
    if let Some(crate::emacs_core::regex::SearchedString::Heap(searched)) = saved_match_data
        .as_ref()
        .and_then(Option::as_ref)
        .and_then(crate::emacs_core::regex::MatchData::searched_string)
    {
        match_roots.pin(*searched);
    }

    // GNU `display_mode_lines` (xdisp.c) makes the window being redisplayed the
    // selected window before walking its mode/tab/header-line format, so that
    // `:eval` forms reading `(selected-window)` — e.g. the default
    // `tab-line-tabs-function` `tab-line-tabs-window-buffers` — operate on this
    // window rather than the globally selected one.  Without it every window's
    // tab line shows the selected window's buffer.
    let saved_window_selection = window
        .as_window_id()
        .map(|wid| eval.frames.select_window_for_mode_line(WindowId(wid)));

    // GNU `display_mode_line` also runs with the buffer point set to the
    // window's `w->pointm`, so point-dependent specs (`%l`, `%c`, `(point)` in
    // `:eval`) reflect THIS window. Do it only for a window that is NOT the
    // originally-selected one: that window's live buffer point is already
    // correct and must not be clobbered. Without this, every window's mode line
    // showed the selected window's line/column (the layout temp-selects each
    // window here, which otherwise fools the "is selected" check).
    let saved_point = window.as_window_id().and_then(|wid| {
        let prev_selected = saved_window_selection
            .and_then(|(_, prev_frame_window)| prev_frame_window)
            .map(|(_, prev_window)| prev_window);
        if prev_selected == Some(WindowId(wid)) {
            return None;
        }
        let frame_id = eval.frames.find_window_frame_id(WindowId(wid))?;
        let window_point = match eval.frames.get(frame_id)?.find_window(WindowId(wid))? {
            crate::window::Window::Leaf { point, .. } => *point,
            _ => return None,
        };
        let buffer_id = eval.buffers.current_buffer_id()?;
        let buffer = eval.buffers.get_mut(buffer_id)?;
        let saved = buffer.point_emacs_byte_pos();
        let target = buffer.char_pos_to_emacs_byte_pos_clamped(window_point.to_char_pos());
        buffer.goto_emacs_byte_pos(target);
        Some((buffer_id, saved))
    });

    let result_value = if format_val.is_nil() {
        Ok(ModeLineDisplayOutput::from_root_string(Value::string("")))
    } else {
        let face_spec = resolve_mode_line_face_spec(&args);
        let mut pctx = build_mode_line_percent_context(
            &eval.frames,
            &eval.buffers,
            Some(&eval.coding_systems),
            &eval.obarray,
            args.get(2),
        );
        pctx.target = ModeLineTarget::Display {
            columns: target_cols,
        };
        let mut rendered = ModeLineRendered::default();
        {
            // pctx caches heap Values (frame name, eol indicator) captured
            // before the walk; an :eval that renames the frame would orphan
            // them mid-walk. Root them for the walk's span.
            let _accumulator_roots = mode_line_gc::ScratchRoots::new();
            let pctx_root_scope = eval.save_specpdl_roots();
            for value in [pctx.frame_name, pctx.eol_indicator, face_spec.face]
                .into_iter()
                .flatten()
            {
                eval.push_specpdl_root(value);
            }
            let output = format_mode_line_recursive(
                eval,
                &pctx,
                &format_val,
                &mut rendered,
                0,
                false,
                title_selection.as_mut(),
            )
            .map(|()| rendered.into_display_output(face_spec));
            eval.restore_specpdl_roots(pctx_root_scope);
            output
        }
    };

    if let Some((buffer_id, saved)) = saved_point
        && let Some(buffer) = eval.buffers.get_mut(buffer_id)
    {
        buffer.goto_emacs_byte_pos(saved);
    }
    if let Some(saved) = saved_window_selection {
        eval.frames.restore_selected_window_for_mode_line(saved);
    }
    if let Some(selection) = title_selection {
        selection.restore(eval);
    }
    if let Some(buffer_id) = saved_buffer {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    if let Some(match_data) = saved_match_data {
        eval.match_data = match_data;
    }
    result_value
}

pub(crate) fn finish_format_mode_line_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_args_range("format-mode-line", args, 1, 4)?;
    validate_optional_window_designator(
        eval,
        args.get(2),
        crate::emacs_core::window_cmds::WindowDomain::Any,
    )?;
    validate_optional_buffer_designator(eval, args.get(3))?;

    let saved_buffer = eval.buffers.current_buffer_id();
    let mut selection =
        if mode_line_flow_policy::enabled() && !args[0].is_nil() && !eval.noninteractive() {
            Some(mode_line_flow_policy::FormatSelection::enter(
                eval,
                args.get(2),
            )?)
        } else {
            None
        };
    let result = (|| {
        let target_buffer = resolve_mode_line_buffer(eval, args.get(2), args.get(3));
        if let Some(buffer_id) = target_buffer {
            eval.set_current_buffer_unrecorded(buffer_id)?;
        }
        let result = if args[0].is_nil() || eval.noninteractive() {
            Ok(Value::string(""))
        } else {
            let format_val = args[0];
            let face_spec = resolve_mode_line_face_spec(args);
            let pctx = build_mode_line_percent_context(
                &eval.frames,
                &eval.buffers,
                Some(&eval.coding_systems),
                &eval.obarray,
                args.get(2),
            );
            let mut result = ModeLineRendered::default();
            {
                // pctx caches heap Values (frame name, eol indicator) captured
                // before the walk; an :eval that renames the frame would orphan
                // them mid-walk. Root them for the walk's span.
                let _accumulator_roots = mode_line_gc::ScratchRoots::new();
                let pctx_root_scope = eval.save_specpdl_roots();
                for value in [pctx.frame_name, pctx.eol_indicator, face_spec.face]
                    .into_iter()
                    .flatten()
                {
                    eval.push_specpdl_root(value);
                }
                let output = format_mode_line_recursive(
                    eval,
                    &pctx,
                    &format_val,
                    &mut result,
                    0,
                    false,
                    selection.as_mut(),
                )
                .map(|()| result.into_value(face_spec));
                eval.restore_specpdl_roots(pctx_root_scope);
                output
            }
        };

        result
    })();
    if let Some(selection) = selection {
        selection.restore(eval);
    }
    if let Some(buffer_id) = saved_buffer {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    result
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn finish_format_mode_line_in_state_with_eval(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    frames: &crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
    args: &[Value],
    mut eval_form: impl FnMut(&Value, &crate::buffer::BufferManager) -> Result<Value, Flow>,
) -> EvalResult {
    // The compatibility callback must collect from the same active heap as
    // these split-state Values; the scratch registry carries its identity.
    let roots = mode_line_gc::ScratchRoots::new();
    for &arg in args {
        roots.pin(arg);
    }
    expect_args_range("format-mode-line", args, 1, 4)?;
    validate_optional_window_designator_in_state(
        frames,
        args.get(2),
        crate::emacs_core::window_cmds::WindowDomain::Any,
    )?;
    validate_optional_buffer_designator_in_state(buffers, args.get(3))?;

    let target_buffer = resolve_mode_line_buffer_in_state(frames, args.get(2), args.get(3));
    let saved_buffer = buffers.current_buffer_id();
    if let Some(buffer_id) = target_buffer {
        buffers.switch_current_unrecorded(buffer_id);
    }

    let result = if args[0].is_nil() {
        Ok(Value::string(""))
    } else {
        let format_val = args[0];
        let face_spec = resolve_mode_line_face_spec(args);
        let pctx = build_mode_line_percent_context(frames, &*buffers, None, obarray, args.get(2));
        if let Some(face) = face_spec.face {
            roots.pin(face);
        }
        for value in [pctx.frame_name, pctx.eol_indicator].into_iter().flatten() {
            roots.pin(value);
        }
        let mut result = ModeLineRendered::default();
        format_mode_line_recursive_in_state_with_eval_rooted(
            obarray,
            dynamic,
            &*buffers,
            processes,
            &pctx,
            &format_val,
            &mut result,
            0,
            false,
            &mut eval_form,
            &roots,
        )
        .map(|()| result.into_value(face_spec))
    };

    if let Some(buffer_id) = saved_buffer {
        buffers.switch_current_unrecorded(buffer_id);
    }
    result
}

fn mode_line_symbol_value_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    name: &str,
) -> Option<Value> {
    let sym = crate::emacs_core::intern::intern(name);
    if let Some(buf) = buffers.current_buffer()
        && let Some(value) = buf.get_buffer_local_by_sym_id_gated(sym, obarray.is_localized(sym))
    {
        return Some(value);
    }

    obarray.symbol_value(name).copied()
}

fn mode_line_human_readable_size(mut quotient: usize) -> String {
    const POWER_LETTER: [char; 11] = ['\0', 'k', 'M', 'G', 'T', 'P', 'E', 'Z', 'Y', 'R', 'Q'];

    let mut tenths = None;
    let mut exponent = 0_usize;

    if quotient >= 1000 {
        let mut remainder: usize;
        loop {
            remainder = quotient % 1000;
            quotient /= 1000;
            exponent += 1;
            if quotient < 1000 {
                break;
            }
        }

        if quotient <= 9 {
            let rounded_tenths = remainder / 100;
            if remainder % 100 >= 50 {
                if rounded_tenths < 9 {
                    tenths = Some(rounded_tenths + 1);
                } else {
                    quotient += 1;
                    if quotient < 10 {
                        tenths = Some(0);
                    } else {
                        tenths = None;
                    }
                }
            } else {
                tenths = Some(rounded_tenths);
            }
        } else if remainder >= 500 {
            if quotient < 999 {
                quotient += 1;
            } else {
                quotient = 1;
                exponent += 1;
                tenths = Some(0);
            }
        }
    }

    let mut rendered = quotient.to_string();
    if let Some(tenths) = tenths {
        rendered.push('.');
        rendered.push(char::from(b'0' + tenths as u8));
    }
    let suffix = POWER_LETTER[exponent];
    if suffix != '\0' {
        rendered.push(suffix);
    }
    rendered
}

fn mode_line_process_status_in_state(
    buffers: &crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
) -> &'static str {
    let Some(buffer_id) = buffers.current_buffer_id() else {
        return "no process";
    };
    let Some(process_id) = processes.find_by_buffer_id(buffer_id) else {
        return "no process";
    };
    // GNU's `%s` is `Fsymbol_name (Fprocess_status (obj))`
    // (src/xdisp.c:29717-29725), so in GNU it harvests the child status like
    // every other `Fprocess_status` caller.  This frame holds
    // `&ProcessManager`, so it cannot; the hole is enumerated rather than
    // implicit -- see `process::UnrecordedStatusRead`.
    let Some(observed) = processes.read_status_without_recording(
        crate::emacs_core::process::UnrecordedStatusRead::ModeLinePercentS,
        process_id,
    ) else {
        return "no process";
    };
    observed
        .public_status_symbol()
        .as_symbol_name()
        .unwrap_or("no process")
}

fn mode_line_symbol_is_risky(obarray: &crate::emacs_core::symbol::Obarray, name: &str) -> bool {
    obarray
        .get_property(name, "risky-local-variable")
        .is_some_and(|value| !value.is_nil())
}

fn mode_line_conditional_branch(cdr: Value, branch_is_then: bool) -> Option<Value> {
    if !cdr.is_cons() {
        return None;
    }
    if branch_is_then {
        return Some(cdr.cons_car());
    }
    let else_tail = cdr.cons_cdr();
    if else_tail.is_cons() {
        Some(else_tail.cons_car())
    } else {
        None
    }
}

/// Window and frame context for GNU-compatible mode-line percent specs.
///
/// Corresponds to the `struct window *w` and `struct frame *f` parameters
/// in GNU's `decode_mode_spec` (xdisp.c:29083).
#[derive(Clone)]
struct ModeLinePercentContext {
    /// Internal zero-based character offset of the first visible character.
    /// Derived from GNU `marker_position(w->start)`, which is Lisp one-based.
    window_start: usize,
    /// Window end position (last visible character position).
    /// In GNU this is `BUF_Z(b) - w->window_end_pos`.
    window_end: usize,
    /// Frame name for `%F`.  GNU: `f->title` then `f->name` then "Emacs".
    frame_name: Option<Value>,
    /// Buffer coding presentation for `%z`/`%Z`.
    /// GNU deliberately displays a blank instead of any coding mnemonic when
    /// the buffer is unibyte.  The enum makes that state impossible to confuse
    /// with a multibyte coding system whose mnemonic happens to be a space.
    buffer_coding: BufferCodingDisplay,
    /// The frame-dependent prefix GNU places before the buffer mnemonic.
    /// A closed enum prevents a TTY context from existing without both coding
    /// systems and keeps their GNU-defined ordering out of call sites.
    frame_coding_mnemonics: FrameCodingMnemonics,
    /// EOL type string for `%Z` (`:`, `\`, `/`, or undecided).
    eol_indicator: Option<Value>,
    /// GNU's closed `mode_line_target` state.  This controls the target-specific
    /// `%-` expansion without letting an untyped optional width conflate string
    /// formatting with direct display formatting.
    target: ModeLineTarget,
    /// Byte position of THIS window's point, for point-dependent specs (`%l`,
    /// `%c`). GNU sets the buffer's point to `w->pointm` while displaying a
    /// window's mode line, so these reflect that window — not the selected
    /// window's point (which is the live buffer point). `None` falls back to the
    /// buffer point. Set per-window in `build_mode_line_percent_context`.
    window_point: Option<EmacsBytePos>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameCodingMnemonics {
    WindowSystem,
    Tty { keyboard: char, terminal: char },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BufferCodingDisplay {
    Multibyte { mnemonic: char },
    UnibyteBlank,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ModeLineTarget {
    #[default]
    String,
    Display {
        columns: usize,
    },
}

impl BufferCodingDisplay {
    /// GNU `decode_mode_spec` emits keyboard, terminal, then buffer coding on a
    /// non-window-system frame.  It calls the same helper for all three, and
    /// that helper blanks every mnemonic when the current buffer is unibyte
    /// (`src/xdisp.c:29238-29276`).
    fn with_frame(self, frame: FrameCodingMnemonics) -> String {
        match (self, frame) {
            (Self::Multibyte { mnemonic }, FrameCodingMnemonics::WindowSystem) => {
                mnemonic.to_string()
            }
            (Self::Multibyte { mnemonic }, FrameCodingMnemonics::Tty { keyboard, terminal }) => {
                format!("{keyboard}{terminal}{mnemonic}")
            }
            (Self::UnibyteBlank, FrameCodingMnemonics::WindowSystem) => " ".to_owned(),
            (Self::UnibyteBlank, FrameCodingMnemonics::Tty { .. }) => "   ".to_owned(),
        }
    }
}

impl Default for ModeLinePercentContext {
    fn default() -> Self {
        Self {
            window_start: 0,
            window_end: 0,
            frame_name: None,
            buffer_coding: BufferCodingDisplay::Multibyte { mnemonic: '-' },
            frame_coding_mnemonics: FrameCodingMnemonics::WindowSystem,
            eol_indicator: None,
            target: ModeLineTarget::String,
            window_point: None,
        }
    }
}

/// Build a `ModeLinePercentContext` from frame/window/buffer state.
fn build_mode_line_percent_context(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    coding_systems: Option<&crate::emacs_core::coding::CodingSystemManager>,
    obarray: &crate::emacs_core::symbol::Obarray,
    window_arg: Option<&Value>,
) -> ModeLinePercentContext {
    let mut ctx = ModeLinePercentContext {
        buffer_coding: BufferCodingDisplay::Multibyte { mnemonic: '-' },
        ..Default::default()
    };

    // --- Frame name (GNU: f->title, f->name, "Emacs") ---
    if let Some(frame) = frames.selected_frame() {
        let title = frame.title_value();
        if title.is_string() {
            ctx.frame_name = Some(title);
        } else if frame.explicit_name || frame.effective_window_system().is_none() {
            let name = frame.name_value();
            if name.is_string() {
                ctx.frame_name = Some(name);
            }
        }
    }

    // --- Window start/end (GNU: w->start, BUF_Z(b) - w->window_end_pos) ---
    let resolved_window = resolve_mode_line_window(frames, window_arg);
    let context_buffer = resolved_window
        .and_then(|window| window.buffer_id())
        .and_then(|buffer_id| buffers.get(buffer_id))
        .or_else(|| buffers.current_buffer());
    if let Some(window) = resolved_window {
        if let crate::window::Window::Leaf {
            id,
            window_start,
            point,
            ..
        } = window
        {
            // Window positions are 1-indexed (Elisp convention); convert to
            // 0-indexed to match buffer begv/zv.
            ctx.window_start = window_start.to_char_pos().get();
            if let Some(buf) = context_buffer {
                let buffer_z = LispCharPos1::from_one_based_usize(
                    buf.point_max_char_pos().get().saturating_add(1),
                );
                ctx.window_end = window
                    .window_end_charpos(buffer_z)
                    .unwrap_or(buffer_z)
                    .to_one_based_usize();
                // GNU displays a window's mode line with the buffer point set to
                // that window's `w->pointm`, so `%l`/`%c` reflect THIS window.
                // The selected window's point is the live buffer point; a
                // non-selected window keeps its own stored point.
                let is_selected = frames
                    .selected_frame()
                    .is_some_and(|frame| frame.selected_window == *id);
                ctx.window_point = Some(if is_selected {
                    buf.point_emacs_byte_pos()
                } else {
                    buf.char_pos_to_emacs_byte_pos_clamped(point.to_char_pos())
                });
            } else {
                ctx.window_end = ctx.window_start;
            }
        }
    } else if let Some(buf) = context_buffer {
        // Fallback: use buffer positions when no window is available.
        ctx.window_start = 0;
        ctx.window_end = buf.point_max_char_pos().get();
    }

    // --- Coding system mnemonic (GNU: decode_mode_spec_coding) ---
    let cs_name = context_buffer
        .and_then(|b| b.buffer_local_value("buffer-file-coding-system"))
        .and_then(|v| v.as_symbol_id());
    let coding_mnemonic = cs_name
        .map(|name| coding_system_mnemonic_char(coding_systems, name))
        .unwrap_or('-');
    ctx.buffer_coding = if context_buffer.is_some_and(|buffer| !buffer.get_multibyte()) {
        BufferCodingDisplay::UnibyteBlank
    } else {
        BufferCodingDisplay::Multibyte {
            mnemonic: coding_mnemonic,
        }
    };
    if let Some(name) = cs_name {
        ctx.eol_indicator = coding_system_eol_indicator_value(obarray, name);
    }

    // --- Terminal and keyboard coding mnemonics (TTY only) ---
    // GNU `decode_mode_spec` emits keyboard, terminal, then buffer.
    if frames
        .selected_frame()
        .is_some_and(|frame| frame.effective_window_system().is_none())
    {
        let (keyboard, terminal) = if let Some(coding_systems) = coding_systems {
            (
                coding_system_mnemonic_char(
                    Some(coding_systems),
                    coding_systems.keyboard_coding_sym(),
                ),
                coding_system_mnemonic_char(
                    Some(coding_systems),
                    coding_systems.terminal_coding_sym(),
                ),
            )
        } else {
            let term_cs = obarray
                .symbol_value("terminal-coding-system")
                .and_then(|v| v.as_symbol_id());
            let kbd_cs = obarray
                .symbol_value("keyboard-coding-system")
                .and_then(|v| v.as_symbol_id());
            (
                kbd_cs
                    .map(|name| coding_system_mnemonic_char(None, name))
                    .unwrap_or('-'),
                term_cs
                    .map(|name| coding_system_mnemonic_char(None, name))
                    .unwrap_or('-'),
            )
        };
        ctx.frame_coding_mnemonics = FrameCodingMnemonics::Tty { keyboard, terminal };
    }

    ctx
}

/// Resolve the WINDOW argument to an actual Window reference.
fn resolve_mode_line_window<'a>(
    frames: &'a crate::window::FrameManager,
    window_arg: Option<&Value>,
) -> Option<&'a crate::window::Window> {
    // Try explicit window argument first.
    if let Some(windowish) = window_arg
        && !windowish.is_nil()
    {
        let wid = if let Some(id) = windowish.as_window_id() {
            Some(crate::window::WindowId(id))
        } else {
            windowish
                .as_fixnum()
                .filter(|&id| id >= 0)
                .map(|id| crate::window::WindowId(id as u64))
        };
        if let Some(wid) = wid {
            for fid in frames.frame_list() {
                if let Some(frame) = frames.get(fid)
                    && let Some(window) = frame.find_window(wid)
                {
                    return Some(window);
                }
            }
        }
    }

    // Fall back to selected window of selected frame.
    if let Some(frame) = frames.selected_frame() {
        let selected = frame.selected_window;
        return frame.find_window(selected);
    }

    None
}

/// Derive coding system mnemonic character from coding system name.
///
/// Matches GNU `decode_mode_spec_coding` heuristics for common systems.
fn coding_system_mnemonic_char(
    coding_systems: Option<&crate::emacs_core::coding::CodingSystemManager>,
    cs_name: crate::emacs_core::intern::SymId,
) -> char {
    if let Some(mnemonic) = coding_systems.and_then(|manager| manager.mode_line_mnemonic(cs_name)) {
        return mnemonic;
    }
    let cs_name = crate::emacs_core::intern::resolve_sym(cs_name);
    let base = cs_name
        .strip_suffix("-unix")
        .or_else(|| cs_name.strip_suffix("-dos"))
        .or_else(|| cs_name.strip_suffix("-mac"))
        .unwrap_or(cs_name);
    match base {
        "utf-8"
        | "utf-8-emacs"
        | "utf-8-auto"
        | "mule-utf-8"
        | "utf-16"
        | "utf-16-be"
        | "utf-16-le"
        | "utf-16be"
        | "utf-16le"
        | "utf-16be-with-signature"
        | "utf-16le-with-signature" => 'U',
        "undecided" | "prefer-utf-8" | "nil" => '-',
        "raw-text" => '=',
        "no-conversion" | "binary" => '=',
        "us-ascii" | "ascii" => '.',
        "iso-8859-1" | "iso-latin-1" | "latin-1" => '1',
        "iso-8859-2" | "iso-latin-2" | "latin-2" => '2',
        "iso-8859-3" | "latin-3" => '3',
        "iso-8859-4" | "latin-4" => '4',
        "iso-8859-5" | "latin-5" => '5',
        "iso-2022-jp" | "junet" => 'J',
        "euc-jp" => 'E',
        "shift_jis" | "sjis" => 'S',
        "iso-2022-kr" => 'K',
        "euc-kr" => 'e',
        "gb2312" | "euc-cn" | "cn-gb" => 'C',
        "big5" => 'B',
        _ => '-',
    }
}

/// Derive EOL type indicator from coding system name, using the
/// `eol-mnemonic-*` variables from the obarray (matches GNU semantics).
fn coding_system_eol_indicator_value(
    obarray: &crate::emacs_core::symbol::Obarray,
    cs_name: crate::emacs_core::intern::SymId,
) -> Option<Value> {
    let cs_name = crate::emacs_core::intern::resolve_sym(cs_name);
    let var_name = if cs_name.ends_with("-dos") {
        "eol-mnemonic-dos"
    } else if cs_name.ends_with("-mac") {
        "eol-mnemonic-mac"
    } else if cs_name.ends_with("-unix") {
        "eol-mnemonic-unix"
    } else {
        "eol-mnemonic-undecided"
    };
    obarray
        .symbol_value(var_name)
        .copied()
        .filter(|value| value.is_string() || value.as_char().is_some())
}

/// Check if a directory path looks like a Tramp remote path.
///
/// Tramp paths match `/METHOD:...` where METHOD is a lowercase alpha string.
fn is_remote_directory(dir: &str) -> bool {
    if !dir.starts_with('/') {
        return false;
    }
    let rest = &dir[1..];
    if let Some(colon_pos) = rest.find(':') {
        colon_pos >= 2 && rest[..colon_pos].bytes().all(|b| b.is_ascii_lowercase())
    } else {
        false
    }
}

fn mode_line_runtime_string(value: &Value) -> Option<String> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

/// Compute GNU `percent99` — percentage capped at 99, rounded up.
fn percent99(n: usize, d: usize) -> usize {
    if d == 0 {
        return 0;
    }
    let pct = (100 * n).div_ceil(d);
    pct.min(99)
}

/// One range of formatted mode-line output that still originates in an exact
/// Lisp string object.
///
/// GNU keeps this association directly in every display glyph's `object` and
/// `charpos` fields.  Neomacs formats chrome before layout, so the formatter
/// returns this compact sidecar for layout to restore that provenance without
/// exposing VM objects through the renderer protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModeLineDisplaySourceSpan {
    output_start: usize,
    output_end: usize,
    source: Value,
    source_start: usize,
}

impl ModeLineDisplaySourceSpan {
    fn new(output_start: usize, output_end: usize, source: Value, source_start: usize) -> Self {
        Self {
            output_start,
            output_end,
            source,
            source_start,
        }
    }

    pub const fn output_start(self) -> usize {
        self.output_start
    }

    pub const fn output_end(self) -> usize {
        self.output_end
    }

    pub const fn source(self) -> Value {
        self.source
    }

    pub const fn source_start(self) -> usize {
        self.source_start
    }

    pub const fn source_position(self, output_position: usize) -> Option<usize> {
        if output_position < self.output_start || output_position >= self.output_end {
            return None;
        }
        Some(
            self.source_start
                .saturating_add(output_position - self.output_start),
        )
    }

    fn shifted_output(self, offset: usize) -> Self {
        Self::new(
            self.output_start.saturating_add(offset),
            self.output_end.saturating_add(offset),
            self.source,
            self.source_start,
        )
    }
}

/// Fully formatted chrome text plus the original string identity of each
/// directly rendered segment.
#[derive(Clone, Debug)]
pub struct ModeLineDisplayOutput {
    value: Value,
    source_spans: Vec<ModeLineDisplaySourceSpan>,
}

impl ModeLineDisplayOutput {
    pub fn from_root_string(value: Value) -> Self {
        let source_spans = value
            .as_lisp_string()
            .map(|string| vec![ModeLineDisplaySourceSpan::new(0, string.schars(), value, 0)])
            .unwrap_or_default();
        Self {
            value,
            source_spans,
        }
    }

    pub const fn value(&self) -> Value {
        self.value
    }

    pub fn into_value(self) -> Value {
        self.value
    }

    pub fn source_spans(&self) -> &[ModeLineDisplaySourceSpan] {
        &self.source_spans
    }
}

#[derive(Clone, Default)]
struct ModeLineRendered {
    /// Accumulated Emacs character codes (one entry per character). Storing
    /// codes rather than a `String` lets the mode line carry raw eight-bit and
    /// non-Unicode characters byte-faithfully — a Rust `String` cannot hold
    /// them, and the legacy storage-String round-trip that used to bridge the
    /// gap has been retired (issue #131).
    text: Vec<u32>,
    text_props: TextPropertyTable,
    source_spans: Vec<ModeLineDisplaySourceSpan>,
    min_width_transitions: Vec<ModeLineMinWidthTransition>,
    /// The enclosing walk owns the scratch-root scope. Mutations publish new
    /// Lisp values there before any later element can evaluate Lisp.
    gc_roots: Option<mode_line_gc::AccumulatorRoots>,
}

/// One `display (min-width WIDTH-SPEC)` run in GNU's direct mode-line
/// display stream.  `WIDTH-SPEC` identity is deliberately retained: GNU
/// partitions adjacent runs with `EQ`, not structural equality.
#[derive(Clone, Debug)]
struct ModeLineMinWidthRun {
    width_spec: Value,
    columns: usize,
    padding_properties: std::collections::HashMap<Value, Value>,
}

#[derive(Clone, Debug)]
struct ModeLineMinWidthTransition {
    output_start: usize,
    run: ModeLineMinWidthRun,
}

/// Decode a `LispString` into its sequence of Emacs character codes. Multibyte
/// strings are scanned one Emacs character at a time (eight-bit characters
/// surface as `0x3FFF00+`); unibyte strings yield one code per raw byte.
fn mode_line_string_char_codes(string: &crate::heap_types::LispString) -> Vec<u32> {
    let bytes = string.as_bytes();
    if string.is_multibyte() {
        let mut codes = Vec::new();
        let mut pos = 0;
        while pos < bytes.len() {
            codes.push(crate::emacs_core::emacs_char::string_char_advance(
                bytes, &mut pos,
            ));
        }
        codes
    } else {
        bytes.iter().map(|&b| u32::from(b)).collect()
    }
}

/// Build the final mode-line `LispString` from accumulated character codes: a
/// multibyte result encodes each code via `char_string`, a unibyte result maps
/// each code straight to a byte.
fn mode_line_lisp_string_from_codes(
    codes: &[u32],
    multibyte: bool,
) -> crate::heap_types::LispString {
    if multibyte {
        let mut bytes = Vec::new();
        for &code in codes {
            let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
            bytes.extend_from_slice(&buf[..len]);
        }
        crate::heap_types::LispString::from_emacs_bytes(bytes)
    } else {
        crate::heap_types::LispString::from_unibyte(codes.iter().map(|&c| c as u8).collect())
    }
}

#[inline]
fn display_char_range(start: usize, end: usize) -> CharRange {
    CharRange::from_start_len(
        CharPos0::new(start),
        CharLen::new(end.saturating_sub(start)),
    )
}

#[derive(Clone, Copy)]
struct ModeLineFaceSpec {
    no_props: bool,
    face: Option<Value>,
}

impl ModeLineRendered {
    fn root_for_walk(&mut self) {
        if self.gc_roots.is_none() {
            self.gc_roots = Some(mode_line_gc::pin_rendered_scratch(self));
        }
    }

    fn pin_value(&mut self, value: Value) {
        if let Some(roots) = &mut self.gc_roots {
            mode_line_gc::pin_accumulator_value(roots, value);
        }
    }

    fn pin_properties(&mut self) {
        if let Some(roots) = &mut self.gc_roots {
            self.text_props.for_each_root(|value| {
                mode_line_gc::pin_accumulator_value(roots, value);
            });
        }
    }

    fn append_properties(&mut self, other: &TextPropertyTable, offset: usize) {
        if let Some(roots) = &mut self.gc_roots {
            self.text_props.append_shifted_at_char_offset_with_roots(
                other,
                CharLen::new(offset),
                |value| mode_line_gc::pin_accumulator_value(roots, value),
            );
        } else {
            self.text_props
                .append_shifted_at_char_offset(other, CharLen::new(offset));
        }
    }

    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into().chars().map(|c| c as u32).collect(),
            text_props: TextPropertyTable::new(),
            source_spans: Vec::new(),
            min_width_transitions: Vec::new(),
            gc_roots: None,
        }
    }

    fn append_rendered(&mut self, other: &Self) {
        let char_offset = self.char_len();
        self.text.extend_from_slice(&other.text);
        self.append_properties(&other.text_props, char_offset);
        for span in &other.source_spans {
            self.pin_value(span.source());
        }
        for transition in &other.min_width_transitions {
            self.pin_value(transition.run.width_spec);
            for (&name, &value) in &transition.run.padding_properties {
                self.pin_value(name);
                self.pin_value(value);
            }
        }
        self.source_spans.extend(
            other
                .source_spans
                .iter()
                .copied()
                .map(|span| span.shifted_output(char_offset)),
        );
        self.min_width_transitions
            .extend(
                other
                    .min_width_transitions
                    .iter()
                    .cloned()
                    .map(|transition| ModeLineMinWidthTransition {
                        output_start: transition.output_start.saturating_add(char_offset),
                        ..transition
                    }),
            );
    }

    fn record_source_span(
        &mut self,
        output_start: usize,
        output_end: usize,
        source: Value,
        source_start: usize,
    ) {
        if output_start >= output_end || !source.is_string() {
            return;
        }
        if let Some(previous) = self.source_spans.last_mut()
            && previous.source == source
            && previous.output_end == output_start
            && previous
                .source_start
                .saturating_add(previous.output_end - previous.output_start)
                == source_start
        {
            previous.output_end = output_end;
            return;
        }
        self.pin_value(source);
        self.source_spans.push(ModeLineDisplaySourceSpan::new(
            output_start,
            output_end,
            source,
            source_start,
        ));
    }

    fn append_string_value_preserving_props(&mut self, value: &Value) {
        match value.as_lisp_string() {
            Some(string) => {
                let char_offset = self.char_len();
                self.text.extend(mode_line_string_char_codes(string));
                self.record_source_span(char_offset, self.char_len(), *value, 0);
                if let Some(props) = get_string_text_properties_table_for_value(*value) {
                    self.append_properties(&props, char_offset);
                }
            }
            None => {
                let Some(text) = value.as_utf8_str() else {
                    return;
                };
                let char_offset = self.char_len();
                self.text.extend(text.chars().map(|c| c as u32));
                self.record_source_span(char_offset, self.char_len(), *value, 0);
            }
        }
    }

    fn append_string_or_char_value_preserving_props(&mut self, value: &Value) {
        if value.is_string() {
            self.append_string_value_preserving_props(value);
        } else if let Some(ch) = value.as_char() {
            self.text.push(ch as u32);
        }
    }

    fn append_string_char_slice_preserving_props(
        &mut self,
        value: &Value,
        start_char: usize,
        end_char: usize,
    ) {
        if start_char >= end_char {
            return;
        }
        match value.as_lisp_string() {
            Some(string) => {
                let char_offset = self.char_len();
                self.text.extend(
                    mode_line_string_char_codes(string)
                        .into_iter()
                        .skip(start_char)
                        .take(end_char - start_char),
                );
                self.record_source_span(char_offset, self.char_len(), *value, start_char);
                if let Some(props) = get_string_text_properties_table_for_value(*value) {
                    self.append_properties(
                        &props.slice_char_range(display_char_range(start_char, end_char)),
                        char_offset,
                    );
                }
            }
            None => {
                let Some(text) = value.as_utf8_str() else {
                    return;
                };
                let char_offset = self.char_len();
                self.text.extend(
                    text.chars()
                        .skip(start_char)
                        .take(end_char - start_char)
                        .map(|c| c as u32),
                );
                self.record_source_span(char_offset, self.char_len(), *value, start_char);
                if value.is_string()
                    && let Some(props) = get_string_text_properties_table_for_value(*value)
                {
                    self.append_properties(
                        &props.slice_char_range(display_char_range(start_char, end_char)),
                        char_offset,
                    );
                }
            }
        }
    }

    fn push_plain_char(&mut self, ch: char) {
        self.text.push(ch as u32);
    }

    fn char_len(&self) -> usize {
        self.text.len()
    }

    fn slice_chars(&self, precision: usize) -> Self {
        Self {
            gc_roots: None,
            text: self.text.iter().take(precision).copied().collect(),
            text_props: self
                .text_props
                .slice_char_range(display_char_range(0, precision)),
            source_spans: self
                .source_spans
                .iter()
                .copied()
                .filter(|span| span.output_start < precision)
                .map(|span| ModeLineDisplaySourceSpan {
                    output_end: span.output_end.min(precision),
                    ..span
                })
                .collect(),
            min_width_transitions: self
                .min_width_transitions
                .iter()
                .filter(|transition| transition.output_start < precision)
                .cloned()
                .collect(),
        }
    }

    fn pad_plain_spaces(&mut self, padding_chars: usize) {
        if padding_chars == 0 {
            return;
        }
        self.text
            .extend(std::iter::repeat_n(' ' as u32, padding_chars));
    }

    fn materialize_display_min_width(&mut self, props: Value) {
        let Some(min_width) = mode_line_display_min_width(props) else {
            return;
        };
        let current_width = self.char_len();
        if current_width < min_width.columns {
            self.pad_plain_spaces(min_width.columns - current_width);
        }
    }

    fn note_display_min_width_transition(&mut self, props: Value) {
        let Some(mut run) = mode_line_display_min_width(props) else {
            return;
        };
        if self.text.is_empty() {
            return;
        }

        // The outer :propertize owns the resulting display property over this
        // entire subtree, so any nested min-width markers it overwrote are no
        // longer observable by GNU's iterator.
        self.min_width_transitions.clear();
        run.padding_properties = self.text_props.get_properties_at_char_pos(CharPos0::ZERO);
        self.pin_value(run.width_spec);
        for (&name, &value) in &run.padding_properties {
            self.pin_value(name);
            self.pin_value(value);
        }
        self.min_width_transitions.push(ModeLineMinWidthTransition {
            output_start: 0,
            run,
        });
    }

    fn apply_propertize_properties(&mut self, props: Value, target: ModeLineTarget) {
        match target {
            ModeLineTarget::String => {
                self.materialize_display_min_width(props);
                self.overlay_properties(props);
            }
            ModeLineTarget::Display { .. } => {
                self.overlay_properties(props);
                self.note_display_min_width_transition(props);
            }
        }
    }

    /// Reproduce GNU `display_min_width`: an active run is closed only when a
    /// different min-width identity starts.  Ordinary following strings and
    /// the end of the flattened mode line do not themselves flush it.
    fn realize_display_min_width_transitions(&mut self) {
        let transitions = std::mem::take(&mut self.min_width_transitions);
        let Some(first) = transitions.first().cloned() else {
            return;
        };

        let mut active = first;
        let mut inserted = 0usize;
        for next in transitions.into_iter().skip(1) {
            if next.run.width_spec.bits() == active.run.width_spec.bits() {
                continue;
            }

            let mut next_start = next.output_start.saturating_add(inserted);
            let run_width = next_start.saturating_sub(active.output_start);
            if run_width < active.run.columns {
                let padding = active.run.columns - run_width;
                self.insert_min_width_padding(next_start, padding, &active.run.padding_properties);
                inserted = inserted.saturating_add(padding);
                next_start = next_start.saturating_add(padding);
            }
            active = ModeLineMinWidthTransition {
                output_start: next_start,
                run: next.run,
            };
        }
    }

    fn insert_min_width_padding(
        &mut self,
        at: usize,
        columns: usize,
        properties: &std::collections::HashMap<Value, Value>,
    ) {
        if columns == 0 {
            return;
        }
        let at = at.min(self.text.len());
        self.text
            .splice(at..at, std::iter::repeat_n(' ' as u32, columns));
        self.text_props
            .adjust_for_insert_at_char_pos(CharPos0::new(at), CharLen::new(columns));
        for (name, value) in properties {
            // The synthetic stretch inherits appearance, but it is not itself
            // another min-width source run.
            if !name.is_symbol_named("display") {
                self.text_props.put_property_in_char_range(
                    display_char_range(at, at.saturating_add(columns)),
                    *name,
                    *value,
                );
            }
        }

        let mut shifted = Vec::with_capacity(self.source_spans.len().saturating_add(1));
        for span in std::mem::take(&mut self.source_spans) {
            if span.output_end <= at {
                shifted.push(span);
            } else if span.output_start >= at {
                shifted.push(span.shifted_output(columns));
            } else {
                // Keep the inserted padding synthetic even in the unlikely
                // event that a source span crosses a transition boundary.
                shifted.push(ModeLineDisplaySourceSpan {
                    output_end: at,
                    ..span
                });
                shifted.push(ModeLineDisplaySourceSpan {
                    output_start: at.saturating_add(columns),
                    output_end: span.output_end.saturating_add(columns),
                    source_start: span
                        .source_start
                        .saturating_add(at.saturating_sub(span.output_start)),
                    ..span
                });
            }
        }
        self.source_spans = shifted;
    }

    fn overlay_properties(&mut self, props: Value) {
        if self.text.is_empty() {
            return;
        }
        let Some(items) = list_to_vec(&props) else {
            return;
        };
        for chunk in items.chunks(2) {
            if chunk.len() != 2 {
                continue;
            }
            self.text_props.put_property_in_char_range(
                display_char_range(0, self.char_len()),
                chunk[0],
                chunk[1],
            );
        }
        self.pin_properties();
    }

    fn overlay_property_map(&mut self, props: std::collections::HashMap<Value, Value>) {
        if self.text.is_empty() || props.is_empty() {
            return;
        }
        for (name, value) in props {
            self.text_props.put_property_in_char_range(
                display_char_range(0, self.char_len()),
                name,
                value,
            );
        }
        self.pin_properties();
    }

    fn apply_default_face(&mut self, face: Value) {
        if self.text.is_empty() {
            return;
        }

        let end = self.char_len();
        let intervals = self.text_props.intervals_snapshot();
        let mut cursor = 0;

        for interval in intervals {
            let start = interval.start.min(end);
            let interval_end = interval.end.min(end);

            if cursor < start {
                self.text_props.put_property_in_char_range(
                    display_char_range(cursor, start),
                    Value::symbol("face"),
                    face,
                );
            }

            if start < interval_end {
                let merged_face = interval
                    .properties
                    .get(&Value::symbol("face"))
                    .copied()
                    .map(|existing| Value::list(vec![existing, face]))
                    .unwrap_or(face);
                self.text_props.put_property_in_char_range(
                    display_char_range(start, interval_end),
                    Value::symbol("face"),
                    merged_face,
                );
                cursor = interval_end;
            }

            if cursor >= end {
                break;
            }
        }

        if cursor < end {
            self.text_props.put_property_in_char_range(
                display_char_range(cursor, end),
                Value::symbol("face"),
                face,
            );
        }
    }

    fn into_display_output(mut self, face_spec: ModeLineFaceSpec) -> ModeLineDisplayOutput {
        self.realize_display_min_width_transitions();
        // A multibyte result iff any accumulated character exceeds a single
        // byte; otherwise every code fits in a unibyte byte. Mirrors the old
        // storage path's `decode_storage_char_codes_auto(..).any(> 0xFF)`.
        let multibyte = self.text.iter().any(|&code| code > 0xFF);
        if face_spec.no_props {
            return ModeLineDisplayOutput {
                value: Value::heap_string(mode_line_lisp_string_from_codes(&self.text, multibyte)),
                source_spans: self.source_spans,
            };
        }
        if let Some(face) = face_spec.face {
            self.apply_default_face(face);
        }
        let value = Value::heap_string(mode_line_lisp_string_from_codes(&self.text, multibyte));
        if value.is_string() {
            set_string_text_properties_table_for_value(value, self.text_props);
        }
        ModeLineDisplayOutput {
            value,
            source_spans: self.source_spans,
        }
    }

    fn into_value(self, face_spec: ModeLineFaceSpec) -> Value {
        self.into_display_output(face_spec).into_value()
    }
}

fn mode_line_display_min_width(props: Value) -> Option<ModeLineMinWidthRun> {
    let items = list_to_vec(&props)?;
    for chunk in items.chunks(2) {
        if chunk.len() != 2 || !chunk[0].is_symbol_named("display") {
            continue;
        }
        if let Some(run) = mode_line_display_spec_min_width(chunk[1]) {
            return Some(run);
        }
    }
    None
}

fn mode_line_display_spec_min_width(value: Value) -> Option<ModeLineMinWidthRun> {
    if !value.is_cons() || !value.cons_car().is_symbol_named("min-width") {
        return None;
    }
    let width_spec = value.cons_cdr().cons_car();
    Some(ModeLineMinWidthRun {
        width_spec,
        columns: mode_line_display_width_chars(width_spec)?,
        padding_properties: std::collections::HashMap::new(),
    })
}

fn mode_line_display_width_chars(value: Value) -> Option<usize> {
    if let Some(width) = value.as_fixnum().filter(|width| *width > 0) {
        return Some(width as usize);
    }
    if let Some(width) = value.as_float().filter(|width| *width > 0.0) {
        return Some(width.ceil() as usize);
    }
    if value.is_cons() {
        return mode_line_display_width_chars(value.cons_car());
    }
    None
}

fn resolve_mode_line_face_spec(args: &[Value]) -> ModeLineFaceSpec {
    let face = args.get(1).copied().unwrap_or(Value::NIL);
    let no_props = face.is_fixnum();
    let face = if no_props || face.is_nil() || face.is_symbol_named("default") {
        None
    } else {
        Some(face)
    };
    ModeLineFaceSpec { no_props, face }
}

fn append_mode_line_rendered_segment(
    result: &mut ModeLineRendered,
    rendered: &ModeLineRendered,
    field_width: i64,
    precision: i64,
) {
    let mut segment = if precision > 0 {
        rendered.slice_chars(precision as usize)
    } else {
        rendered.clone()
    };
    let rendered_len = segment.char_len() as i64;
    if field_width > 0 && rendered_len < field_width {
        segment.pad_plain_spaces((field_width - rendered_len) as usize);
    }
    result.append_rendered(&segment);
}

fn append_mode_line_percent_string_spec(
    result: &mut ModeLineRendered,
    spec: &str,
    props_at_percent: &std::collections::HashMap<Value, Value>,
    field_width: i64,
) {
    append_mode_line_percent_segment(
        result,
        ModeLineRendered::plain(spec),
        props_at_percent,
        field_width,
    );
}

fn append_mode_line_percent_segment(
    result: &mut ModeLineRendered,
    mut segment: ModeLineRendered,
    props_at_percent: &std::collections::HashMap<Value, Value>,
    field_width: i64,
) {
    let rendered_len = segment.char_len() as i64;
    if field_width > 0 && rendered_len < field_width {
        segment.pad_plain_spaces((field_width - rendered_len) as usize);
    }
    // GNU applies the source format string's properties to the entire
    // expanded field, including spaces introduced by `%12b`-style padding.
    // Inner rendered values still do not donate their properties to padding.
    segment.overlay_property_map(props_at_percent.clone());
    result.append_rendered(&segment);
}

fn append_mode_line_percent_lisp_text_spec(
    result: &mut ModeLineRendered,
    value: &Value,
    props_at_percent: &std::collections::HashMap<Value, Value>,
    field_width: i64,
) {
    let mut segment = ModeLineRendered::default();
    segment.append_string_or_char_value_preserving_props(value);
    append_mode_line_percent_segment(result, segment, props_at_percent, field_width);
}

#[allow(clippy::too_many_arguments)] // split evaluator state avoids aliasing the full Context
fn append_mode_line_string_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
    command_loop_depth: usize,
    pctx: &ModeLinePercentContext,
    result: &mut ModeLineRendered,
    value: &Value,
    literal: bool,
) {
    // `%` is ASCII (0x25), which can never appear as a UTF-8 continuation or
    // lead byte, so scanning the raw bytes detects format specs without a lossy
    // decode and stays byte-faithful for raw-unibyte literal segments.
    let has_percent = if let Some(string) = value.as_lisp_string() {
        string.as_bytes().contains(&b'%')
    } else if let Some(text) = value.as_utf8_str() {
        text.contains('%')
    } else {
        return;
    };
    if literal || !has_percent {
        result.append_string_value_preserving_props(value);
    } else {
        expand_mode_line_percent_in_state(
            obarray,
            dynamic,
            buffers,
            processes,
            command_loop_depth,
            pctx,
            value,
            result,
        );
    }
}

/// GNU FOR_EACH_TAIL_SAFE's Brent checkpoints, without signaling or quitting.
/// Evaluator-backed walkers root the checkpoint separately from the current
/// tail so detaching it cannot let collection reuse its address.
struct ModeLineTailCycle {
    checkpoint: usize,
    remaining: usize,
    period: usize,
}

impl ModeLineTailCycle {
    fn new(tail: Value) -> Self {
        Self {
            checkpoint: tail.bits(),
            remaining: 2,
            period: 2,
        }
    }

    fn step(&mut self, tail: Value, mut update_checkpoint: impl FnMut(Value)) -> bool {
        self.remaining -= 1;
        if self.remaining == 0 {
            self.period = self.period.saturating_mul(2);
            self.remaining = self.period;
            self.checkpoint = tail.bits();
            update_checkpoint(tail);
            false
        } else {
            tail.bits() == self.checkpoint
        }
    }
}

/// Recursively process a mode-line format spec, appending output to `result`.
///
/// FORMAT can be:
/// - A string: expand %-constructs (%b, %f, %*, %l, %c, %p, etc.)
/// - A symbol: look up its value, recursively format
/// - A list: process each element in sequence
/// - `(:eval FORM)`: evaluate FORM, use result as format
/// - `(:propertize ELT PROPS...)`: process ELT and apply text properties
/// - A cons `(SYMBOL . REST)`: if SYMBOL's value is non-nil, process REST
fn format_mode_line_recursive(
    eval: &mut super::eval::Context,
    pctx: &ModeLinePercentContext,
    format: &Value,
    result: &mut ModeLineRendered,
    depth: usize,
    risky: bool,
    selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<(), Flow> {
    if depth > 20 {
        return Ok(()); // Guard against infinite recursion
    }

    // Formats have a per-element scope; accumulator mutations publish roots
    // into the enclosing walk's scope, which also retains parent accumulators.
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(*format);
    result.root_for_walk();
    let output =
        format_mode_line_recursive_rooted(eval, pctx, format, result, depth, risky, selection);
    eval.restore_specpdl_roots(root_scope);
    output
}

fn format_mode_line_recursive_rooted(
    eval: &mut super::eval::Context,
    pctx: &ModeLinePercentContext,
    format: &Value,
    result: &mut ModeLineRendered,
    depth: usize,
    risky: bool,
    mut selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<(), Flow> {
    match format.kind() {
        ValueKind::Nil => {}

        ValueKind::String => append_mode_line_string_in_state(
            &eval.obarray,
            &[],
            &eval.buffers,
            &eval.processes,
            eval.recursive_command_loop_depth(),
            pctx,
            result,
            format,
            false,
        ),

        ValueKind::Fixnum(n) => {
            let _ = n;
        }

        _ if format.is_symbol() => {
            // GNU xdisp.c:28438-28468 (display_mode_element, Lisp_Symbol
            // branch): resolve the symbol's value and recurse. There is
            // no special case for mode-line-front-space or
            // mode-line-end-spaces — GNU treats every mode-line symbol
            // the same way. Previously this branch short-circuited those
            // two names to a single hardcoded space, which silently
            // discarded the `(:eval (unless (display-graphic-p) "-%-"))`
            // dash-fill construct that bindings.el installs on TTY.
            if let Some(name) = format.as_symbol_name()
                && let Some(val) =
                    mode_line_symbol_value_in_state(&eval.obarray, &[], &eval.buffers, name)
                && !val.is_nil()
            {
                if val.is_string() {
                    append_mode_line_string_in_state(
                        &eval.obarray,
                        &[],
                        &eval.buffers,
                        &eval.processes,
                        eval.recursive_command_loop_depth(),
                        pctx,
                        result,
                        &val,
                        true,
                    );
                } else {
                    format_mode_line_recursive(
                        eval,
                        pctx,
                        &val,
                        result,
                        depth + 1,
                        risky || !mode_line_symbol_is_risky(&eval.obarray, name),
                        selection.as_deref_mut(),
                    )?;
                }
            }
        }

        _ if format.is_cons() => {
            let car = format.cons_car();
            let cdr = format.cons_cdr();
            // A nested :eval may detach these from FORMAT before collecting.
            eval.push_specpdl_root(car);
            eval.push_specpdl_root(cdr);

            if car.is_symbol_named(":eval") {
                if risky {
                    return Ok(());
                }
                if cdr.is_cons() {
                    let form_val = cdr.cons_car();
                    eval.push_specpdl_root(form_val);
                    if let Some(selection) = selection.as_deref_mut() {
                        selection.before_eval(eval);
                    }
                    let val = mode_line_flow_policy::eval_form(eval, &form_val)?;
                    // The recursive entry roots the fresh return value before
                    // a nested element can run Lisp again.
                    format_mode_line_recursive(
                        eval,
                        pctx,
                        &val,
                        result,
                        depth + 1,
                        risky,
                        selection.as_deref_mut(),
                    )?;
                }
                return Ok(());
            }

            if car.is_symbol_named(":propertize") {
                if risky {
                    return Ok(());
                }
                if cdr.is_cons() {
                    let elt = cdr.cons_car();
                    let mut nested = ModeLineRendered::default();
                    format_mode_line_recursive(
                        eval,
                        pctx,
                        &elt,
                        &mut nested,
                        depth + 1,
                        risky,
                        selection.as_deref_mut(),
                    )?;
                    nested.apply_propertize_properties(cdr.cons_cdr(), pctx.target);
                    result.append_rendered(&nested);
                }
                return Ok(());
            }

            if let Some(lim) = car.as_fixnum() {
                let mut nested = ModeLineRendered::default();
                format_mode_line_recursive(
                    eval,
                    pctx,
                    &cdr,
                    &mut nested,
                    depth + 1,
                    risky,
                    selection.as_deref_mut(),
                )?;
                append_mode_line_rendered_segment(
                    result,
                    &nested,
                    if lim > 0 { lim } else { 0 },
                    if lim < 0 { -lim } else { 0 },
                );
                return Ok(());
            }

            if car.is_symbol() && !car.is_symbol_named("t") {
                if let Some(sym_name) = car.as_symbol_name()
                    && mode_line_symbol_value_in_state(&eval.obarray, &[], &eval.buffers, sym_name)
                        .is_some_and(|value| value.is_truthy())
                    && let Some(branch) = mode_line_conditional_branch(cdr, true)
                {
                    format_mode_line_recursive(
                        eval,
                        pctx,
                        &branch,
                        result,
                        depth + 1,
                        risky,
                        selection.as_deref_mut(),
                    )?;
                } else if let Some(branch) = mode_line_conditional_branch(cdr, false) {
                    format_mode_line_recursive(
                        eval,
                        pctx,
                        &branch,
                        result,
                        depth + 1,
                        risky,
                        selection.as_deref_mut(),
                    )?;
                }
                return Ok(());
            }

            // GNU FOR_EACH_TAIL_SAFE reads XCDR after rendering each element:
            // :eval can detach or replace the remaining live list spine.
            let mut tail = *format;
            let tail_root = eval.push_specpdl_root_slot(tail);
            let checkpoint_root = eval.push_specpdl_root_slot(tail);
            let mut cycle = ModeLineTailCycle::new(tail);
            while tail.is_cons() {
                let element = tail.cons_car();
                format_mode_line_recursive(
                    eval,
                    pctx,
                    &element,
                    result,
                    depth + 1,
                    risky,
                    selection.as_deref_mut(),
                )?;
                tail = tail.cons_cdr();
                eval.set_specpdl_root_slot(&tail_root, tail);
                if cycle.step(tail, |checkpoint| {
                    eval.set_specpdl_root_slot(&checkpoint_root, checkpoint);
                }) {
                    break;
                }
            }
        }

        _ => {
            result.append_string_value_preserving_props(format);
        }
    }
    Ok(())
}

#[allow(dead_code, clippy::too_many_arguments)] // split-state mode-line compatibility seam
fn format_mode_line_recursive_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
    pctx: &ModeLinePercentContext,
    format: &Value,
    result: &mut ModeLineRendered,
    depth: usize,
    risky: bool,
) -> bool {
    if depth > 20 {
        return false;
    }

    match format.kind() {
        ValueKind::Nil => {}

        ValueKind::String => append_mode_line_string_in_state(
            obarray, dynamic, buffers, processes, 0, pctx, result, format, false,
        ),

        ValueKind::Fixnum(_) => {}

        _ if format.is_symbol() => {
            // GNU xdisp.c:28438-28468 — symbol branch of display_mode_element.
            // No special case for mode-line-front-space or
            // mode-line-end-spaces; see the note on the equivalent
            // branch in `format_mode_line_recursive`.
            if let Some(name) = format.as_symbol_name()
                && let Some(val) = mode_line_symbol_value_in_state(obarray, dynamic, buffers, name)
                && !val.is_nil()
            {
                if val.is_string() {
                    append_mode_line_string_in_state(
                        obarray, dynamic, buffers, processes, 0, pctx, result, &val, true,
                    );
                } else if format_mode_line_recursive_in_state(
                    obarray,
                    dynamic,
                    buffers,
                    processes,
                    pctx,
                    &val,
                    result,
                    depth + 1,
                    risky || !mode_line_symbol_is_risky(obarray, name),
                ) {
                    return true;
                }
            }
        }

        _ if format.is_cons() => {
            let car = format.cons_car();
            let cdr = format.cons_cdr();

            if car.is_symbol_named(":eval") {
                if risky {
                    return false;
                }
                return true;
            }

            if car.is_symbol_named(":propertize") {
                if risky {
                    return false;
                }
                if cdr.is_cons() {
                    let elt = cdr.cons_car();
                    let mut nested = ModeLineRendered::default();
                    let needs_eval = format_mode_line_recursive_in_state(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        &elt,
                        &mut nested,
                        depth + 1,
                        risky,
                    );
                    nested.apply_propertize_properties(cdr.cons_cdr(), pctx.target);
                    result.append_rendered(&nested);
                    return needs_eval;
                }
                return false;
            }

            if let Some(lim) = car.as_fixnum() {
                let mut nested = ModeLineRendered::default();
                let needs_eval = format_mode_line_recursive_in_state(
                    obarray,
                    dynamic,
                    buffers,
                    processes,
                    pctx,
                    &cdr,
                    &mut nested,
                    depth + 1,
                    risky,
                );
                append_mode_line_rendered_segment(
                    result,
                    &nested,
                    if lim > 0 { lim } else { 0 },
                    if lim < 0 { -lim } else { 0 },
                );
                return needs_eval;
            }

            if car.is_symbol() && !car.is_symbol_named("t") {
                let branch = if let Some(sym_name) = car.as_symbol_name()
                    && mode_line_symbol_value_in_state(obarray, dynamic, buffers, sym_name)
                        .is_some_and(|value| value.is_truthy())
                {
                    mode_line_conditional_branch(cdr, true)
                } else {
                    mode_line_conditional_branch(cdr, false)
                };
                if let Some(branch) = branch {
                    return format_mode_line_recursive_in_state(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        &branch,
                        result,
                        depth + 1,
                        risky,
                    );
                }
                return false;
            }

            let mut tail = *format;
            let mut cycle = ModeLineTailCycle::new(tail);
            while tail.is_cons() {
                let element = tail.cons_car();
                if format_mode_line_recursive_in_state(
                    obarray,
                    dynamic,
                    buffers,
                    processes,
                    pctx,
                    &element,
                    result,
                    depth + 1,
                    risky,
                ) {
                    return true;
                }
                tail = tail.cons_cdr();
                if cycle.step(tail, |_| {}) {
                    break;
                }
            }
        }

        _ => {
            result.append_string_value_preserving_props(format);
        }
    }

    false
}

#[allow(clippy::too_many_arguments)] // split-state mode-line compatibility seam
fn format_mode_line_recursive_in_state_with_eval_rooted(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
    pctx: &ModeLinePercentContext,
    format: &Value,
    result: &mut ModeLineRendered,
    depth: usize,
    risky: bool,
    eval_form: &mut impl FnMut(&Value, &crate::buffer::BufferManager) -> Result<Value, Flow>,
    roots: &mode_line_gc::ScratchRoots,
) -> Result<(), Flow> {
    if depth > 20 {
        return Ok(());
    }
    roots.pin(*format);
    result.root_for_walk();

    match format.kind() {
        ValueKind::Nil => {}

        ValueKind::String => append_mode_line_string_in_state(
            obarray, dynamic, buffers, processes, 0, pctx, result, format, false,
        ),

        ValueKind::Fixnum(_) => {}

        _ if format.is_symbol() => {
            // GNU xdisp.c:28438-28468 — symbol branch of display_mode_element.
            // No special case for mode-line-front-space or
            // mode-line-end-spaces; they are ordinary symbols whose
            // value must be resolved and recursed on.
            if let Some(name) = format.as_symbol_name()
                && let Some(val) = mode_line_symbol_value_in_state(obarray, dynamic, buffers, name)
                && !val.is_nil()
            {
                if val.is_string() {
                    append_mode_line_string_in_state(
                        obarray, dynamic, buffers, processes, 0, pctx, result, &val, true,
                    );
                } else {
                    format_mode_line_recursive_in_state_with_eval_rooted(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        &val,
                        result,
                        depth + 1,
                        risky || !mode_line_symbol_is_risky(obarray, name),
                        eval_form,
                        roots,
                    )?;
                }
            }
        }

        _ if format.is_cons() => {
            let car = format.cons_car();
            let cdr = format.cons_cdr();
            roots.pin(car);
            roots.pin(cdr);

            if car.is_symbol_named(":eval") {
                if risky {
                    return Ok(());
                }
                if cdr.is_cons() {
                    let form_val = cdr.cons_car();
                    roots.pin(form_val);
                    let val = mode_line_flow_policy::split_eval_result(
                        &form_val,
                        eval_form(&form_val, buffers),
                    )?;
                    format_mode_line_recursive_in_state_with_eval_rooted(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        &val,
                        result,
                        depth + 1,
                        risky,
                        eval_form,
                        roots,
                    )?;
                }
                return Ok(());
            }

            if car.is_symbol_named(":propertize") {
                if risky {
                    return Ok(());
                }
                if cdr.is_cons() {
                    let elt = cdr.cons_car();
                    let mut nested = ModeLineRendered::default();
                    format_mode_line_recursive_in_state_with_eval_rooted(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        &elt,
                        &mut nested,
                        depth + 1,
                        risky,
                        eval_form,
                        roots,
                    )?;
                    nested.apply_propertize_properties(cdr.cons_cdr(), pctx.target);
                    result.append_rendered(&nested);
                }
                return Ok(());
            }

            if let Some(lim) = car.as_fixnum() {
                let mut nested = ModeLineRendered::default();
                format_mode_line_recursive_in_state_with_eval_rooted(
                    obarray,
                    dynamic,
                    buffers,
                    processes,
                    pctx,
                    &cdr,
                    &mut nested,
                    depth + 1,
                    risky,
                    eval_form,
                    roots,
                )?;
                append_mode_line_rendered_segment(
                    result,
                    &nested,
                    if lim > 0 { lim } else { 0 },
                    if lim < 0 { -lim } else { 0 },
                );
                return Ok(());
            }

            if car.is_symbol() && !car.is_symbol_named("t") {
                let branch = if let Some(sym_name) = car.as_symbol_name()
                    && mode_line_symbol_value_in_state(obarray, dynamic, buffers, sym_name)
                        .is_some_and(|value| value.is_truthy())
                {
                    mode_line_conditional_branch(cdr, true)
                } else {
                    mode_line_conditional_branch(cdr, false)
                };
                if let Some(branch) = branch {
                    format_mode_line_recursive_in_state_with_eval_rooted(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        &branch,
                        result,
                        depth + 1,
                        risky,
                        eval_form,
                        roots,
                    )?;
                }
                return Ok(());
            }

            let mut tail = *format;
            let tail_root = roots.slot(tail);
            let checkpoint_root = roots.slot(tail);
            let mut cycle = ModeLineTailCycle::new(tail);
            while tail.is_cons() {
                let element = tail.cons_car();
                format_mode_line_recursive_in_state_with_eval_rooted(
                    obarray,
                    dynamic,
                    buffers,
                    processes,
                    pctx,
                    &element,
                    result,
                    depth + 1,
                    risky,
                    eval_form,
                    roots,
                )?;
                tail = tail.cons_cdr();
                roots.set(tail_root, tail);
                if cycle.step(tail, |checkpoint| roots.set(checkpoint_root, checkpoint)) {
                    break;
                }
            }
        }

        _ => {
            result.append_string_value_preserving_props(format);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)] // split evaluator state avoids aliasing the full Context
fn expand_mode_line_percent_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    processes: &crate::emacs_core::process::ProcessManager,
    command_loop_depth: usize,
    pctx: &ModeLinePercentContext,
    value: &Value,
    result: &mut ModeLineRendered,
) {
    let fmt_storage = if let Some(string) = value.as_lisp_string() {
        crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes())
    } else if let Some(text) = value.as_utf8_str() {
        text.to_owned()
    } else {
        return;
    };
    let fmt_str = fmt_storage.as_str();
    let buf = buffers.current_buffer();
    let buf_name = buf
        .map(|b| b.name_runtime_string_owned())
        .unwrap_or_else(|| "*scratch*".to_string());
    let file_name_storage = buf.and_then(|b| {
        b.file_name_value()
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    });
    let file_name = file_name_storage.as_deref().unwrap_or("");
    let modified = buf.map(|b| b.is_modified()).unwrap_or(false);
    let read_only = buf.is_some_and(|b| {
        crate::emacs_core::editfns::buffer_read_only_active_in_state(obarray, dynamic, b)
    });
    let narrowed = buf.is_some_and(|b| b.is_narrowed());

    // GNU computes %l/%c lazily inside decode_mode_spec's switch — the
    // O(point) line count and O(column) scan run only when the format
    // actually contains the spec. Memoize on first demand: one compute per
    // walk even when both %l and %c appear.
    let mut point_line_column_memo: Option<LineColumn> = None;
    let mut point_line_column = || -> LineColumn {
        *point_line_column_memo.get_or_insert_with(|| {
            if let Some(b) = buf {
                // Use THIS window's point (GNU sets point to `w->pointm` per
                // window), falling back to the live buffer point when no
                // window context.
                let point_byte = pctx
                    .window_point
                    .unwrap_or_else(|| b.point_emacs_byte_pos());
                prefix_line_and_column(b, b.accessible_emacs_byte_region(), point_byte)
            } else {
                LineColumn { line: 1, column: 0 }
            }
        })
    };

    let chars: Vec<char> = fmt_str.chars().collect();
    let mut index = 0;
    let mut literal_start = 0;

    while index < chars.len() {
        if chars[index] != '%' {
            index += 1;
            continue;
        }

        if literal_start < index {
            result.append_string_char_slice_preserving_props(value, literal_start, index);
        }

        let percent_char_pos = index;
        index += 1;

        let mut field_width = 0_i64;
        while index < chars.len() && chars[index].is_ascii_digit() {
            let digit = chars[index] as u8;
            field_width = field_width * 10 + i64::from(digit - b'0');
            index += 1;
        }

        let props_at_percent = if value.is_string() {
            get_string_text_properties_table_for_value(*value)
                .map(|table| table.get_properties_at_char_pos(CharPos0::new(percent_char_pos)))
                .unwrap_or_default()
        } else {
            Default::default()
        };

        match chars.get(index).copied() {
            Some('b') => {
                append_mode_line_percent_string_spec(
                    result,
                    &buf_name,
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('f') => {
                append_mode_line_percent_string_spec(
                    result,
                    file_name,
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('i') => {
                let size = buf
                    .map(|buffer| buffer.accessible_char_len().get())
                    .unwrap_or(0);
                append_mode_line_percent_string_spec(
                    result,
                    &size.to_string(),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('I') => {
                let size = buf
                    .map(|buffer| buffer.accessible_char_len().get())
                    .unwrap_or(0);
                append_mode_line_percent_string_spec(
                    result,
                    &mode_line_human_readable_size(size),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('F') => {
                // GNU xdisp.c:29208 — f->title, f->name, or "Emacs".
                if let Some(frame_name) = pctx.frame_name {
                    append_mode_line_percent_lisp_text_spec(
                        result,
                        &frame_name,
                        &props_at_percent,
                        field_width,
                    );
                } else {
                    append_mode_line_percent_string_spec(
                        result,
                        "Emacs",
                        &props_at_percent,
                        field_width,
                    );
                }
                index += 1;
            }
            Some('*') => {
                append_mode_line_percent_string_spec(
                    result,
                    if read_only {
                        "%"
                    } else if modified {
                        "*"
                    } else {
                        "-"
                    },
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('+') => {
                append_mode_line_percent_string_spec(
                    result,
                    if modified {
                        "*"
                    } else if read_only {
                        "%"
                    } else {
                        "-"
                    },
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('&') => {
                append_mode_line_percent_string_spec(
                    result,
                    if modified { "*" } else { "-" },
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('-') => {
                // GNU xdisp.c:29154-29172 — `%-` dispatch depends on
                // mode_line_target. MODE_LINE_STRING (the default,
                // used by `(format-mode-line FORMAT)`) returns the
                // literal two-dash string "--". MODE_LINE_DISPLAY
                // (used by the redisplay walker) returns
                // `lots_of_dashes` — enough dashes to fill the
                // remaining row width; GNU's caller trims at
                // `it->last_visible_x`.
                //
                // We model this with `pctx.target`: string mode emits "--";
                // display mode emits `columns - current` dashes. The entry
                // point that enables display mode is
                //              `format_mode_line_for_display` below,
                //              used by the layout engine for TTY and
                //              GUI mode-line rendering.
                // `%-` needs to read `result.char_len()` to compute
                // the dash-fill width (in MODE_LINE_DISPLAY mode),
                // but `append_spec` holds a captured mutable borrow
                // on `result`. Drop the closure here by calling
                // `append_mode_line_rendered_segment` directly with
                // the pre-computed dash string.
                let dash_string: String = match pctx.target {
                    ModeLineTarget::String => "--".to_string(),
                    ModeLineTarget::Display { columns } => {
                        let current = result.char_len();
                        if columns > current {
                            "-".repeat(columns - current)
                        } else {
                            "--".to_string()
                        }
                    }
                };
                let mut segment = ModeLineRendered::plain(&dash_string);
                segment.overlay_property_map(props_at_percent.clone());
                append_mode_line_rendered_segment(result, &segment, field_width, 0);
                index += 1;
            }
            Some('%') => {
                append_mode_line_percent_string_spec(result, "%", &props_at_percent, field_width);
                index += 1;
            }
            Some('n') => {
                append_mode_line_percent_string_spec(
                    result,
                    if narrowed { " Narrow" } else { "" },
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('s') => {
                append_mode_line_percent_string_spec(
                    result,
                    mode_line_process_status_in_state(buffers, processes),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('l') => {
                append_mode_line_percent_string_spec(
                    result,
                    &point_line_column().line.to_string(),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('c') => {
                note_column_spec_consumed();
                append_mode_line_percent_string_spec(
                    result,
                    &point_line_column().column.to_string(),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('C') => {
                // GNU: 1-indexed column number at point.
                note_column_spec_consumed();
                append_mode_line_percent_string_spec(
                    result,
                    &(point_line_column().column + 1).to_string(),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('m') => {
                // GNU: major mode name from buffer-local `mode-name`.
                if let Some(mode_name) =
                    mode_line_symbol_value_in_state(obarray, dynamic, buffers, "mode-name")
                        .filter(|value| value.is_string())
                {
                    append_mode_line_percent_lisp_text_spec(
                        result,
                        &mode_name,
                        &props_at_percent,
                        field_width,
                    );
                } else {
                    append_mode_line_percent_string_spec(
                        result,
                        "",
                        &props_at_percent,
                        field_width,
                    );
                }
                index += 1;
            }
            Some('p') => {
                // GNU xdisp.c:29406 — percentage of buffer above window top.
                // pos = marker_position(w->start), checks window_end_pos.
                let text = if let Some(b) = buf {
                    let pos = pctx.window_start;
                    let botpos = pctx.window_end;
                    let begv = b.point_min_char_pos().get();
                    let zv = b
                        .point_max_char_pos()
                        .get()
                        .max(b.point_min_char_pos().get());
                    if botpos >= zv {
                        if pos <= begv {
                            "All".to_owned()
                        } else {
                            "Bottom".to_owned()
                        }
                    } else if pos <= begv {
                        "Top".to_owned()
                    } else {
                        format!("{}%", percent99(pos - begv, zv - begv))
                    }
                } else {
                    String::new()
                };
                append_mode_line_percent_string_spec(result, &text, &props_at_percent, field_width);
                index += 1;
            }
            Some('P') => {
                // GNU xdisp.c:29425 — percentage of buffer above window bottom.
                let text = if let Some(b) = buf {
                    let toppos = pctx.window_start;
                    let botpos = pctx.window_end;
                    let begv = b.point_min_char_pos().get();
                    let zv = b
                        .point_max_char_pos()
                        .get()
                        .max(b.point_min_char_pos().get());
                    if botpos >= zv {
                        if toppos <= begv {
                            "All".to_owned()
                        } else {
                            "Bottom".to_owned()
                        }
                    } else {
                        let pct = percent99(botpos.saturating_sub(begv), zv.saturating_sub(begv));
                        if toppos <= begv {
                            format!("{}%", pct)
                        } else {
                            format!("Top{}%", pct)
                        }
                    }
                } else {
                    String::new()
                };
                append_mode_line_percent_string_spec(result, &text, &props_at_percent, field_width);
                index += 1;
            }
            Some('o') => {
                // GNU xdisp.c:29386 — degree of travel of window through buffer.
                let text = if let Some(b) = buf {
                    let toppos = pctx.window_start;
                    let botpos = pctx.window_end;
                    let begv = b.point_min_char_pos().get();
                    let zv = b
                        .point_max_char_pos()
                        .get()
                        .max(b.point_min_char_pos().get());
                    if botpos >= zv {
                        if toppos <= begv {
                            "All".to_owned()
                        } else {
                            "Bottom".to_owned()
                        }
                    } else if toppos <= begv {
                        "Top".to_owned()
                    } else {
                        let top_dist = toppos - begv;
                        let bot_dist = zv - botpos;
                        format!("{}%", percent99(top_dist, top_dist + bot_dist))
                    }
                } else {
                    String::new()
                };
                append_mode_line_percent_string_spec(result, &text, &props_at_percent, field_width);
                index += 1;
            }
            Some('q') => {
                // GNU xdisp.c:29445 — percentage offsets of top and bottom of window.
                let text = if let Some(b) = buf {
                    let toppos = pctx.window_start;
                    let botpos = pctx.window_end;
                    let begv = b.point_min_char_pos().get();
                    let zv = b
                        .point_max_char_pos()
                        .get()
                        .max(b.point_min_char_pos().get());
                    if toppos <= begv && botpos >= zv {
                        "All   ".to_owned()
                    } else {
                        let range = zv.saturating_sub(begv);
                        let top_pct = if toppos <= begv {
                            0
                        } else {
                            percent99(toppos - begv, range)
                        };
                        let bot_pct = if botpos >= zv {
                            100
                        } else {
                            percent99(botpos.saturating_sub(begv), range)
                        };
                        if top_pct == bot_pct {
                            format!("{}%", top_pct)
                        } else {
                            format!("{}-{}%", top_pct, bot_pct)
                        }
                    }
                } else {
                    String::new()
                };
                append_mode_line_percent_string_spec(result, &text, &props_at_percent, field_width);
                index += 1;
            }
            Some('z') => {
                // GNU xdisp.c:29494 — coding system mnemonic without EOL indicator.
                // On TTY frames GNU includes terminal + keyboard + buffer coding
                // mnemonics, regardless of MODE_LINE_STRING vs MODE_LINE_DISPLAY.
                append_mode_line_percent_string_spec(
                    result,
                    &pctx.buffer_coding.with_frame(pctx.frame_coding_mnemonics),
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('@') => {
                // GNU xdisp.c:29477 — "@" if default-directory is remote, "-" otherwise.
                let remote =
                    mode_line_symbol_value_in_state(obarray, dynamic, buffers, "default-directory")
                        .and_then(|v| mode_line_runtime_string(&v))
                        .map(|dir| is_remote_directory(&dir))
                        .unwrap_or(false);
                append_mode_line_percent_string_spec(
                    result,
                    if remote { "@" } else { "-" },
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('Z') => {
                // GNU xdisp.c:29496 — coding system mnemonic WITH EOL indicator.
                let mut segment = ModeLineRendered::plain(
                    pctx.buffer_coding.with_frame(pctx.frame_coding_mnemonics),
                );
                if let Some(eol_indicator) = pctx.eol_indicator {
                    segment.append_string_or_char_value_preserving_props(&eol_indicator);
                } else {
                    segment.push_plain_char(':');
                }
                segment.overlay_property_map(props_at_percent.clone());
                append_mode_line_rendered_segment(result, &segment, field_width, 0);
                index += 1;
            }
            Some(c @ ('[' | ']')) => {
                let repeated = match (c, command_loop_depth) {
                    ('[', depth) if depth > 5 => "[[[... ".to_string(),
                    (']', depth) if depth > 5 => " ...]]]".to_string(),
                    (bracket, depth) => std::iter::repeat_n(bracket, depth).collect(),
                };
                append_mode_line_percent_string_spec(
                    result,
                    &repeated,
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            Some('e') => {
                append_mode_line_percent_string_spec(result, "", &props_at_percent, field_width);
                index += 1;
            }
            Some(' ') => {
                append_mode_line_percent_string_spec(result, " ", &props_at_percent, field_width);
                index += 1;
            }
            Some(c) => {
                let mut unknown = String::from("%");
                unknown.push(c);
                append_mode_line_percent_string_spec(
                    result,
                    &unknown,
                    &props_at_percent,
                    field_width,
                );
                index += 1;
            }
            None => {
                append_mode_line_percent_string_spec(result, "%", &props_at_percent, field_width)
            }
        }

        literal_start = index;
    }

    if literal_start < chars.len() {
        result.append_string_char_slice_preserving_props(value, literal_start, chars.len());
    }
}

impl Invisibility {
    const fn elides_source_for(self, context: InvisibleRunContext) -> bool {
        match (context, self) {
            (_, Self::Visible) => false,
            (_, Self::Hidden) | (InvisibleRunContext::DisplayMotion, Self::HiddenWithEllipsis) => {
                true
            }
            (InvisibleRunContext::ColumnScan, Self::HiddenWithEllipsis) => false,
        }
    }
}

/// GNU's `skip_invisible` gives ellipsis-bearing invisibility different
/// semantics depending on its WINDOW argument: display motion elides the
/// source and realizes an ellipsis, while `current-column`/`move-to-column`
/// pass nil and count the underlying source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InvisibleRunContext {
    DisplayMotion,
    ColumnScan,
}

/// Port of GNU `xdisp.c:display_prop_intangible_p`: does this `display` property
/// value *replace* the underlying text (a string, image, space, fringe, …),
/// making the covered buffer positions intangible? Mirrors `handle_display_spec`'s
/// dispatch over a single spec, a list of specs, or a vector of specs.
/// `frame_window_p` is GNU's `FRAME_WINDOW_P`: image- and xwidget-class specs
/// replace text only on a window (GUI) frame, never on a tty.
///
/// Pure over the `Value` (no eval, no host state) so the command loop's
/// `adjust_point_for_property` and the layout engine's classifier can share one
/// decision. `when` forms are resolved structurally rather than evaluated,
/// matching GNU's own `single_display_spec_string_p` shortcut (the text was
/// already displayed, so the condition was non-nil).
pub(crate) fn display_prop_replacing_p(spec: Value, frame_window_p: bool) -> bool {
    let mut replacing = false;
    // The shape decode (single spec / list of specs / vector of specs, minus a
    // `(disable-eval …)` wrapper) lives in ONE place -- see `display_spec`.
    display_spec::DisplayPropertySpecs::of(spec).for_each(|single| {
        if display_single_spec_replacing_p(single, frame_window_p) {
            replacing = true;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    });
    replacing
}

/// Replacing-determination for a *single* `display` spec, mirroring the
/// `it == NULL` paths of GNU `xdisp.c:handle_single_display_spec`. The head
/// taxonomy comes from [`display_spec::display_spec_kind`], so this reads as the
/// decision alone.
fn display_single_spec_replacing_p(spec: Value, frame_window_p: bool) -> bool {
    match display_spec::display_spec_kind(spec).replaces_text(frame_window_p) {
        Some(replacing) => replacing,
        // `(when FORM . VALUE)`: a nil FORM disables the spec; otherwise the
        // decision is VALUE's. Resolved structurally rather than evaluated -- see
        // the note on `display_prop_replacing_p`.
        None => match display_spec::display_spec_when_parts(spec) {
            // GNU continues with its SINGLE-spec arms on VALUE -- it does not
            // re-enter `handle_display_spec` -- so `(when t . SPEC)` never treats
            // SPEC as a list of specs.
            Some((form, value)) => {
                !form.is_nil() && display_single_spec_replacing_p(value, frame_window_p)
            }
            // `((margin AREA) VALUE)`: replacing iff VALUE is itself replacing
            // (typically a string). An AREA GNU rejects displays nothing.
            None => display_spec::display_spec_margin_value(spec)
                .is_some_and(|value| display_single_spec_replacing_p(value, frame_window_p)),
        },
    }
}

pub(crate) fn invisible_status_for_value(
    eval: &mut super::eval::Context,
    pos_or_prop: Value,
) -> Result<Invisibility, Flow> {
    let prop = match pos_or_prop.kind() {
        ValueKind::Fixnum(v) if v >= 0 => super::textprop::builtin_get_char_property(
            eval,
            vec![pos_or_prop, Value::symbol("invisible"), Value::NIL],
        )?,
        _ if super::marker::is_marker(&pos_or_prop) => super::textprop::builtin_get_char_property(
            eval,
            vec![pos_or_prop, Value::symbol("invisible"), Value::NIL],
        )?,
        _ => pos_or_prop,
    };
    let invisibility_spec = eval.eval_symbol_by_id(intern("buffer-invisibility-spec"))?;
    Ok(text_prop_means_invisible(prop, invisibility_spec))
}

pub(crate) fn invisible_source_run_end_byte(
    eval: &mut super::eval::Context,
    buffer_id: BufferId,
    byte_pos: usize,
    context: InvisibleRunContext,
) -> Result<Option<usize>, Flow> {
    let byte_pos = EmacsBytePos::new(byte_pos);
    let lisp_pos = {
        let Some(buf) = eval.buffers.get(buffer_id) else {
            return Ok(None);
        };
        if byte_pos >= buf.accessible_emacs_byte_region().end() {
            return Ok(None);
        }
        super::textprop::byte_to_elisp_pos(buf, byte_pos)
    };

    // This helper answers only the source boundary, never an estimated
    // ellipsis width. The typed context preserves GNU's deliberate distinction
    // between display motion and column scans.
    if !invisible_status_for_value(eval, Value::fixnum(lisp_pos))?.elides_source_for(context) {
        return Ok(None);
    }

    let next =
        super::builtins::builtin_next_char_property_change(eval, vec![Value::fixnum(lisp_pos)])?;
    let next_byte = {
        let Some(buf) = eval.buffers.get(buffer_id) else {
            return Ok(None);
        };
        match next.as_fixnum() {
            Some(pos) => EmacsBytePos::new(super::textprop::validate_buffer_point(buf, pos)?),
            None => buf.accessible_emacs_byte_region().end(),
        }
    };

    Ok(Some(next_byte.get()))
}

/// (invisible-p POS-OR-PROP) -> boolean
pub(crate) fn builtin_invisible_p(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("invisible-p", &args, 1)?;
    Ok(invisible_status_for_value(eval, args[0])?.into_lisp())
}

/// (line-pixel-height) -> integer
///
/// Batch-compatible behavior returns 1.
pub(crate) fn builtin_line_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_args("line-pixel-height", &args, 0)?;
    Ok(Value::fixnum(1))
}

/// (window-text-pixel-size &optional WINDOW FROM TO X-LIMIT Y-LIMIT MODE) -> (WIDTH . HEIGHT)
///
/// Batch-compatible behavior returns `(0 . 0)` and enforces argument
/// validation for WINDOW / FROM / TO.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_window_text_pixel_size(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-text-pixel-size", &args, 0, 7)?;

    if let Some(window) = args.first()
        && !window.is_nil()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), *window],
        ));
    }
    if let Some(from) = args.get(1) {
        validate_window_text_pixel_size_from_arg(*from)?;
    }
    if let Some(to) = args.get(2) {
        validate_window_text_pixel_size_to_arg(*to)?;
    }

    Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)))
}

fn buffer_lisp_pos_to_emacs_byte_pos_clipped(
    buf: &Buffer,
    pos: i64,
    lower: LispCharPos1,
) -> EmacsBytePos {
    let max = buf.point_max_lisp_char_pos().as_i64();
    let lower = lower.as_i64().clamp(1, max);
    let clipped = pos.clamp(lower, max);
    buf.char_pos_to_emacs_byte_pos_clamped(
        LispCharPos1::from_one_based_usize(
            usize::try_from(clipped).expect("Lisp character position fits usize"),
        )
        .to_char_pos(),
    )
}

fn first_non_empty_line_start_in_region(buf: &Buffer, region: EmacsByteRange) -> EmacsBytePos {
    let mut bytes = Vec::new();
    buf.copy_emacs_byte_range_to(region, &mut bytes);

    let mut offset = 0;
    while offset < bytes.len() && matches!(bytes[offset], b' ' | b'\t' | b'\n' | b'\r') {
        offset += 1;
    }
    while offset > 0 && matches!(bytes[offset - 1], b' ' | b'\t') {
        offset -= 1;
    }
    region
        .start()
        .add_len(crate::buffer::EmacsByteLen::new(offset))
}

fn window_text_pixel_size_from_pos(
    buffers: &crate::buffer::BufferManager,
    buf: &Buffer,
    from: Option<&Value>,
) -> Result<(EmacsBytePos, Option<i64>), Flow> {
    let beg = buf.point_min_lisp_char_pos();
    let end = buf.point_max_lisp_char_pos();
    let beg_byte = buf.lisp_pos_to_emacs_byte_pos(beg);
    let end_byte = buf.lisp_pos_to_emacs_byte_pos(end);

    match from {
        None => Ok((beg_byte, None)),
        Some(value) if value.is_nil() => Ok((beg_byte, None)),
        Some(value) if value.is_t() => Ok((
            first_non_empty_line_start_in_region(buf, EmacsByteRange::new(beg_byte, end_byte)),
            None,
        )),
        Some(value) if value.is_cons() => {
            let pos = integer_or_marker_value_in_buffers(buffers, value.cons_car())?;
            let y_offset = value.cons_cdr();
            expect_fixnum_arg("integerp", &y_offset)?;
            let y_offset = y_offset.as_fixnum().expect("validated fixnum");
            Ok((
                buffer_lisp_pos_to_emacs_byte_pos_clipped(buf, pos, beg),
                (y_offset != 0).then_some(y_offset),
            ))
        }
        Some(value) => {
            let pos = integer_or_marker_value_in_buffers(buffers, *value)?;
            Ok((
                buffer_lisp_pos_to_emacs_byte_pos_clipped(buf, pos, beg),
                None,
            ))
        }
    }
}

fn window_text_pixel_size_to_pos(
    buffers: &crate::buffer::BufferManager,
    buf: &Buffer,
    to: Option<&Value>,
    from_pos: EmacsBytePos,
) -> Result<EmacsBytePos, Flow> {
    let end = buf.point_max_lisp_char_pos();
    let end_byte = buf.lisp_pos_to_emacs_byte_pos(end);
    match to {
        None => Ok(end_byte),
        Some(value) if value.is_nil() || value.is_t() => Ok(end_byte),
        Some(value) => {
            let pos = integer_or_marker_value_in_buffers(buffers, *value)?;
            Ok(buffer_lisp_pos_to_emacs_byte_pos_clipped(
                buf,
                pos,
                LispCharPos1::from_one_based_usize(
                    buf.emacs_byte_pos_to_lisp_char_pos(from_pos)
                        .to_one_based_usize(),
                ),
            ))
        }
    }
}

/// Resolve named `face` text-property runs traversed by
/// `window-text-pixel-size`, returning GNU-compatible log diagnostics for
/// missing faces.
///
/// GNU's measurement uses the normal display iterator, whose
/// `face_at_buffer_position` merges the `face` property once per property run
/// with diagnostics enabled.  Neomacs performs a headless monospace scan in
/// this primitive, so make that otherwise-observable part of face resolution
/// explicit at the same measurement boundary.
fn invalid_named_face_diagnostics_in_buffer_region(
    eval: &super::eval::Context,
    frame_id: FrameId,
    buffer_id: BufferId,
    from: EmacsBytePos,
    to: EmacsBytePos,
) -> Vec<String> {
    let Some(buf) = eval.buffers.get(buffer_id) else {
        return Vec::new();
    };
    let face = Value::symbol("face");
    let mut pos = buf.emacs_byte_pos_to_char_pos_clamped(from);
    let end = buf.emacs_byte_pos_to_char_pos_clamped(to);
    let mut diagnostics = Vec::new();

    while pos < end {
        let (face_ref, _, run_end) = buf.get_property_run_at_char_pos(pos, face);
        if let Some(face_ref) = face_ref {
            diagnostics.extend(
                super::xfaces::invalid_display_face_references(eval, frame_id, face_ref)
                    .into_iter()
                    .map(|invalid| {
                        format!(
                            "Invalid face reference: {}",
                            super::print::print_value(&invalid)
                        )
                    }),
            );
        }

        pos = if run_end > pos {
            run_end.min(end)
        } else {
            CharPos0::new(pos.get().saturating_add(1)).min(end)
        };
    }

    diagnostics
}

/// Resolve a vertical pixel offset from the last live layout.
///
/// GNU moves a display iterator through rows using each row's realized pixel
/// height.  The redisplay snapshot is neomacs's equivalent source of truth;
/// the character-height scanner in `window_text_pixel_offset_target` is only
/// a fallback when the requested motion leaves the cached visible rows.
enum LiveWindowTextPixelOffset {
    Target {
        position: EmacsBytePos,
        occupied: bool,
    },
    ScanLines(i64),
}

fn live_window_text_pixel_offset(
    eval: &super::eval::Context,
    frame_id: FrameId,
    window_id: WindowId,
    buffer_id: BufferId,
    from: EmacsBytePos,
    y_offset: i64,
    char_height: f32,
) -> Option<LiveWindowTextPixelOffset> {
    let buffer = eval.buffers.get(buffer_id)?;
    let from = buffer.emacs_byte_pos_to_lisp_char_pos(from);
    let accessible_start = buffer.point_min_lisp_char_pos();
    let accessible_end = buffer.point_max_lisp_char_pos();
    let snapshot = eval.fresh_window_display_snapshot(frame_id, window_id, buffer_id)?;

    let is_text_row = |row: &&DisplayRowSnapshot| {
        row.start_buffer_pos
            .zip(row.end_buffer_pos)
            .is_some_and(|(start, end)| {
                accessible_start <= start && start <= end && end <= accessible_end
            })
    };
    let current_row = snapshot.rows.iter().filter(is_text_row).find(|row| {
        row.start_buffer_pos
            .zip(row.end_buffer_pos)
            .is_some_and(|(start, end)| start <= from && from <= end)
    })?;
    let target_y = current_row.y.saturating_add(y_offset);
    if let Some(target_row) = snapshot
        .rows
        .iter()
        .filter(is_text_row)
        .find(|row| target_y >= row.y && target_y < row.y.saturating_add(row.height.max(1)))
    {
        let target = target_row.start_buffer_pos?;
        let occupied = target < accessible_end
            || target_row.end_x > target_row.start_x
            || target_row.end_col > target_row.start_col;
        return Some(LiveWindowTextPixelOffset::Target {
            position: buffer.lisp_pos_to_emacs_byte_pos(target),
            occupied,
        });
    }

    let char_height = f64::from(char_height.max(1.0));
    if y_offset > 0 {
        let last_row = snapshot.rows.iter().rfind(is_text_row)?;
        let last_bottom = last_row.y.saturating_add(last_row.height.max(1));
        if target_y < last_bottom {
            return None;
        }
        let known_crossings = snapshot
            .rows
            .iter()
            .filter(is_text_row)
            .filter(|row| row.y > current_row.y)
            .count();
        let remaining_pixels = target_y.saturating_sub(last_bottom) as f64;
        let unknown_crossings = (remaining_pixels / char_height).floor() as usize;
        let lines = known_crossings
            .saturating_add(1)
            .saturating_add(unknown_crossings);
        return Some(LiveWindowTextPixelOffset::ScanLines(
            i64::try_from(lines).unwrap_or(i64::MAX),
        ));
    }

    let first_row = snapshot.rows.iter().find(is_text_row)?;
    if target_y >= first_row.y {
        return None;
    }
    let known_crossings = snapshot
        .rows
        .iter()
        .filter(is_text_row)
        .filter(|row| row.y < current_row.y)
        .count();
    let remaining_pixels = first_row.y.saturating_sub(target_y) as f64;
    let unknown_crossings = (remaining_pixels / char_height).ceil() as usize;
    let lines = known_crossings.saturating_add(unknown_crossings);
    Some(LiveWindowTextPixelOffset::ScanLines(
        -i64::try_from(lines).unwrap_or(i64::MAX),
    ))
}

#[allow(clippy::too_many_arguments)]
fn window_text_pixel_offset_target(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    window_id: WindowId,
    buffer_id: BufferId,
    from: EmacsBytePos,
    y_offset: i64,
    char_height: f32,
    max_offset_rows: usize,
) -> Result<(EmacsBytePos, bool), Flow> {
    let lines = match live_window_text_pixel_offset(
        eval,
        frame_id,
        window_id,
        buffer_id,
        from,
        y_offset,
        char_height,
    ) {
        Some(LiveWindowTextPixelOffset::Target { position, occupied }) => {
            return Ok((position, y_offset > 0 && occupied));
        }
        Some(LiveWindowTextPixelOffset::ScanLines(lines)) => lines,
        None => {
            let offset_rows = (y_offset.unsigned_abs() as f64) / f64::from(char_height.max(1.0));
            // GNU's forward pixel motion stays on the current display row
            // until the offset reaches its lower edge. Backward motion instead
            // selects the preceding row for any negative displacement.
            let rows = if y_offset > 0 {
                offset_rows.floor() as usize
            } else {
                offset_rows.ceil() as usize
            }
            .min(max_offset_rows);
            i64::try_from(rows).unwrap_or(i64::MAX) * y_offset.signum()
        }
    };
    let max_lines = i64::try_from(max_offset_rows).unwrap_or(i64::MAX);
    let lines = lines.clamp(-max_lines, max_lines);
    let window = Some(Value::make_window(window_id.0));
    let motion =
        super::indent::scan_screen_line_motion_target(eval, buffer_id, from, window, lines)?;
    let target = if lines > 0 && motion.moved < lines {
        motion.last_occupied_target
    } else {
        motion.target
    };
    let occupied = y_offset > 0
        && eval
            .buffers
            .get(buffer_id)
            .is_some_and(|buffer| target < buffer.accessible_emacs_byte_region().end());
    Ok((target, occupied))
}

/// `(window-text-pixel-size &optional WINDOW FROM TO X-LIMIT Y-LIMIT MODE)` evaluator-backed variant.
///
/// Computes the region's dimensions in pixels the way GNU's display iterator
/// does: text is approximated by the frame's character cell, and a `display`
/// property that replaces its text -- an image -- contributes its own measured
/// advance and baseline split (see [`region_text_metrics_with_display`]).
pub(crate) fn builtin_window_text_pixel_size_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-text-pixel-size", &args, 0, 7)?;
    let (fid, wid) = resolve_live_window_for_text_pixel_size(&eval.frames, args.first())?;

    let cell = eval
        .frames
        .get(fid)
        .map(|frame| {
            TextCellPixels::new(
                frame.char_width,
                frame.char_height,
                frame.font_cell_ascent(),
            )
        })
        .unwrap_or_else(|| TextCellPixels::new(1.0, 1.0, 1.0));
    let char_h = cell.height;
    let Some((buf_id, is_terminal)) = eval.frames.get(fid).and_then(|frame| {
        let window = frame.find_window(wid)?;
        let buf_id = window.buffer_id()?;
        Some((buf_id, frame.effective_window_system().is_none()))
    }) else {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    };
    // GNU's `it.line_wrap` is resolved once per window+buffer pair and decides
    // what happens at the row edge; it is the only input that does.
    let line_wrap = window_line_wrap(eval, wid, buf_id);
    // GNU `window-text-pixel-size' is
    // (WINDOW &optional FROM TO X-LIMIT Y-LIMIT MODE-LINES IGNORE-LINE-AT-END),
    // so Y-LIMIT is argument 4.  It was never read: the scanner was handed
    // `None` and walked BEGV..ZV however large the buffer was, which is what
    // made `fit-window-to-buffer' -- whose vertical branch passes
    // `(frame-pixel-height frame)' as Y-LIMIT (lisp/window.el) -- scan the
    // whole buffer.  GNU stops after one frame's worth of rows: 2,154ms
    // against its flat 31ms on a 320,000-character buffer.
    //
    // Y-LIMIT is in PIXELS, which is the unit the scanner now works in: GNU
    // stops the walk at the first row whose pixel span contains it and clamps
    // the returned height to it (`if (y > max_y) y = max_y`, src/xdisp.c:12012).
    let y_limit = match args.get(4) {
        Some(value) if !value.is_nil() && !value.is_t() => match value.kind() {
            ValueKind::Fixnum(pixels) if pixels >= 0 => Some(pixels as f32),
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("natnump"), *value],
                ));
            }
        },
        _ => None,
    };
    // X-LIMIT is argument 3, in PIXELS.  GNU has exactly three cases and they
    // are not "some value or none":
    //
    //     if (RANGED_FIXNUMP (0, x_limit, INT_MAX))  max_x = XFIXNUM (x_limit);
    //     else if (!NILP (x_limit))                  max_x = INT_MAX;
    //
    // (src/xdisp.c:11796-11799).  So a fixnum replaces the row edge whole
    // (`it.last_visible_x = max_x`, src/xdisp.c:11924, keeping the window's
    // `line_wrap`); anything else non-nil -- `t` included, and a negative or
    // non-numeric value too -- is unbounded; and nil leaves the edge
    // `init_iterator` set, which is the window's BODY width (`it.last_visible_x
    // = it.first_visible_x + body_width`, src/xdisp.c:3507).  That last case is
    // why a truncated line cannot measure wider than the window; Neomacs applied
    // it to terminal frames only, in COLUMNS, and left a GUI frame's row
    // officially unbounded.
    let explicit_x_limit = match x_limit_arg(args.get(3)) {
        XLimitArg::Body => None,
        XLimitArg::Unbounded => Some(RowEdge::new(i32::MAX as f32, line_wrap)),
        XLimitArg::Pixels(pixels) => Some(RowEdge::new(pixels, line_wrap)),
    };
    let body_edge = eval.frames.get(fid).and_then(|frame| {
        let window = frame.find_window(wid)?;
        window_body_row_edge(
            &eval.frames,
            fid,
            window,
            is_terminal,
            frame.char_width,
            line_wrap,
        )
    });

    let (initial_from_pos, to_pos, y_offset, max_offset_rows) = {
        let Some(buf) = eval.buffers.get(buf_id) else {
            return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
        };
        let (from_pos, y_offset) =
            window_text_pixel_size_from_pos(&eval.buffers, buf, args.get(1))?;
        let to_pos = window_text_pixel_size_to_pos(&eval.buffers, buf, args.get(2), from_pos)?;
        let accessible = buf.accessible_emacs_byte_region();
        (
            from_pos,
            to_pos,
            y_offset,
            accessible
                .end()
                .get()
                .saturating_sub(accessible.start().get())
                .saturating_add(1),
        )
    };

    // Precision scrolling measures backwards from the current row boundary,
    // excluding that row. Use the same offscreen producer as display motion;
    // multiplying source lines by the default font height loses pixels on
    // every crossing of a mixed-font, raised, wrapped, or overlay row.
    if let Some(offset) = y_offset.filter(|offset| *offset < 0)
        && initial_from_pos == to_pos
        && args.get(3).is_none_or(|v| v.is_nil())
        && args.get(4).is_none_or(|v| v.is_nil())
        && args.get(5).is_none_or(|v| v.is_nil())
        && args.get(6).is_some_and(|v| v.is_truthy())
    {
        let origin = eval
            .buffers
            .get(buf_id)
            .expect("resolved buffer")
            .emacs_byte_pos_to_lisp_char_pos(initial_from_pos);
        if let Some(extent) =
            motion::pixels::backward_extent(eval, fid, wid, buf_id, origin, offset)?
        {
            return Ok(Value::list(vec![
                Value::fixnum(extent.width),
                Value::fixnum(extent.height),
                Value::fixnum(extent.start.as_i64()),
            ]));
        }
    }

    // Determine FROM/TO range.
    let (from_pos, offset_landed_on_occupied_row) = if let Some(y_offset) = y_offset {
        window_text_pixel_offset_target(
            eval,
            fid,
            wid,
            buf_id,
            initial_from_pos,
            y_offset,
            char_h,
            max_offset_rows,
        )?
    } else {
        (initial_from_pos, false)
    };
    let reported_start = {
        let Some(buf) = eval.buffers.get(buf_id) else {
            return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
        };
        y_offset.map(|_| Value::fixnum(buf.emacs_byte_pos_to_lisp_char_pos(from_pos).as_i64()))
    };
    // GNU's TO=t means measure through the line ending the last non-empty line,
    // not through trailing blank lines.
    let apply_trim = args
        .get(2)
        .is_some_and(|v| v.is_t() || v.is_symbol_named("t"));

    // Collect first so the immutable buffer borrow ends before `add_to_log`
    // mutates the buffer manager to append to *Messages*.
    let face_diagnostics =
        invalid_named_face_diagnostics_in_buffer_region(eval, fid, buf_id, from_pos, to_pos);
    for diagnostic in face_diagnostics {
        eval.add_to_log(&diagnostic);
    }

    // GNU measures from the beginning of FROM's DISPLAY line, not from FROM:
    // `window_text_pixel_size` rewinds the iterator (`move_it_by_lines (&it,
    // 0)`, then `it.current_x = it.hpos = it.wrap_prefix_width = 0`,
    // src/xdisp.c:11833-11899) and keeps the x it reaches at FROM as `start_x`,
    // returning `x - start_x`.  The prefix of FROM's line is therefore inside
    // the measurement -- which is why `abcd\n` reports 36 from every start
    // position, not 36/27/18/9.
    //
    // A FROM given as `(POS . VOFFSET)` skips the rewind: GNU takes a wholly
    // different branch there, moving the iterator vertically by VOFFSET and
    // leaving `start_x` at the x the offset landed on.
    let measured = {
        let Some(buf) = eval.buffers.get(buf_id) else {
            return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
        };
        let line_start = if y_offset.is_some() {
            from_pos
        } else {
            display_line_start_byte(buf, from_pos)
        };
        MeasuredRange {
            line_start,
            from: from_pos,
            to: to_pos,
        }
    };

    // Measure the region in pixels, honoring `display` text properties: a
    // `(space :align-to N)` stretch, or an image, which contributes its own
    // width and its own baseline split.
    let edge = explicit_x_limit.or(body_edge);
    let mut text_metrics = region_text_metrics_with_display(
        eval,
        fid,
        buf_id,
        measured,
        apply_trim,
        cell,
        CharColumnWidth::One,
        edge,
        y_limit,
    );
    if offset_landed_on_occupied_row && from_pos >= to_pos {
        // GNU keeps the adjusted iterator's current row in the vertical
        // extent even when its original, pre-clipped TO is now at or before
        // FROM.  No text width is traversed in that case.
        text_metrics = RegionTextMetrics::empty_row(char_h);
    }

    let width = text_metrics.max_width.ceil() as i64;
    let mode_line_rows = if window_text_pixel_size_includes_mode_line(args.get(5)) {
        1.0
    } else {
        0.0
    };
    let height = (text_metrics.height + mode_line_rows * char_h).ceil() as i64;

    if let Some(reported_start) = reported_start {
        Ok(Value::list(vec![
            Value::fixnum(width),
            Value::fixnum(height),
            reported_start,
        ]))
    } else {
        Ok(Value::cons(Value::fixnum(width), Value::fixnum(height)))
    }
}

/// GNU's three-way X-LIMIT argument (src/xdisp.c:11796-11799).
enum XLimitArg {
    /// nil or omitted: keep `init_iterator`'s edge, the window's body width.
    Body,
    /// `t`, a negative fixnum, or any other non-nil non-fixnum: `max_x =
    /// INT_MAX`, an unbounded row.
    Unbounded,
    /// A non-negative fixnum: the row edge itself.
    Pixels(f32),
}

fn x_limit_arg(value: Option<&Value>) -> XLimitArg {
    match value {
        None => XLimitArg::Body,
        Some(value) if value.is_nil() => XLimitArg::Body,
        Some(value) => match value.as_int().filter(|pixels| *pixels >= 0) {
            Some(pixels) => XLimitArg::Pixels(if pixels > i64::from(i32::MAX) {
                i32::MAX as f32
            } else {
                pixels as f32
            }),
            None => XLimitArg::Unbounded,
        },
    }
}

/// The window's `truncate-lines` / `word-wrap` decision, from a window
/// identity the caller has already decoded.
fn window_line_wrap(
    eval: &mut super::eval::Context,
    window_id: crate::window::WindowId,
    buffer_id: BufferId,
) -> LineWrap {
    super::window_cmds::window_line_wrap(
        eval,
        Some(Value::make_window(window_id.0)),
        buffer_id,
        MotionEngine::DisplayIterator,
    )
}

/// GNU's default row edge when X-LIMIT is nil: the window's BODY width.
///
/// `init_iterator` sets `it.last_visible_x = it.first_visible_x +
/// window_box_width (w, TEXT_AREA)` (src/xdisp.c:3507) and then takes the
/// truncation or continuation glyph's width back off for a window with no
/// right fringe (src/xdisp.c:3508-3516) -- which is every terminal window.
/// `window_body_width (w, WINDOW_BODY_IN_PIXELS)` is that same text-area width.
///
/// A terminal window is expressed in columns first, because its pixel geometry
/// is derived from the character grid and GNU's reserve is exactly one column;
/// a window-system window keeps the exact pixel width.
fn window_body_row_edge(
    frames: &FrameManager,
    fid: FrameId,
    window: &crate::window::Window,
    is_terminal: bool,
    char_width: f32,
    line_wrap: LineWrap,
) -> Option<RowEdge> {
    let body_pixels = super::window_cmds::window_body_width_pixels(frames, fid, window);
    if body_pixels <= 0 {
        return None;
    }
    if is_terminal {
        let columns = (body_pixels as f32 / char_width.max(1.0)).floor() as usize;
        return Some(RowEdge::tty(columns, char_width, line_wrap));
    }
    Some(RowEdge::new(body_pixels as f32, line_wrap))
}

/// First byte of the display line that `from` sits on.
///
/// This is the position GNU rewinds to with `move_it_by_lines (&it, 0)`:
/// the beginning of FROM's *screen* line.  For a soft-wrapped line the true
/// screen line starts later than the logical one, so this returns the logical
/// line start and lets the scanner -- which knows [`RowEdge::wrap`] -- discard
/// the rows before FROM.  That is exactly what GNU's `it.current_y = start_y`
/// reset does after its own forward walk (src/xdisp.c:11906).
fn display_line_start_byte(buf: &crate::buffer::Buffer, from: EmacsBytePos) -> EmacsBytePos {
    let begin = buf.accessible_emacs_byte_region().start().get();
    let mut pos = from.get();
    while pos > begin {
        // Newline bytes cannot occur inside a multi-byte character in emacs
        // byte encoding, so a raw backwards scan is safe.
        if buf.emacs_byte_at_pos(EmacsBytePos::new(pos - 1)) == Some(b'\n') {
            break;
        }
        pos -= 1;
    }
    EmacsBytePos::new(pos)
}

fn resolve_live_window_for_text_pixel_size(
    frames: &FrameManager,
    window: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    if window.is_none_or(|value| value.is_nil()) {
        let Some(frame) = frames.selected_frame() else {
            return Err(signal("error", vec![Value::string("No selected frame")]));
        };
        return Ok((frame.id, frame.selected_window));
    }

    let value = window.expect("non-nil window argument");
    let wid = value.as_window_id().map(WindowId);
    let Some(wid) = wid else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), *value],
        ));
    };
    frames
        .find_window_frame_id(wid)
        .map(|fid| (fid, wid))
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), *value],
            )
        })
}

/// (pos-visible-in-window-p &optional POS WINDOW PARTIALLY) -> boolean
///
/// Batch-compatible behavior: no window visibility is reported, so this
/// returns nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_pos_visible_in_window_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("pos-visible-in-window-p", &args, 0, 3)?;
    if let Some(window) = args.get(1)
        && !window.is_nil()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), *window],
        ));
    }
    // POS can be nil (point), t (end of buffer), or an integer/marker.
    if let Some(pos) = args.first()
        && !pos.is_nil()
        && !pos.is_t()
        && !pos.is_symbol_named("t")
    {
        expect_integer_or_marker(pos)?;
    }
    Ok(Value::NIL)
}

/// `(pos-visible-in-window-p &optional POS WINDOW PARTIALLY)` evaluator-backed variant.
///
/// Mirror GNU Emacs using current logical rows, including top/bottom clipping.
/// The last renderer presentation is not authoritative for a live-state query.
pub(crate) fn builtin_pos_visible_in_window_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("pos-visible-in-window-p", &args, 0, 3)?;
    validate_optional_window_designator_in_state(
        &eval.frames,
        args.get(1),
        crate::emacs_core::window_cmds::WindowDomain::Live,
    )?;
    // GNU `pos_visible_p` (xdisp.c): `if (FRAME_INITIAL_P (frame)) return
    // false;` — nothing is ever visible on the bootstrap/--batch frame, no
    // matter where window-start sits.  It is a frame-kind rule, not a
    // `noninteractive` one: a real (GUI/tty) frame answers geometrically
    // from the CURRENT window-start even before redisplay has run, which is
    // what keeps queued interactive scrolls monotonic.  `window_scroll_*`
    // gate their recenter-around-point on exactly this predicate.
    let on_initial_frame = resolve_live_window_identity(&eval.frames, args.get(1))?
        .and_then(|(fid, _)| eval.frames.get(fid))
        .is_some_and(|frame| frame.initial);
    if on_initial_frame {
        if let Some(pos) = args.first()
            && !pos.is_nil()
            && !pos.is_t()
            && !pos.is_symbol_named("t")
        {
            expect_integer_or_marker(pos)?;
        }
        return Ok(Value::NIL);
    }
    // GNU pos_visible_p walks from the LIVE window start, not the renderer's
    // last presentation. A scroll command asks this before redisplay in order
    // to keep point visible; answering from the old presentation creates a
    // circular dependency and makes redisplay undo the scroll.
    if let Some(visibility) = live_position_visibility(eval, args.get(1), args.first())? {
        return Ok(visibility.into_lisp(args.get(2).is_some_and(|value| value.is_truthy())));
    }
    pos_visible_in_window_p_impl(&mut eval.frames, &mut eval.buffers, args)
}

fn pos_visible_in_window_p_impl(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("pos-visible-in-window-p", &args, 0, 3)?;
    validate_optional_window_designator_in_state(
        &*frames,
        args.get(1),
        crate::emacs_core::window_cmds::WindowDomain::Live,
    )?;
    let partially = args.get(2).is_some_and(|v| v.is_truthy());
    let Some(ctx) = resolve_live_window_display_context(frames, buffers, args.get(1))? else {
        return Ok(Value::NIL);
    };
    let Some(pos_lisp) = resolve_pos_visible_target_lisp_pos(&ctx, args.first())? else {
        return Ok(Value::NIL);
    };
    let Some(metrics) = approximate_pos_visible_metrics(&ctx, pos_lisp) else {
        return Ok(Value::NIL);
    };
    if !partially && !metrics.fully_visible {
        return Ok(Value::NIL);
    }
    if !partially {
        return Ok(Value::T);
    }
    let mut out = vec![Value::fixnum(metrics.x), Value::fixnum(metrics.y)];
    if !metrics.fully_visible {
        out.extend([
            Value::fixnum(metrics.rtop),
            Value::fixnum(metrics.rbot),
            Value::fixnum(metrics.row_height),
            Value::fixnum(metrics.vpos),
        ]);
    }
    Ok(Value::list(out))
}

/// `(fringe-bitmaps-at-pos &optional POS WINDOW)`.
///
/// GNU keeps this in `src/fringe.c` (`Ffringe_bitmaps_at_pos`), but it is a
/// pure reader of the window's current matrix, so it lives here beside the
/// other matrix readers (`pos-visible-in-window-p`, `window-line-height`) that
/// share the redisplay-snapshot seam.
///
/// GNU locates the glyph row containing POS via `row_containing_pos` and
/// returns `(LEFT RIGHT OVERLAY)` from that row's three fringe slots, or nil
/// when no row contains POS. A row that carries no bitmaps still answers
/// `(nil nil nil)` — only an off-window POS gives nil.
pub(crate) fn builtin_fringe_bitmaps_at_pos(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("fringe-bitmaps-at-pos", &args, 0, 2)?;
    let window_arg = args.get(1).copied().unwrap_or(Value::NIL);
    // GNU opens with `w = decode_any_window (window);` (`src/fringe.c`), the
    // WIDEST decoder -- so a non-window signals `windowp`, not `window-live-p`:
    //
    //     (fringe-bitmaps-at-pos nil 'foo)  =>  (wrong-type-argument windowp foo)
    //
    // verified against GNU Emacs 31.1.  An internal or deleted window is
    // accepted by that decode and fails (or, for an internal window, crashes)
    // further in; neomacs rejecting them early is strictly better behaviour
    // and is left alone -- what is fixed here is the PREDICATE, which is
    // observable for every non-window argument.
    validate_optional_window_designator_in_state(
        &eval.frames,
        args.get(1),
        crate::emacs_core::window_cmds::WindowDomain::Any,
    )?;
    let Some((frame_id, window_id)) = resolve_live_window_identity(&eval.frames, args.get(1))?
    else {
        return Ok(Value::NIL);
    };
    // GNU reports the error against the window OBJECT, not the argument as
    // written, so a nil WINDOW still names the selected window.
    let window_value = if window_arg.is_nil() {
        Value::make_window(window_id.0)
    } else {
        window_arg
    };

    let Some(buffer_id) = eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.buffer_id())
    else {
        return Ok(Value::NIL);
    };
    let Some((begv, zv, buffer_point)) = eval.buffers.get(buffer_id).map(|buffer| {
        (
            buffer.point_min_lisp_char_pos(),
            buffer.point_max_lisp_char_pos(),
            buffer.point_lisp_char_pos(),
        )
    }) else {
        return Ok(Value::NIL);
    };

    let textpos = match args.first() {
        Some(pos) if !pos.is_nil() => {
            let raw = super::buffer::expect_integer_or_marker_in_buffers(&eval.buffers, pos)?;
            if raw < begv.to_one_based_usize() as i64 || raw > zv.to_one_based_usize() as i64 {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![window_value, *pos],
                ));
            }
            LispCharPos1::from_one_based_usize(raw as usize)
        }
        // GNU: the selected window tracks the live buffer point, any other
        // window its own stored `w->pointm`.
        _ => {
            let is_selected = eval
                .frames
                .selected_frame()
                .is_some_and(|frame| frame.selected_window == window_id);
            if is_selected {
                buffer_point
            } else {
                eval.frames
                    .get(frame_id)
                    .and_then(|frame| frame.find_window(window_id))
                    .and_then(|window| match window {
                        crate::window::Window::Leaf { point, .. } => Some(*point),
                        _other => None,
                    })
                    .unwrap_or(begv)
            }
        }
    };

    let Some(fringe) = eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.redisplay_snapshot(window_id))
        .and_then(|snapshot| snapshot.fringe_bitmaps_for_buffer_pos(textpos))
    else {
        return Ok(Value::NIL);
    };

    let name = |index: crate::window::FringeBitmapIndex| {
        eval.fringe_bitmap_registry()
            .symbol_for_index(u32::from(index.0))
            .map(Value::from_sym_id)
            .unwrap_or(Value::NIL)
    };
    Ok(Value::list(vec![
        fringe.left.map(name).unwrap_or(Value::NIL),
        fringe.right.map(name).unwrap_or(Value::NIL),
        match fringe.overlay_arrow {
            crate::window::RowOverlayArrowBitmap::Absent => Value::NIL,
            crate::window::RowOverlayArrowBitmap::Unresolved => Value::T,
            crate::window::RowOverlayArrowBitmap::Bitmap(index) => name(index),
        },
    ]))
}

/// `(window-line-height &optional LINE WINDOW)` evaluator-backed variant.
///
/// GNU Emacs returns `(HEIGHT VPOS YPOS OFFBOT)` for a live GUI window.  We
/// approximate this from the current frame/window geometry so commands in
/// `simple.el` can reason about visual line movement without batch fallbacks.
pub(crate) fn builtin_window_line_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let noninteractive = eval.noninteractive();
    window_line_height_impl(&mut eval.frames, &mut eval.buffers, noninteractive, args)
}

fn window_line_height_impl(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    noninteractive: bool,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-line-height", &args, 0, 2)?;
    validate_optional_window_designator_in_state(
        &*frames,
        args.get(1),
        crate::emacs_core::window_cmds::WindowDomain::Live,
    )?;
    // GNU bails out here, AFTER decoding WINDOW and BEFORE looking at LINE
    // (`src/window.c`):
    //
    //     w = decode_live_window (window);
    //     if (noninteractive || w->pseudo_window_p)
    //       return Qnil;
    //     ...
    //     CHECK_FIXNUM (line);
    //
    // so in batch every call answers nil whatever LINE is -- there is no
    // display matrix to measure, which is exactly what the docstring's "Return
    // nil if window display is not up-to-date" describes.  Falling through to
    // the LINE type-check signalled `integerp` where GNU returns nil.  The
    // ORDER matters: a bad WINDOW must still signal, even though the answer
    // would have been nil anyway.
    if noninteractive {
        return Ok(Value::NIL);
    }
    if let Some((fid, wid)) = resolve_live_window_identity(frames, args.get(1))?
        && let Some(frame) = frames.get(fid)
        && let Some(snapshot) = frame.redisplay_snapshot(wid)
    {
        let line_spec = args.first().copied().unwrap_or(Value::NIL);
        let metrics = if line_spec.is_nil() {
            resolve_exact_visible_metrics(
                frames,
                buffers,
                args.get(1),
                None,
                PositionGeometrySource::Presented,
            )?
            .and_then(|(_, metrics)| {
                snapshot
                    .row_metrics(metrics.row)
                    .map(|row| snapshot_text_row_line_metrics(snapshot, row))
            })
        } else if let Some(selector) = WindowLineSelector::from_lisp_value(line_spec) {
            snapshot_chrome_line_metrics(snapshot, selector)
        } else {
            let line_num = match line_spec.kind() {
                ValueKind::Fixnum(n) => n,
                _other => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("integerp"), line_spec],
                    ));
                }
            };
            snapshot_text_line_metrics(snapshot, line_num)
        };
        if let Some(metrics) = metrics {
            return Ok(Value::list(vec![
                Value::fixnum(metrics.height),
                Value::fixnum(metrics.vpos),
                Value::fixnum(metrics.ypos),
                Value::fixnum(metrics.offbot),
            ]));
        }
        return Ok(Value::NIL);
    }
    // No redisplay snapshot means no current matrix, and GNU
    // `Fwindow_line_height` (src/window.c:2048) then answers nothing at all:
    //
    //   /* Fail if current matrix is not up-to-date.  */
    //   if (!w->window_end_valid || windows_or_buffers_changed
    //       || b->clip_changed || b->prevent_redisplay_optimizations_p
    //       || window_outdated (w))
    //     return Qnil;
    //
    // (src/window.c:2082-2089). Its docstring says what the nil is for: "Return
    // nil if window display is not up-to-date. In that case, use
    // `pos-visible-in-window-p' to obtain the information." A geometry
    // approximation offered here would be a number describing no matrix, in
    // place of GNU's refusal -- so the LINE argument is still type-checked, and
    // then nothing is answered.
    //
    // `pos-visible-in-window-p' keeps its approximation on purpose: GNU's
    // `pos_visible_p' does not read the matrix, it runs `move_it_to'. That
    // asymmetry is GNU's own, and it is the one the docstring points at.
    let line_spec = args.first().copied().unwrap_or(Value::NIL);
    if !line_spec.is_nil()
        && WindowLineSelector::from_lisp_value(line_spec).is_none()
        && !matches!(line_spec.kind(), ValueKind::Fixnum(_))
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), line_spec],
        ));
    }
    Ok(Value::NIL)
}

/// (move-point-visually DIRECTION) -> boolean
///
/// Batch semantics: direction is validated as a fixnum and the command
/// signals `args-out-of-range` in non-window contexts.
pub(crate) fn builtin_move_point_visually(args: Vec<Value>) -> EvalResult {
    expect_args("move-point-visually", &args, 1)?;
    match args[0].kind() {
        ValueKind::Fixnum(v) => Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(v), Value::fixnum(v)],
        )),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), args[0]],
        )),
    }
}

/// (lookup-image-map MAP X Y) -> symbol or nil
///
/// Lookup an image map at coordinates. Stub implementation
/// returns nil while preserving arity validation.
pub(crate) fn builtin_lookup_image_map(args: Vec<Value>) -> EvalResult {
    expect_args("lookup-image-map", &args, 3)?;
    if !args[0].is_nil() {
        expect_fixnum_arg("fixnump", &args[1])?;
        expect_fixnum_arg("fixnump", &args[2])?;
    }
    Ok(Value::NIL)
}

/// (current-bidi-paragraph-direction &optional BUFFER) -> symbol
///
/// Get the bidi paragraph direction. Returns the symbol 'left-to-right.
/// A UTF-8 lead byte / ASCII byte (GNU `CHAR_HEAD_P`): not a `0x80..=0xBF`
/// continuation byte.
fn bidi_is_char_head(byte: u8) -> bool {
    !(0x80..=0xBF).contains(&byte)
}

/// Byte position of the beginning of the line containing `pos`.
fn bidi_line_bol(buf: &Buffer, mut pos: usize, begv: usize) -> usize {
    while pos > begv {
        if buf.emacs_byte_at_pos(EmacsBytePos::new(pos - 1)) == Some(b'\n') {
            break;
        }
        pos -= 1;
    }
    pos
}

/// Byte position of the newline (or ZV) ending the line containing `pos`.
fn bidi_line_eol(buf: &Buffer, mut pos: usize, zv: usize) -> usize {
    while pos < zv {
        if buf.emacs_byte_at_pos(EmacsBytePos::new(pos)) == Some(b'\n') {
            break;
        }
        pos += 1;
    }
    pos
}

/// Whether `[bol, eol)` contains only whitespace — a bidi paragraph separator
/// (the default `bidi-paragraph-separate-re` is an empty/whitespace-only line).
fn bidi_line_blank(buf: &Buffer, bol: usize, eol: usize) -> bool {
    let mut b = bol;
    while b < eol {
        match buf.emacs_byte_at_pos(EmacsBytePos::new(b)) {
            Some(b' ') | Some(b'\t') | Some(0x0c) => b += 1,
            _ => return false,
        }
    }
    true
}

/// Start (byte) of the bidi paragraph containing `point`, mirroring GNU's
/// `bidi_paragraph_init` / `Fcurrent_bidi_paragraph_direction`: from point
/// (stepped back from end of buffer, and off a trailing whitespace-only line),
/// walk back across consecutive non-blank lines to the line after the previous
/// blank line (or BEGV). A single newline does not separate paragraphs.
fn bidi_paragraph_start(buf: &Buffer, point: usize, begv: usize, zv: usize) -> usize {
    let mut pos = point;
    if pos >= zv && pos > begv {
        pos -= 1;
        while pos > begv
            && !bidi_is_char_head(buf.emacs_byte_at_pos(EmacsBytePos::new(pos)).unwrap_or(0))
        {
            pos -= 1;
        }
    }
    let mut start = bidi_line_bol(buf, pos, begv);
    // If point sits on a blank line, use the previous non-blank line's paragraph.
    let eol = bidi_line_eol(buf, start, zv);
    if start > begv && bidi_line_blank(buf, start, eol) {
        while start > begv {
            let prev_bol = bidi_line_bol(buf, start - 1, begv);
            let blank = bidi_line_blank(buf, prev_bol, start - 1);
            start = prev_bol;
            if !blank {
                break;
            }
        }
    }
    // Walk back to the paragraph start over consecutive non-blank lines.
    while start > begv {
        let prev_eol = start - 1;
        let prev_bol = bidi_line_bol(buf, prev_eol, begv);
        if bidi_line_blank(buf, prev_bol, prev_eol) {
            break;
        }
        start = prev_bol;
    }
    start
}

fn bidi_buffer_var(ctx: &super::eval::Context, buf_id: BufferId, name: &str) -> Value {
    // `bidi-display-reordering` / `bidi-paragraph-direction` are per-buffer slot
    // variables (BUFFER_OBJFWD), so read the slot directly (like indent.rs),
    // falling back to a buffer-local binding then the global value.
    if let Some(buf) = ctx.buffers.get(buf_id) {
        if let Some(info) = crate::buffer::buffer::lookup_buffer_slot(name) {
            return buf.slots[info.offset.index()];
        }
        if let Some(value) = buf.get_buffer_local(name) {
            return value;
        }
    }
    ctx.obarray
        .symbol_value(name)
        .copied()
        .unwrap_or(Value::NIL)
}

pub(crate) fn builtin_current_bidi_paragraph_direction(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("current-bidi-paragraph-direction", &args, 0, 1)?;
    let ltr = Value::symbol("left-to-right");
    let rtl = Value::symbol("right-to-left");

    let buf_id = match args.first() {
        Some(b) if b.is_buffer() => match b.as_buffer_id() {
            Some(id) => id,
            None => return Ok(ltr),
        },
        Some(b) if !b.is_nil() => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("bufferp"), *b],
            ));
        }
        _ => match ctx.buffers.current_buffer_id() {
            Some(id) => id,
            None => return Ok(ltr),
        },
    };

    // GNU returns left-to-right when reordering is off or the buffer is unibyte.
    let multibyte = ctx
        .buffers
        .get(buf_id)
        .map(|b| b.get_multibyte())
        .unwrap_or(false);
    if bidi_buffer_var(ctx, buf_id, "bidi-display-reordering").is_nil() || !multibyte {
        return Ok(ltr);
    }
    // An explicit `bidi-paragraph-direction` wins.
    let para_dir = bidi_buffer_var(ctx, buf_id, "bidi-paragraph-direction");
    if !para_dir.is_nil() {
        return Ok(para_dir);
    }

    // Auto-detect: scan the paragraph at point for the first strong character.
    let (begv, zv, point) = {
        let Some(buf) = ctx.buffers.get(buf_id) else {
            return Ok(ltr);
        };
        let acc = buf.accessible_emacs_byte_region();
        (
            acc.start().get(),
            acc.end().get(),
            buf.point_emacs_byte_pos().get(),
        )
    };
    let para_start = {
        let Some(buf) = ctx.buffers.get(buf_id) else {
            return Ok(ltr);
        };
        bidi_paragraph_start(buf, point, begv, zv)
    };

    let l = intern("L");
    let r = intern("R");
    let al = intern("AL");
    let mut p = para_start;
    let mut at_bol = true;
    while p < zv {
        let (code, len) = {
            let Some(buf) = ctx.buffers.get(buf_id) else {
                break;
            };
            // A blank line after the paragraph start ends the paragraph.
            if at_bol && p > para_start {
                let eol = bidi_line_eol(buf, p, zv);
                if bidi_line_blank(buf, p, eol) {
                    break;
                }
            }
            match buf.char_code_after_emacs_byte_pos(EmacsBytePos::new(p)) {
                Some(c) => {
                    let len = buf
                        .char_after_emacs_byte_len(EmacsBytePos::new(p))
                        .map(|x| x.get().max(1))
                        .unwrap_or(1);
                    (c, len)
                }
                None => break,
            }
        };
        if code != b'\n' as u32 {
            let cls = ctx.funcall_general(
                Value::symbol("get-char-code-property"),
                vec![Value::fixnum(code as i64), Value::symbol("bidi-class")],
            )?;
            match cls.as_symbol_id() {
                Some(s) if s == l => return Ok(ltr),
                Some(s) if s == r || s == al => return Ok(rtl),
                _ => {}
            }
        }
        at_bol = code == b'\n' as u32;
        p += len;
    }
    Ok(ltr)
}

/// `(bidi-resolved-levels &optional PARAGRAPH-DIRECTION)` -> nil
///
/// Batch compatibility: this currently returns nil and only enforces the
/// `fixnump` argument contract when PARAGRAPH-DIRECTION is non-nil.
pub(crate) fn builtin_bidi_resolved_levels(args: Vec<Value>) -> EvalResult {
    expect_args_range("bidi-resolved-levels", &args, 0, 1)?;
    if let Some(direction) = args.first()
        && !direction.is_nil()
    {
        expect_fixnum_arg("fixnump", direction)?;
    }
    Ok(Value::NIL)
}

/// `(bidi-find-overridden-directionality STRING/START END/START STRING/END
/// &optional DIRECTION)` -> nil
///
/// Batch compatibility mirrors oracle argument guards:
/// - when arg3 is a string, this path accepts arg1/arg2 without additional
///   type checks and returns nil;
/// - when arg3 is nil, arg1 and arg2 must satisfy `integer-or-marker-p`.
pub(crate) fn builtin_bidi_find_overridden_directionality(args: Vec<Value>) -> EvalResult {
    expect_args_range("bidi-find-overridden-directionality", &args, 3, 4)?;
    let third = &args[2];
    if third.is_nil() {
        expect_integer_or_marker(&args[0])?;
        expect_integer_or_marker(&args[1])?;
        return Ok(Value::NIL);
    }
    if !third.is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *third],
        ));
    }
    Ok(Value::NIL)
}

/// `(move-to-window-line ARG)` -> integer
///
/// GNU `Fmove_to_window_line` (src/window.c:7498-7573) is four lines of real
/// work on top of `vertical-motion`, and every one of them is about SCREEN
/// lines:
///
/// ```c
///   else
///     Fgoto_char (w->start);
///   lines = displayed_window_lines (w);
///   if (NILP (arg))
///     XSETFASTINT (arg, lines / 2);
///   else
///     {
///       EMACS_INT iarg = XFIXNUM (Fprefix_numeric_value (arg));
///       if (iarg < 0)
///         iarg = iarg + lines;
///       arg = make_fixnum (iarg);
///     }
///   if (w->vscroll)
///     XSETINT (arg, XFIXNUM (arg) + 1);
///   return Fvertical_motion (arg, window, Qnil);
/// ```
///
/// So the answer is `vertical-motion`'s answer -- the number of screen lines
/// actually moved over, which is SMALLER than ARG when the buffer runs out --
/// and a positive ARG is not clamped to the window at all.  Verified under GNU
/// Emacs 31.0.90 in a 47-line TTY window: over 200 logical lines
/// `(move-to-window-line 100)` answers 100 and lands on line 101, while over a
/// three-line buffer `(move-to-window-line 5)` answers 3 and stops at ZV.
///
/// `displayed_window_lines` (src/window.c:7166-7211) pads the rows it walks
/// with the empty lines below them, so for a window with a uniform line height
/// it is the window's body height in lines; that is what is used here.  A
/// partially visible last line is not modelled.
pub(crate) fn builtin_move_to_window_line(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("move-to-window-line", &args, 1)?;

    let Some(frame) = eval.frames.selected_frame() else {
        return Err(signal(
            "error",
            vec![Value::string(
                "move-to-window-line called from unrelated buffer",
            )],
        ));
    };
    let wid = frame.selected_window;
    let (window_start, buf_id, vscroll_nonzero) = match frame.find_window(wid) {
        Some(Window::Leaf {
            window_start,
            buffer_id,
            vscroll,
            ..
        }) => (*window_start, *buffer_id, *vscroll != 0),
        _ => {
            return Err(signal(
                "error",
                vec![Value::string("Selected window is not a leaf window")],
            ));
        }
    };
    let Some(current_id) = eval.buffers.current_buffer_id() else {
        return Err(signal("error", vec![Value::string("No current buffer")]));
    };
    // GNU: "This test is needed to make sure PT/PT_BYTE make sense in
    // w->contents when passed below to set_marker_both."
    if current_id != buf_id {
        return Err(signal(
            "error",
            vec![Value::string(
                "move-to-window-line called from unrelated buffer",
            )],
        ));
    }

    // GNU `displayed_window_lines (w)`.
    let window_value = Value::make_window(wid.0);
    let lines =
        super::window_cmds::builtin_window_body_height(eval, vec![window_value, Value::NIL])
            .ok()
            .and_then(|value| value.as_fixnum())
            .unwrap_or(1)
            .max(1);

    let accessible = match eval.buffers.get(buf_id) {
        Some(buf) => buf.accessible_emacs_byte_region(),
        None => return Err(signal("error", vec![Value::string("No buffer")])),
    };
    let start_byte = eval
        .buffers
        .get(buf_id)
        .map(|buf| buf.lisp_pos_to_emacs_byte_pos(window_start));

    // GNU: `XFIXNUM (Fprefix_numeric_value (arg))`, so a RAW prefix argument
    // -- `(4)`, `-` -- is a number here, not a type error; the command is
    // `interactive "P"` and GNU never sees a bare integer from the keyboard.
    let mut arg = if args[0].is_nil() {
        lines / 2
    } else {
        let numeric = super::builtins::misc_pure::builtin_prefix_numeric_value(vec![args[0]])?;
        let value = numeric.as_fixnum().unwrap_or(0);
        if value < 0 { value + lines } else { value }
    };

    // GNU: when `w->start' is outside the accessible portion, recenter first;
    // otherwise simply start counting screen lines from `window-start'.
    match start_byte.filter(|byte| accessible.contains(*byte)) {
        Some(byte) => {
            let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, byte);
        }
        None => {
            super::indent::vertical_motion(eval, vec![Value::fixnum(-(lines / 2))])?;
            let point = eval
                .buffers
                .get(current_id)
                .map(|buf| buf.emacs_byte_pos_to_lisp_char_pos(buf.point_emacs_byte_pos()));
            if let Some(point) = point {
                super::window_cmds::builtin_set_window_start(
                    eval,
                    vec![window_value, Value::fixnum(point.as_i64())],
                )?;
            }
        }
    }

    // GNU: "Skip past a partially visible first line."
    if vscroll_nonzero {
        arg += 1;
    }

    super::indent::vertical_motion(eval, vec![Value::fixnum(arg), window_value])
}

/// (tool-bar-height &optional FRAME PIXELWISE) -> integer
///
/// Get the height of the tool bar. Returns 0 (no tool bar).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_tool_bar_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("tool-bar-height", &args, 0, 2)?;
    // Return 0 (no tool bar)
    Ok(Value::fixnum(0))
}

/// `(tool-bar-height &optional FRAME PIXELWISE)` evaluator-backed variant.
///
/// Accepts nil or a live frame designator for FRAME.
pub(crate) fn builtin_tool_bar_height_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("tool-bar-height", &args, 0, 2)?;
    let fid = match args.first().filter(|frame| !frame.is_nil()) {
        Some(frame) => super::window_cmds::resolve_frame_id_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            Some(frame),
            crate::emacs_core::window_cmds::FrameDomain::Any,
        )?,
        None => super::window_cmds::ensure_selected_frame_id_in_state(
            &mut eval.frames,
            &mut eval.buffers,
        ),
    };
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let lines = frame
        .frame_parameter_int("tool-bar-lines")
        .unwrap_or(0)
        .max(0);
    if args.get(1).is_some_and(|pixelwise| !pixelwise.is_nil()) {
        Ok(Value::fixnum(frame.tool_bar_height as i64))
    } else {
        Ok(Value::fixnum(lines))
    }
}

/// (tab-bar-height &optional FRAME PIXELWISE) -> integer
///
/// Get the height of the tab bar. Returns 0 (no tab bar).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_tab_bar_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("tab-bar-height", &args, 0, 2)?;
    // Return 0 (no tab bar)
    Ok(Value::fixnum(0))
}

/// `(tab-bar-height &optional FRAME PIXELWISE)` evaluator-backed variant.
///
/// Accepts nil or a live frame designator for FRAME.
pub(crate) fn builtin_tab_bar_height_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("tab-bar-height", &args, 0, 2)?;
    let fid = match args.first().filter(|frame| !frame.is_nil()) {
        Some(frame) => super::window_cmds::resolve_frame_id_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            Some(frame),
            crate::emacs_core::window_cmds::FrameDomain::Any,
        )?,
        None => super::window_cmds::ensure_selected_frame_id_in_state(
            &mut eval.frames,
            &mut eval.buffers,
        ),
    };
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let lines = frame
        .frame_parameter_int("tab-bar-lines")
        .unwrap_or(0)
        .max(0);
    if args.get(1).is_some_and(|pixelwise| !pixelwise.is_nil()) {
        Ok(Value::fixnum(frame.tab_bar_height as i64))
    } else {
        Ok(Value::fixnum(lines))
    }
}

/// Number of digit columns a line-number gutter shows, before the two
/// padding columns (one leading, one trailing).  Mirrors GNU
/// `maybe_produce_line_number`'s `it->lnum_width` (`max(width, log10(max)+1)`,
/// src/xdisp.c).  `visible_lines` is the floor of the window's visible row
/// count; GNU widens to whichever is larger so the gutter never shrinks below
/// what the visible rows need.
pub(crate) fn line_number_digit_width(buffer: &Buffer, visible_lines: i64) -> i64 {
    let total_lines = display_line_number_total_lines(buffer)
        .max(visible_lines)
        .max(1);
    let digit_count = total_lines.to_string().len() as i64;
    let min_width = buffer
        .buffer_local_value("display-line-numbers-width")
        .and_then(|value| value.as_fixnum())
        .filter(|width| *width > 0)
        .unwrap_or(1);
    digit_count.max(min_width)
}

/// Width in display columns of this window's line-number gutter, or 0 when
/// `display-line-numbers` is off for the buffer.
///
/// This is the column form of GNU's `x_offset` (`it->lnum_pixel_width`,
/// src/xdisp.c STEP 3) that `hscroll_window_tree` subtracts from the text
/// area: the gutter shows `digit_width` digits plus one leading and one
/// trailing padding column, so the usable line-content width is
/// `text_cols - (digit_width + 2)`.
pub(crate) fn line_number_gutter_cols(
    buffer: &Buffer,
    window_bounds_height: f32,
    char_height: f32,
) -> i64 {
    let enabled = buffer
        .buffer_local_value("display-line-numbers")
        .is_some_and(|value| value.is_truthy());
    if !enabled {
        return 0;
    }
    let char_height = char_height.max(1.0);
    let visible_lines = ((window_bounds_height / char_height).floor() as i64).max(1);
    line_number_digit_width(buffer, visible_lines) + 2
}

fn display_line_number_total_lines(buffer: &Buffer) -> i64 {
    let end = buffer.total_emacs_byte_end_pos();
    buffer.count_newlines_emacs_byte(EmacsBytePos::ZERO, end) as i64 + 1
}

/// (long-line-optimizations-p) -> boolean
///
/// Check if long-line optimizations are enabled. Returns nil.
pub(crate) fn builtin_long_line_optimizations_p(args: Vec<Value>) -> EvalResult {
    expect_args("long-line-optimizations-p", &args, 0)?;
    // Return nil (optimizations not enabled)
    Ok(Value::NIL)
}

fn validate_optional_window_designator(
    eval: &super::eval::Context,
    value: Option<&Value>,
    predicate: crate::emacs_core::window_cmds::WindowDomain,
) -> Result<(), Flow> {
    validate_optional_window_designator_in_state(&eval.frames, value, predicate)
}

/// Validate an optional WINDOW argument against the GNU decoder the caller
/// names.
///
/// The domain used to arrive as a `&str` that chose only the error TEXT, while
/// the check was always `find_window` -- so asking for `window-live-p` still
/// admitted an internal window and merely misreported the reason on failure.
/// Taking `WindowDomain` makes the predicate and the lookup the same decision.
fn validate_optional_window_designator_in_state(
    frames: &crate::window::FrameManager,
    value: Option<&Value>,
    predicate: crate::emacs_core::window_cmds::WindowDomain,
) -> Result<(), Flow> {
    let Some(windowish) = value else {
        return Ok(());
    };
    if windowish.is_nil() {
        return Ok(());
    }
    let wid = if let Some(id) = windowish.as_window_id() {
        Some(WindowId(id))
    } else {
        windowish
            .as_fixnum()
            .filter(|&id| id >= 0)
            .map(|id| WindowId(id as u64))
    };
    if let Some(wid) = wid
        && predicate.frame_of(frames, wid).is_some()
    {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(predicate.predicate()), *windowish],
    ))
}

fn validate_optional_buffer_designator(
    eval: &super::eval::Context,
    value: Option<&Value>,
) -> Result<(), Flow> {
    validate_optional_buffer_designator_in_state(&eval.buffers, value)
}

fn validate_optional_buffer_designator_in_state(
    buffers: &crate::buffer::BufferManager,
    value: Option<&Value>,
) -> Result<(), Flow> {
    let Some(bufferish) = value else {
        return Ok(());
    };
    if bufferish.is_nil() {
        return Ok(());
    }
    if let Some(id) = bufferish.as_buffer_id()
        && buffers.get(id).is_some()
    {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("bufferp"), *bufferish],
    ))
}

fn resolve_optional_window_buffer(
    eval: &super::eval::Context,
    value: Option<&Value>,
) -> Option<BufferId> {
    let windowish = value?;
    if windowish.is_nil() {
        return None;
    }

    let wid = if let Some(id) = windowish.as_window_id() {
        Some(WindowId(id))
    } else {
        windowish
            .as_fixnum()
            .filter(|&id| id >= 0)
            .map(|id| WindowId(id as u64))
    }?;

    for fid in eval.frames.frame_list() {
        let Some(frame) = eval.frames.get(fid) else {
            continue;
        };
        if let Some(window) = frame.find_window(wid) {
            return window.buffer_id();
        }
    }

    None
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn resolve_optional_window_buffer_in_state(
    frames: &crate::window::FrameManager,
    value: Option<&Value>,
) -> Option<BufferId> {
    let windowish = value?;
    if windowish.is_nil() {
        return None;
    }

    let wid = if let Some(id) = windowish.as_window_id() {
        Some(WindowId(id))
    } else {
        windowish
            .as_fixnum()
            .filter(|&id| id >= 0)
            .map(|id| WindowId(id as u64))
    }?;

    for fid in frames.frame_list() {
        let Some(frame) = frames.get(fid) else {
            continue;
        };
        if let Some(window) = frame.find_window(wid) {
            return window.buffer_id();
        }
    }

    None
}

fn resolve_mode_line_buffer(
    eval: &super::eval::Context,
    window: Option<&Value>,
    buffer: Option<&Value>,
) -> Option<BufferId> {
    if let Some(buf_val) = buffer
        && let Some(id) = buf_val.as_buffer_id()
    {
        return Some(id);
    }
    resolve_optional_window_buffer(eval, window)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn resolve_mode_line_buffer_in_state(
    frames: &crate::window::FrameManager,
    window: Option<&Value>,
    buffer: Option<&Value>,
) -> Option<BufferId> {
    if let Some(buf_val) = buffer
        && let Some(id) = buf_val.as_buffer_id()
    {
        return Some(id);
    }
    resolve_optional_window_buffer_in_state(frames, window)
}

#[derive(Clone)]
struct ApproxWindowDisplayContext {
    body_height: i64,
    body_lines: i64,
    body_cols: i64,
    char_width: i64,
    char_height: i64,
    window_start: LispCharPos1,
    window_point: LispCharPos1,
    /// The buffer text from char `text_start` (the window start) on -- as much
    /// as the window's rows can show plus slack, or the rest of the buffer
    /// when that is shorter (`ApproxWindowText::bounded`). Copying the whole
    /// buffer made every `posn-at-point` O(Z): the company/corfu cliff.
    text: ApproxWindowText,
    /// The buffer's character count (`Z - 1`).
    total_chars: usize,
    /// The start of the window's last visible line (0-based char index),
    /// found by scanning the buffer's newlines in place, so a line longer
    /// than `text` still resolves exactly.
    last_visible_row_start: usize,
}

/// A window of buffer text, indexed by absolute 0-based char position.
#[derive(Clone)]
struct ApproxWindowText {
    start: usize,
    chars: Vec<char>,
    /// Whether `chars` runs to the end of the buffer.
    reaches_end: bool,
}

impl ApproxWindowText {
    /// The char at absolute position `index`, if this window holds it.
    fn get(&self, index: usize) -> Option<char> {
        index
            .checked_sub(self.start)
            .and_then(|offset| self.chars.get(offset).copied())
    }

    /// One past the last absolute position held.
    fn end(&self) -> usize {
        self.start + self.chars.len()
    }

    /// The first `\n` at or after absolute `from` within this window.
    fn find_newline(&self, from: usize) -> Option<usize> {
        let offset = from.checked_sub(self.start)?;
        self.chars
            .get(offset..)?
            .iter()
            .position(|ch| *ch == '\n')
            .map(|found| from + found)
    }
}

#[derive(Clone, Copy)]
struct ApproxVisibleMetrics {
    x: i64,
    y: i64,
    rtop: i64,
    rbot: i64,
    row_height: i64,
    vpos: i64,
    fully_visible: bool,
}

#[derive(Clone, Copy)]
struct WindowLineMetrics {
    height: i64,
    vpos: i64,
    ypos: i64,
    offbot: i64,
}

fn snapshot_tab_line_row(snapshot: &WindowDisplaySnapshot) -> Option<&DisplayRowSnapshot> {
    (snapshot.tab_line_height > 0)
        .then(|| snapshot.row_metrics(0))
        .flatten()
}

fn snapshot_header_line_row(snapshot: &WindowDisplaySnapshot) -> Option<&DisplayRowSnapshot> {
    if snapshot.header_line_height <= 0 {
        return None;
    }
    let row = i64::from(snapshot.tab_line_height > 0);
    snapshot.row_metrics(row)
}

fn snapshot_mode_line_row(snapshot: &WindowDisplaySnapshot) -> Option<&DisplayRowSnapshot> {
    if snapshot.mode_line_height <= 0 {
        return None;
    }
    snapshot.rows.iter().max_by_key(|row| row.row)
}

fn snapshot_chrome_line_metrics(
    snapshot: &WindowDisplaySnapshot,
    selector: WindowLineSelector,
) -> Option<WindowLineMetrics> {
    let row = match selector {
        WindowLineSelector::TabLine => snapshot_tab_line_row(snapshot)?,
        WindowLineSelector::HeaderLine => snapshot_header_line_row(snapshot)?,
        WindowLineSelector::ModeLine => snapshot_mode_line_row(snapshot)?,
    };
    Some(match selector {
        WindowLineSelector::TabLine | WindowLineSelector::HeaderLine => WindowLineMetrics {
            height: row.height,
            vpos: 0,
            ypos: 0,
            offbot: 0,
        },
        WindowLineSelector::ModeLine => WindowLineMetrics {
            height: row.height,
            vpos: 0,
            ypos: row.y,
            offbot: 0,
        },
    })
}

fn snapshot_text_rows(snapshot: &WindowDisplaySnapshot) -> Vec<&DisplayRowSnapshot> {
    let top_chrome_rows = snapshot.top_chrome_rows();
    let mode_row = snapshot_mode_line_row(snapshot).map(|row| row.row);
    let mut rows = snapshot
        .rows
        .iter()
        .filter(|row| row.row >= top_chrome_rows && Some(row.row) != mode_row)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.row);
    rows
}

fn snapshot_text_row_line_metrics(
    snapshot: &WindowDisplaySnapshot,
    row: &DisplayRowSnapshot,
) -> WindowLineMetrics {
    let top_chrome_rows = snapshot.top_chrome_rows();
    let top_chrome_height = snapshot.top_chrome_height();
    WindowLineMetrics {
        height: row.height,
        vpos: row.row - top_chrome_rows,
        ypos: row.y - top_chrome_height,
        offbot: 0,
    }
}

fn snapshot_text_line_metrics(
    snapshot: &WindowDisplaySnapshot,
    line_num: i64,
) -> Option<WindowLineMetrics> {
    let rows = snapshot_text_rows(snapshot);
    let idx = if line_num < 0 {
        rows.len() as i64 + line_num
    } else {
        line_num
    };
    if idx < 0 {
        return None;
    }
    rows.get(idx as usize)
        .map(|row| snapshot_text_row_line_metrics(snapshot, row))
}

fn resolve_live_window_display_context(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    window: Option<&Value>,
) -> Result<Option<ApproxWindowDisplayContext>, Flow> {
    let Some((fid, wid)) = resolve_live_window_identity(frames, window)? else {
        return Ok(None);
    };
    live_window_display_context_for(frames, buffers, fid, wid)
}

/// Build the approximate display context for a window that has ALREADY been
/// resolved.
///
/// Callers that decoded their own argument must use this rather than
/// re-deriving an identity from the raw `Value`: `posn-at-x-y` takes a
/// FRAME-OR-WINDOW, and re-resolving its argument through the window-only rule
/// discarded the frame it had already accepted, so `(posn-at-x-y X Y FRAME)`
/// signalled `window-live-p` against a perfectly live frame.  One argument, one
/// decode.
fn live_window_display_context_for(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<Option<ApproxWindowDisplayContext>, Flow> {
    let Some(frame) = frames.get(fid) else {
        return Ok(None);
    };
    let Some(window_ref) = frame.find_window(wid) else {
        return Ok(None);
    };
    let Some(buffer_id) = window_ref.buffer_id() else {
        return Ok(None);
    };
    let Some(buffer) = buffers.get(buffer_id) else {
        return Ok(None);
    };

    let Window::Leaf {
        bounds,
        window_start,
        point,
        ..
    } = window_ref
    else {
        return Ok(None);
    };

    let char_width = frame.char_width.max(1.0).round() as i64;
    let char_height = frame.char_height.max(1.0).round() as i64;
    let body_top = bounds.y.max(0.0) as i64;
    let body_bottom = (bounds.y + bounds.height).max(0.0) as i64
        - if frame.minibuffer_window == Some(wid) {
            0
        } else {
            char_height
        };
    let body_height = (body_bottom - body_top).max(1);
    let body_lines = ((body_height + char_height - 1) / char_height).max(1);
    let body_cols = ((bounds.width.max(1.0) as i64 + char_width - 1) / char_width).max(1);
    let window_point = if frame.selected_window == wid {
        buffer.point_char_pos().to_lisp()
    } else {
        (*point).max(LispCharPos1::ONE)
    };
    let total_chars = buffer.total_char_len().get();
    let text_start = window_start
        .to_one_based_usize()
        .saturating_sub(1)
        .min(total_chars);
    // Every visual row the approximations count consumes at most
    // `wrap_cols + 1` characters (a wrapped segment or a line and its
    // newline), so this many characters from the window start cover every
    // row the window shows with two rows to spare; see
    // `approximate_point_at_coords` for why the spare rows are needed.
    let wrap_cols = body_cols.saturating_sub(1).max(1) as usize;
    let budget = if bounded_window_text_enabled() {
        (body_lines as usize)
            .saturating_add(2)
            .saturating_mul(wrap_cols + 1)
            .saturating_add(1)
    } else {
        usize::MAX
    };
    let text = approx_window_text(buffer, text_start, text_start.saturating_add(budget));
    let last_visible_row_start = nth_line_start_in_buffer(
        buffer,
        text_start,
        body_lines.saturating_sub(1),
        total_chars,
    );

    Ok(Some(ApproxWindowDisplayContext {
        body_height,
        body_lines,
        body_cols,
        char_width,
        char_height,
        window_start: *window_start,
        window_point,
        text,
        total_chars,
        last_visible_row_start,
    }))
}

#[cfg(test)]
thread_local! {
    static REDISPLAY_IDLE_SKIP_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOMACS_REDISPLAY_IDLE_SKIP` on this thread (tests only).
#[cfg(test)]
pub(crate) fn set_redisplay_idle_skip_for_test(enabled: Option<bool>) {
    REDISPLAY_IDLE_SKIP_OVERRIDE.with(|cell| cell.set(enabled));
}

/// `NEOMACS_REDISPLAY_IDLE_SKIP=on` (P3.5 J): `(redisplay t)` also skips the
/// layout when the visible state is unchanged. GNU 31.1 ignores the historical
/// FORCE argument (dispnew.c:Fredisplay); unchanged windows still skip layout.
/// Read once; default on. Explicit off retains forced layout.
pub(crate) fn redisplay_idle_skip_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = REDISPLAY_IDLE_SKIP_OVERRIDE.with(std::cell::Cell::get) {
        return enabled;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("NEOMACS_REDISPLAY_IDLE_SKIP")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            None | Some("on" | "1" | "true" | "yes")
        )
    })
}

#[cfg(test)]
thread_local! {
    static BOUNDED_WINDOW_TEXT_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOMACS_POSN_BOUNDED_TEXT` on this thread (tests only).
#[cfg(test)]
pub(crate) fn set_bounded_window_text_for_test(enabled: Option<bool>) {
    BOUNDED_WINDOW_TEXT_OVERRIDE.with(|cell| cell.set(enabled));
}

/// `NEOMACS_POSN_BOUNDED_TEXT=on` (P3.5 H): the approximate window geometry
/// behind `posn-at-point`, `pos-visible-in-window-p` and `posn-at-x-y` reads
/// only the text the window can show. Off, it reads to the end of the
/// buffer, which is O(Z) per call. Read once; default off.
fn bounded_window_text_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = BOUNDED_WINDOW_TEXT_OVERRIDE.with(std::cell::Cell::get) {
        return enabled;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("NEOMACS_POSN_BOUNDED_TEXT")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("on" | "1" | "true" | "yes")
        )
    })
}

/// [`live_window_display_context_for`] holding the rest of the buffer from
/// the window start: for a coordinate below every row the window shows.
fn live_window_display_context_with_all_text(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<Option<ApproxWindowDisplayContext>, Flow> {
    let Some(mut ctx) = live_window_display_context_for(frames, buffers, fid, wid)? else {
        return Ok(None);
    };
    let Some(buffer) = frames
        .get(fid)
        .and_then(|frame| frame.find_window(wid))
        .and_then(|window| window.buffer_id())
        .and_then(|buffer_id| buffers.get(buffer_id))
    else {
        return Ok(None);
    };
    ctx.text = approx_window_text(buffer, ctx.text.start, ctx.total_chars);
    Ok(Some(ctx))
}

#[cfg(test)]
thread_local! {
    static APPROX_WINDOW_TEXT_COPIED_CHARS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The buffer's characters in `[start, end)` (0-based, clamped to Z).
fn approx_window_text(
    buffer: &crate::buffer::Buffer,
    start: usize,
    end: usize,
) -> ApproxWindowText {
    let total_chars = buffer.total_char_len().get();
    let end = end.min(total_chars);
    let start = start.min(end);
    #[cfg(test)]
    APPROX_WINDOW_TEXT_COPIED_CHARS.with(|count| count.set(count.get() + end - start));
    let byte = |char_pos: usize| {
        buffer.char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(char_pos))
    };
    let chars = buffer
        .buffer_substring_range(crate::buffer::EmacsByteRange::new(byte(start), byte(end)))
        .chars()
        .collect();
    ApproxWindowText {
        start,
        chars,
        reaches_end: end == total_chars,
    }
}

/// The char position past the `lines`-th newline at or after `start`, or Z
/// when the buffer has fewer: `nth_visible_row_start_char` over the whole
/// buffer, scanned in place.
fn nth_line_start_in_buffer(
    buffer: &crate::buffer::Buffer,
    start: usize,
    lines: i64,
    total_chars: usize,
) -> usize {
    let start = start.min(total_chars);
    let Ok(lines) = usize::try_from(lines) else {
        return start;
    };
    if lines == 0 {
        return start;
    }
    let from = buffer.char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(start));
    let limit =
        buffer.char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(total_chars));
    let (past, crossed) = buffer.nth_newline_emacs_byte(from, limit, lines);
    if crossed < lines {
        return total_chars;
    }
    buffer.emacs_byte_pos_to_char_pos_clamped(past).get()
}

fn approx_wrap_cols(ctx: &ApproxWindowDisplayContext) -> i64 {
    // GNU TTY redisplay reserves one column for the continuation glyph on
    // wrapped rows, so an 80-column text area displays 79 source characters
    // before wrapping to the next visual row.
    ctx.body_cols.saturating_sub(1).max(1)
}

fn resolve_live_window_identity(
    frames: &crate::window::FrameManager,
    window: Option<&Value>,
) -> Result<Option<(FrameId, WindowId)>, Flow> {
    let Some(windowish) = window else {
        return Ok(frames
            .selected_frame()
            .map(|frame| (frame.id, frame.selected_window)));
    };
    if windowish.is_nil() {
        return Ok(frames
            .selected_frame()
            .map(|frame| (frame.id, frame.selected_window)));
    }
    let wid = if let Some(id) = windowish.as_window_id() {
        WindowId(id)
    } else if let Some(id) = windowish.as_fixnum().filter(|&id| id >= 0) {
        WindowId(id as u64)
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), *windowish],
        ));
    };
    for fid in frames.frame_list() {
        if frames
            .get(fid)
            .is_some_and(|frame| frame.find_window(wid).is_some())
        {
            return Ok(Some((fid, wid)));
        }
    }
    Ok(None)
}

fn resolve_pos_visible_target_lisp_pos(
    ctx: &ApproxWindowDisplayContext,
    pos: Option<&Value>,
) -> Result<Option<LispCharPos1>, Flow> {
    resolve_target_position_value(pos, ctx.total_chars, ctx.window_point, || {
        ctx.last_visible_row_start
    })
}

/// Decode only the target position. Exact geometry consumers do not need an
/// approximate text copy, and ordinary point/integer queries need no line scan.
fn resolve_target_position_value(
    pos: Option<&Value>,
    total_chars: usize,
    window_point: LispCharPos1,
    last_visible_row_start: impl FnOnce() -> usize,
) -> Result<Option<LispCharPos1>, Flow> {
    let lisp_pos = match pos {
        Some(value) if value.is_t() || value.is_symbol_named("t") => {
            last_visible_row_start().saturating_add(1)
        }
        Some(value) if !value.is_nil() => {
            expect_integer_or_marker(value)?;
            value.as_int().unwrap_or(0).max(1) as usize
        }
        _ => window_point.to_one_based_usize(),
    };
    Ok(Some(LispCharPos1::from_one_based_usize(
        lisp_pos.min(total_chars.saturating_add(1)),
    )))
}

fn resolve_live_target_position(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    fid: FrameId,
    wid: WindowId,
    pos: Option<&Value>,
) -> Result<Option<LispCharPos1>, Flow> {
    let Some(frame) = frames.get(fid) else {
        return Ok(None);
    };
    let Some(window) = frame.find_window(wid) else {
        return Ok(None);
    };
    let Some(buffer) = window.buffer_id().and_then(|id| buffers.get(id)) else {
        return Ok(None);
    };
    let Window::Leaf {
        bounds,
        window_start,
        point,
        ..
    } = window
    else {
        return Ok(None);
    };
    let window_point = if frame.selected_window == wid {
        buffer.point_char_pos().to_lisp()
    } else {
        (*point).max(LispCharPos1::ONE)
    };
    let total_chars = buffer.total_char_len().get();
    resolve_target_position_value(pos, total_chars, window_point, || {
        // Preserve the approximate POS=t convention without materializing text.
        // This scan is lazy: the common point and integer cases are O(1).
        let char_height = frame.char_height.max(1.0).round() as i64;
        let body_top = bounds.y.max(0.0) as i64;
        let body_bottom = (bounds.y + bounds.height).max(0.0) as i64
            - if frame.minibuffer_window == Some(wid) {
                0
            } else {
                char_height
            };
        let body_height = (body_bottom - body_top).max(1);
        let body_lines = ((body_height + char_height - 1) / char_height).max(1);
        let start = window_start
            .to_one_based_usize()
            .saturating_sub(1)
            .min(total_chars);
        nth_line_start_in_buffer(buffer, start, body_lines.saturating_sub(1), total_chars)
    })
}

/// The visual row and column of `lisp_pos` counted from `start_char`.
///
/// When the scan runs past the context's text it has already counted more
/// rows than the window shows (the text covers them all), so the position is
/// reported on the row reached, which every caller treats as not visible.
fn row_col_for_lisp_pos(
    ctx: &ApproxWindowDisplayContext,
    start_char: usize,
    lisp_pos: LispCharPos1,
    wrap_cols: i64,
) -> Option<(i64, i64)> {
    let lisp_pos = usize::try_from(lisp_pos.as_i64().max(1)).ok()?;
    let target = lisp_pos.saturating_sub(1).min(ctx.total_chars);
    let mut row = 0_i64;
    let mut col = 0_i64;
    let wrap_cols = wrap_cols.max(1);
    let mut idx = start_char.min(ctx.total_chars);
    while idx < target {
        let Some(ch) = ctx.text.get(idx) else {
            debug_assert!(row >= ctx.body_lines, "the text covers every visible row");
            return Some((row.max(ctx.body_lines), col));
        };
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
            if col >= wrap_cols && idx + 1 < target {
                row += 1;
                col = 0;
            }
        }
        idx += 1;
    }
    Some((row, col))
}

fn approximate_pos_visible_metrics(
    ctx: &ApproxWindowDisplayContext,
    pos_lisp: LispCharPos1,
) -> Option<ApproxVisibleMetrics> {
    if pos_lisp < ctx.window_start {
        return None;
    }
    let start_char = usize::try_from(ctx.window_start.as_i64().max(1))
        .ok()?
        .saturating_sub(1);
    let (row, col) = row_col_for_lisp_pos(ctx, start_char, pos_lisp, approx_wrap_cols(ctx))?;
    if row < 0 || row >= ctx.body_lines {
        return None;
    }
    let row_metrics = window_row_metrics(ctx, row);
    Some(ApproxVisibleMetrics {
        x: col.saturating_mul(ctx.char_width),
        y: row_metrics.ypos,
        rtop: 0,
        rbot: row_metrics.offbot,
        row_height: row_metrics.height,
        vpos: row_metrics.vpos,
        fully_visible: row_metrics.offbot == 0,
    })
}

fn window_row_metrics(ctx: &ApproxWindowDisplayContext, row: i64) -> WindowLineMetrics {
    let ypos = row.saturating_mul(ctx.char_height);
    let row_bottom = (row + 1).saturating_mul(ctx.char_height);
    let offbot = (row_bottom - ctx.body_height).max(0);
    WindowLineMetrics {
        height: (ctx.char_height - offbot).max(1),
        vpos: row,
        ypos,
        offbot,
    }
}

/// A completed logical visibility query. Missing layout is represented by
/// `None` at the query seam, never by `NotVisible`, so an authoritative negative
/// answer cannot fall through to stale presented geometry or an approximation.
enum PositionVisibility {
    NotVisible,
    Fully {
        x: i64,
        y: i64,
    },
    Partially {
        x: i64,
        y: i64,
        top: i64,
        bottom: i64,
        height: i64,
        row: i64,
    },
}

impl PositionVisibility {
    fn into_lisp(self, partially: bool) -> Value {
        match (self, partially) {
            (Self::NotVisible, _) | (Self::Partially { .. }, false) => Value::NIL,
            (Self::Fully { .. }, false) => Value::T,
            (Self::Fully { x, y }, true) => Value::list(vec![Value::fixnum(x), Value::fixnum(y)]),
            (
                Self::Partially {
                    x,
                    y,
                    top,
                    bottom,
                    height,
                    row,
                },
                true,
            ) => Value::list(
                vec![x, y, top, bottom, height, row]
                    .into_iter()
                    .map(Value::fixnum)
                    .collect(),
            ),
        }
    }
}

fn live_position_visibility(
    eval: &mut super::eval::Context,
    window: Option<&Value>,
    position: Option<&Value>,
) -> Result<Option<PositionVisibility>, Flow> {
    let Some((fid, wid)) = resolve_live_window_identity(&eval.frames, window)? else {
        return Ok(Some(PositionVisibility::NotVisible));
    };
    let frame = eval.frames.get(fid).expect("resolved live frame");
    let Some(Window::Leaf {
        buffer_id,
        window_start,
        point,
        bounds,
        ..
    }) = frame.find_window(wid)
    else {
        return Ok(Some(PositionVisibility::NotVisible));
    };
    let buffer_id = *buffer_id;
    let buffer = eval.buffers.get(buffer_id).expect("live window buffer");
    let start = *window_start;
    let beg = buffer.point_min_lisp_char_pos();
    let end = buffer.point_max_lisp_char_pos();
    let fallback_height = bounds.height.round() as i64;
    let last_row = position.is_some_and(|value| value.is_t());
    let pos = if last_row {
        None
    } else if let Some(value) = position.filter(|value| !value.is_nil()) {
        let raw = super::buffer::expect_integer_or_marker_in_buffers(&eval.buffers, value)?;
        if raw < start.as_i64() || raw < beg.as_i64() || raw > end.as_i64() {
            return Ok(Some(PositionVisibility::NotVisible));
        }
        Some(LispCharPos1::new(raw))
    } else {
        Some(
            if eval
                .frames
                .selected_frame()
                .is_some_and(|selected| selected.selected_window == wid)
            {
                buffer.point_lisp_char_pos()
            } else {
                *point
            },
        )
    };
    if start < beg || start > end || pos.is_some_and(|pos| pos < start || pos < beg || pos > end) {
        return Ok(Some(PositionVisibility::NotVisible));
    }
    let measure = |snapshot: &WindowDisplaySnapshot| {
        let pos = pos.or_else(|| {
            snapshot
                .rows
                .iter()
                .rev()
                .find_map(|row| row.start_buffer_pos)
        });
        let Some(point) = pos.and_then(|pos| snapshot.point_for_buffer_pos(pos)) else {
            return PositionVisibility::NotVisible;
        };
        let (row, y) = snapshot.text_body_position(point.row, point.y);
        let row_height = snapshot
            .row_metrics(point.row)
            .map_or(point.height, |row| row.height);
        let body_height = if snapshot.regions_materialized {
            snapshot.regions.text_body.height.round() as i64
        } else {
            fallback_height - snapshot.top_chrome_height() - snapshot.mode_line_height
        };
        let top = (-y).max(0);
        let bottom = (y + row_height - body_height).max(0);
        let height = (row_height - top - bottom).max(0);
        if height == 0 {
            PositionVisibility::NotVisible
        } else if top == 0 && bottom == 0 {
            PositionVisibility::Fully { x: point.x, y }
        } else {
            PositionVisibility::Partially {
                x: point.x,
                y: y.max(0),
                top,
                bottom,
                height,
                row,
            }
        }
    };
    if let Some(snapshot) = eval.fresh_window_display_snapshot(fid, wid, buffer_id) {
        return Ok(Some(measure(snapshot)));
    }
    let scope = pos.map_or(crate::window::WindowLayoutQueryScope::Viewport, |target| {
        crate::window::WindowLayoutQueryScope::Position { target }
    });
    match eval.query_window_layout_scope(fid, wid, scope) {
        crate::window::WindowLayoutQueryOutcome::Ready(query) => {
            Ok(query.into_geometry().as_ref().map(measure))
        }
        crate::window::WindowLayoutQueryOutcome::Unavailable => Ok(None),
        crate::window::WindowLayoutQueryOutcome::LayoutBusy => Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "Window layout query reentered an active layout callback",
            )],
        )),
        crate::window::WindowLayoutQueryOutcome::Failed(failure) => Err(signal(
            LispCondition::Error,
            vec![Value::string(failure.message())],
        )),
    }
}

#[derive(Clone, Copy)]
struct ExactVisibleMetrics {
    point: LispCharPos1,
    x: i64,
    y: i64,
    dx: i64,
    dy: i64,
    width: i64,
    height: i64,
    row: i64,
    col: i64,
}

fn exact_metrics_from_point(
    point: crate::window::geometry::SnapshotPointGeometry,
) -> ExactVisibleMetrics {
    let body_point = point.in_text_body();
    ExactVisibleMetrics {
        point: point.buffer_pos(),
        x: body_point.x().get().round() as i64,
        y: body_point.y().get().round() as i64,
        dx: 0,
        dy: 0,
        width: (point.width().get().round() as i64).max(1),
        height: (point.height().get().round() as i64).max(1),
        row: point.row(),
        col: point.column(),
    }
}

fn exact_metrics_from_redisplay_point(
    snapshot: &WindowDisplaySnapshot,
    point: &crate::window::DisplayPointSnapshot,
) -> ExactVisibleMetrics {
    let (body_row, body_y) = snapshot.text_body_position(point.row, point.y);
    ExactVisibleMetrics {
        point: point.buffer_pos,
        x: point.x,
        y: body_y,
        dx: 0,
        dy: 0,
        width: point.width.max(1),
        height: point.height.max(1),
        row: body_row,
        col: point.col,
    }
}

/// What `approximate_point_at_coords` found.
enum ApproxPointAtCoords {
    Point(ExactVisibleMetrics),
    /// The coordinates lie below the rows the context's text covers: only the
    /// whole buffer's text answers them (`live_window_display_context_with_all_text`).
    NeedsAllText,
}

fn approximate_point_at_coords(
    ctx: &ApproxWindowDisplayContext,
    x: i64,
    y: i64,
) -> Option<ApproxPointAtCoords> {
    if x < 0 || y < 0 {
        return None;
    }
    let total = ctx.total_chars;
    let start = usize::try_from(ctx.window_start.as_i64().max(1))
        .ok()?
        .saturating_sub(1)
        .min(total);
    let char_width = ctx.char_width.max(1);
    let char_height = ctx.char_height.max(1);
    let query_row = (y / char_height).max(0);
    let query_col = (x / char_width).max(0);
    let wrap_cols = approx_wrap_cols(ctx);

    let mut row = 0_i64;
    let mut line_start = start;
    loop {
        let line_end = match ctx.text.find_newline(line_start) {
            Some(line_end) => line_end,
            None if ctx.text.reaches_end => total,
            None => {
                // This line runs past the text. Its segments that lie wholly
                // inside the text are exact; the text covers two rows more
                // than the window shows, so any row the window shows is one
                // of them.
                let held = ctx.text.end().saturating_sub(line_start);
                let whole_rows = i64::try_from(held).ok()? / wrap_cols;
                if query_row < row + whole_rows {
                    let visual_row = query_row - row;
                    let segment_start = line_start.saturating_add(
                        usize::try_from(visual_row.saturating_mul(wrap_cols)).ok()?,
                    );
                    let chosen_col = query_col.min(wrap_cols);
                    let point = segment_start
                        .saturating_add(usize::try_from(chosen_col).ok()?)
                        .saturating_add(1)
                        .min(total.saturating_add(1));
                    return Some(ApproxPointAtCoords::Point(ExactVisibleMetrics {
                        point: LispCharPos1::from_one_based_usize(point),
                        x,
                        y,
                        dx: x - chosen_col.saturating_mul(char_width),
                        dy: y - query_row.saturating_mul(char_height),
                        width: 0,
                        height: 0,
                        row: query_row,
                        col: query_col,
                    }));
                }
                return Some(ApproxPointAtCoords::NeedsAllText);
            }
        };
        let line_len = i64::try_from(line_end.saturating_sub(line_start)).ok()?;
        let visual_rows = ((line_len + wrap_cols - 1) / wrap_cols).max(1);

        if query_row < row + visual_rows {
            let visual_row = query_row - row;
            let segment_start = line_start
                .saturating_add(usize::try_from(visual_row.saturating_mul(wrap_cols)).ok()?);
            let segment_len = i64::try_from(line_end.saturating_sub(segment_start)).ok()?;
            let chosen_col = query_col.min(segment_len.min(wrap_cols));
            let point = segment_start
                .saturating_add(usize::try_from(chosen_col).ok()?)
                .saturating_add(1)
                .min(total.saturating_add(1));

            return Some(ApproxPointAtCoords::Point(ExactVisibleMetrics {
                point: LispCharPos1::from_one_based_usize(point),
                x,
                y,
                dx: x - chosen_col.saturating_mul(char_width),
                dy: y - query_row.saturating_mul(char_height),
                width: 0,
                height: 0,
                row: query_row,
                col: query_col,
            }));
        }

        if line_end >= total {
            break;
        }
        row += visual_rows;
        line_start = line_end + 1;
    }

    Some(ApproxPointAtCoords::Point(ExactVisibleMetrics {
        point: LispCharPos1::from_one_based_usize(total.saturating_add(1)),
        x,
        y,
        dx: x,
        dy: y - query_row.saturating_mul(char_height),
        width: 0,
        height: 0,
        row: query_row,
        col: query_col,
    }))
}

/// Lisp motion queries use current redisplay rows, recomputed when stale.
/// Native pointer events retain their captured presentation coordinates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PositionGeometrySource {
    Redisplay,
    Presented,
}

/// Result of asking an immutable GUI presentation for one buffer position.
///
/// A live Emacs window and the renderer's active presentation are updated on
/// different clocks. `Unavailable` therefore means the core window is newer
/// than the active presentation, not that Lisp supplied an invalid window.
/// Keeping that state distinct from `NotVisible` and invalid geometry prevents
/// normal presentation lag from escaping through `pos-visible-in-window-p` as
/// a Lisp error.
enum PresentedBufferPosition {
    Visible(crate::window::geometry::SnapshotPointGeometry),
    NotVisible,
    Unavailable,
}

fn resolve_presented_buffer_position(
    publication: &crate::window::geometry::PresentationGeometry,
    window: WindowId,
    position: LispCharPos1,
) -> Result<PresentedBufferPosition, crate::window::geometry::GeometryQueryError> {
    use crate::window::geometry::GeometryQueryError;

    match publication.resolve(crate::window::geometry::BufferPositionQuery::new(
        publication.presentation(),
        window,
        position,
    )) {
        Ok(point) => Ok(PresentedBufferPosition::Visible(point)),
        Err(GeometryQueryError::PositionNotVisible { .. }) => {
            Ok(PresentedBufferPosition::NotVisible)
        }
        Err(
            GeometryQueryError::NotYetActive { .. }
            | GeometryQueryError::StalePresentation { .. }
            | GeometryQueryError::MissingWindow(_)
            | GeometryQueryError::MissingMaterializedGeometry(_),
        ) => Ok(PresentedBufferPosition::Unavailable),
        Err(
            error @ (GeometryQueryError::MissingRegion { .. }
            | GeometryQueryError::CoordinateNotVisible { .. }
            | GeometryQueryError::VisualAnchorUnavailable(_)
            | GeometryQueryError::InvalidGeometry(_)),
        ) => Err(error),
    }
}

/// Resolve exact `posn` geometry, recomputing the window when redisplay has
/// left nothing behind.
///
/// GNU never faces this question: `Fposn_at_point` goes through
/// `Fpos_visible_in_window_p` -> `pos_visible_p`, which runs `start_display`
/// from `w->start` and `move_it_to` on *every* call (src/xdisp.c:1772-1774),
/// and `buffer_posn_from_coords` does the same (src/dispnew.c:6277-6286). The
/// only glyph-matrix read in that whole path fills in the WIDTH/HEIGHT cell of
/// the posn and is guarded — `if (it_vpos < w->current_matrix->nrows &&
/// row->enabled_p) ... else { *width = *height = 0; }` — so a window that has
/// never been displayed costs GNU one cell of a ten-element list, measured:
/// cold, `emacs -nw` answers `(83 (20 . 0) 0 nil 83 (20 . 0) nil (0 . 0)
/// (0 . 0))` where warm it answers `... (1 . 0)`.
///
/// This port serves the same query from the retained redisplay snapshot, which
/// is the same rows and cheaper, so it stays the preferred source. When there
/// is none, ask the frontend to run the canonical row producer for this one
/// window rather than answering nil — that is the same seam `(window-end
/// WINDOW t)` already uses, and its rule is the rule here: there is no second
/// approximation algorithm.
fn resolve_exact_visible_metrics_with_layout(
    eval: &mut super::eval::Context,
    window: Option<&Value>,
    pos: Option<&Value>,
) -> Result<Option<(WindowId, ExactVisibleMetrics)>, Flow> {
    let Some((fid, wid)) = resolve_live_window_identity(&eval.frames, window)? else {
        return Ok(None);
    };
    // Prefer retained rows only while they are still VALID for the live
    // window.  Their columns are window-relative and therefore meaningless
    // without the horizontal origin they were produced at; see the freshness
    // note in `compute_live_window_geometry`.
    let retained_rows_valid = retained_rows_answer_for_live_window(eval, window)?;
    if retained_rows_valid
        && let Some(found) = resolve_exact_visible_metrics(
            &eval.frames,
            &eval.buffers,
            window,
            pos,
            PositionGeometrySource::Redisplay,
        )?
    {
        return Ok(Some(found));
    }
    if let Some(geometry) = compute_live_window_geometry(eval, fid, wid)? {
        let Some(pos_lisp) =
            resolve_live_target_position(&eval.frames, &eval.buffers, fid, wid, pos)?
        else {
            return Ok(None);
        };
        return Ok(geometry
            .point_for_buffer_pos(pos_lisp)
            .map(|point| (wid, exact_metrics_from_redisplay_point(&geometry, &point))));
    }
    if retained_rows_valid {
        return Ok(None);
    }
    // No recompute was available.  Rows that are merely STALE are still the
    // closest thing to GNU's unconditional re-walk that exists here, so they
    // answer rather than nothing -- which is exactly what they did before the
    // preference above, so the freshness gate can only replace a stale answer
    // with a recomputed one and never with silence.
    resolve_exact_visible_metrics(
        &eval.frames,
        &eval.buffers,
        window,
        pos,
        PositionGeometrySource::Presented,
    )
}

/// Whether the retained row map for WINDOW still describes the live window.
///
/// This is `vertical-motion`'s predicate, not a second one:
/// `Context::fresh_window_display_snapshot` compares the whole
/// [`crate::window::WindowDisplaySnapshotFreshness`] token, whose fields are
/// deliberately opaque so that "individual snapshot consumers [cannot] invent
/// partial freshness checks that drift apart". This includes graphical
/// windows: a renderer acknowledgement does not make old scroll coordinates
/// suitable for a new Lisp motion query.
fn retained_rows_answer_for_live_window(
    eval: &super::eval::Context,
    window: Option<&Value>,
) -> Result<bool, Flow> {
    let Some((fid, wid)) = resolve_live_window_identity(&eval.frames, window)? else {
        return Ok(true);
    };
    let Some(frame) = eval.frames.get(fid) else {
        return Ok(true);
    };
    let Some(buffer_id) = frame.find_window(wid).and_then(|window| window.buffer_id()) else {
        return Ok(true);
    };
    Ok(eval
        .fresh_window_display_snapshot(fid, wid, buffer_id)
        .is_some())
}

fn resolve_exact_visible_metrics(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    window: Option<&Value>,
    pos: Option<&Value>,
    source: PositionGeometrySource,
) -> Result<Option<(WindowId, ExactVisibleMetrics)>, Flow> {
    let Some((fid, wid)) = resolve_live_window_identity(frames, window)? else {
        return Ok(None);
    };
    let Some(frame) = frames.get(fid) else {
        return Ok(None);
    };
    let Some(pos_lisp) = resolve_live_target_position(frames, buffers, fid, wid, pos)? else {
        return Ok(None);
    };
    if frame.effective_window_system().is_none() {
        let Some(snapshot) = frame.redisplay_snapshot(wid) else {
            return Ok(None);
        };
        return Ok(snapshot
            .point_for_buffer_pos(pos_lisp)
            .map(|point| (wid, exact_metrics_from_redisplay_point(snapshot, &point))));
    }
    let publication = match source {
        PositionGeometrySource::Redisplay => frame.completed_presentation_geometry(),
        PositionGeometrySource::Presented => frame.active_presentation_geometry(),
    };
    let Some(publication) = publication else {
        return Ok(None);
    };
    let point = match resolve_presented_buffer_position(publication, wid, pos_lisp)
        .map_err(geometry_query_flow)?
    {
        PresentedBufferPosition::Visible(point) => point,
        PresentedBufferPosition::NotVisible | PresentedBufferPosition::Unavailable => {
            return Ok(None);
        }
    };
    Ok(Some((wid, exact_metrics_from_point(point))))
}

fn geometry_query_flow(error: crate::window::geometry::GeometryQueryError) -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(format!(
            "Invalid presented geometry query: {error:?}"
        ))],
    )
}

fn make_text_area_position(window_id: WindowId, metrics: ExactVisibleMetrics) -> Value {
    Value::list(vec![
        Value::make_window(window_id.0),
        Value::fixnum(metrics.point.as_i64()),
        Value::cons(Value::fixnum(metrics.x), Value::fixnum(metrics.y)),
        Value::fixnum(0),
        Value::NIL,
        Value::fixnum(metrics.point.as_i64()),
        Value::cons(Value::fixnum(metrics.col), Value::fixnum(metrics.row)),
        Value::NIL,
        Value::cons(Value::fixnum(metrics.dx), Value::fixnum(metrics.dy)),
        Value::cons(Value::fixnum(metrics.width), Value::fixnum(metrics.height)),
    ])
}

fn validate_posn_pixel_coordinate(value: Value) -> Result<i64, Flow> {
    let coordinate = match value.kind() {
        ValueKind::Fixnum(v) => v,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), value],
            ));
        }
    };
    if coordinate != -1 && coordinate < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), value],
        ));
    }
    Ok(coordinate)
}

// ---------------------------------------------------------------------------
// Redisplay fontification
// ---------------------------------------------------------------------------

fn get_fontified_property(ctx: &mut super::eval::Context, pos: i64) -> EvalResult {
    super::textprop::builtin_get_char_property(
        ctx,
        vec![Value::fixnum(pos), Value::symbol("fontified")],
    )
}

fn next_fontified_property_change(
    ctx: &mut super::eval::Context,
    pos: i64,
    limit: i64,
) -> EvalResult {
    super::textprop::builtin_next_single_property_change(
        ctx,
        vec![
            Value::fixnum(pos),
            Value::symbol("fontified"),
            Value::NIL,
            Value::fixnum(limit),
        ],
    )
}

fn call_fontification_functions_at(ctx: &mut super::eval::Context, hook_value: Value, pos: i64) {
    let hook_sym = intern("fontification-functions");
    let functions = hook_runtime::collect_hook_functions_in_state(ctx, hook_sym, hook_value, true);
    if functions.is_empty() {
        return;
    }

    let roots = ctx.save_specpdl_roots();
    ctx.push_specpdl_root(hook_value);
    for function in functions.iter().copied() {
        ctx.push_specpdl_root(function);
    }
    let arg = Value::fixnum(pos);
    ctx.push_specpdl_root(arg);

    let binding_count = ctx.specpdl.len();
    if let Err(flow) = ctx.try_specbind_or_unwind_to(binding_count, hook_sym, Value::NIL) {
        let rendered = super::error::format_flow_with_eval(ctx, &flow);
        tracing::warn!(
            "error binding redisplay fontification hook at {}: {}",
            pos,
            rendered
        );
        ctx.restore_specpdl_roots(roots);
        return;
    }

    // GNU `handle_fontified_prop` calls each function with `dsafe_call1`,
    // which binds `inhibit-redisplay` and logs ordinary errors without
    // aborting the redisplay pass.  Do the same per function so one broken
    // hook cannot prevent later hooks from running.
    for function in functions {
        let call_count = ctx.specpdl.len();
        if let Err(flow) =
            ctx.try_specbind_or_unwind_to(call_count, intern("inhibit-redisplay"), Value::T)
        {
            let rendered = super::error::format_flow_with_eval(ctx, &flow);
            tracing::warn!(
                "error binding redisplay inhibition at {}: {}",
                pos,
                rendered
            );
            continue;
        }
        let result = ctx.apply(function, vec![arg]);
        let result = ctx.unbind_to_with_result(call_count, result);
        if let Err(flow) = result {
            let rendered = super::error::format_flow_with_eval(ctx, &flow);
            tracing::warn!(
                "error during redisplay fontification at {}: {}",
                pos,
                rendered
            );
        }
    }

    if let Err(flow) = ctx.unbind_to_with_result(binding_count, Ok(Value::NIL)) {
        let rendered = super::error::format_flow_with_eval(ctx, &flow);
        tracing::warn!(
            "error restoring redisplay fontification bindings at {}: {}",
            pos,
            rendered
        );
    }
    ctx.restore_specpdl_roots(roots);
}

/// Whether a redisplay fontification request changed an unfontified position
/// into a fontified one.
///
/// GNU's `handle_fontified_prop` recomputes iterator properties only for the
/// second state.  Keeping that distinction typed lets the immutable layout
/// engine retry after a successful callback without looping forever when a
/// fontification function declines to mark the requested position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedisplayFontificationOutcome {
    #[default]
    Unchanged,
    Fontified,
}

impl RedisplayFontificationOutcome {
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Fontified, _) | (_, Self::Fontified) => Self::Fontified,
            (Self::Unchanged, Self::Unchanged) => Self::Unchanged,
        }
    }

    pub const fn requires_layout_retry(self) -> bool {
        matches!(self, Self::Fontified)
    }
}

/// Fontify a visible buffer region the same way GNU redisplay does from
/// `handle_fontified_prop`.
///
/// The layout engine's window parameters use 0-based character positions.
/// Lisp hooks receive GNU buffer positions, so this function performs the
/// single conversion at the redisplay boundary and keeps the rest of the walk
/// in Lisp character coordinates.
pub fn ensure_fontified_for_redisplay(
    ctx: &mut super::eval::Context,
    buf_id: BufferId,
    from_char: i64,
    to_char: i64,
) -> Result<RedisplayFontificationOutcome, Flow> {
    let Some((point_min, point_max)) = ctx.buffers.get(buf_id).map(|buffer| {
        (
            buffer.point_min_lisp_char_pos().as_i64(),
            buffer.point_max_lisp_char_pos().as_i64(),
        )
    }) else {
        return Ok(RedisplayFontificationOutcome::Unchanged);
    };

    let start = from_char.saturating_add(1).clamp(point_min, point_max);
    let end = to_char.saturating_add(1).clamp(start, point_max);
    if start >= end {
        return Ok(RedisplayFontificationOutcome::Unchanged);
    }

    let saved_current = ctx.buffers.current_buffer_id();
    if saved_current != Some(buf_id) {
        ctx.set_current_buffer_unrecorded(buf_id)?;
    }

    let result = (|| -> Result<RedisplayFontificationOutcome, Flow> {
        if ctx
            .eval_symbol("memory-full")
            .unwrap_or(Value::NIL)
            .is_truthy()
        {
            return Ok(RedisplayFontificationOutcome::Unchanged);
        }

        let hook_sym = intern("fontification-functions");
        let hook_value = hook_runtime::hook_value_by_id(ctx, hook_sym).unwrap_or(Value::NIL);
        if hook_value.is_nil() {
            return Ok(RedisplayFontificationOutcome::Unchanged);
        }

        let mut pos = start;
        let mut iterations = 0usize;
        let mut outcome = RedisplayFontificationOutcome::Unchanged;
        let max_iterations = (end - start).max(1) as usize * 2;

        while pos < end && pos < point_max {
            iterations += 1;
            if iterations > max_iterations {
                tracing::warn!(
                    "redisplay fontification did not converge for buffer {:?}, range {}..{}",
                    buf_id,
                    start,
                    end
                );
                break;
            }

            let before = get_fontified_property(ctx, pos)?;
            if before.is_nil() {
                call_fontification_functions_at(ctx, hook_value, pos);
                if ctx.buffers.current_buffer_id() != Some(buf_id) {
                    ctx.restore_current_buffer_if_live(buf_id);
                }
                if ctx.buffers.current_buffer_id() != Some(buf_id) {
                    break;
                }

                // GNU recomputes properties only if the hook actually
                // marked the current character fontified.  If it did not,
                // advance one character to avoid looping forever on the
                // same unfontified position.
                let after = get_fontified_property(ctx, pos)?;
                if after.is_nil() {
                    pos += 1;
                    continue;
                }
                outcome = RedisplayFontificationOutcome::Fontified;
            }

            let next = next_fontified_property_change(ctx, pos, end)?;
            let Some(next_pos) = next.as_int() else {
                break;
            };
            if next_pos <= pos {
                pos += 1;
            } else {
                pos = next_pos.min(end);
            }
        }

        Ok(outcome)
    })();

    if let Some(saved) = saved_current {
        ctx.restore_current_buffer_if_live(saved);
    }

    result
}

fn resolve_posn_at_xy_window(
    frames: &crate::window::FrameManager,
    frame_or_window: Option<&Value>,
) -> Result<Option<(FrameId, WindowId, bool)>, Flow> {
    let Some(frameish) = frame_or_window else {
        return Ok(frames
            .selected_frame()
            .map(|frame| (frame.id, frame.selected_window, true)));
    };
    if frameish.is_nil() {
        return Ok(frames
            .selected_frame()
            .map(|frame| (frame.id, frame.selected_window, true)));
    }
    // GNU dispatches on WINDOWP, not on "is it a frame"
    // (`Fposn_at_x_y`, src/keyboard.c):
    //
    //     if (WINDOWP (frame_or_window))
    //       { struct window *w = decode_live_window (frame_or_window); ... }
    //     CHECK_LIVE_FRAME (frame_or_window);
    //
    // Those two guards agree only for values that are a window or a frame.
    // Anything that is NEITHER -- a symbol, a buffer, a window that has been
    // deleted -- is "not a frame", so guarding on that sent it into the window
    // decoder and it came back reporting `window-live-p` (or `framep`) where
    // GNU reports `frame-live-p`.  An internal window slipped through with no
    // signal at all, which is worse: the caller reads a missing position
    // rather than a type error.
    if frameish.is_window() {
        // WINDOWP: from here GNU runs `decode_live_window`, which is
        // `CHECK_LIVE_WINDOW` -- an internal or deleted window is rejected.
        // `resolve_live_window_identity` cannot stand in for that on its own:
        // it resolves through `find_window`, which matches ANY node of the
        // window tree, so an internal window resolved happily and only turned
        // into nil further down where a leaf was required.  A caller then
        // could not tell a type error from "no position at those coordinates".
        let live = frameish
            .as_window_id()
            .map(crate::window::WindowId)
            .and_then(|wid| frames.find_window_frame_id(wid).map(|fid| (fid, wid)));
        let Some((fid, wid)) = live else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), *frameish],
            ));
        };
        return Ok(Some((fid, wid, true)));
    }
    let fid = if let Some(id) = frameish.as_frame_id() {
        FrameId(id)
    } else if let Some(id) = frameish.as_fixnum().filter(|&id| id >= 0) {
        FrameId(id as u64)
    } else {
        // CHECK_LIVE_FRAME: everything that is not a window lands here.
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frameish],
        ));
    };
    let Some(frame) = frames.get(fid) else {
        return Ok(None);
    };
    Ok(Some((fid, frame.selected_window, false)))
}

/// `(posn-at-point &optional POS WINDOW)` evaluator-backed variant.
pub(crate) fn builtin_posn_at_point(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("posn-at-point", &args, 0, 2)?;
    validate_optional_window_designator_in_state(
        &eval.frames,
        args.get(1),
        crate::emacs_core::window_cmds::WindowDomain::Live,
    )?;
    let Some((window_id, metrics)) =
        resolve_exact_visible_metrics_with_layout(eval, args.get(1), args.first())?
    else {
        return Ok(Value::NIL);
    };
    Ok(make_text_area_position(window_id, metrics))
}

/// `(posn-at-x-y X Y &optional FRAME-OR-WINDOW WHOLE)` evaluator-backed variant.
pub(crate) fn builtin_posn_at_x_y(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    // GNU's `make_lispy_position` reaches `buffer_posn_from_coords`, which runs
    // the same on-demand walk from `w->start` that `pos_visible_p` does. Give
    // this the same source as `posn-at-point` so the two cannot disagree about
    // a window redisplay has not drawn yet.
    // `posn-at-x-y` takes a FRAME-OR-WINDOW, so it resolves the target through
    // its own designator rule rather than the window-only one.
    let (computed, source) = match resolve_posn_at_xy_window(&eval.frames, args.get(2))? {
        Some((fid, wid, _)) => {
            let computed = compute_live_window_geometry(eval, fid, wid)?;
            let fresh =
                retained_rows_answer_for_live_window(eval, Some(&Value::make_window(wid.0)))?;
            let source = if computed.is_some() || fresh {
                PositionGeometrySource::Redisplay
            } else {
                PositionGeometrySource::Presented
            };
            (computed, source)
        }
        None => (None, PositionGeometrySource::Presented),
    };
    posn_at_x_y_impl(
        &mut eval.frames,
        &mut eval.buffers,
        computed.as_ref(),
        source,
        args,
    )
}

/// Run the canonical row producer when the live window has no current rows.
/// This does not publish or activate a presentation. `None` means a fresh
/// redisplay snapshot can answer, the frame is not interactive, or no frontend
/// adapter is installed.
fn compute_live_window_geometry(
    eval: &mut super::eval::Context,
    fid: FrameId,
    wid: WindowId,
) -> Result<Option<WindowDisplaySnapshot>, Flow> {
    let Some(frame) = eval.frames.get(fid) else {
        return Ok(None);
    };
    // GNU's coordinate queries walk from the live window start even on a
    // graphical frame. Using the renderer's older presentation during queued
    // pixel-scroll commands makes point correction undo part of the gesture.
    if frame.initial || eval.noninteractive() {
        return Ok(None);
    }
    // A populated snapshot that is still FRESH already answered (or correctly
    // said "not visible"); recomputing would only re-derive the same rows.
    //
    // The predicate used to be EMPTINESS, and emptiness is not validity.  A
    // retained row map is expressed in WINDOW-relative columns, so its columns
    // mean nothing without the horizontal origin they were produced at --
    // GNU's `it->first_visible_x`, which `init_iterator` takes from
    // `w->hscroll` (src/xdisp.c:3500).  `set-window-hscroll` after a redisplay
    // therefore leaves a populated snapshot whose origin is no longer the
    // window's, and every coordinate query kept answering from it: measured,
    // GNU 31.0.90 vs this port, 80x24 pty, `truncate-lines' t, a line starting
    // at 202, hscroll set to 100 after a redisplay that auto-hscrolled to 8 --
    // `posn-at-x-y' column 0 answered 210 here where GNU answers 302
    // (`scripts/l216-hscroll-origin-probe.el', PART E).
    //
    // `vertical-motion' had the right predicate all along
    // (`Context::fresh_window_display_snapshot', whose token carries
    // `WindowLayoutInputState::hscroll`), and declined the same snapshot in the
    // same breath; this is the second consumer of one model being taught the
    // first one's validity rule rather than inventing a partial check of its
    // own, which `WindowDisplaySnapshotFreshness`'s own doc forbids.
    //
    // GNU never faces the question because `buffer_posn_from_coords` and
    // `pos_visible_p` re-run the iterator on EVERY call (src/dispnew.c:6277-6286,
    // src/xdisp.c:1772-1774).  Recomputing when the rows are invalid is that
    // behaviour expressed as a cache.
    let buffer_id = frame.find_window(wid).and_then(|window| window.buffer_id());
    if buffer_id
        .and_then(|buffer_id| eval.fresh_window_display_snapshot(fid, wid, buffer_id))
        .is_some_and(|snapshot| snapshot.has_points())
    {
        return Ok(None);
    }
    match eval.query_window_layout(fid, wid) {
        crate::window::WindowLayoutQueryOutcome::Ready(query) => Ok(query.into_geometry()),
        crate::window::WindowLayoutQueryOutcome::Unavailable => Ok(None),
        crate::window::WindowLayoutQueryOutcome::LayoutBusy => Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "Window layout query reentered an active layout callback",
            )],
        )),
        crate::window::WindowLayoutQueryOutcome::Failed(failure) => Err(signal(
            LispCondition::Error,
            vec![Value::string(failure.message())],
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PresentedFrameCoordinate {
    Content(neomacs_display_protocol::PresentedFramePoint),
    Expose,
}

fn protocol_geometry_query_error(
    _error: neomacs_display_protocol::GeometryError,
) -> crate::window::geometry::GeometryQueryError {
    crate::window::geometry::GeometryQueryError::InvalidGeometry(
        crate::window::geometry::GeometryError::NonFiniteCoordinate,
    )
}

fn map_live_frame_coordinate_to_presentation(
    frame: &crate::window::Frame,
    publication: &crate::window::geometry::PresentationGeometry,
    x: i64,
    y: i64,
) -> Result<PresentedFrameCoordinate, crate::window::geometry::GeometryQueryError> {
    use neomacs_display_protocol::{
        DeviceScale, GeometryPoint, LogicalPixels, PresentMapping, PresentationExtent,
        RootSurfaceSpace, SurfaceState,
    };

    let raw_point = neomacs_display_protocol::PresentedFramePoint::from_px(x as f32, y as f32)
        .map_err(protocol_geometry_query_error)?;

    // GNU accepts -1 as a sentinel coordinate for an overflowing R2L newline.
    // It belongs to the glyph query convention, not the native surface, so do
    // not classify it as expose.
    if x < 0 || y < 0 {
        return Ok(PresentedFrameCoordinate::Content(raw_point));
    }

    // Older compatibility fixtures have no published frame extent.  Preserve
    // their historical query behavior while every live publication uses the
    // explicit placement constructor and therefore takes the mapping path.
    let Some(content_size) = publication.content_extent() else {
        return Ok(PresentedFrameCoordinate::Content(raw_point));
    };
    let surface = SurfaceState::from_device_size(
        frame.width,
        frame.height,
        DeviceScale::new(1.0).expect("unit device scale is valid"),
    )
    .map_err(protocol_geometry_query_error)?;
    let SurfaceState::Drawable(surface) = surface else {
        return Ok(PresentedFrameCoordinate::Expose);
    };
    let content = PresentationExtent::new(
        neomacs_display_protocol::PresentationId::new(publication.presentation().get()),
        content_size,
    );
    let mapping = PresentMapping::top_left_clip(surface, content);
    let surface_point =
        GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(x as f32, y as f32)
            .map_err(protocol_geometry_query_error)?;
    Ok(match mapping.frame_from_surface(surface_point) {
        Some(point) => PresentedFrameCoordinate::Content(point),
        None => PresentedFrameCoordinate::Expose,
    })
}

/// The click a text-area `posn` is reported for, as distinct from the position
/// the walk resolved it to.
///
/// GNU fills the two coordinate cells of a text-area posn from two different
/// places, and neither is the resolved glyph's own origin:
///
/// * the `(X . Y)` cell is the CLICK, verbatim -- `make_lispy_position` sets
///   `xret = mx - window_box_left (w, TEXT_AREA)` and `yret = wy -
///   WINDOW_TAB_LINE_HEIGHT (w) - WINDOW_HEADER_LINE_HEIGHT (w)`
///   (src/keyboard.c:5882-5883) before any position lookup happens. It matters
///   because `posn-col-row` is DERIVED from it by dividing out the frame's
///   character cell (lisp/subr.el:2053-2090), so this cell is what a caller
///   asking "which screen row did I click" actually reads.
/// * the `(COL . ROW)` cell is the iterator's `it.hpos`/`it.vpos`
///   (src/dispnew.c:6432-6433), after GNU's "Add extra (default width) columns
///   if clicked after EOL": `x1 = max (0, it.current_x + it.pixel_width); if
///   (to_x > x1) it.hpos += (to_x - x1) / WINDOW_FRAME_COLUMN_WIDTH (w)`
///   (src/dispnew.c:6428-6430).
///
/// Answering both from the resolved position is right only while the click
/// lands on a glyph. Past the end of a line -- which is every click in the
/// empty area under a short buffer -- GNU keeps counting columns and this port
/// used to report the last glyph's. Measured, GNU Emacs 31.0.90, 80x24 pty,
/// `"abcdef\nghijkl\n"`: column 40 of row 0 answers `(7 (40 . 0) (40 . 0))`
/// where this port answered `(7 (6 . 0) (6 . 0))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextAreaClick {
    /// X relative to the text area's left edge, in the frame's pixel units.
    x: i64,
    /// Y relative to the top of the text area, i.e. below any tab or header
    /// line, in the frame's pixel units.
    y: i64,
    /// GNU's `WINDOW_FRAME_COLUMN_WIDTH`, the unit the after-EOL column count
    /// is measured in. One on a terminal frame, where a "pixel" is a column.
    column_width: i64,
}

impl TextAreaClick {
    fn new(x: i64, y: i64, column_width: i64) -> Self {
        Self {
            x,
            y,
            column_width: column_width.max(1),
        }
    }

    /// Rewrite the two coordinate cells of a resolved posn the way
    /// `make_lispy_position` and `buffer_posn_from_coords` fill them.
    ///
    /// The column comes from [`crate::window::DisplayPointSnapshot::column_for_click`],
    /// which is also what the mouse-event path uses, so the two cannot answer
    /// GNU's one rule differently.
    fn apply(
        self,
        metrics: ExactVisibleMetrics,
        point: &crate::window::DisplayPointSnapshot,
    ) -> ExactVisibleMetrics {
        ExactVisibleMetrics {
            x: self.x,
            y: self.y,
            col: point.column_for_click(self.x, self.column_width),
            ..metrics
        }
    }

    /// Rewrite a posn this port derived without a display point of its own --
    /// the approximate scanner ledger 201 named as residual 4. Same two cells
    /// and the same rule, with the metrics' own column standing in for the
    /// point's.
    fn apply_to_metrics(self, metrics: ExactVisibleMetrics) -> ExactVisibleMetrics {
        let past_end = self.x.saturating_sub(metrics.x).max(0);
        ExactVisibleMetrics {
            x: self.x,
            y: self.y,
            col: metrics.col.saturating_add(past_end / self.column_width),
            ..metrics
        }
    }
}

/// Build the posn `make_lispy_position` returns for a window part that is not
/// the text area but still carries a buffer position -- the fringes, the
/// margins, the vertical border, the scroll bars and the dividers.
///
/// GNU reaches every one of these through the same `if (!textpos)` block that
/// serves the text area (src/keyboard.c:5975-6000): `posn` is already the
/// part's symbol, so it is not overwritten by the position, but `textpos` is
/// filled from `buffer_posn_from_coords` all the same and lands in the posn's
/// sixth slot. `posn-point` therefore answers a buffer position for a click on
/// a fringe, and `posn-area` answers the fringe.
fn make_window_part_position(
    window_id: WindowId,
    part: crate::window::WindowPart,
    metrics: ExactVisibleMetrics,
) -> Value {
    let Some(area) = part.area_symbol() else {
        return make_text_area_position(window_id, metrics);
    };
    // "For fringes ... X is meaningless": GNU presets `col = 0` for both
    // fringes (src/keyboard.c:5928 and 5937) instead of taking the walk's
    // column.
    let col = match part {
        crate::window::WindowPart::LeftFringe | crate::window::WindowPart::RightFringe => 0,
        _ => metrics.col,
    };
    Value::list(vec![
        Value::make_window(window_id.0),
        Value::symbol(area),
        Value::cons(Value::fixnum(metrics.x), Value::fixnum(metrics.y)),
        Value::fixnum(0),
        Value::NIL,
        Value::fixnum(metrics.point.as_i64()),
        Value::cons(Value::fixnum(col), Value::fixnum(metrics.row)),
        Value::NIL,
        Value::cons(Value::fixnum(metrics.dx), Value::fixnum(metrics.dy)),
        Value::cons(Value::fixnum(metrics.width), Value::fixnum(metrics.height)),
    ])
}

/// Build the posn `make_lispy_position` returns for a click on a tab, header or
/// mode line (src/keyboard.c:5888-5905).
///
/// `textpos = -1` there, so the sixth slot is nil and `posn-point` answers
/// nothing: a chrome line owns no buffer position. The reported `(X . Y)` is
/// the WINDOW-relative click, not the text-area-relative one the text branch
/// reports.
fn make_chrome_line_position(
    window_id: WindowId,
    line: crate::window::WindowChromeLine,
    window_x: i64,
    window_y: i64,
    hit: crate::window::ChromeLineHit,
) -> Value {
    Value::list(vec![
        Value::make_window(window_id.0),
        Value::symbol(
            line.part()
                .area_symbol()
                .expect("a chrome line always names an area"),
        ),
        Value::cons(Value::fixnum(window_x), Value::fixnum(window_y)),
        Value::fixnum(0),
        // GNU fills this with `(STRING . CHARPOS)` when the glyph under the
        // click carries a displayed string object; this port's chrome rows
        // publish their extent rather than their individual glyphs, so the
        // slot is nil. Named in ledger 209's residuals.
        Value::NIL,
        Value::NIL,
        Value::cons(Value::fixnum(hit.col), Value::fixnum(hit.row)),
        Value::NIL,
        Value::cons(Value::fixnum(hit.dx), Value::fixnum(hit.dy)),
        Value::cons(Value::fixnum(hit.width), Value::fixnum(hit.height)),
    ])
}

/// GNU's frame branch (src/keyboard.c:6059-6075): no window of the frame owns
/// the coordinate, so the posn names the FRAME and carries the click and
/// nothing else.
///
/// The list is four elements long, which is what makes `posn-actual-col-row`
/// nil (it is `(nth 6 ...)`, lisp/subr.el:2103-2116) while `posn-col-row`
/// still answers, because that one is derived from `posn-x-y`.
fn make_frame_position(frame_id: FrameId, x: i64, y: i64) -> Value {
    Value::list(vec![
        Value::make_frame(frame_id.0),
        Value::NIL,
        Value::cons(Value::fixnum(x), Value::fixnum(y)),
        Value::fixnum(0),
    ])
}

fn posn_at_x_y_impl(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    computed: Option<&WindowDisplaySnapshot>,
    source: PositionGeometrySource,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("posn-at-x-y", &args, 2, 4)?;
    let x = validate_posn_pixel_coordinate(*args.first().unwrap())?;
    let y = validate_posn_pixel_coordinate(*args.get(1).unwrap())?;
    let whole = args.get(3).is_some_and(|v| v.is_truthy());
    let Some((fid, wid, window_relative_input)) = resolve_posn_at_xy_window(frames, args.get(2))?
    else {
        return Ok(Value::NIL);
    };
    let Some(frame) = frames.get(fid) else {
        return Ok(Value::NIL);
    };
    let Some(window_ref) = frame.find_window(wid) else {
        return Ok(Value::NIL);
    };

    if computed.is_none() && frame.effective_window_system().is_some() {
        let publication = match source {
            PositionGeometrySource::Redisplay => frame.completed_presentation_geometry(),
            PositionGeometrySource::Presented => frame.active_presentation_geometry(),
        };
        let publication = publication.ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("GUI frame has no presented geometry")],
            )
        })?;
        let query = if window_relative_input {
            if whole {
                crate::window::geometry::WindowCoordinateQuery::in_whole_window(
                    publication.presentation(),
                    wid,
                    x,
                    y,
                )
            } else {
                crate::window::geometry::WindowCoordinateQuery::in_text_body(
                    publication.presentation(),
                    wid,
                    x,
                    y,
                )
            }
        } else {
            let point = match map_live_frame_coordinate_to_presentation(frame, publication, x, y)
                .map_err(geometry_query_flow)?
            {
                PresentedFrameCoordinate::Content(point) => point,
                PresentedFrameCoordinate::Expose => return Ok(Value::NIL),
            };
            crate::window::geometry::WindowCoordinateQuery::in_frame(
                publication.presentation(),
                wid,
                point,
            )
        };
        return match publication.resolve(query) {
            Ok(point) => Ok(make_text_area_position(
                wid,
                exact_metrics_from_point(point),
            )),
            Err(crate::window::geometry::GeometryQueryError::CoordinateNotVisible { .. }) => {
                Ok(Value::NIL)
            }
            Err(error) => Err(geometry_query_flow(error)),
        };
    }

    // GNU `Fposn_at_x_y` does no geometry of its own: it converts a WINDOW
    // argument into FRAME pixels and hands them to `make_lispy_position`
    // (src/keyboard.c:13036-13052), which asks `window_from_coordinates` which
    // window and which part they land on. The window the caller named is an
    // ORIGIN for the conversion, not the answer -- which is why a Y one row
    // past a window with no mode line answers the minibuffer window below it.
    let column_width = frame.char_width.max(1.0).round() as i64;
    let line_height = frame.char_height.max(1.0).round() as i64;
    let (frame_x, frame_y) = if window_relative_input {
        let bounds = *window_ref.bounds();
        let left_offset = if whole {
            0
        } else {
            computed
                .or_else(|| frame.redisplay_snapshot(wid))
                .map_or(0, |snapshot| snapshot.text_area_left_offset)
        };
        (
            bounds.x.round() as i64 + left_offset + x,
            bounds.y.round() as i64 + y,
        )
    } else {
        (x, y)
    };

    // The recomputed layout, when there is one, is the matrix that will answer
    // a text coordinate, so it is also the one the classification must see.
    let Some(hit) =
        frame.coordinate_hit_with(frame_x, frame_y, computed.map(|snapshot| (wid, snapshot)))
    else {
        return Ok(make_frame_position(fid, frame_x, frame_y));
    };

    match hit.coordinate {
        crate::window::WindowCoordinate::ChromeLine {
            line,
            window_x,
            window_y,
        } => {
            // `mode_line_string` reads `w->current_matrix` and never re-runs a
            // walk (src/dispnew.c:6444-6519), unlike `buffer_posn_from_coords`
            // which always does. Answer from the RETAINED snapshot even where
            // the text branch below would consult a freshly computed one, or
            // the asymmetry GNU has here would be lost.
            // GNU's rows for a window that has never been redisplayed are
            // allocated but not `enabled_p`, so `mode_line_string` answers
            // column 0 with zero width and height (src/dispnew.c:6497-6502).
            // An empty snapshot is that matrix.
            let unfilled = WindowDisplaySnapshot::default();
            let retained = frame.redisplay_snapshot(hit.window).unwrap_or(&unfilled);
            let window_height = hit.geometry.bottom_y - hit.geometry.top_y;
            let chrome = retained.chrome_line_hit(
                line,
                window_x,
                window_y,
                window_height,
                line_height,
                column_width,
            );
            Ok(make_chrome_line_position(
                hit.window, line, window_x, window_y, chrome,
            ))
        }
        crate::window::WindowCoordinate::Buffer {
            part,
            window_x,
            window_y,
            at,
        } => {
            // GNU reports the CLICK in the posn's `(X . Y)` cell, and which
            // click depends on the part: the text area and the margins report
            // it relative to the text area's top, the fringes likewise, and the
            // vertical border and the scroll bars relative to the window's own
            // corner (src/keyboard.c:5878-5975).
            let (report_x, report_y) = match part {
                crate::window::WindowPart::Text => (at.text_area_x(), at.text_area_y()),
                crate::window::WindowPart::LeftMargin
                | crate::window::WindowPart::RightMargin
                | crate::window::WindowPart::LeftFringe
                | crate::window::WindowPart::RightFringe => (window_x, at.text_area_y()),
                _ => (window_x, window_y),
            };
            let snapshot = if hit.window == wid {
                computed.or_else(|| frame.redisplay_snapshot(hit.window))
            } else {
                frame.redisplay_snapshot(hit.window)
            };
            if let Some(snapshot) = snapshot {
                let click = TextAreaClick::new(report_x, report_y, column_width);
                if let Some(point) = snapshot.point_at_coords(at) {
                    return Ok(make_window_part_position(
                        hit.window,
                        part,
                        click.apply(exact_metrics_from_redisplay_point(snapshot, &point), &point),
                    ));
                }
                return Ok(Value::NIL);
            }
            if hit.window != wid {
                // The approximate scanner below is built from the window the
                // CALLER named; there is no snapshot for the one the
                // coordinate resolved to and nothing to approximate it from.
                return Ok(Value::NIL);
            }
            // Use the identity `resolve_posn_at_xy_window` already produced
            // for this call.  Re-resolving `args.get(2)` here would apply the
            // window-only rule to a FRAME-OR-WINDOW argument.
            let Some(ctx) = live_window_display_context_for(frames, buffers, fid, wid)? else {
                return Ok(Value::NIL);
            };
            let metrics = match approximate_point_at_coords(&ctx, at.text_area_x(), at.window_y()) {
                Some(ApproxPointAtCoords::Point(metrics)) => metrics,
                Some(ApproxPointAtCoords::NeedsAllText) => {
                    let Some(ctx) =
                        live_window_display_context_with_all_text(frames, buffers, fid, wid)?
                    else {
                        return Ok(Value::NIL);
                    };
                    match approximate_point_at_coords(&ctx, at.text_area_x(), at.window_y()) {
                        Some(ApproxPointAtCoords::Point(metrics)) => metrics,
                        _ => return Ok(Value::NIL),
                    }
                }
                None => return Ok(Value::NIL),
            };
            Ok(make_window_part_position(
                hit.window,
                part,
                TextAreaClick::new(report_x, report_y, column_width).apply_to_metrics(metrics),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Bootstrap variables
// ---------------------------------------------------------------------------

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    fn defvar_buffer_local(
        obarray: &mut crate::emacs_core::symbol::Obarray,
        name: &str,
        default: Value,
    ) {
        obarray.define_lisp_variable(name, default, LispVariableLocality::BufferLocalIfSet);
    }

    obarray.set_symbol_value("inhibit-redisplay", Value::NIL);
    obarray.make_special("inhibit-redisplay");
    // The five `syms_of_xdisp' DEFVAR_LISPs entry 173's sweep found this port
    // short of.  Each carries GNU's own initializer; none is a nil placeholder
    // standing in for one.
    //
    // xdisp.c:39191 DEFVAR_LISP, `Vdebug_on_message = Qnil'.
    obarray.define_special_variable("debug-on-message", Value::NIL);
    // xdisp.c:38549 DEFVAR_LISP, `Vdisplay_pixels_per_inch = make_float (72.0)'
    // -- a float, not the fixnum 72: `default_pixels_per_inch_x' reads it with
    // `XFLOATINT' after a `NUMBERP' test, and `frame-char-width' arithmetic
    // divides by it.
    obarray.define_special_variable("display-pixels-per-inch", Value::make_float(72.0));
    // xdisp.c:38910 DEFVAR_LISP, `Vmenu_updating_frame = Qnil'.
    obarray.define_special_variable("menu-updating-frame", Value::NIL);
    // xdisp.c:39225 / 39230 DEFVAR_LISP, both `Fmake_hash_table (0, NULL)' --
    // an ordinary `eql' table, which `redisplay_internal' fills with cause
    // counters when `redisplay--variables' is instrumented.  An empty table is
    // not the same value as nil: `puthash' on nil signals.
    obarray.define_special_variable(
        "redisplay--all-windows-cause",
        Value::hash_table(crate::emacs_core::value::HashTableTest::Eql),
    );
    obarray.define_special_variable(
        "redisplay--mode-lines-cause",
        Value::hash_table(crate::emacs_core::value::HashTableTest::Eql),
    );
    // GNU xdisp.c `DEFVAR_LISP ("special-mirror-table", Vspecial_mirror_table)`:
    // a char-table of characters bidi display mirrors specially (paired
    // punctuation such as ¶<->‹). GNU inits it to an empty char-table
    // (`Vspecial_mirror_table = Fmake_char_table (Qnil, Qnil)`);
    // international/characters.el populates it and the redisplay bidi path reads
    // it via CHAR_TABLE_REF. New in 31.0.90 (absent from the 705c0e3 base), so
    // it must be defined for characters.el to load.
    obarray.set_symbol_value(
        "special-mirror-table",
        Value::make_char_table(Value::NIL, Value::NIL, 0),
    );
    obarray.make_special("special-mirror-table");
    obarray.set_symbol_value("blink-matching-delay", Value::fixnum(1));
    obarray.set_symbol_value("blink-matching-paren", Value::T);
    obarray.set_symbol_value("mouse-autoselect-window", Value::NIL);
    // xdisp.c:38695-38795 tab/tool bar DEFVARs (values are GNU's C inits:
    // DEFAULT_TAB_BAR_BUTTON_MARGIN 1 / _RELIEF 1, DEFAULT_TOOL_BAR_BUTTON_MARGIN 4
    // / _RELIEF 1, DEFAULT_TOOL_BAR_LABEL_SIZE 14 -- dispextern.h:3419-3499).
    obarray.define_special_variable("auto-resize-tab-bars", Value::T);
    obarray.define_special_variable("auto-resize-tool-bars", Value::T);
    obarray.define_special_variable("tab-bar-border", Value::symbol("internal-border-width"));
    obarray.define_special_variable("tab-bar-button-margin", Value::fixnum(1));
    obarray.define_int_variable("tab-bar-button-relief", 1);
    obarray.define_special_variable("tool-bar-border", Value::symbol("internal-border-width"));
    obarray.define_special_variable("tool-bar-button-margin", Value::fixnum(4));
    obarray.define_int_variable("tool-bar-button-relief", 1);
    obarray.define_int_variable("tool-bar-max-label-size", 14);
    obarray.set_symbol_value("tool-bar-style", Value::NIL);
    obarray.set_symbol_value("global-font-lock-mode", Value::NIL);
    // GNU xdisp.c registers these as DEFVAR_LISP/INT/BOOL variables and
    // calls Fmake_variable_buffer_local for the variables documented as
    // buffer-local. In particular, `display-line-numbers-mode' relies on
    // `display-line-numbers' being local-if-set so enabling it in one buffer
    // does not mutate the global default.
    defvar_buffer_local(obarray, "wrap-prefix", Value::NIL);
    defvar_buffer_local(obarray, "line-prefix", Value::NIL);
    defvar_buffer_local(obarray, "display-line-numbers", Value::NIL);
    defvar_buffer_local(obarray, "display-line-numbers-width", Value::NIL);
    obarray.set_symbol_value("display-line-numbers-current-absolute", Value::T);
    obarray.make_special("display-line-numbers-current-absolute");
    defvar_buffer_local(obarray, "display-line-numbers-widen", Value::NIL);
    // `display-line-numbers-offset' is BOTH, and in this order
    // (`src/xdisp.c:38999-39005'): `DEFVAR_INT' first, then
    // `Fmake_variable_buffer_local'.  `make_blv' copies the descriptor into the
    // BLV (`src/data.c:2112-2140'), so the integer rule applies to a per-buffer
    // binding as well as to the default -- `(setq-local
    // display-line-numbers-offset "x")' is `(wrong-type-argument integerp "x")'
    // in GNU.  Registering it as a plain buffer-local cell got the locality and
    // dropped the type.
    obarray.define_int_variable("display-line-numbers-offset", 0);
    obarray.make_buffer_local("display-line-numbers-offset", true);
    defvar_buffer_local(obarray, "display-fill-column-indicator", Value::NIL);
    // GNU `src/xdisp.c:38644-38652` defines this with DEFVAR_LISP,
    // initializes it to Qt, then calls Fmake_variable_buffer_local.
    defvar_buffer_local(obarray, "display-fill-column-indicator-column", Value::T);
    defvar_buffer_local(
        obarray,
        "display-fill-column-indicator-character",
        Value::NIL,
    );
    // xdisp.c:38558 DEFVAR_LISP, make_fixnum (50).
    obarray.define_special_variable("truncate-partial-width-windows", Value::fixnum(50));
    obarray.set_symbol_value("line-number-display-limit", Value::NIL);
    // xdisp.c:38514 / 38521 DEFVAR_INT, init 0.
    obarray.define_int_variable("scroll-step", 0);
    obarray.define_int_variable("scroll-conservatively", 0);
    // xdisp.c:38535 DEFVAR_INT, init 0.
    obarray.define_int_variable("scroll-margin", 0);
    // xdisp.c:38541 DEFVAR_LISP, make_float (0.25) -- a float, not a fixnum.
    obarray.define_special_variable("maximum-scroll-margin", Value::make_float(0.25));
    // xdisp.c:38875 DEFVAR_INT, init 5.
    obarray.define_int_variable("hscroll-margin", 5);
    // xdisp.c:38880 DEFVAR_LISP, make_fixnum (0).
    obarray.define_special_variable("hscroll-step", Value::fixnum(0));
    obarray.set_symbol_value("auto-hscroll-mode", Value::T);
    // xdisp.c:38479 DEFVAR_LISP, init Qarrow.
    obarray.define_special_variable("void-text-area-pointer", Value::symbol("arrow"));
    // `inhibit-message' and every other GNU `DEFVAR_BOOL' variable are
    // registered from `defvar_bool::GNU_BOOL_VARIABLES', which is where the
    // declaration's `byte-boolean-vars' visibility lives too.
    obarray.set_symbol_value("make-cursor-line-fully-visible", Value::T);
    // GNU `src/xdisp.c:38708` (`DEFVAR_BOOL ("inhibit-try-cursor-movement", ...)`)
    // controls the `try_cursor_movement` redisplay optimization. neomacs has
    // no equivalent optimization (the layout engine recomputes per frame),
    // so this knob is currently inert — but the symbol must exist so Lisp
    // code that does `(boundp 'inhibit-try-cursor-movement)` or
    // `(setq inhibit-try-cursor-movement ...)` does not raise void-variable.
    // Cursor audit Finding 7 in `drafts/cursor-audit.md`.
    obarray.define_special_variable("inhibit-try-cursor-movement", Value::NIL);
    obarray.set_symbol_value("show-trailing-whitespace", Value::NIL);
    obarray.make_special("show-trailing-whitespace");
    obarray.make_buffer_local("show-trailing-whitespace", true);
    // `syms_of_xdisp` calls `Fmake_variable_buffer_local' on this one right
    // after its `DEFVAR_BOOL' (`src/xdisp.c:38731-38735'); the declaration
    // itself lives in `defvar_bool::GNU_BOOL_VARIABLES', which has already
    // run, so the BLV inherits the Boolean forwarder the way `make_blv' does.
    obarray.make_buffer_local("make-window-start-visible", true);
    obarray.set_symbol_value("show-paren-context-when-offscreen", Value::NIL);
    // xdisp.c:38443 DEFVAR_LISP, init Qt.
    obarray.define_special_variable("nobreak-char-display", Value::T);
    // GNU inits this to `(overlay-arrow-position)` (xdisp.c: `Voverlay_arrow_variable_list
    // = list1 (intern_c_string ("overlay-arrow-position"))`), so the plain
    // `overlay-arrow-position` marker (used by e.g. gud) is scanned by redisplay.
    obarray.define_lisp_variable(
        "overlay-arrow-variable-list",
        Value::cons(Value::symbol("overlay-arrow-position"), Value::NIL),
        LispVariableLocality::Global,
    );
    obarray.define_lisp_variable(
        "overlay-arrow-string",
        Value::string("=>"),
        LispVariableLocality::Global,
    );
    obarray.define_lisp_variable(
        "overlay-arrow-position",
        Value::NIL,
        LispVariableLocality::Global,
    );
    // Mirror GNU Emacs: set char-table-extra-slots property for all subtypes
    // that need extra slots. Fmake_char_table reads this property to allocate
    // the correct number of extra slots.
    // See: casetab.c:249, category.c:426, character.c:1143, coding.c:11737,
    //      fontset.c:2158-2160, xdisp.c:31594, keymap.c:3346, syntax.c:3659
    obarray
        .put_property("case-table", "char-table-extra-slots", Value::fixnum(3))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("category-table", "char-table-extra-slots", Value::fixnum(2))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property(
            "char-script-table",
            "char-table-extra-slots",
            Value::fixnum(1),
        )
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property(
            "translation-table",
            "char-table-extra-slots",
            Value::fixnum(2),
        )
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("fontset", "char-table-extra-slots", Value::fixnum(8))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("fontset-info", "char-table-extra-slots", Value::fixnum(1))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property(
            "glyphless-char-display",
            "char-table-extra-slots",
            Value::fixnum(1),
        )
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("keymap", "char-table-extra-slots", Value::fixnum(0))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("syntax-table", "char-table-extra-slots", Value::fixnum(0))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray.set_symbol_value(
        "char-script-table",
        make_char_table_with_extra_slots(Value::symbol("char-script-table"), Value::NIL, 1),
    );
    // GNU DEFVAR_LISP (src/character.c:1138): a special, so a lexical-binding
    // `let` of it binds dynamically and internal matcher reads see it.
    obarray.make_special("char-script-table");
    // GNU's C default for `pre-redisplay-function` is `ignore` (xdisp.c:39133),
    // NOT nil. simple.el upgrades it to `redisplay--pre-redisplay-functions`
    // (the driver of the `pre-redisplay-functions` hook) ONLY when it still
    // equals `ignore` (simple.el:7352). Initialising it to nil here made that
    // guard fail, so the driver was never installed and `pre-redisplay-functions`
    // (hl-line with sticky 'window, the region overlay, …) never ran.
    obarray.define_special_variable("pre-redisplay-function", Value::symbol("ignore"));
    // xdisp.c:38835 DEFVAR_LISP: contrary to the docstring, GNU initializes
    // this to nil so loadup.el does not try to resize windows before
    // window.el is loaded; loadup.el:142 assigns `grow-only' right after.
    obarray.define_special_variable("resize-mini-windows", Value::NIL);
    // xdisp.c:38387 DEFVAR_LISP, build_string ("*Messages*").
    obarray.define_special_variable("messages-buffer-name", Value::string("*Messages*"));
    // xdisp.c:38602 DEFVAR_INT, init 200.
    obarray.define_int_variable("line-number-display-limit-width", 200);
    // xdisp.c:39086 / 39092 DEFVAR_INT, init 2 / 1.
    obarray.define_int_variable("overline-margin", 2);
    obarray.define_int_variable("underline-minimum-offset", 1);
    // xdisp.c:39108 DEFVAR_LISP, make_fixnum (DEFAULT_HOURGLASS_DELAY) = 1.
    obarray.define_special_variable("hourglass-delay", Value::fixnum(1));
    // xdisp.c:38827 DEFVAR_LISP, make_float (0.25).
    obarray.define_special_variable("max-mini-window-height", Value::make_float(0.25));
    // Do NOT pre-bind the *plural* `pre-redisplay-functions`: it is a pure lisp
    // defvar (simple.el) whose default is `(redisplay--update-region-highlight)`
    // — the function that creates the active-region highlight overlay. Binding it
    // to nil here shadowed that defvar, so the region overlay was never created
    // (and `global-hl-line-mode` then `add-hook`'d onto an empty list). Leaving
    // it unbound lets simple.el install GNU's default.
    obarray.define_int_variable("display-line-numbers-major-tick", 0);
    obarray.define_int_variable("display-line-numbers-minor-tick", 0);
    // xdisp.c:39295 DEFVAR_INT, init 0 ("The default value is zero, which
    // disables this feature").
    obarray.define_int_variable("max-redisplay-ticks", 0);
    // GNU `src/xdisp.c:38428-38438` defines this with DEFVAR_LISP,
    // initializes it to nil, then calls Fmake_variable_buffer_local.
    // `jit-lock.el` installs `jit-lock-function` here buffer-locally.
    {
        let id = intern("fontification-functions");
        obarray.set_symbol_value("fontification-functions", Value::NIL);
        obarray.make_special("fontification-functions");
        obarray.make_symbol_localized(id, Value::NIL);
        obarray.set_blv_local_if_set(id, true);
    }

    // auto-fill-chars: a char-table for characters which invoke auto-filling.
    // Official Emacs (character.c) creates it with sub-type `auto-fill-chars`
    // and sets space and newline to t.
    let auto_fill = make_char_table_value(Value::symbol("auto-fill-chars"), Value::NIL);
    // Set space and newline entries to t.  We use set-char-table-range
    // via the underlying data: store single-char entries.
    use super::chartable::ct_set_single;
    ct_set_single(&auto_fill, ' ' as i64, Value::T);
    ct_set_single(&auto_fill, '\n' as i64, Value::T);
    // character.c:1104 DEFVAR_LISP -- special like every C DEFVAR.
    obarray.define_special_variable("auto-fill-chars", auto_fill);

    // char-width-table: a char-table for character display widths.
    // Official Emacs (character.c) creates it with default 1.
    obarray.set_symbol_value(
        "char-width-table",
        make_char_table_value(Value::symbol("char-width-table"), Value::fixnum(1)),
    );

    // translation-table-vector: vector recording all translation tables.
    // Official Emacs (character.c) creates a 16-element nil vector.
    obarray.set_symbol_value(
        "translation-table-vector",
        Value::vector(vec![Value::NIL; 16]),
    );

    // translation-hash-table-vector: vector of translation hash tables.
    // Official Emacs (ccl.c:2382 DEFVAR_LISP) initializes to nil.
    obarray.define_special_variable("translation-hash-table-vector", Value::NIL);

    // printable-chars: a char-table of printable characters.
    // Official Emacs (character.c) creates it with default t.
    obarray.set_symbol_value(
        "printable-chars",
        make_char_table_value(Value::symbol("printable-chars"), Value::T),
    );

    // default-process-coding-system: cons of coding systems for process I/O.
    // Official Emacs (coding.c:12139 DEFVAR_LISP) initializes to nil.
    obarray.define_special_variable("default-process-coding-system", Value::NIL);

    // ambiguous-width-chars: char-table for characters whose width can be 1 or 2.
    // Official Emacs (character.c) creates empty char-table; populated by characters.el.
    obarray.set_symbol_value(
        "ambiguous-width-chars",
        make_char_table_value(Value::NIL, Value::NIL),
    );

    // text-property-default-nonsticky: alist of properties vs non-stickiness.
    // The effective GNU default is assembled by two C files, so it is kept in
    // one place -- see `default_text_property_nonsticky_alist'.
    obarray.set_symbol_value(
        "text-property-default-nonsticky",
        crate::emacs_core::textprop::default_text_property_nonsticky_alist(),
    );
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Frame snapshot (`neomacs--frame-snapshot`)
// ---------------------------------------------------------------------------
//
// Expose 100% of what redisplay produced — the real `FrameDisplayState` — as
// plain text for agents/tests, per
// docs/plans/2026-07-02-gui-observability-agent-driving-design.md.
//
// The subr lives in the internal `neomacs--` namespace on purpose: GNU's
// equivalent debug subrs (`dump-glyph-matrix` etc., src/xdisp.c) exist only
// under GLYPH_DEBUG, so a GNU-named subr would be an `fboundp` divergence
// against release reference binaries.
//
// neovm-core cannot see the layout engine (dependency direction), so the
// actual layout+serialize step is a frontend-installed callback on the
// evaluator (`Context::frame_snapshot_fn`), exactly like `redisplay_fn`.

/// Which frames a snapshot request covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotTarget {
    /// The selected frame only.
    Selected,
    /// Every visible frame in bottom-to-top z order — the full composited
    /// screen, including child frames (posframe/corfu popups, tooltips).
    All,
    /// One specific frame (core `FrameId.0`).
    Frame(u64),
}

/// Serialization format of a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    /// Greppable logical text grid (`FrameDisplayState::render_text`).
    Text,
    /// Text plus per-row face runs with names and resolved hex colors.
    TextFaces,
    /// Full-fidelity JSON: serde on the real protocol structs.
    Json,
    /// Frame and window geometry without replay assets or glyph matrices.
    JsonGeometry,
}

/// One `neomacs--frame-snapshot` request, handed to the frontend hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotRequest {
    pub target: SnapshotTarget,
    pub format: SnapshotFormat,
}

/// Decode the optional FRAME / FORMAT subr arguments (`rest` starts at the
/// FRAME argument). FRAME: nil = selected frame, t = all visible frames, or
/// a live frame object (a fixnum frame id is also accepted, mirroring
/// `frame_id_from_designator` in font.rs). FORMAT: nil/`text`,
/// `text-faces`, `json`, or `json-geometry`.
fn snapshot_request_from_args(
    eval: &super::eval::Context,
    rest: &[Value],
) -> Result<SnapshotRequest, Flow> {
    let target = match rest.first() {
        None => SnapshotTarget::Selected,
        Some(value) if value.is_nil() => SnapshotTarget::Selected,
        Some(value) if value.is_t() => SnapshotTarget::All,
        Some(value) => {
            let id = match value.kind() {
                ValueKind::Fixnum(id) if id >= 0 => Some(id as u64),
                ValueKind::Veclike(VecLikeType::Frame) => value.as_frame_id(),
                _ => None,
            };
            let Some(id) = id else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("framep"), *value],
                ));
            };
            if eval.frames.get(crate::window::FrameId(id)).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string(format!("No such live frame: {id}"))],
                ));
            }
            SnapshotTarget::Frame(id)
        }
    };
    let format = match rest.get(1) {
        None => SnapshotFormat::Text,
        Some(value) if value.is_nil() => SnapshotFormat::Text,
        Some(value) => match value.as_symbol_name() {
            Some("text") => SnapshotFormat::Text,
            Some("text-faces") => SnapshotFormat::TextFaces,
            Some("json") => SnapshotFormat::Json,
            Some("json-geometry") => SnapshotFormat::JsonGeometry,
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid frame snapshot format: {value} (use text, text-faces, json or json-geometry)"
                    ))],
                ));
            }
        },
    };
    Ok(SnapshotRequest { target, format })
}

/// Force a full redisplay, then run the frontend snapshot hook.
///
/// The redisplay first is essential: `redisplay_with_force` performs the
/// marker/point syncs, `pre-redisplay-function`, and auto-hscroll that
/// layout correctness depends on (GNU `redisplay_internal` preamble). The
/// hook then lays out the target frames on demand and serializes. The
/// take/call/reinstall dance mirrors `redisplay_fn` (eval.rs).
fn run_frame_snapshot(
    eval: &mut super::eval::Context,
    request: &SnapshotRequest,
) -> Result<String, Flow> {
    eval.redisplay_with_force(true)?;
    let Some(mut hook) = eval.frame_snapshot_fn.take() else {
        return Err(signal(
            "error",
            vec![Value::string(
                "neomacs--frame-snapshot: no display attached (batch mode?)",
            )],
        ));
    };
    let result = hook(eval, request);
    eval.frame_snapshot_fn = Some(hook);
    if let Some(flow) = eval.take_mode_line_display_flow() {
        return Err(flow);
    }
    result.map_err(|message| signal("error", vec![Value::string(message)]))
}

/// `(neomacs--frame-snapshot &optional FRAME FORMAT)` — force a redisplay
/// and return what is on screen as a string. See `SnapshotRequest`.
pub(crate) fn builtin_neomacs_frame_snapshot(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let request = snapshot_request_from_args(eval, &args)?;
    let snapshot = run_frame_snapshot(eval, &request)?;
    Ok(Value::string(snapshot))
}

/// `(neomacs--write-frame-snapshot PATH &optional FRAME FORMAT)` — like
/// `neomacs--frame-snapshot` but write the result to PATH and return t.
pub(crate) fn builtin_neomacs_write_frame_snapshot(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let Some(path) = args
        .first()
        .and_then(|value| value.as_lisp_string())
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![
                Value::symbol("stringp"),
                args.first().copied().unwrap_or(Value::NIL),
            ],
        ));
    };
    let request = snapshot_request_from_args(eval, &args[1..])?;
    let snapshot = run_frame_snapshot(eval, &request)?;
    std::fs::write(&path, snapshot).map_err(|error| {
        signal(
            "error",
            vec![Value::string(format!(
                "Cannot write frame snapshot to {path}: {error}"
            ))],
        )
    })?;
    Ok(Value::T)
}

/// `(neomacs--debug-lose-device)` — hidden debug hook: ask the display to
/// simulate a GPU device loss so the device-loss recovery path (GPU rebuild,
/// media re-resolution, full redisplay) can be exercised against a healthy
/// device. Returns t when a display host received the request, nil in batch
/// mode. NeoMacs extension; never call from production code.
pub(crate) fn builtin_neomacs_debug_lose_device(
    eval: &mut super::eval::Context,
    _args: Vec<Value>,
) -> EvalResult {
    let Some(host) = eval.display_host.as_deref() else {
        return Ok(Value::NIL);
    };
    host.debug_lose_device();
    Ok(Value::T)
}

// Tests
// ---------------------------------------------------------------------------

pub(crate) fn builtin_buffer_text_pixel_size(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("buffer-text-pixel-size", &args, 0, 4)?;

    // GNU's FIRST statement is `struct window *w = decode_live_window (window);`
    // (`src/xdisp.c`), so WINDOW is decoded before BUFFER-OR-NAME is resolved
    // and an internal or deleted window signals `window-live-p`.  The check
    // here used to be `!window.is_nil() && !window.is_window()` -- a tag test
    // that named `window-live-p` while enforcing `windowp`, so every window
    // object was accepted -- and it ran AFTER the buffer was resolved, so a bad
    // buffer name masked a bad window.
    let window_id = crate::emacs_core::window_cmds::decode_live_window_id(eval, args.get(1))?;

    // GNU `buffer-text-pixel-size` returns PIXELS: the measured column/row counts
    // scaled by the frame's character cell size. On a TTY the cell is 1x1 (so the
    // result equals the cell counts), on a GUI frame it is the real font
    // width/height. This mirrors `window-text-pixel-size` (which already scales by
    // `frame.char_width`). Without it, `string-pixel-width` returns columns, and the
    // mode-line `(space :align-to (- right-margin (string-pixel-width …)))` produced
    // by `mode-line-format-right-align` mis-aligns on GUI frames — the Doom dashboard
    // "DOOM vX" right segment is pushed off-screen.
    let (cell, frame_id) = eval
        .frames
        .selected_frame()
        .map(|f| {
            (
                TextCellPixels::new(f.char_width, f.char_height, f.font_cell_ascent()),
                f.id,
            )
        })
        .unwrap_or_else(|| (TextCellPixels::new(1.0, 1.0, 1.0), FrameId(0)));

    let buffers = &eval.buffers;

    let buffer_id = if args.is_empty() {
        resolve_buffer_designator_allow_nil_current_in_manager(buffers, &Value::NIL)?
    } else {
        resolve_buffer_designator_allow_nil_current_in_manager(buffers, &args[0])?
    };

    let limit_from_value = |value: &Value| -> Result<Option<usize>, Flow> {
        match value.kind() {
            ValueKind::Nil | ValueKind::T => Ok(None),
            ValueKind::Fixnum(n) if n >= 0 => Ok(Some(n as usize)),
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("natnump"), *value],
            )),
        }
    };

    let x_limit = if args.len() > 2 {
        limit_from_value(&args[2])?
    } else {
        None
    };
    let y_limit = if args.len() > 3 {
        limit_from_value(&args[3])?
    } else {
        None
    };

    let Some(buffer_id) = buffer_id else {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    };

    // Determine the accessible byte range to measure.  An empty buffer yields
    // (0 . 0) just like the previous text-based implementation.
    let range = match buffers.get(buffer_id) {
        Some(buf) => buf.accessible_emacs_byte_range(),
        None => return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0))),
    };
    if range.end().get() <= range.start().get() {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    }

    // The window's frame supplies the cell, the row edge and `line_wrap`; a
    // window whose frame has gone keeps the selected frame's cell and an
    // unbounded row, which is the best a measurement can say.
    let (fid, is_terminal, char_width) = eval
        .frames
        .find_window_frame_id(window_id)
        .and_then(|fid| {
            eval.frames.get(fid).map(|frame| {
                (
                    fid,
                    frame.effective_window_system().is_none(),
                    frame.char_width,
                )
            })
        })
        .unwrap_or((frame_id, false, cell.width));
    let line_wrap = window_line_wrap(eval, window_id, buffer_id);

    // Measure the whole buffer in pixels, honoring `display` text properties
    // (e.g. `(space :align-to N)` / `(space :width N)`, or an image), through
    // the same scanner `window-text-pixel-size` uses.  Wide chars contribute
    // their display width, preserving the previous accounting.
    //
    // X-LIMIT and Y-LIMIT have "the same meaning as with
    // `window-text-pixel-size`" (GNU docstring): PIXELS, and GNU's
    // implementation forwards them to the same `window_text_pixel_size`, so
    // X-LIMIT nil means WINDOW's body width here too and the row edge is the
    // window's.  There is no display line to rewind to: the measurement starts
    // at the accessible portion's start.
    let edge = match x_limit {
        Some(pixels) => Some(RowEdge::new(pixels as f32, line_wrap)),
        None => eval.frames.get(fid).and_then(|frame| {
            let window = frame.find_window(window_id)?;
            window_body_row_edge(
                &eval.frames,
                fid,
                window,
                is_terminal,
                char_width,
                line_wrap,
            )
        }),
    };
    let metrics = region_text_metrics_with_display(
        eval,
        fid,
        buffer_id,
        MeasuredRange::starting_at(range.start(), range.end()),
        false,
        cell,
        CharColumnWidth::DisplayWidth,
        edge,
        y_limit.map(|pixels| pixels as f32),
    );

    if metrics.lines == 0 {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    }
    Ok(Value::cons(
        Value::fixnum(metrics.max_width.ceil() as i64),
        Value::fixnum(metrics.height.ceil() as i64),
    ))
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/mode_line_gc_roots.rs"]
mod mode_line_gc_roots;

#[cfg(test)]
#[path = "tests/mode_line_incremental_roots.rs"]
mod mode_line_incremental_roots;

#[cfg(test)]
#[path = "tests/mode_line_live_spine.rs"]
mod mode_line_live_spine;

#[cfg(test)]
#[path = "tests/mode_line_flow.rs"]
mod mode_line_flow;
