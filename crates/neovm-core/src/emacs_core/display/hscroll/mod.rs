//! Automatic horizontal scrolling (GNU `hscroll_window_tree`, src/xdisp.c).
//!
//! When a window's buffer line is *truncated* (does not wrap) and point moves
//! off the visible right edge — the classic `C-e` on a long line with
//! `truncate-lines` non-nil — GNU recomputes `w->hscroll` so the cursor stays
//! visible.  neomacs renders with whatever `hscroll` it is given but, before
//! this module existed, never *computed* one to follow the cursor, so the
//! cursor dropped off-screen (issue #140).
//!
//! GNU's real algorithm walks the laid-out display matrix in pixels.  neomacs
//! splits text layout into a separate engine that runs *after* this point, so
//! we reproduce GNU's decision in the equivalent **column form** (LTR), using
//! `current-column` (tab/char-width aware) for point's true column even when it
//! is off-screen.  The pixel form and the column form agree because every term
//! in GNU's comparison is a multiple of the frame column width `colw`:
//! dividing GNU's `>=`/`<=` pixel comparisons through by `colw` yields the
//! column comparisons below, and GNU's `hscroll = max(0, X - wanted_px) / colw`
//! becomes `max(0, point_col - wanted_col)`.
//!
//! See `tmp/hscroll_gnu_spec.txt` for the full pixel-form spec this mirrors.
//!
//! This module is the **pure** core (`compute_auto_hscroll`) plus a thin
//! per-window redisplay pass (`update_auto_hscroll_before_redisplay`) that
//! feeds it the live window/buffer geometry and writes the result back to
//! `Window::Leaf.hscroll`.  Writing that single field makes BOTH the cursor
//! visible (the layout engine renders with the new hscroll) AND
//! `(window-hscroll)` correct (GNU parity) — no separate write-back is needed.

use crate::emacs_core::value::Value;

/// Decoded form of the `hscroll-step` variable (GNU STEP 0 in the spec).
///
/// `hscroll-step` is a single Lisp value with three meanings: 0/nil/negative
/// means "center point", a positive integer means "scroll that many columns",
/// and a positive float means "that fraction of the window width".
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HscrollStep {
    /// Default: center point (or near the right end at EOL) in the window.
    Center,
    /// Scroll so point sits this many columns in from the relevant edge.
    Columns(i64),
    /// Scroll so point sits this fraction of the window width in from the edge.
    Fraction(f64),
}

impl HscrollStep {
    /// Decode the raw `hscroll-step` Lisp value (GNU STEP 0).
    ///
    /// * A float >= 0 selects the relative (fraction) form.  A negative float
    ///   degrades to `Center` (GNU clears `hscroll_relative_p`).
    /// * An integer > 0 selects the absolute (columns) form.  <= 0 -> `Center`.
    /// * Anything else (nil, t, symbols, out-of-range) -> `Center`.
    pub(crate) fn decode(value: Option<&Value>) -> Self {
        match value {
            Some(v) => {
                if let Some(f) = v.as_float() {
                    if f >= 0.0 {
                        HscrollStep::Fraction(f)
                    } else {
                        HscrollStep::Center
                    }
                } else if let Some(n) = v.as_int() {
                    if n > 0 {
                        HscrollStep::Columns(n)
                    } else {
                        HscrollStep::Center
                    }
                } else {
                    HscrollStep::Center
                }
            }
            None => HscrollStep::Center,
        }
    }
}

