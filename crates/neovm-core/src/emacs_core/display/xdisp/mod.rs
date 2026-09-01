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

use super::buffer::resolve_buffer_designator_allow_nil_current_in_manager;
use super::chartable::{make_char_table_value, make_char_table_with_extra_slots};
use super::display_spec;
use super::error::{EvalResult, Flow, signal};
use super::hook_runtime;
use super::intern::intern;
use super::symbol::LispVariableLocality;
use super::value::*;
use crate::buffer::{
    Buffer, BufferId, CharLen, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1,
    TextPropertyTable,
};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range};
use crate::window::{
    DisplayRowSnapshot, FrameId, FrameManager, Window, WindowDisplaySnapshot, WindowId,
};
use std::ops::ControlFlow;
use strum::{EnumString, IntoStaticStr};

impl super::eval::Context {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RegionTextMetrics {
    pub(crate) lines: usize,
    pub(crate) max_columns: usize,
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

fn prefix_line_and_column(buf: &Buffer, end_byte: EmacsBytePos) -> LineColumn {
    let end = end_byte.min(buf.point_max_emacs_byte_pos());
    // Column only needs the current line's prefix: find its start with a
    // backward newline scan (O(column) via memrchr, not O(point)) and count
    // chars over just that span.
    let bol = buf
        .prev_newline_emacs_byte(end, EmacsBytePos::ZERO)
        .map(|nl| nl.add_len(crate::buffer::EmacsByteLen::new(1)))
        .unwrap_or(EmacsBytePos::ZERO);
    // Line number: GNU keeps a base_line_pos/base_line_number anchor so the
    // newline count runs from a recently displayed line, not from the buffer
    // start (xdisp.c:29486-29620) — O(distance moved), not O(point). The
    // anchor (held per buffer) is valid only when no edit landed at or
    // before it since the last accepted redisplay: the unchanged-region
    // accumulator is the BEG_UNCHANGED analog of GNU's
    // BASE_LINE_NUMBER_VALID_P (xdisp.c:19351). An invalid or missing
    // anchor falls back to the full prefix scan (SIMD over the gap buffer).
    let (anchor_byte, anchor_line) = buf.line_number_anchor.get();
    let anchor_valid = anchor_line > 0
        && anchor_byte <= end.get()
        && buf.changed_char_range().is_none_or(|(dirty_start, _)| {
            buf.char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(
                dirty_start.max(0) as usize
            ))
            .get()
                > anchor_byte
        });
    let line = if anchor_valid {
        anchor_line as usize + buf.count_newlines_emacs_byte(EmacsBytePos::new(anchor_byte), end)
    } else {
        buf.count_newlines_emacs_byte(EmacsBytePos::ZERO, end) + 1
    };
    // Re-seat the anchor at this line's start whenever it was invalid or
    // point moved far from it (GNU re-seats near the window when the count
    // drifts past a few window heights, xdisp.c:29544-29552).
    const RESEAT_DISTANCE_BYTES: usize = 32 * 1024;
    if !anchor_valid || end.get().abs_diff(anchor_byte) > RESEAT_DISTANCE_BYTES {
        buf.line_number_anchor.set((bol.get(), line as i64));
    }
    let mut tail = Vec::new();
    buf.copy_emacs_byte_range_to(EmacsByteRange::new(bol, end), &mut tail);
    let col = emacs_char_count(&tail, buf.get_multibyte());
    LineColumn { line, column: col }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn region_text_metrics(bytes: &[u8], multibyte: bool) -> RegionTextMetrics {
    if bytes.is_empty() {
        return RegionTextMetrics {
            lines: 0,
            max_columns: 0,
        };
    }

    let mut max_cols = 0usize;
    let mut lines = 1usize;
    let mut cur_col = 0usize;

    let mut visit = |code: u32| {
        if code == '\n' as u32 {
            lines += 1;
            max_cols = max_cols.max(cur_col);
            cur_col = 0;
        } else if code == '\t' as u32 {
            cur_col = (cur_col + 8) & !7;
        } else {
            cur_col += 1;
        }
    };

    if multibyte {
        let mut pos = 0usize;
        while pos < bytes.len() {
            let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
            visit(code);
            pos += len;
        }
    } else {
        for &byte in bytes {
            visit(byte as u32);
        }
    }

    if bytes.last() == Some(&b'\n') {
        lines = lines.saturating_sub(1);
    }

    RegionTextMetrics {
        lines,
        max_columns: max_cols.max(cur_col),
    }
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

/// `display`-aware variant of [`region_text_metrics`].  Scans the buffer's
/// byte range `[from, to)` honoring the `display` text property / overlay at
/// each position: a `(space :align-to N)` / `(space :width N)` spec contributes
/// its real column width (GNU resolves these through the display iterator's
/// `calc_pixel_width_or_height`), instead of being counted as a single
/// character column.  Plain text, tabs and newlines are counted exactly as
/// `region_text_metrics` does.  `apply_trim` requests the GNU `TO == t`
/// trailing-blank-line trimming, applied to the *byte* range before scanning.
///
/// `char_width` selects the per-char column accounting (see [`CharColumnWidth`]).
/// `x_limit` / `y_limit` cap the measured columns-per-line and lines (used by
/// `buffer-text-pixel-size`); pass `None` for an unbounded scan.
/// `wrap_columns` counts soft-wrapped display rows at the supplied text width,
/// as `window-text-pixel-size` must do for a live window.
#[allow(clippy::too_many_arguments)] // measurement bounds and display policy are independent inputs
pub(crate) fn region_text_metrics_with_display(
    eval: &super::eval::Context,
    buffer_id: BufferId,
    from: EmacsBytePos,
    to: EmacsBytePos,
    apply_trim: bool,
    char_width: CharColumnWidth,
    x_limit: Option<usize>,
    y_limit: Option<usize>,
    wrap_columns: Option<usize>,
) -> RegionTextMetrics {
    let Some(buf) = eval.buffers.get(buffer_id) else {
        return RegionTextMetrics {
            lines: 0,
            max_columns: 0,
        };
    };

    // The GNU `TO == t` semantics measure through the line ending the last
    // non-empty line, not through trailing blank lines.  We reuse the existing
    // byte-level trimmer to find the trimmed end, then scan with display props.
    let scan_end = if apply_trim {
        let mut bytes = Vec::new();
        buf.copy_emacs_byte_range_to(EmacsByteRange::new(from, to), &mut bytes);
        let trimmed_len = trim_window_text_to_non_empty_line_end(&bytes).len();
        EmacsBytePos::new(from.get() + trimmed_len)
    } else {
        to
    };

    if scan_end.get() <= from.get() {
        return RegionTextMetrics {
            lines: 0,
            max_columns: 0,
        };
    }

    let display_sym = Value::symbol("display");
    let mut state = ScanState::new(char_width, x_limit, y_limit, wrap_columns);

    let mut scan = from.get();
    let end = scan_end.get();
    while scan < end {
        if state.y_limit_reached() {
            break;
        }

        // GNU's display iterator processes overlay strings anchored at a
        // position *before* the buffer character there: before-strings of
        // overlays starting here, plus after-strings of overlays ending here
        // (see `load_overlay_strings`/`get_overlay_strings` in xdisp.c).  Each
        // contributes its own laid-out columns to the running line width.
        process_overlay_strings_at(eval, buf, scan, &display_sym, &mut state);

        // A `display` property (text property or overlay) whose value is a
        // `(space ...)` spec replaces the covered text for layout.  Resolve its
        // column width and advance over the whole property/overlay run atomically.
        if let Some((width, run_end)) =
            display_space_run(eval, buf, scan, state.cur_col, &display_sym, end)
            && run_end > scan
        {
            state.advance_columns(width);
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
        process_overlay_strings_at(eval, buf, end, &display_sym, &mut state);
    }

    state.finish()
}

/// Mutable accounting state shared between the buffer-text scan and the
/// overlay-string walk in [`region_text_metrics_with_display`].  Tracks the
/// running column on the current line, the widest line seen, the line count,
/// and the `x_limit`/`y_limit` caps, so overlay strings contribute their
/// columns and embedded newlines exactly like buffer text.
struct ScanState {
    char_width: CharColumnWidth,
    x_limit: Option<usize>,
    y_limit: Option<usize>,
    wrap_columns: Option<usize>,
    max_cols: usize,
    lines: usize,
    cur_col: usize,
    last_code: Option<u32>,
    /// Once the running line is capped by `x_limit`, ignore further width on it.
    line_capped: bool,
}

impl ScanState {
    fn new(
        char_width: CharColumnWidth,
        x_limit: Option<usize>,
        y_limit: Option<usize>,
        wrap_columns: Option<usize>,
    ) -> Self {
        Self {
            char_width,
            x_limit,
            y_limit,
            wrap_columns: wrap_columns.filter(|columns| *columns > 0),
            max_cols: 0,
            lines: 1,
            cur_col: 0,
            last_code: None,
            line_capped: false,
        }
    }

    /// True once the line count has exceeded `y_limit`; the caller stops and the
    /// over-counted line is rolled back so the result matches GNU's cap.
    fn y_limit_reached(&mut self) -> bool {
        if self.y_limit.is_some_and(|limit| self.lines > limit) {
            self.lines = self.lines.saturating_sub(1);
            true
        } else {
            false
        }
    }

    fn cap_line(&mut self) {
        if let Some(limit) = self.x_limit
            && self.cur_col >= limit
        {
            self.cur_col = limit;
            self.line_capped = true;
        }
    }

    /// Advance the running column by a resolved `(space ...)` width.
    fn advance_columns(&mut self, width: usize) {
        if self.line_capped {
            return;
        }

        let mut remaining = width;
        while remaining > 0 {
            if let Some(wrap_columns) = self.wrap_columns {
                if self.cur_col >= wrap_columns {
                    self.soft_wrap();
                }
                let available = wrap_columns.saturating_sub(self.cur_col);
                let advanced = remaining.min(available);
                self.cur_col = self.cur_col.saturating_add(advanced);
                remaining = remaining.saturating_sub(advanced);
                if remaining > 0 {
                    self.soft_wrap();
                }
            } else {
                self.cur_col = self.cur_col.saturating_add(remaining);
                remaining = 0;
            }
            self.cap_line();
            if self.line_capped {
                break;
            }
        }
    }

    fn soft_wrap(&mut self) {
        self.lines += 1;
        self.max_cols = self.max_cols.max(self.cur_col);
        self.cur_col = 0;
        self.line_capped = false;
    }

    /// End the current line, record its width, and reset for the next line.
    fn newline(&mut self) {
        self.lines += 1;
        self.max_cols = self.max_cols.max(self.cur_col);
        self.cur_col = 0;
        self.line_capped = false;
    }

    /// Account for one character `code` (newline, tab, or normal), tracking it
    /// as the last code seen so a trailing newline can be rolled back.
    fn push_char(&mut self, code: u32) {
        if code == '\n' as u32 {
            self.newline();
        } else if !self.line_capped {
            if code == '\t' as u32 {
                let next_tab = (self.cur_col + 8) & !7;
                self.advance_columns(next_tab.saturating_sub(self.cur_col));
            } else {
                let columns = match self.char_width {
                    CharColumnWidth::One => 1,
                    CharColumnWidth::DisplayWidth => char::from_u32(code)
                        .map(crate::encoding::char_width)
                        .unwrap_or(1),
                };
                self.advance_columns(columns);
            }
        }
        self.last_code = Some(code);
    }

    fn finish(mut self) -> RegionTextMetrics {
        // Match `region_text_metrics`: a trailing newline does not open a line.
        if self.last_code == Some('\n' as u32) {
            self.lines = self.lines.saturating_sub(1);
        }
        RegionTextMetrics {
            lines: self.lines,
            max_columns: self.max_cols.max(self.cur_col),
        }
    }
}

/// If buffer byte `pos` carries a `display` property (text property OR overlay)
/// whose value is a `(space ...)` spec with a column-bearing keyword, return
/// `(column_width, run_end_byte)`.  Mirrors the `display`-spec branch of GNU's
/// display iterator: the covered run is laid out as a single stretch of the
/// resolved width.  Returns `None` when there is no `display` property, or its
/// value is not a width-bearing `(space ...)` spec we model.
fn display_space_run(
    eval: &super::eval::Context,
    buf: &Buffer,
    pos: usize,
    cur_col: usize,
    display_sym: &Value,
    region_end: usize,
) -> Option<(usize, usize)> {
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
            vec![Value::fixnum(charpos1), *display_sym],
        )
        .ok()?;
        if v.is_nil() { None } else { Some((v, None)) }
    })?;

