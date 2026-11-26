//! Unit tests for the pure auto-hscroll computation
//! (`super::compute_auto_hscroll`).
//!
//! These are the fast iteration loop for issue #140.  Each case is the column
//! form of a GNU `hscroll_window_tree` scenario; the headline case is the
//! oracle-verified `C-e` on a 300-column line in a 160-column window with the
//! default `hscroll-step` of 0, which GNU resolves to `window-hscroll` = 220.

use super::*;

/// Convenience builder: a mid-line centered (default `hscroll-step`) truncated
/// line (`point_at_eol = false`, so the `text_cols / 2` target applies).
fn centered(point_col: i64, text_cols: i64, h_margin: i64, cur_hscroll: i64) -> AutoHscrollInput {
    AutoHscrollInput {
        point_col,
        text_cols,
        h_margin,
        cur_hscroll,
        min_hscroll: 0,
        // GNU's default `auto-hscroll-mode' is t, not `current-line', so `hscl'
        // is false in every scenario that does not say otherwise.
        hscrolling_current_line: false,
        line_truncated: true,
        point_at_eol: false,
        step: HscrollStep::Center,
    }
}

/// Convenience builder for the end-of-line (`C-e`) case: GNU targets
/// `text_cols - 4` instead of the center.
fn at_eol(point_col: i64, text_cols: i64, h_margin: i64, cur_hscroll: i64) -> AutoHscrollInput {
    let mut input = centered(point_col, text_cols, h_margin, cur_hscroll);
    input.point_at_eol = true;
    input
}

// -------------------------------------------------------------------------
// The issue #140 headline case.
// -------------------------------------------------------------------------

#[test]
fn ce_on_long_line_targets_window_right_end_oracle_144() {
    // C-e puts point at end of line, so GNU uses the `text_cols - 4` target,
    // NOT the center. Live GNU on a 300-col line / 160-col window: cursor lands
    // at screen col 156, window-hscroll = 300 - (160 - 4) = 144.
    let input = at_eol(300, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), Some(144));
}

#[test]
fn ce_already_scrolled_to_eol_target_no_change() {
    // If we are already at the EOL target, no further change.
    let input = at_eol(300, 160, 5, 144);
    assert_eq!(compute_auto_hscroll(&input), None);
}

#[test]
fn mid_line_centers_point_220() {
    // The same column but NOT at end of line (point in the middle of a longer
    // line) uses the center target: 300 - 160/2 = 220.
    let input = centered(300, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), Some(220));
}

// -------------------------------------------------------------------------
// Point already visible -> no scroll.
// -------------------------------------------------------------------------

#[test]
fn point_visible_unscrolled_no_change() {
    // Short line, point at column 40 in a 160-col window, not hscrolled.
    // Point is well within the window (40 < 160 - 5), so no trigger fires.
    let input = centered(40, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), None);
}

#[test]
fn point_just_inside_right_margin_no_trigger() {
    // screen_col = 154, right_edge = 160 - 5 = 155. 154 < 155 -> not in margin.
    let input = centered(154, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), None);
}

#[test]
fn point_exactly_at_right_margin_triggers() {
    // screen_col = 155 == right_edge -> triggers (>= comparison).
    // new = 155 - 80 = 75.
    let input = centered(155, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), Some(75));
}

// -------------------------------------------------------------------------
// Short-line reset (case C): hscrolled, then point fits unscrolled.
// -------------------------------------------------------------------------

#[test]
fn reset_onto_short_line_back_to_zero() {
    // Window is hscrolled to 220 but point is now at column 10 (short line):
    // 10 < 155 fits unscrolled, and cur_hscroll(220) != min_hscroll(0).
    // Centering target: 10 - 80 = -70 -> max(0) = 0.
    let input = centered(10, 160, 5, 220);
    assert_eq!(compute_auto_hscroll(&input), Some(0));
}

#[test]
fn reset_respects_min_hscroll_lower_bound() {
    // Same short line but min_hscroll = 30: never go below it.
    let mut input = centered(10, 160, 5, 220);
    input.min_hscroll = 30;
    assert_eq!(compute_auto_hscroll(&input), Some(30));
}

#[test]
fn eol_target_respects_min_hscroll_floor() {
    // FOLLOW-UP A: when scroll-left set a min_hscroll higher than the natural
    // EOL target, the floor wins. C-e on a 300-col line / 160-col window would
    // compute 300 - 156 = 144, but min_hscroll = 200 floors it to 200.
    let mut input = at_eol(300, 160, 5, 0);
    input.min_hscroll = 200;
    assert_eq!(compute_auto_hscroll(&input), Some(200));
}