/// Inputs to the pure auto-hscroll computation, all in display columns.
///
/// Mirrors the per-leaf-window state GNU's `hscroll_window_tree` reads, reduced
/// to the LTR column form (see module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AutoHscrollInput {
    /// Display column of point on its line (`current-column` at point).
    pub point_col: i64,
    /// Window text-area width in columns.
    pub text_cols: i64,
    /// `hscroll-margin` clamped to a sane range, in columns.
    pub h_margin: i64,
    /// The window's current `hscroll` (columns).
    pub cur_hscroll: i64,
    /// Lower bound on auto hscroll (`w->min_hscroll`); usually 0.
    pub min_hscroll: i64,
    /// Whether point's line is *truncated* (does not wrap) in this window —
    /// i.e. the effective wrap mode is Truncate.  GNU only auto-hscrolls
    /// truncated lines; a wrapped line is fully visible by construction.
    pub line_truncated: bool,
    /// GNU's `hscl` (`hscrolling_current_line_p`, src/xdisp.c:3074): auto
    /// hscroll is not suspended *and* `auto-hscroll-mode` is `current-line`.
    /// Only that mode arms trigger (C) below.
    pub hscrolling_current_line: bool,
    /// Whether point sits at the end of its display line (GNU
    /// `ITERATOR_AT_END_OF_LINE_P`).  This is the `C-e` case: GNU then targets
    /// the right end of the window (`text_cols - 4`) instead of the center, so
    /// the maximum amount of preceding text stays visible.
    pub point_at_eol: bool,
    /// Decoded `hscroll-step`.
    pub step: HscrollStep,
}

impl AutoHscrollInput {
    /// Point's column *relative to the current left edge* (what is on screen).
    ///
    /// GNU's STEP 5 compares `w->cursor.x` (a screen-relative pixel x) against
    /// the margins; in column form that is `point_col - cur_hscroll`.
    fn screen_col(&self) -> i64 {
        self.point_col - self.cur_hscroll
    }
}

/// Compute the new `hscroll` for a window following point, or `None` if no
/// change is needed.
///
/// Pure mirror of GNU `hscroll_window_tree`'s STEP 5 (trigger) + STEP 7
/// (compute) + STEP 8 (commit), in LTR column form.  Returns `Some(new)` only
/// when a recompute is triggered AND `new != cur_hscroll`.
pub(crate) fn compute_auto_hscroll(input: &AutoHscrollInput) -> Option<i64> {
    // STEP 5 trigger: GNU only auto-hscrolls when the line is truncated. A
    // wrapped line is fully visible, so the cursor never leaves the window
    // horizontally and there is nothing to follow.
    if !input.line_truncated {
        return None;
    }

    let screen_col = input.screen_col();
    // GNU's `text_area_width - h_margin` right edge.  When point's
    // screen-relative column reaches this, it is inside the right margin.
    let right_edge = input.text_cols - input.h_margin;

    // (A) Point inside the LEFT margin while already hscrolled -> scroll back
    //     toward column 0.  GNU: `w->hscroll != 0 && cursor.x <= h_margin`.
    let in_left_margin = input.cur_hscroll != 0 && screen_col <= input.h_margin;
    // (B) Point inside the RIGHT margin on a right-truncated line -> scroll
    //     right (the `C-e` case).  GNU: `truncated_on_right_p && cursor.x >=
    //     text_area_width - h_margin`.  In column form a truncated line whose
    //     point sits at/after the right edge is by construction truncated on
    //     the right.
    let in_right_margin = screen_col >= right_edge;
    // (C) Moved onto a short line that needs no hscroll while still hscrolled
    //     -> reset.  GNU: `hscl && w->hscroll != w->min_hscroll &&
    //     !truncated_on_left_p`.  In column form: currently hscrolled away from
    //     the minimum, yet point fits unscrolled (its true column is left of
    //     the right margin).  `truncated_on_left_p` is the consequence of
    //     hscroll>0; we approximate it via "point fits unscrolled".
    //     `hscl` is load-bearing: it is only true under
    //     `auto-hscroll-mode' = `current-line', so with the default `t' GNU
    //     never resets a window's hscroll from this branch, and an explicit
    //     `set-window-hscroll' survives redisplay.
    let fits_unscrolled = input.point_col < right_edge;
    let reset_short_line =
        input.hscrolling_current_line && input.cur_hscroll != input.min_hscroll && fits_unscrolled;

    if !(in_left_margin || in_right_margin || reset_short_line) {
        return None;
    }

    // STEP 7 compute (column form).  X is point's *absolute* column.
    let x = input.point_col;
    let new_hscroll = match input.step {
        HscrollStep::Center => {
            // Default centering target (GNU STEP 7):
            //   target_left = ITERATOR_AT_END_OF_LINE_P ? text_cols - 4
            //                                            : text_cols / 2
            // At end of line (the `C-e` case) GNU pushes point to the right end
            // of the window — only 4 columns of slack — so the maximum amount
            // of the preceding line stays visible.  Otherwise it centers point.
            // Live GNU on a 300-col line / 160-col window: at EOL hscroll =
            // 300 - 156 = 144 (cursor at screen col 156); mid-line it would be
            // 300 - 80 = 220 (cursor at screen col 80).
            let target_left = if input.point_at_eol {
                (input.text_cols - 4).max(0)
            } else {
                input.text_cols / 2
            };
            (x - target_left).max(0)
        }
        HscrollStep::Columns(n) => {
            // Absolute step.  When point is at the right edge, leave N columns
            // of slack on the right; otherwise leave N columns on the left.
            let wanted = if in_right_margin {
                input.text_cols - n - input.h_margin
            } else {
                n + input.h_margin
            };
            (x - wanted).max(0)
        }
        HscrollStep::Fraction(f) => {
            // Relative step: a fraction of the window width from the edge.
            let wanted = if in_right_margin {
                ((input.text_cols as f64) * (1.0 - f)) as i64 - input.h_margin
            } else {
                ((input.text_cols as f64) * f) as i64 + input.h_margin
            };
            (x - wanted).max(0)
        }
    };

    // Never go below the user lower bound.
    let new_hscroll = new_hscroll.max(input.min_hscroll);

    // STEP 8 commit: only report a change.
    (new_hscroll != input.cur_hscroll).then_some(new_hscroll)
}