    // Only `(space ...)` specs affect the measured column width here.
    if !(display.is_cons() && display.cons_car() == Value::symbol("space")) {
        return None;
    }
    let width = space_spec_advance_columns(display.cons_cdr(), cur_col)?;

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
            vec![Value::fixnum(charpos1), *display_sym],
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

    Some((width, run_end.min(region_end)))
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
    buf: &Buffer,
    pos: usize,
    display_sym: &Value,
    state: &mut ScanState,
) {
    if buf.overlays.is_empty() {
        return;
    }
    for entry in collect_overlay_strings_at(buf, pos, true) {
        walk_overlay_string(eval, entry.string, display_sym, state);
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
        // width-bearing `(space ...)` spec replaces the run up to the next
        // `display` change, advancing the column by the resolved width.
        if let Some((width, run_end_char)) =
            string_display_space_run(eval, string, char_index, state.cur_col, display_sym)
            && run_end_char > char_index
        {
            state.advance_columns(width);
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

/// String-object analogue of [`display_space_run`]: if the overlay string
/// carries a `display` text property at char position `char_index` whose value
/// is a width-bearing `(space ...)` spec, return `(column_width, run_end_char)`.
/// `cur_col` is the running column at the spec, needed for `:align-to` (an
/// absolute target).  `run_end_char` is the next `display`-property change in
/// the string (the end of the covered run), defaulting to the string length.
fn string_display_space_run(
    eval: &super::eval::Context,
    string: Value,
    char_index: usize,
    cur_col: usize,
    display_sym: &Value,
) -> Option<(usize, usize)> {
    // `get-text-property` on a string takes a 0-based char position.
    let display = super::textprop::builtin_get_text_property_in_state(
        &eval.obarray,
        &eval.buffers,
        vec![Value::fixnum(char_index as i64), *display_sym, string],
    )
    .ok()?;

    // Only `(space ...)` specs affect the measured column width here.
    if !(display.is_cons() && display.cons_car() == Value::symbol("space")) {
        return None;
    }
    let width = space_spec_advance_columns(display.cons_cdr(), cur_col)?;

    let schars = string.as_lisp_string().map(|s| s.schars()).unwrap_or(0);
    let run_end_char = super::textprop::builtin_next_single_property_change_in_state(
        &eval.obarray,
        &eval.buffers,
        vec![Value::fixnum(char_index as i64), *display_sym, string],
    )
    .ok()
    .and_then(|v| match v.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Some(n as usize),
        _ => None,
    })
    .unwrap_or(schars)
    .min(schars);

    Some((width, run_end_char))
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
    validate_optional_window_designator_in_state(frames, args.get(2), "windowp")?;
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
/// Returns the fully rendered mode-line as a propertized string. On
/// any evaluator error the function returns an empty string; callers
/// that need to distinguish failure from empty should check `as_str`.
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
    let args = [format_val, Value::NIL, window, buffer];
    if validate_optional_window_designator(eval, args.get(2), "windowp").is_err() {
        return ModeLineDisplayOutput::from_root_string(Value::string(""));
    }
    if validate_optional_buffer_designator(eval, args.get(3)).is_err() {
        return ModeLineDisplayOutput::from_root_string(Value::string(""));
    }
    let target_buffer = resolve_mode_line_buffer(eval, args.get(2), args.get(3));
    let saved_buffer = eval.buffers.current_buffer_id();
    if let Some(buffer_id) = target_buffer
        && eval.set_current_buffer_unrecorded(buffer_id).is_err()
    {
        return ModeLineDisplayOutput::from_root_string(Value::string(""));
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
        ModeLineDisplayOutput::from_root_string(Value::string(""))
    } else {
        let face_spec = resolve_mode_line_face_spec(&args);
        let mut pctx = build_mode_line_percent_context(
            &eval.frames,
            &eval.buffers,
            Some(&eval.coding_systems),
            &eval.obarray,
            args.get(2),
        );
        pctx.target_width = Some(target_cols);
        let mut rendered = ModeLineRendered::default();
        {
            // pctx caches heap Values (frame name, eol indicator) captured
            // before the walk; an :eval that renames the frame would orphan
            // them mid-walk. Root them for the walk's span.
            let pctx_root_scope = eval.save_specpdl_roots();
            for value in [pctx.frame_name, pctx.eol_indicator].into_iter().flatten() {
                if value.is_heap_object() {
                    eval.push_specpdl_root(value);
                }
            }
            format_mode_line_recursive(eval, &pctx, &format_val, &mut rendered, 0, false);
            eval.restore_specpdl_roots(pctx_root_scope);
        }
        rendered.into_display_output(face_spec)
    };

    if let Some((buffer_id, saved)) = saved_point
        && let Some(buffer) = eval.buffers.get_mut(buffer_id)
    {
        buffer.goto_emacs_byte_pos(saved);
    }
    if let Some(saved) = saved_window_selection {
        eval.frames.restore_selected_window_for_mode_line(saved);
    }
    if let Some(buffer_id) = saved_buffer {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    result_value
}

pub(crate) fn finish_format_mode_line_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_args_range("format-mode-line", args, 1, 4)?;
    validate_optional_window_designator(eval, args.get(2), "windowp")?;
    validate_optional_buffer_designator(eval, args.get(3))?;

    let target_buffer = resolve_mode_line_buffer(eval, args.get(2), args.get(3));
    let saved_buffer = eval.buffers.current_buffer_id();
    if let Some(buffer_id) = target_buffer {
        eval.set_current_buffer_unrecorded(buffer_id)?;
    }

    let result = if args[0].is_nil() || eval.noninteractive() {
        Value::string("")
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
            let pctx_root_scope = eval.save_specpdl_roots();
            for value in [pctx.frame_name, pctx.eol_indicator].into_iter().flatten() {
                if value.is_heap_object() {
                    eval.push_specpdl_root(value);
                }
            }
            format_mode_line_recursive(eval, &pctx, &format_val, &mut result, 0, false);
            eval.restore_specpdl_roots(pctx_root_scope);
        }
        result.into_value(face_spec)
    };

    if let Some(buffer_id) = saved_buffer {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    Ok(result)
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
    expect_args_range("format-mode-line", args, 1, 4)?;
    validate_optional_window_designator_in_state(frames, args.get(2), "windowp")?;
    validate_optional_buffer_designator_in_state(buffers, args.get(3))?;

    let target_buffer = resolve_mode_line_buffer_in_state(frames, args.get(2), args.get(3));
    let saved_buffer = buffers.current_buffer_id();
    if let Some(buffer_id) = target_buffer {
        buffers.switch_current_unrecorded(buffer_id);
    }

    let result = if args[0].is_nil() {
        Value::string("")
    } else {
        let format_val = args[0];
        let face_spec = resolve_mode_line_face_spec(args);
        let pctx = build_mode_line_percent_context(frames, &*buffers, None, obarray, args.get(2));
        let mut result = ModeLineRendered::default();
        format_mode_line_recursive_in_state_with_eval(
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
        )?;
        result.into_value(face_spec)
    };

    if let Some(buffer_id) = saved_buffer {
        buffers.switch_current_unrecorded(buffer_id);
    }
    Ok(result)
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
    /// Coding system mnemonic character for `%z`/`%Z`.
    /// GNU: `CODING_ATTR_MNEMONIC` from the coding system spec.
    coding_mnemonic: char,
    /// Terminal output coding mnemonic (TTY only). For `%z` on TTY
    /// frames, GNU outputs 3 chars: terminal + keyboard + buffer.
    terminal_coding_mnemonic: char,
    /// Keyboard input coding mnemonic (TTY only).
    keyboard_coding_mnemonic: char,
    /// True when the selected frame is a TTY (no window-system).
    is_tty_frame: bool,
    /// EOL type string for `%Z` (`:`, `\`, `/`, or undecided).
    eol_indicator: Option<Value>,
    /// When `Some(n)`, the walker is running in GNU's
    /// `MODE_LINE_DISPLAY` mode with a target column width of `n`.
    /// In that mode, the `%-` percent construct expands to enough
    /// dashes to fill the remaining row width (GNU
    /// `xdisp.c:29154-29172`: `lots_of_dashes`). Callers that want
    /// GNU's `MODE_LINE_STRING` behavior (the Lisp-facing
    /// `format-mode-line` builtin) leave this `None`, and `%-`
    /// returns the literal two-dash string `"--"`.
    ///
    /// This mirrors GNU's internal `mode_line_target` state: the
    /// string API `Fformat_mode_line` uses `MODE_LINE_STRING`, while
    /// `display_mode_line` uses `MODE_LINE_DISPLAY`. The same walker
    /// serves both; the only difference is the dispatch for `%-`.
    target_width: Option<usize>,
    /// Byte position of THIS window's point, for point-dependent specs (`%l`,
    /// `%c`). GNU sets the buffer's point to `w->pointm` while displaying a
    /// window's mode line, so these reflect that window — not the selected
    /// window's point (which is the live buffer point). `None` falls back to the
    /// buffer point. Set per-window in `build_mode_line_percent_context`.
    window_point: Option<EmacsBytePos>,
}

impl Default for ModeLinePercentContext {
    fn default() -> Self {
        Self {
            window_start: 0,
            window_end: 0,
            frame_name: None,
            coding_mnemonic: '-',
            terminal_coding_mnemonic: '\0',
            keyboard_coding_mnemonic: '\0',
            is_tty_frame: false,
            eol_indicator: None,
            target_width: None,
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
        coding_mnemonic: '-',
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

    // --- TTY detection (GNU: FRAME_WINDOW_P) ---
    if let Some(frame) = frames.selected_frame() {
        ctx.is_tty_frame = frame.effective_window_system().is_none();
    }

    // --- Coding system mnemonic (GNU: decode_mode_spec_coding) ---
    let cs_name = context_buffer
        .and_then(|b| b.buffer_local_value("buffer-file-coding-system"))
        .and_then(|v| v.as_symbol_id());
    if let Some(name) = cs_name {
        ctx.coding_mnemonic = coding_system_mnemonic_char(name);
        ctx.eol_indicator = coding_system_eol_indicator_value(obarray, name);
    }

    // --- Terminal and keyboard coding mnemonics (TTY only) ---
    // GNU xdisp.c:29494: on TTY, %z outputs 3 chars —
    // terminal-coding-system mnemonic, keyboard-coding-system mnemonic,
    // and buffer-file-coding-system mnemonic.
    if ctx.is_tty_frame {
        if let Some(coding_systems) = coding_systems {
            ctx.terminal_coding_mnemonic =
                coding_system_mnemonic_char(coding_systems.terminal_coding_sym());
            ctx.keyboard_coding_mnemonic =
                coding_system_mnemonic_char(coding_systems.keyboard_coding_sym());
        } else {
            let term_cs = obarray
                .symbol_value("terminal-coding-system")
                .and_then(|v| v.as_symbol_id());
            ctx.terminal_coding_mnemonic = term_cs.map(coding_system_mnemonic_char).unwrap_or('-');

            let kbd_cs = obarray
                .symbol_value("keyboard-coding-system")
                .and_then(|v| v.as_symbol_id());
            ctx.keyboard_coding_mnemonic = kbd_cs.map(coding_system_mnemonic_char).unwrap_or('-');
        }
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
fn coding_system_mnemonic_char(cs_name: crate::emacs_core::intern::SymId) -> char {
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
        | "prefer-utf-8"
        | "mule-utf-8"
        | "utf-16"
        | "utf-16-be"
        | "utf-16-le"
        | "utf-16be"
        | "utf-16le"
        | "utf-16be-with-signature"
        | "utf-16le-with-signature" => 'U',
        "undecided" => '-',
        "raw-text" => '=',
        "no-conversion" | "binary" => '0',
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
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into().chars().map(|c| c as u32).collect(),
            text_props: TextPropertyTable::new(),
            source_spans: Vec::new(),
        }
    }

    fn append_rendered(&mut self, other: &Self) {
        let char_offset = self.char_len();
        self.text.extend_from_slice(&other.text);
        self.text_props
            .append_shifted_at_char_offset(&other.text_props, CharLen::new(char_offset));
        self.source_spans.extend(
            other
                .source_spans
                .iter()
                .copied()
                .map(|span| span.shifted_output(char_offset)),
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
                    self.text_props
                        .append_shifted_at_char_offset(&props, CharLen::new(char_offset));
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
                    self.text_props.append_shifted_at_char_offset(
                        &props.slice_char_range(display_char_range(start_char, end_char)),
                        CharLen::new(char_offset),
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
                    self.text_props.append_shifted_at_char_offset(
                        &props.slice_char_range(display_char_range(start_char, end_char)),
                        CharLen::new(char_offset),
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
        }
    }

    fn pad_plain_spaces(&mut self, padding_chars: usize) {
        if padding_chars == 0 {
            return;
        }
        self.text
            .extend(std::iter::repeat_n(' ' as u32, padding_chars));
    }

    fn apply_display_min_width(&mut self, props: Value) {
        let Some(min_width) = mode_line_display_min_width_chars(props) else {
            return;
        };
        let current_width = self.char_len();
        if current_width < min_width {
            self.pad_plain_spaces(min_width - current_width);
        }
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

fn mode_line_display_min_width_chars(props: Value) -> Option<usize> {
    let items = list_to_vec(&props)?;
    for chunk in items.chunks(2) {
        if chunk.len() != 2 || !chunk[0].is_symbol_named("display") {
            continue;
        }
        if let Some(width) = mode_line_display_spec_min_width_chars(chunk[1]) {
            return Some(width);
        }
    }
    None
}

fn mode_line_display_spec_min_width_chars(value: Value) -> Option<usize> {
    if !value.is_cons() || !value.cons_car().is_symbol_named("min-width") {
        return None;
    }
    mode_line_display_width_chars(value.cons_cdr().cons_car())
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
) {
    if depth > 20 {
        return; // Guard against infinite recursion
    }

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
                    );
                }
            }
        }

        _ if format.is_cons() => {
            let car = format.cons_car();
            let cdr = format.cons_cdr();

            if car.is_symbol_named(":eval") {
                if risky {
                    return;
                }
                if cdr.is_cons() {
                    let form_val = cdr.cons_car();
                    if let Ok(val) = eval.eval_value(&form_val) {
                        // val is a FRESH structure held only in this Rust
                        // local; a nested :eval inside the recursion runs
                        // Lisp whose GC would free it mid-walk. Root it for
                        // the recursion span (mode lines contain few :evals,
                        // so the push/pop pair is negligible).
                        let root_scope = eval.save_specpdl_roots();
                        eval.push_specpdl_root(val);
                        format_mode_line_recursive(eval, pctx, &val, result, depth + 1, risky);
                        eval.restore_specpdl_roots(root_scope);
                    }
                }
                return;
            }

            if car.is_symbol_named(":propertize") {
                if risky {
                    return;
                }
                if cdr.is_cons() {
                    let elt = cdr.cons_car();
                    let mut nested = ModeLineRendered::default();
                    format_mode_line_recursive(eval, pctx, &elt, &mut nested, depth + 1, risky);
                    nested.apply_display_min_width(cdr.cons_cdr());
                    nested.overlay_properties(cdr.cons_cdr());
                    result.append_rendered(&nested);
                }
                return;
            }

            if let Some(lim) = car.as_fixnum() {
                let mut nested = ModeLineRendered::default();
                format_mode_line_recursive(eval, pctx, &cdr, &mut nested, depth + 1, risky);
                append_mode_line_rendered_segment(
                    result,
                    &nested,
                    if lim > 0 { lim } else { 0 },
                    if lim < 0 { -lim } else { 0 },
                );
                return;
            }

            if car.is_symbol() && !car.is_symbol_named("t") {
                if let Some(sym_name) = car.as_symbol_name()
                    && mode_line_symbol_value_in_state(&eval.obarray, &[], &eval.buffers, sym_name)
                        .is_some_and(|value| value.is_truthy())
                    && let Some(branch) = mode_line_conditional_branch(cdr, true)
                {
                    format_mode_line_recursive(eval, pctx, &branch, result, depth + 1, risky);
                } else if let Some(branch) = mode_line_conditional_branch(cdr, false) {
                    format_mode_line_recursive(eval, pctx, &branch, result, depth + 1, risky);
                }
                return;
            }

            if let Some(elements) = list_to_vec(format) {
                for elem in &elements {
                    format_mode_line_recursive(eval, pctx, elem, result, depth + 1, risky);
                }
            }
        }

        _ => {
            result.append_string_value_preserving_props(format);
        }
    }
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
                    nested.apply_display_min_width(cdr.cons_cdr());
                    nested.overlay_properties(cdr.cons_cdr());
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

            if let Some(elements) = list_to_vec(format) {
                for elem in &elements {
                    if format_mode_line_recursive_in_state(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        elem,
                        result,
                        depth + 1,
                        risky,
                    ) {
                        return true;
                    }
                }
            }
        }

        _ => {
            result.append_string_value_preserving_props(format);
        }
    }

    false
}

#[allow(dead_code, clippy::too_many_arguments)] // split-state mode-line compatibility seam
fn format_mode_line_recursive_in_state_with_eval(
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
) -> Result<(), Flow> {
    if depth > 20 {
        return Ok(());
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
                    format_mode_line_recursive_in_state_with_eval(
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
                    )?;
                }
            }
        }