/// GNU arms case (C) only through `hscl` (`hscrolling_current_line_p`,
/// src/xdisp.c:16644 and :3074), i.e. only under `auto-hscroll-mode' =
/// `current-line'.  Under the default `t' a window whose point sits to the
/// RIGHT of its hscroll -- so cases (A) and (B) both stay quiet -- must keep the
/// hscroll it was explicitly given.  This is the shape `set-window-hscroll'
/// leaves behind, and without the `hscl` guard the next redisplay pass reset it
/// to 0.
#[test]
fn explicit_hscroll_survives_when_only_current_line_mode_would_reset() {
    // A 40-column window, point at column 9, window hscrolled to 2: point is
    // on screen (screen col 7) and nowhere near either margin.
    let input = centered(9, 40, 0, 2);
    assert_eq!(compute_auto_hscroll(&input), None);
}

/// The same scenario with `auto-hscroll-mode' = `current-line' does reset,
/// which is the behaviour case (C) exists to provide.
#[test]
fn current_line_mode_resets_the_same_short_line() {
    let mut input = centered(9, 40, 0, 2);
    input.hscrolling_current_line = true;
    assert_eq!(compute_auto_hscroll(&input), Some(0));
}

#[test]
fn at_min_hscroll_short_line_no_change() {
    // Already at min_hscroll and point fits -> case C does not fire
    // (cur_hscroll == min_hscroll), and point is left of the right edge so the
    // right-margin case does not fire either.
    let input = centered(10, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), None);
}

// -------------------------------------------------------------------------
// Left-margin scroll-back (case A).
// -------------------------------------------------------------------------

#[test]
fn left_margin_scrolls_back() {
    // Hscrolled to 220, point moved left to column 222: screen_col = 2 <=
    // h_margin(5) -> case A. Point still well past column 80, so centering
    // pulls hscroll back to 222 - 80 = 142.
    let input = centered(222, 160, 5, 220);
    assert_eq!(compute_auto_hscroll(&input), Some(142));
}

#[test]
fn left_margin_not_triggered_when_not_hscrolled() {
    // cur_hscroll == 0 disables case A even though screen_col <= h_margin.
    // And point fits unscrolled so nothing else fires.
    let input = centered(3, 160, 5, 0);
    assert_eq!(compute_auto_hscroll(&input), None);
}

// -------------------------------------------------------------------------
// Wrapped lines never auto-hscroll.
// -------------------------------------------------------------------------

#[test]
fn wrapped_line_never_scrolls() {
    let mut input = centered(300, 160, 5, 0);
    input.line_truncated = false;
    assert_eq!(compute_auto_hscroll(&input), None);
}

// -------------------------------------------------------------------------
// Integer hscroll-step = N.
// -------------------------------------------------------------------------

#[test]
fn integer_step_right_margin_leaves_slack() {
    // hscroll-step = 10. Right-margin case (point at col 300, screen_col 300 >=
    // 155). wanted = text_cols - N - h_margin = 160 - 10 - 5 = 145.
    // new = 300 - 145 = 155.
    let mut input = centered(300, 160, 5, 0);
    input.step = HscrollStep::Columns(10);
    assert_eq!(compute_auto_hscroll(&input), Some(155));
}

#[test]
fn integer_step_left_margin_case() {
    // hscroll-step = 10, hscrolled to 220, point at col 222 (left margin).
    // Not the right-margin branch -> wanted = N + h_margin = 10 + 5 = 15.
    // new = 222 - 15 = 207.
    let mut input = centered(222, 160, 5, 220);
    input.step = HscrollStep::Columns(10);
    assert_eq!(compute_auto_hscroll(&input), Some(207));
}

// -------------------------------------------------------------------------
// Float (relative) hscroll-step.
// -------------------------------------------------------------------------

#[test]
fn fraction_step_right_margin() {
    // hscroll-step = 0.25 (fraction). Right-margin case.
    // wanted = text_cols * (1 - 0.25) - h_margin = 160 * 0.75 - 5 = 120 - 5 = 115.
    // new = 300 - 115 = 185.
    let mut input = centered(300, 160, 5, 0);
    input.step = HscrollStep::Fraction(0.25);
    assert_eq!(compute_auto_hscroll(&input), Some(185));
}

// -------------------------------------------------------------------------
// hscroll-step decode (STEP 0).
// -------------------------------------------------------------------------

#[test]
fn decode_default_zero_is_center() {
    assert_eq!(
        HscrollStep::decode(Some(&Value::fixnum(0))),
        HscrollStep::Center
    );
}

#[test]
fn decode_positive_integer_is_columns() {
    assert_eq!(
        HscrollStep::decode(Some(&Value::fixnum(7))),
        HscrollStep::Columns(7)
    );
}

#[test]
fn decode_negative_integer_is_center() {
    assert_eq!(
        HscrollStep::decode(Some(&Value::fixnum(-3))),
        HscrollStep::Center
    );
}