// ---------------------------------------------------------------------------
// Redisplay integration: the per-window pass that feeds the pure function.
// ---------------------------------------------------------------------------

use crate::emacs_core::eval::Context;
use crate::window::{FrameId, Window, WindowId};

/// One leaf window's snapshot, gathered while the frame is borrowed immutably,
/// so the column computation (which needs `&mut Context`) and the write-back
/// (which needs `&mut` the window) run after the frame borrow is released.
struct LeafHscrollSnapshot {
    frame_id: FrameId,
    window_id: WindowId,
    buffer_id: crate::buffer::BufferId,
    /// Byte position of point in this window (selected -> buffer PT; otherwise
    /// the window's own `pointm`).
    point_byte: crate::buffer::EmacsBytePos,
    /// Lisp char position of point in this window, used to drive the GNU
    /// STEP 4 un-suspend comparison against the window's `old_point` and to
    /// record the new `old_point` after the pass.
    point_lisp: crate::buffer::LispCharPos1,
    /// The window's stored `old_point` (`w->old_pointm`) at the start of this
    /// pass.
    old_point_lisp: crate::buffer::LispCharPos1,
    /// Text-area width in columns, already reduced by the line-number gutter
    /// (FOLLOW-UP B: GNU subtracts `x_offset`).  This is the usable
    /// line-content width fed to the pure computation.
    text_cols: i64,
    h_margin: i64,
    cur_hscroll: i64,
    /// The window's `min_hscroll` lower bound (`w->min_hscroll`).
    min_hscroll: i64,
    /// The window's `suspend_auto_hscroll` flag at the start of this pass.
    suspend_auto_hscroll: bool,
    /// GNU's `hscl`.  Note GNU computes it (src/xdisp.c:16644) *before* the
    /// STEP 4 un-suspend below, so it reads the pre-pass suspend flag.
    hscrolling_current_line: bool,
    line_truncated: bool,
    point_at_eol: bool,
    step: HscrollStep,
}

/// Decoded `auto-hscroll-mode`.
///
/// GNU reads this variable buffer-locally in two places that mean different
/// things: `hscroll_window_tree` gates the whole pass on it being non-nil, and
/// `hscrolling_current_line_p` (src/xdisp.c:3074) tests it against
/// `current-line` specifically.  Naming the three cases keeps the second test
/// from silently degrading into "non-nil", which is what made an explicit
/// `set-window-hscroll` get reset to 0 by the very next redisplay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoHscrollMode {
    /// nil: no automatic horizontal scrolling for this window at all.
    Off,
    /// `current-line`: hscroll only the line point is on.
    CurrentLine,
    /// t, or any other non-nil value: hscroll the whole window.
    AllLines,
}