        _ if format.is_cons() => {
            let car = format.cons_car();
            let cdr = format.cons_cdr();

            if car.is_symbol_named(":eval") {
                if risky {
                    return Ok(());
                }
                if cdr.is_cons() {
                    let form_val = cdr.cons_car();
                    let val = eval_form(&form_val, buffers)?;
                    format_mode_line_recursive_in_state_with_eval(
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
                    format_mode_line_recursive_in_state_with_eval(
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
                    )?;
                    nested.apply_display_min_width(cdr.cons_cdr());
                    nested.overlay_properties(cdr.cons_cdr());
                    result.append_rendered(&nested);
                }
                return Ok(());
            }

            if let Some(lim) = car.as_fixnum() {
                let mut nested = ModeLineRendered::default();
                format_mode_line_recursive_in_state_with_eval(
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
                    format_mode_line_recursive_in_state_with_eval(
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
                    )?;
                }
                return Ok(());
            }

            if let Some(elements) = list_to_vec(format) {
                for elem in &elements {
                    format_mode_line_recursive_in_state_with_eval(
                        obarray,
                        dynamic,
                        buffers,
                        processes,
                        pctx,
                        elem,
                        result,
                        depth + 1,
                        risky,
                        eval_form,
                    )?;
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
                prefix_line_and_column(b, point_byte)
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
                    .map(|buffer| buffer.accessible_emacs_byte_region().range().len().get())
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
                    .map(|buffer| buffer.accessible_emacs_byte_region().range().len().get())
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
                // We model this with `pctx.target_width`:
                //   None    -> MODE_LINE_STRING, emit "--"
                //   Some(w) -> MODE_LINE_DISPLAY, emit `w - current`
                //              dashes. The entry point that enables
                //              display mode is
                //              `format_mode_line_for_display` below,
                //              used by the layout engine for TTY and
                //              GUI mode-line rendering.
                // `%-` needs to read `result.char_len()` to compute
                // the dash-fill width (in MODE_LINE_DISPLAY mode),
                // but `append_spec` holds a captured mutable borrow
                // on `result`. Drop the closure here by calling
                // `append_mode_line_rendered_segment` directly with
                // the pre-computed dash string.
                let dash_string: String = match pctx.target_width {
                    None => "--".to_string(),
                    Some(target) => {
                        let current = result.char_len();
                        if target > current {
                            "-".repeat(target - current)
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
                if pctx.is_tty_frame {
                    append_mode_line_percent_string_spec(
                        result,
                        &format!(
                            "{}{}{}",
                            pctx.terminal_coding_mnemonic,
                            pctx.keyboard_coding_mnemonic,
                            pctx.coding_mnemonic,
                        ),
                        &props_at_percent,
                        field_width,
                    );
                } else {
                    append_mode_line_percent_string_spec(
                        result,
                        &pctx.coding_mnemonic.to_string(),
                        &props_at_percent,
                        field_width,
                    );
                }
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
                let mut segment = if pctx.is_tty_frame {
                    ModeLineRendered::plain(format!(
                        "{}{}{}",
                        pctx.terminal_coding_mnemonic,
                        pctx.keyboard_coding_mnemonic,
                        pctx.coding_mnemonic,
                    ))
                } else {
                    ModeLineRendered::plain(pctx.coding_mnemonic.to_string())
                };
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

fn invisible_prop_member(propval: Value, list: Value) -> i64 {
    let mut tail = list;
    while tail.is_cons() {
        let element = tail.cons_car();
        if eq_value(&propval, &element) {
            return 1;
        }
        if element.is_cons() && eq_value(&propval, &element.cons_car()) {
            return if element.cons_cdr().is_nil() { 1 } else { 2 };
        }
        tail = tail.cons_cdr();
    }
    0
}

fn invisible_prop(propval: Value, list: Value) -> i64 {
    let direct = invisible_prop_member(propval, list);
    if direct != 0 {
        return direct;
    }

    let mut proptail = propval;
    while proptail.is_cons() {
        let result = invisible_prop_member(proptail.cons_car(), list);
        if result != 0 {
            return result;
        }
        proptail = proptail.cons_cdr();
    }
    0
}

pub(crate) fn text_prop_means_invisible(prop: Value, invisibility_spec: Value) -> i64 {
    if invisibility_spec == Value::T {
        i64::from(prop.is_truthy())
    } else {
        invisible_prop(prop, invisibility_spec)
    }
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
) -> Result<i64, Flow> {
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

pub(crate) fn zero_width_invisible_run_end_byte(
    eval: &mut super::eval::Context,
    buffer_id: BufferId,
    byte_pos: usize,
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

    if invisible_status_for_value(eval, Value::fixnum(lisp_pos))? != 1 {
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
    match invisible_status_for_value(eval, args[0])? {
        0 => Ok(Value::NIL),
        1 => Ok(Value::T),
        other => Ok(Value::fixnum(other)),
    }
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
        let last_row = snapshot.rows.iter().filter(is_text_row).next_back()?;
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

    let first_row = snapshot.rows.iter().filter(is_text_row).next()?;
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
/// Computes approximate pixel dimensions of text in the window region.
/// Uses the frame's character width/height as a monospace approximation.
pub(crate) fn builtin_window_text_pixel_size_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-text-pixel-size", &args, 0, 7)?;
    let (fid, wid) = resolve_live_window_for_text_pixel_size(&eval.frames, args.first())?;

    let Some(frame) = eval.frames.get(fid) else {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    };
    let char_w = frame.char_width;
    let char_h = frame.char_height;
    let window = frame.find_window(wid);
    let buf_id = window.and_then(|window| window.buffer_id());
    let wrap_columns = window
        .filter(|_| frame.effective_window_system().is_none())
        .map(|window| {
            let body_pixels =
                super::window_cmds::window_body_width_pixels(&eval.frames, fid, window).max(0);
            let body_columns = (body_pixels as f32 / char_w.max(1.0)).floor() as usize;
            // GNU reserves the final TTY column for the continuation glyph,
            // so wrapped text has one fewer usable column than
            // `window-body-width` reports.
            body_columns.saturating_sub(1).max(1)
        });

    let Some(buf_id) = buf_id else {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    };

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

    // Count lines and max columns in the region, honoring `display` text
    // properties (e.g. `(space :align-to N)`) that change a line's pixel width.
    let mut text_metrics = region_text_metrics_with_display(
        eval,
        buf_id,
        from_pos,
        to_pos,
        apply_trim,
        CharColumnWidth::One,
        None,
        None,
        wrap_columns,
    );
    if offset_landed_on_occupied_row && from_pos >= to_pos {
        // GNU keeps the adjusted iterator's current row in the vertical
        // extent even when its original, pre-clipped TO is now at or before
        // FROM.  No text width is traversed in that case.
        text_metrics = RegionTextMetrics {
            lines: 1,
            max_columns: 0,
        };
    }

    let width = (text_metrics.max_columns as f32 * char_w).ceil() as i64;
    let mode_line_rows = if window_text_pixel_size_includes_mode_line(args.get(5)) {
        1.0
    } else {
        0.0
    };
    let height = ((text_metrics.lines as f32 + mode_line_rows) * char_h).ceil() as i64;

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
/// Mirror GNU Emacs: return t if POS is visible in WINDOW, nil otherwise.
/// Checks if position is between window-start and an estimated window-end.
pub(crate) fn builtin_pos_visible_in_window_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("pos-visible-in-window-p", &args, 0, 3)?;
    validate_optional_window_designator_in_state(&eval.frames, args.get(1), "window-live-p")?;
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
    // GNU BUILDS `posn-at-point` out of this call with PARTIALLY non-nil
    // (src/keyboard.c:13073), so the two must not answer from different
    // geometry. Give this the same exact source, on-demand recomputation
    // included, before anything else runs.
    if let Some((_, metrics)) =
        resolve_exact_visible_metrics_with_layout(eval, args.get(1), args.first())?
    {
        if !args.get(2).is_some_and(|value| value.is_truthy()) {
            return Ok(Value::T);
        }
        return Ok(Value::list(vec![
            Value::fixnum(metrics.x),
            Value::fixnum(metrics.y),
        ]));
    }
    pos_visible_in_window_p_impl(&mut eval.frames, &mut eval.buffers, args)
}

fn pos_visible_in_window_p_impl(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("pos-visible-in-window-p", &args, 0, 3)?;
    validate_optional_window_designator_in_state(&*frames, args.get(1), "window-live-p")?;
    let partially = args.get(2).is_some_and(|v| v.is_truthy());
    if let Some((_, metrics)) =
        resolve_exact_visible_metrics(frames, buffers, args.get(1), args.first())?
    {
        if !partially {
            return Ok(Value::T);
        }
        return Ok(Value::list(vec![
            Value::fixnum(metrics.x),
            Value::fixnum(metrics.y),
        ]));
    }
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
    validate_optional_window_designator_in_state(&eval.frames, args.get(1), "window-live-p")?;
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
    window_line_height_impl(&mut eval.frames, &mut eval.buffers, args)
}

fn window_line_height_impl(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-line-height", &args, 0, 2)?;
    validate_optional_window_designator_in_state(&*frames, args.get(1), "window-live-p")?;
    if let Some((fid, wid)) = resolve_live_window_identity(frames, args.get(1))?
        && let Some(frame) = frames.get(fid)
        && let Some(snapshot) = frame.redisplay_snapshot(wid)
    {
        let line_spec = args.first().copied().unwrap_or(Value::NIL);
        let metrics = if line_spec.is_nil() {
            resolve_exact_visible_metrics(frames, buffers, args.get(1), None)?.and_then(
                |(_, metrics)| {
                    snapshot
                        .row_metrics(metrics.row)
                        .map(|row| snapshot_text_row_line_metrics(snapshot, row))
                },
            )
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
            "framep",
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
            "framep",
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
    predicate: &str,
) -> Result<(), Flow> {
    validate_optional_window_designator_in_state(&eval.frames, value, predicate)
}

fn validate_optional_window_designator_in_state(
    frames: &crate::window::FrameManager,
    value: Option<&Value>,
    predicate: &str,
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
    if let Some(wid) = wid {
        for fid in frames.frame_list() {
            if let Some(frame) = frames.get(fid)
                && frame.find_window(wid).is_some()
            {
                return Ok(());
            }
        }
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(predicate), *windowish],
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
    chars: Vec<char>,
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
    let chars = buffer.full_text_string().chars().collect::<Vec<_>>();
    let window_point = if frame.selected_window == wid {
        buffer.point_char_pos().to_lisp()
    } else {
        (*point).max(LispCharPos1::ONE)
    };

    Ok(Some(ApproxWindowDisplayContext {
        body_height,
        body_lines,
        body_cols,
        char_width,
        char_height,
        window_start: *window_start,
        window_point,
        chars,
    }))
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
    match pos {
        Some(value) if value.is_t() || value.is_symbol_named("t") => {
            Ok(Some(last_visible_row_start_lisp_pos(ctx)))
        }
        Some(value) if !value.is_nil() => {
            expect_integer_or_marker(value)?;
            let lisp_pos = value.as_int().unwrap_or(0).max(1) as usize;
            Ok(Some(LispCharPos1::from_one_based_usize(
                lisp_pos.min(ctx.chars.len().saturating_add(1)),
            )))
        }
        _ => Ok(Some(current_window_point_lisp(ctx))),
    }
}

fn current_window_point_lisp(ctx: &ApproxWindowDisplayContext) -> LispCharPos1 {
    LispCharPos1::from_one_based_usize(
        ctx.window_point
            .to_one_based_usize()
            .min(ctx.chars.len().saturating_add(1)),
    )
}

fn last_visible_row_start_lisp_pos(ctx: &ApproxWindowDisplayContext) -> LispCharPos1 {
    let row_start = nth_visible_row_start_char(
        &ctx.chars,
        ctx.window_start.to_one_based_usize().saturating_sub(1),
        ctx.body_lines.saturating_sub(1),
    );
    LispCharPos1::from_one_based_usize(
        row_start
            .saturating_add(1)
            .min(ctx.chars.len().saturating_add(1)),
    )
}

fn nth_visible_row_start_char(chars: &[char], mut start_char: usize, rows: i64) -> usize {
    start_char = start_char.min(chars.len());
    for _ in 0..rows.max(0) {
        if start_char >= chars.len() {
            return chars.len();
        }
        match chars[start_char..].iter().position(|ch| *ch == '\n') {
            Some(offset) => start_char += offset + 1,
            None => return chars.len(),
        }
    }
    start_char
}

fn row_col_for_lisp_pos(
    chars: &[char],
    start_char: usize,
    lisp_pos: LispCharPos1,
    wrap_cols: i64,
) -> Option<(i64, i64)> {
    let lisp_pos = usize::try_from(lisp_pos.as_i64().max(1)).ok()?;
    let target = lisp_pos.saturating_sub(1).min(chars.len());
    let mut row = 0_i64;
    let mut col = 0_i64;
    let wrap_cols = wrap_cols.max(1);
    let mut idx = start_char.min(chars.len());
    while idx < target {
        if chars[idx] == '\n' {
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
    let (row, col) = row_col_for_lisp_pos(&ctx.chars, start_char, pos_lisp, approx_wrap_cols(ctx))?;
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

fn approximate_point_at_coords(
    ctx: &ApproxWindowDisplayContext,
    x: i64,
    y: i64,
) -> Option<ExactVisibleMetrics> {
    if x < 0 || y < 0 {
        return None;
    }
    let start = usize::try_from(ctx.window_start.as_i64().max(1))
        .ok()?
        .saturating_sub(1)
        .min(ctx.chars.len());
    let char_width = ctx.char_width.max(1);
    let char_height = ctx.char_height.max(1);
    let query_row = (y / char_height).max(0);
    let query_col = (x / char_width).max(0);
    let wrap_cols = approx_wrap_cols(ctx);

    let mut row = 0_i64;
    let mut line_start = start;
    loop {
        let line_end = ctx.chars[line_start..]
            .iter()
            .position(|ch| *ch == '\n')
            .map_or(ctx.chars.len(), |offset| line_start + offset);
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
                .min(ctx.chars.len().saturating_add(1));

            return Some(ExactVisibleMetrics {
                point: LispCharPos1::from_one_based_usize(point),
                x,
                y,
                dx: x - chosen_col.saturating_mul(char_width),
                dy: y - query_row.saturating_mul(char_height),
                width: 0,
                height: 0,
                row: query_row,
                col: query_col,
            });
        }

        if line_end >= ctx.chars.len() {
            break;
        }
        row += visual_rows;
        line_start = line_end + 1;
    }

    Some(ExactVisibleMetrics {
        point: LispCharPos1::from_one_based_usize(ctx.chars.len().saturating_add(1)),
        x,
        y,
        dx: x,
        dy: y - query_row.saturating_mul(char_height),
        width: 0,
        height: 0,
        row: query_row,
        col: query_col,
    })
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
    // Prefer retained rows only while they are still VALID for the live
    // window.  Their columns are window-relative and therefore meaningless
    // without the horizontal origin they were produced at; see the freshness
    // note in `compute_terminal_window_geometry`.
    let retained_rows_valid = retained_rows_answer_for_live_window(eval, window)?;
    if retained_rows_valid
        && let Some(found) =
            resolve_exact_visible_metrics(&eval.frames, &eval.buffers, window, pos)?
    {
        return Ok(Some(found));
    }
    let Some((fid, wid)) = resolve_live_window_identity(&eval.frames, window)? else {
        return Ok(None);
    };
    if let Some(geometry) = compute_terminal_window_geometry(eval, fid, wid)? {
        let Some(ctx) =
            resolve_live_window_display_context(&mut eval.frames, &mut eval.buffers, window)?
        else {
            return Ok(None);
        };
        let Some(pos_lisp) = resolve_pos_visible_target_lisp_pos(&ctx, pos)? else {
            return Ok(None);
        };
        return Ok(geometry
            .point_for_buffer_pos(pos_lisp)
            .map(|point| (wid, exact_metrics_from_redisplay_point(&geometry, point))));
    }
    if retained_rows_valid {
        return Ok(None);
    }
    // No recompute was available.  Rows that are merely STALE are still the
    // closest thing to GNU's unconditional re-walk that exists here, so they
    // answer rather than nothing -- which is exactly what they did before the
    // preference above, so the freshness gate can only replace a stale answer
    // with a recomputed one and never with silence.
    resolve_exact_visible_metrics(&eval.frames, &eval.buffers, window, pos)
}

/// Whether the retained row map for WINDOW still describes the live window.
///
/// This is `vertical-motion`'s predicate, not a second one:
/// `Context::fresh_window_display_snapshot` compares the whole
/// [`crate::window::WindowDisplaySnapshotFreshness`] token, whose fields are
/// deliberately opaque so that "individual snapshot consumers [cannot] invent
/// partial freshness checks that drift apart".  A window-system frame answers
/// posn queries from its presented geometry, which carries its own staleness
/// states (see [`PresentedBufferPosition`]), so it is not this question.
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
    if frame.effective_window_system().is_some() {
        return Ok(true);
    }
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
) -> Result<Option<(WindowId, ExactVisibleMetrics)>, Flow> {
    let Some((fid, wid)) = resolve_live_window_identity(frames, window)? else {
        return Ok(None);
    };
    let Some(frame) = frames.get(fid) else {
        return Ok(None);
    };
    let Some(ctx) = resolve_live_window_display_context(frames, buffers, window)? else {
        return Ok(None);
    };
    let Some(pos_lisp) = resolve_pos_visible_target_lisp_pos(&ctx, pos)? else {
        return Ok(None);
    };
    if frame.effective_window_system().is_none() {
        let Some(snapshot) = frame.redisplay_snapshot(wid) else {
            return Ok(None);
        };
        return Ok(snapshot
            .point_for_buffer_pos(pos_lisp)
            .map(|point| (wid, exact_metrics_from_redisplay_point(snapshot, point))));
    }
    let Some(publication) = frame.active_presentation_geometry() else {
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
    if frameish.as_frame_id().is_none()
        && let Some(windowish) = resolve_live_window_identity(frames, Some(frameish))?
    {
        return Ok(Some((windowish.0, windowish.1, true)));
    }
    let fid = if let Some(id) = frameish.as_frame_id() {
        FrameId(id)
    } else if let Some(id) = frameish.as_fixnum().filter(|&id| id >= 0) {
        FrameId(id as u64)
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("framep"), *frameish],
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
    validate_optional_window_designator_in_state(&eval.frames, args.get(1), "window-live-p")?;
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
    let computed = match resolve_posn_at_xy_window(&eval.frames, args.get(2))? {
        Some((fid, wid, _)) => compute_terminal_window_geometry(eval, fid, wid)?,
        None => None,
    };
    posn_at_x_y_impl(&mut eval.frames, &mut eval.buffers, computed.as_ref(), args)
}

/// Run the canonical row producer for one terminal window that redisplay has
/// left no rows for; `None` whenever the retained snapshot can answer, the
/// frame is not a live terminal frame, or no frontend adapter is installed.
fn compute_terminal_window_geometry(
    eval: &mut super::eval::Context,
    fid: FrameId,
    wid: WindowId,
) -> Result<Option<WindowDisplaySnapshot>, Flow> {
    let Some(frame) = eval.frames.get(fid) else {
        return Ok(None);
    };
    // GNU's own gate: `pos_visible_p` returns false immediately for
    // `FRAME_INITIAL_P`, and a window-system frame answers from its presented
    // geometry rather than from a terminal row walk.
    if frame.initial || frame.effective_window_system().is_some() || eval.noninteractive() {
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
        .is_some_and(|snapshot| !snapshot.points.is_empty())
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

    if frame.effective_window_system().is_some() {
        let publication = frame.active_presentation_geometry().ok_or_else(|| {
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
            let Some(ctx) = resolve_live_window_display_context(frames, buffers, args.get(2))?
            else {
                return Ok(Value::NIL);
            };
            let Some(metrics) = approximate_point_at_coords(&ctx, at.text_area_x(), at.window_y())
            else {
                return Ok(Value::NIL);
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
/// `text-faces`, or `json`.
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
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid frame snapshot format: {value} (use text, text-faces or json)"
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
    eval.redisplay_with_force(true);
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

    // GNU `buffer-text-pixel-size` returns PIXELS: the measured column/row counts
    // scaled by the frame's character cell size. On a TTY the cell is 1x1 (so the
    // result equals the cell counts), on a GUI frame it is the real font
    // width/height. This mirrors `window-text-pixel-size` (which already scales by
    // `frame.char_width`). Without it, `string-pixel-width` returns columns, and the
    // mode-line `(space :align-to (- right-margin (string-pixel-width …)))` produced
    // by `mode-line-format-right-align` mis-aligns on GUI frames — the Doom dashboard
    // "DOOM vX" right segment is pushed off-screen.
    let (char_w, char_h) = eval
        .frames
        .selected_frame()
        .map(|f| (f.char_width.max(1.0), f.char_height.max(1.0)))
        .unwrap_or((1.0, 1.0));

    let buffers = &eval.buffers;

    let buffer_id = if args.is_empty() {
        resolve_buffer_designator_allow_nil_current_in_manager(buffers, &Value::NIL)?
    } else {
        resolve_buffer_designator_allow_nil_current_in_manager(buffers, &args[0])?
    };

    if args.len() > 1 {
        let window = &args[1];
        if !window.is_nil() && !window.is_window() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), *window],
            ));
        }
    }

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

    // Measure honoring `display` text properties (e.g. `(space :align-to N)` /
    // `(space :width N)`), shared with `window-text-pixel-size`.  Wide chars
    // contribute their display width, preserving the previous accounting.
    let metrics = crate::emacs_core::xdisp::region_text_metrics_with_display(
        eval,
        buffer_id,
        range.start(),
        range.end(),
        false,
        crate::emacs_core::xdisp::CharColumnWidth::DisplayWidth,
        x_limit,
        y_limit,
        None,
    );

    if metrics.lines == 0 {
        return Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)));
    }
    Ok(Value::cons(
        Value::fixnum((metrics.max_columns as f32 * char_w).ceil() as i64),
        Value::fixnum((metrics.lines as f32 * char_h).ceil() as i64),
    ))
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