#[test]
fn decode_positive_float_is_fraction() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        HscrollStep::decode(Some(&Value::make_float(0.3))),
        HscrollStep::Fraction(0.3)
    );
}

#[test]
fn decode_negative_float_is_center() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        HscrollStep::decode(Some(&Value::make_float(-0.3))),
        HscrollStep::Center
    );
}

#[test]
fn decode_nil_is_center() {
    assert_eq!(HscrollStep::decode(Some(&Value::NIL)), HscrollStep::Center);
    assert_eq!(HscrollStep::decode(None), HscrollStep::Center);
}

// -------------------------------------------------------------------------
// h_margin clamping interaction (margin pushes the right edge inward).
// -------------------------------------------------------------------------

#[test]
fn larger_margin_triggers_earlier() {
    // h_margin = 20: right_edge = 140. Point at col 145 (screen_col 145 >= 140)
    // triggers; new = 145 - 80 = 65.
    let input = centered(145, 160, 20, 0);
    assert_eq!(compute_auto_hscroll(&input), Some(65));
}

// -------------------------------------------------------------------------
// The redisplay pass: no column scan for a window that wraps lines.
// -------------------------------------------------------------------------

/// A visible frame whose selected window shows the current buffer, holding
/// one 1,000,000-character line with point at its end.
fn context_with_point_at_the_end_of_a_long_line() -> (Context, neovm_edit::buffer::BufferId) {
    let mut ctx = crate::test_utils::xdisp_test_upper_layer_helpers::interactive_context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    });
    let buffer_id = ctx
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    ctx.frame_manager_mut()
        .create_frame("hscroll-long-line", 800, 480, buffer_id);
    ctx.edit
        .buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .insert(&"a".repeat(1_000_000));
    (ctx, buffer_id)
}

fn point_column_scans_in_one_pass(ctx: &mut Context) -> usize {
    let before = POINT_COLUMN_SCANS.load(std::sync::atomic::Ordering::Relaxed);
    update_auto_hscroll_before_redisplay(ctx);
    POINT_COLUMN_SCANS.load(std::sync::atomic::Ordering::Relaxed) - before
}

/// GNU's `hscroll_window_tree` (src/xdisp.c) reads the cursor row it already
/// has. It only scrolls a row that is truncated, and a window that wraps
/// lines has none. With `truncate-lines` nil, then, the pass must not walk the
/// million characters before point to measure a column it cannot use.
#[test]
fn wrapped_window_pass_does_not_scan_point_column() {
    let (mut ctx, _) = context_with_point_at_the_end_of_a_long_line();

    assert_eq!(point_column_scans_in_one_pass(&mut ctx), 0);
}

/// The control: with `truncate-lines` t the same windows do measure point's
/// column, so the zero above is the wrap check and not a skipped window.
#[test]
fn truncated_window_pass_scans_point_column() {
    let (mut ctx, buffer_id) = context_with_point_at_the_end_of_a_long_line();
    ctx.edit
        .buffers
        .set_buffer_local_property(buffer_id, "truncate-lines", Value::T)
        .expect("enable truncation");

    assert!(point_column_scans_in_one_pass(&mut ctx) > 0);
}

/// Character work one redisplay's hscroll pass spends measuring point's
/// column for a truncated window on a line of `len` characters.
fn truncated_pass_scan_work(len: usize) -> usize {
    let mut ctx = crate::test_utils::xdisp_test_upper_layer_helpers::interactive_context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    });
    let buffer_id = ctx
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    ctx.frame_manager_mut()
        .create_frame("hscroll-flat-line", 800, 480, buffer_id);
    ctx.edit
        .buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .insert(&"a".repeat(len));
    ctx.edit
        .buffers
        .set_buffer_local_property(buffer_id, "truncate-lines", Value::T)
        .expect("enable truncation");

    neovm_edit::test_support::authority::reset_column_scan_characters();
    update_auto_hscroll_before_redisplay(&mut ctx);
    neovm_edit::test_support::authority::column_scan_characters()
}

/// GNU measures a long hscrolled truncated line with
/// `fast_move_it_horizontally` (src/xdisp.c:16797-16807, 11676): it reseats at
/// point and adds `nchars` columns instead of walking the line from its
/// beginning. The pass's decoded-character count must therefore be flat as the
/// line grows; a per-character walk grows with it.
#[test]
fn truncated_window_pass_column_work_is_flat_as_the_line_grows() {
    let short = truncated_pass_scan_work(200_000);
    let long = truncated_pass_scan_work(400_000);
    assert_eq!(
        long, short,
        "column-scan work must not grow with the line: {short} -> {long}"
    );
    assert!(
        short <= 8,
        "the long-line hscroll pass must not walk the line: {short} characters decoded"
    );
}