/// Read `auto-hscroll-mode` buffer-local-then-global.  GNU's default when the
/// variable is unbound is the `t` behaviour.
fn auto_hscroll_mode(ctx: &Context, buffer_id: crate::buffer::BufferId) -> AutoHscrollMode {
    let buf = ctx.buffers.get(buffer_id);
    let value = crate::emacs_core::indent::dynamic_buffer_or_global_symbol_value(
        &ctx.obarray,
        &[],
        buf,
        "auto-hscroll-mode",
    )
    .unwrap_or(Value::T);
    if value.is_nil() {
        AutoHscrollMode::Off
    } else if value.is_symbol_named("current-line") {
        AutoHscrollMode::CurrentLine
    } else {
        AutoHscrollMode::AllLines
    }
}

/// Whether point's line is *truncated* (does not wrap) in this window, matching
/// the layout engine's `effective_wrap_mode` exactly so the trigger condition
/// agrees with how the row will actually be laid out.
fn line_is_truncated(ctx: &Context, frame: &crate::window::Frame, window: &Window) -> bool {
    let Some(buffer_id) = window.buffer_id() else {
        return false;
    };
    let buf = ctx.buffers.get(buffer_id);
    let read = |name: &str| {
        crate::emacs_core::indent::dynamic_buffer_or_global_symbol_value(
            &ctx.obarray,
            &[],
            buf,
            name,
        )
    };

    // truncate-lines non-nil -> always truncate.
    if read("truncate-lines").is_some_and(|v| !v.is_nil()) {
        return true;
    }
    // A horizontally scrolled window is always in truncate mode (GNU
    // init_iterator only enables wrapping when hscroll == 0).
    if leaf_hscroll(window) != 0 {
        return true;
    }

    // Partial-width truncation (GNU `truncate-partial-width-windows`).
    let char_width = frame.char_width;
    let total_cols = if char_width > 0.0 {
        (window.bounds().width / char_width) as i64
    } else {
        0
    };
    let frame_cols = frame
        .parameter("width")
        .and_then(|value| value.as_int())
        .unwrap_or(frame.columns() as i64);
    if total_cols >= frame_cols {
        return false;
    }
    match read("truncate-partial-width-windows") {
        Some(value) if value.is_nil() => false,
        Some(value) if value.is_fixnum() => total_cols < value.as_fixnum().unwrap(),
        Some(_) => true,
        None => false,
    }
}

fn leaf_hscroll(window: &Window) -> i64 {
    match window {
        Window::Leaf { hscroll, .. } => *hscroll as i64,
        Window::Internal { .. } => 0,
    }
}

/// GNU `clip_to_bounds(0, hscroll_margin, 1000000)` of the `hscroll-margin`
/// variable, in columns.
fn hscroll_margin_cols(ctx: &Context) -> i64 {
    ctx.obarray
        .symbol_value("hscroll-margin")
        .and_then(|v| v.as_int())
        .unwrap_or(5)
        .clamp(0, 1_000_000)
}

/// Update each leaf window's `hscroll` to follow point before the layout engine
/// reads it, mirroring GNU `hscroll_window_tree` (called from
/// `redisplay_internal` before window layout, src/xdisp.c).
///
/// Writing `Window::Leaf.hscroll` here is the single source of truth: the
/// layout engine renders with the new value (cursor stays visible) AND
/// `(window-hscroll)` returns it (GNU parity), with no post-layout write-back.
pub(crate) fn update_auto_hscroll_before_redisplay(ctx: &mut Context) {
    // Phase 1: gather snapshots under an immutable frame borrow.
    let mut snapshots: Vec<LeafHscrollSnapshot> = Vec::new();

    let frame_ids: Vec<FrameId> = ctx.frames.frame_list();
    let selected_buffer_pt = ctx
        .buffers
        .current_buffer()
        .map(|b| (b.id, b.point_emacs_byte_pos()));

    for frame_id in &frame_ids {
        let Some(frame) = ctx.frames.get(*frame_id) else {
            continue;
        };
        if !frame.visible {
            continue;
        }
        let selected_window_id = frame.selected_window;

        let mut leaf_ids = frame.root_window.leaf_ids();
        if let Some(mini) = &frame.minibuffer_leaf {
            leaf_ids.push(mini.id());
        }

        for win_id in leaf_ids {
            let window = frame
                .root_window
                .find(win_id)
                .or_else(|| frame.minibuffer_leaf.as_ref().filter(|m| m.id() == win_id));
            let Some(window) = window else {
                continue;
            };
            let Some(buffer_id) = window.buffer_id() else {
                continue;
            };

            // auto-hscroll-mode nil disables the whole thing for this window.
            let mode = auto_hscroll_mode(ctx, buffer_id);
            if mode == AutoHscrollMode::Off {
                continue;
            }

            let cur_hscroll = leaf_hscroll(window);
            let min_hscroll = window.min_hscroll() as i64;
            let suspend_auto_hscroll = window.suspend_auto_hscroll();
            let old_point_lisp = leaf_old_point_lisp(window);
            let line_truncated = line_is_truncated(ctx, frame, window);

            // Text-area width in columns (matches window-body-width / layout).
            let char_width = frame.char_width.max(1.0);
            let char_height = frame.char_height;
            let body_px = crate::emacs_core::window_cmds::window_body_width_pixels(
                &ctx.frames,
                *frame_id,
                window,
            );
            // FOLLOW-UP B: when `display-line-numbers` is on, the gutter
            // consumes columns at the left, so the usable line-content width is
            // `body_cols - gutter_cols` — GNU subtracts the gutter pixel width
            // (`x_offset`) in the same place (hscroll_window_tree STEP 3).
            let body_cols = (body_px as f32 / char_width).floor() as i64;
            let window_height = window.bounds().height;
            let gutter_cols = match ctx.buffers.get(buffer_id) {
                Some(buf) => crate::emacs_core::xdisp::line_number_gutter_cols(
                    buf,
                    window_height,
                    char_height,
                ),
                None => 0,
            };
            let text_cols = body_cols - gutter_cols;
            if text_cols <= 0 {
                continue;
            }

            // Point byte position: buffer PT for the selected window of the
            // selected frame, else the window's own pointm.
            let selected_frame_id = ctx.frames.selected_frame().map(|f| f.id);
            let is_selected_window =
                selected_frame_id == Some(*frame_id) && selected_window_id == win_id;
            let (point_byte, point_lisp) =
                if is_selected_window && selected_buffer_pt.map(|(id, _)| id) == Some(buffer_id) {
                    let byte = selected_buffer_pt.unwrap().1;
                    let lisp = match ctx.buffers.get(buffer_id) {
                        Some(buf) => buf.emacs_byte_pos_to_char_pos_clamped(byte).to_lisp(),
                        None => continue,
                    };
                    (byte, lisp)
                } else {
                    let lisp = window_point_lisp(window);
                    let char_pos = lisp.to_char_pos();
                    match ctx.buffers.get(buffer_id) {
                        Some(buf) => (buf.char_pos_to_emacs_byte_pos_clamped(char_pos), lisp),
                        None => continue,
                    }
                };

            // GNU `ITERATOR_AT_END_OF_LINE_P` (dispextern.h:2941) is true iff the
            // character AT point is a newline (`it->c == '\n'`). It is therefore
            // FALSE at end-of-buffer with no trailing newline (there is no such
            // char), where GNU centers point (`text_cols / 2`) rather than using
            // the end-of-line target (`text_cols - 4`). Checking only the char at
            // point — NOT `point >= ZV` — is what distinguishes `C-e` before a
            // newline (hscroll 144 in the spec scenario) from `C-e` at EOB on a
            // line with no newline (hscroll 220).
            let point_at_eol = match ctx.buffers.get(buffer_id) {
                Some(buf) => buf.emacs_byte_at_pos(point_byte) == Some(b'\n'),
                None => false,
            };

            snapshots.push(LeafHscrollSnapshot {
                frame_id: *frame_id,
                window_id: win_id,
                buffer_id,
                point_byte,
                point_lisp,
                old_point_lisp,
                text_cols,
                h_margin: hscroll_margin_cols(ctx),
                cur_hscroll,
                min_hscroll,
                suspend_auto_hscroll,
                hscrolling_current_line: !suspend_auto_hscroll
                    && mode == AutoHscrollMode::CurrentLine,
                line_truncated,
                point_at_eol,
                step: HscrollStep::decode(ctx.obarray.symbol_value("hscroll-step")),
            });
        }
    }

    if snapshots.is_empty() {
        return;
    }

    // Phase 2 + 3: run GNU `hscroll_window_tree` STEP 4 (un-suspend + record
    // old point) and STEP 5/7/8 (trigger + compute + commit) per window. The
    // un-suspend bookkeeping happens every pass regardless of whether a
    // recompute fires, so it always writes back; the recompute needs
    // `&mut Context` for point's true column and so runs in this phase.
    for snap in snapshots {
        // STEP 4: if auto hscroll is suspended and window point has explicitly
        // moved since the last pass, un-suspend. Record the new old point
        // unconditionally (GNU's `Fset_marker (w->old_pointm, ...)`).
        let effective_suspend = snap.suspend_auto_hscroll && snap.point_lisp == snap.old_point_lisp;
        if (snap.suspend_auto_hscroll != effective_suspend
            || snap.old_point_lisp != snap.point_lisp)
            && let Some(frame) = ctx.frames.get_mut(snap.frame_id)
        {
            let window = frame.root_window.find_mut(snap.window_id).or_else(|| {
                frame
                    .minibuffer_leaf
                    .as_mut()
                    .filter(|m| m.id() == snap.window_id)
            });
            if let Some(Window::Leaf {
                old_point,
                suspend_auto_hscroll,
                ..
            }) = window
            {
                *suspend_auto_hscroll = effective_suspend;
                *old_point = snap.point_lisp.max(crate::buffer::LispCharPos1::ONE);
            }
        }

        // STEP 5 trigger requires auto hscroll not (still) suspended.
        if effective_suspend {
            continue;
        }

        let point_col = match crate::emacs_core::indent::display_column_at_emacs_byte_pos(
            ctx,
            snap.buffer_id,
            snap.point_byte,
        ) {
            Ok(col) => col as i64,
            Err(_) => continue,
        };

        let input = AutoHscrollInput {
            point_col,
            text_cols: snap.text_cols,
            h_margin: snap.h_margin,
            cur_hscroll: snap.cur_hscroll,
            min_hscroll: snap.min_hscroll,
            hscrolling_current_line: snap.hscrolling_current_line,
            line_truncated: snap.line_truncated,
            point_at_eol: snap.point_at_eol,
            step: snap.step,
        };

        let Some(new_hscroll) = compute_auto_hscroll(&input) else {
            continue;
        };

        if let Some(frame) = ctx.frames.get_mut(snap.frame_id) {
            let window = frame.root_window.find_mut(snap.window_id).or_else(|| {
                frame
                    .minibuffer_leaf
                    .as_mut()
                    .filter(|m| m.id() == snap.window_id)
            });
            if let Some(Window::Leaf { hscroll, .. }) = window {
                tracing::debug!(
                    "auto-hscroll: win={:?} point_col={} text_cols={} {} -> {}",
                    snap.window_id,
                    point_col,
                    snap.text_cols,
                    *hscroll,
                    new_hscroll,
                );
                *hscroll = new_hscroll.max(0) as usize;
            }
        }
    }
}

/// Read a window's `old_point` (`w->old_pointm`) as a Lisp char position.
fn leaf_old_point_lisp(window: &Window) -> crate::buffer::LispCharPos1 {
    match window {
        Window::Leaf { old_point, .. } => *old_point,
        Window::Internal { .. } => crate::buffer::LispCharPos1::ONE,
    }
}

/// Read a window's `pointm` as a Lisp char position.
fn window_point_lisp(window: &Window) -> crate::buffer::LispCharPos1 {
    match window {
        Window::Leaf { point, .. } => *point,
        Window::Internal { .. } => crate::buffer::LispCharPos1::ONE,
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
