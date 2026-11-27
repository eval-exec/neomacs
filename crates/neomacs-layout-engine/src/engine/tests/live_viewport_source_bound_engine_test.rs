//! The live viewport source read is bounded by the window geometry and
//! re-acquired when the walk needs more: a tail that does not fit the initial
//! allowance must be pulled again, yet the rows a bounded read produces must
//! be exactly the rows an unbounded read produces.
//!
//! The allowance override is a numeric test witness only (the shipping read
//! derives its allowance from the window); forcing it below one screenful
//! makes a small buffer need several re-acquisitions inside one layout pass.
use super::*;
use crate::buffer_source::window_render::{
    reset_viewport_read_retries_for_test, set_viewport_read_allowance_for_test,
    viewport_read_retries_for_test,
};

struct AllowanceGuard;

impl AllowanceGuard {
    fn set(chars: usize) -> Self {
        set_viewport_read_allowance_for_test(Some(chars));
        Self
    }
}

impl Drop for AllowanceGuard {
    fn drop(&mut self) {
        set_viewport_read_allowance_for_test(None);
    }
}

#[test]
fn bounded_live_source_reacquires_to_the_same_display() {
    // 400 short lines: the window's rows are newline-bounded and stable across
    // passes, and one screenful needs more source than the forced allowance.
    // `MatrixRow` equality is the display's own content equality (row
    // appearance ids are deliberately excluded by `RowAppearance`), so this
    // is the same comparison the renderer uses to decide a row is unchanged.
    let text = "abc\n".repeat(400);
    let (_, _, baseline_rows, _, _) = layout_main_text_rows(&text);

    reset_viewport_read_retries_for_test();
    let _guard = AllowanceGuard::set(8);
    let (_, _, bounded_rows, _, _) = layout_main_text_rows(&text);
    let retries = viewport_read_retries_for_test();

    assert!(
        retries > 0,
        "a screenful of lines must not fit an 8-char initial allowance"
    );
    assert_eq!(
        bounded_rows.len(),
        baseline_rows.len(),
        "bounded acquisition must not change the row count ({retries} re-acquisitions)"
    );
    for (bounded, baseline) in bounded_rows.iter().zip(&baseline_rows) {
        assert_eq!(
            &**bounded, &**baseline,
            "bounded acquisition must not change a produced row ({retries} re-acquisitions)"
        );
    }
}

/// A bounded source read must not consume the retained cursor-only body: the
/// replay has to survive into the attempt that installs it. Dropping it turns
/// every unchanged window into a full body walk on every redisplay (the
/// bounded read would be "correct" but the incremental layout dead).
#[test]
fn bounded_live_source_keeps_the_cursor_only_replay() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert(&"abc\n".repeat(400));
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("cursor-only-replay", 640, 160, buf_id);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame_id);

    // Nothing changes: the next pass classifies this window cursor-only.
    engine.layout_frame_rust(&mut eval, frame_id);

    let stats = engine.last_layout_stats();
    assert_eq!(
        stats.relaid_body_rows, 0,
        "an unchanged body with a moved point must be reused, not re-walked: {stats:?}"
    );
    assert!(
        stats.cursor_only_windows > 0,
        "the cursor-only replay must reach its attempt: {stats:?}"
    );
}

/// Regression for #823: `image-mode` covers an entire multi-megabyte raw file
/// buffer with one `'display` text property. Driving the full window layout
/// loop with a small initial viewport copy allowance must jump directly to
/// the replacement's end byte offset with zero viewport read retries.
#[test]
fn whole_buffer_display_replacement_jumps_past_viewport_allowance_with_zero_retries() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let raw = "x".repeat(64 * 1024);
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert(&raw);
    eval.eval_str("(put-text-property (point-min) (point-max) 'display \"[IMG]\")")
        .expect("set display property");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("display-replacement-zero-retries", 640, 160, buf_id);
    let mut baseline_engine = crate::font::metrics::test_catalog::layout_engine();
    baseline_engine.layout_frame_rust(&mut eval, frame_id);
    let baseline = selected_window_layout_trace(&eval, &baseline_engine, frame_id);

    reset_viewport_read_retries_for_test();
    let _guard = AllowanceGuard::set(64);
    let mut bounded_engine = crate::font::metrics::test_catalog::layout_engine();
    bounded_engine.layout_frame_rust(&mut eval, frame_id);
    let retries = viewport_read_retries_for_test();
    let bounded = selected_window_layout_trace(&eval, &bounded_engine, frame_id);

    assert_eq!(
        retries, 0,
        "whole-buffer display replacement must jump directly over covered bytes without retrying"
    );
    let rendered = backend_trace_text_area_text(&bounded);
    assert!(
        rendered.contains("[IMG]"),
        "replacement text must be displayed: {rendered:?}"
    );
    assert!(
        !rendered.contains("xxxx"),
        "covered buffer bytes must not leak: {rendered:?}"
    );
    assert_eq!(
        rendered,
        backend_trace_text_area_text(&baseline),
        "bounded pass must match the unbounded baseline"
    );
}

/// Issue #702 Round 2 (Correctness): when `buffer-display-table` maps `?\n`
/// without a trailing `?\n` (`[]` or `[? ]`) or static `composition` spans
/// `?\n`, live redisplay AND synchronous queries must join the lines through
/// `point-max` rather than cutting short at `body_read_rows + 2` raw newlines.
#[test]
fn collapsed_newlines_render_full_viewport_and_single_truncated_row() {
    // 1. `(aset dt ?\n [])` with `truncate-lines = nil`: 200 * 6 = 1200 chars
    //    must wrap across multiple rows and reach point-max (1201), not stop at
    //    `(rows + 2) * 6` bytes.
    {
        let text: String = (0..200).map(|i| format!("L{i:03}-\n")).collect();
        let (mut eval, frame_id, buf_id, window_id) = incr_editing_frame(&text, 640, 384);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(
            "(let ((dt (make-char-table 'display-table nil))) \
               (aset dt ?\\n []) \
               (set (make-local-variable 'buffer-display-table) dt) \
               (set (make-local-variable 'truncate-lines) nil) \
               (set (make-local-variable 'mode-line-format) nil))",
        )
        .expect("empty display-table newline");
        let _ = buf_id;
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        let rendered = backend_trace_text_area_text(&trace);
        assert!(
            rendered.contains("L199-"),
            "empty display-table newline must display through L199-, got: {rendered:?}"
        );
        let snap = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                neovm_core::window::WindowLayoutQueryScope::Viewport,
            )
            .expect("viewport query");
        assert_eq!(
            snap.end().as_i64(),
            1201,
            "viewport window-end must reach point-max=1201"
        );
    }

    // 2. `(aset dt ?\n [? ])` with `truncate-lines = t`:
    //    (a) 80 lines (480 chars, < MAX_NEWLINE_DISTANCE=500 past col 80) join
    //        into ONE single truncated row reaching point-max (481).
    //    (b) 200 lines (1200 chars) match GNU `forward_to_next_line_start`'s
    //        `MAX_NEWLINE_DISTANCE = 500` shortcut: row 0 starts at L000-,
    //        row 1 starts at L097-, row 2 starts at L194-, reaching 1201.
    {
        let text: String = (0..80).map(|i| format!("L{i:03}-\n")).collect();
        let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 640, 384);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(
            "(let ((dt (make-char-table 'display-table nil))) \
               (aset dt ?\\n [? ]) \
               (set (make-local-variable 'buffer-display-table) dt) \
               (set (make-local-variable 'truncate-lines) t) \
               (set (make-local-variable 'mode-line-format) nil))",
        )
        .expect("space display-table newline 80 lines");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);
        let snap = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                neovm_core::window::WindowLayoutQueryScope::Viewport,
            )
            .expect("viewport query");
        assert_eq!(snap.end().as_i64(), 481);
        let trace_row_texts = |trace: &BackendLayoutTrace| -> Vec<String> {
            trace
                .matrix_rows
                .iter()
                .filter(|row| row.enabled)
                .map(|row| {
                    row.glyph_areas[1]
                        .iter()
                        .filter(|glyph| !glyph.padding)
                        .filter_map(|glyph| match &glyph.kind {
                            GlyphKindTrace::Char(ch) | GlyphKindTrace::Glyphless(ch) => {
                                Some(ch.to_string())
                            }
                            GlyphKindTrace::Composite(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<String>()
                })
                .filter(|line| !line.trim().is_empty())
                .collect()
        };
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        let non_empty_rows = trace_row_texts(&trace).len();
        assert_eq!(
            non_empty_rows, 1,
            "80 lines (< MAX_NEWLINE_DISTANCE=500 past col 80) must render ONE joined truncated row"
        );
    }
    {
        let text: String = (0..200).map(|i| format!("L{i:03}-\n")).collect();
        let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 640, 384);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(
            "(let ((dt (make-char-table 'display-table nil))) \
               (aset dt ?\\n [? ]) \
               (set (make-local-variable 'buffer-display-table) dt) \
               (set (make-local-variable 'truncate-lines) t) \
               (set (make-local-variable 'mode-line-format) nil))",
        )
        .expect("space display-table newline 200 lines");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);
        let snap = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                neovm_core::window::WindowLayoutQueryScope::Viewport,
            )
            .expect("viewport query");
        assert_eq!(
            snap.end().as_i64(),
            1201,
            "truncated [? ] display-table newline must skip through point-max=1201"
        );
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        let rendered_lines: Vec<String> = trace
            .matrix_rows
            .iter()
            .filter(|row| row.enabled)
            .map(|row| {
                row.glyph_areas[1]
                    .iter()
                    .filter(|glyph| !glyph.padding)
                    .filter_map(|glyph| match &glyph.kind {
                        GlyphKindTrace::Char(ch) | GlyphKindTrace::Glyphless(ch) => {
                            Some(ch.to_string())
                        }
                        GlyphKindTrace::Composite(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(
            rendered_lines.len(),
            3,
            "200 lines on 80-col TTY must match GNU MAX_NEWLINE_DISTANCE=500 (3 rows): {rendered_lines:?}"
        );
        assert!(rendered_lines[0].starts_with("L000-"), "{rendered_lines:?}");
        assert!(rendered_lines[1].starts_with("L097-"), "{rendered_lines:?}");
        assert!(rendered_lines[2].starts_with("L194-"), "{rendered_lines:?}");
    }

    // 3. Static `composition` property spanning `\n`: must not count composed
    //    newlines as row breaks (80 composed lines = 4 wrapped rows + 40
    //    uncomposed lines = 44 rows, fitting inside a 50-row window to EOB=601).
    {
        let text: String = (0..120).map(|i| format!("C{i:03}\n")).collect();
        let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 640, 800);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(
            "(progn \
               (set (make-local-variable 'truncate-lines) nil) \
               (set (make-local-variable 'mode-line-format) nil) \
               (let ((i 0) (p 1)) \
                 (while (< i 80) \
                   (put-text-property (+ p 3) (+ p 5) 'composition (list 0 2 [?X])) \
                   (setq p (+ p 5)) \
                   (setq i (1+ i)))))",
        )
        .expect("composition spanning newline");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);
        let snap = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                neovm_core::window::WindowLayoutQueryScope::Viewport,
            )
            .expect("viewport query");
        assert_eq!(
            snap.end().as_i64(),
            601,
            "composition spanning newline must reach point-max=601"
        );
    }

    // 4. Blocker 1: `hscroll = 10` with display-table `[? ]` and with static
    //    `composition` spanning `-\n` must NOT break on raw `\n` during hscroll.
    {
        let text: String = (0..100).map(|i| format!("L{i:03}-\n")).collect();
        let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 1600, 800);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(
            "(let ((dt (make-char-table 'display-table nil))) \
               (aset dt ?\\n [? ]) \
               (set (make-local-variable 'buffer-display-table) dt) \
               (set (make-local-variable 'truncate-lines) t) \
               (set (make-local-variable 'auto-hscroll-mode) nil) \
               (set (make-local-variable 'mode-line-format) nil) \
               (set-window-hscroll nil 10))",
        )
        .expect("hscroll display-table");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        let row0 = backend_trace_text_area_text(&trace);
        assert!(
            row0.starts_with("$ L002- L003-"),
            "hscroll=10 under [? ] display-table must join lines instead of blanking rows, got: {row0:?}"
        );
        let snap = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                neovm_core::window::WindowLayoutQueryScope::Viewport,
            )
            .expect("viewport query");
        assert_eq!(snap.end().as_i64(), 601);
    }

    // 5. Blocker 2.1 & 2.2: truncation skip respects `'display " "` and
    //    `'invisible t` on `\n`, and does NOT swallow the next line on
    //    `'composition 'invalid`.
    for (label, prop_elisp, expect_next_line_visible) in [
        (
            "display-space-nl",
            "(put-text-property 121 122 'display \" \")",
            false,
        ),
        (
            "invisible-nl",
            "(put-text-property 121 122 'invisible t)",
            false,
        ),
        (
            "invalid-comp-nl",
            "(put-text-property 121 122 'composition 'invalid)",
            true,
        ),
    ] {
        let text = format!("{}\nNEXT-LINE\n", "a".repeat(120));
        let (mut eval, frame_id, _buf_id, _window_id) = incr_editing_frame(&text, 640, 384);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(&format!(
            "(progn \
               (set (make-local-variable 'truncate-lines) t) \
               (set (make-local-variable 'mode-line-format) nil) \
               {prop_elisp})"
        ))
        .expect("setup truncation prop");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        let rendered = backend_trace_text_area_text(&trace);
        assert_eq!(
            rendered.contains("NEXT-LINE"),
            expect_next_line_visible,
            "case={label}: rendered={rendered:?}"
        );
    }
}

/// Issue #702 Round 2 (Budget bound): `WindowTextReadBudget::initial` must stay
/// strictly `O(rows * visible_cols)`, clamped at 64 KiB (`65_536`), and NEVER
/// grow with `params.hscroll`.
#[test]
fn truncated_budget_pins_exact_o_viewport_cap_and_ignores_hscroll() {
    use crate::buffer_source::window_render::initial_viewport_read_budget_chars_for_test;

    let make_params =
        |rows: usize, cols: usize, wrap_mode: crate::types::LineWrapMode, hscroll: i32| {
            let mut params = test_window_params();
            params.char_width = 8.0;
            params.char_height = 16.0;
            params.text_bounds = neomacs_display_protocol::types::Rect::new(
                0.0,
                0.0,
                (cols as f32) * 8.0,
                (rows as f32) * 16.0,
            );
            params.wrap_mode = wrap_mode;
            params.hscroll = hscroll;
            params
        };

    // 50x170 truncated: (50 + 2) * (170 * 4) + 1 = 35_361; CharHorizon stays 1x (8_841).
    let p_h0 = make_params(50, 170, crate::types::LineWrapMode::Truncate, 0);
    let p_h100k = make_params(50, 170, crate::types::LineWrapMode::Truncate, 100_000);
    assert_eq!(
        initial_viewport_read_budget_chars_for_test(&p_h0, "rows_within_chars"),
        35_361
    );
    assert_eq!(
        initial_viewport_read_budget_chars_for_test(&p_h0, "viewport_char_horizon"),
        35_361
    );
    assert_eq!(
        initial_viewport_read_budget_chars_for_test(&p_h0, "char_horizon"),
        8_841
    );
    // Crucial wall: hscroll = 100,000 must NOT inflate any mode's initial budget.
    for mode in ["rows_within_chars", "viewport_char_horizon", "char_horizon"] {
        assert_eq!(
            initial_viewport_read_budget_chars_for_test(&p_h100k, mode),
            initial_viewport_read_budget_chars_for_test(&p_h0, mode),
            "initial budget for {mode} must be independent of hscroll"
        );
    }

    // Wide frame 50x1000 truncated: 1x base is 52_001, 4x would be 208_001 -> clamped to 65_536.
    let p_wide = make_params(50, 1000, crate::types::LineWrapMode::Truncate, 100_000);
    assert_eq!(
        initial_viewport_read_budget_chars_for_test(&p_wide, "rows_within_chars"),
        65_536
    );
    assert_eq!(
        initial_viewport_read_budget_chars_for_test(&p_wide, "viewport_char_horizon"),
        65_536
    );

    // Wrap mode stays 1x across all modes regardless of hscroll.
    let p_wrap = make_params(50, 170, crate::types::LineWrapMode::Wrap, 100_000);
    for mode in ["rows_within_chars", "viewport_char_horizon", "char_horizon"] {
        assert_eq!(
            initial_viewport_read_budget_chars_for_test(&p_wrap, mode),
            8_841,
            "wrap mode {mode} must stay at 1x geometry allowance"
        );
    }
}

/// Issue #702 Round 2: truncated long-line viewports and synchronous queries
/// (TTY `Pixels`/`Rows`/`Viewport` on `RowsWithinChars`, GUI `Rows` on
/// `RowsWithinChars`, and GUI `Pixels`/`Viewport` on `ViewportCharHorizon`)
/// complete in a single pass with zero horizon retries and match forced-retry
/// controls identically across snapshots (including `ViewportCharHorizon`'s
/// retry path under `AllowanceGuard`).
#[test]
fn truncated_long_line_viewport_and_all_query_modes_need_zero_retries_and_match_forced_retry() {
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;

    let long_line = format!("{}\n", "x".repeat(299)); // 300 bytes/line
    let text = long_line.repeat(120); // 36,000 bytes (> 1x budget, < 4x budget)

    // --- Arm A: TTY (RowsWithinChars for live + Pixels + Rows + Viewport) ---
    {
        let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 1600, 800);
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(None);
        eval.eval_str(
            "(progn \
               (set (make-local-variable 'truncate-lines) t) \
               (set (make-local-variable 'word-wrap) nil) \
               (set (make-local-variable 'mode-line-format) nil))",
        )
        .expect("truncate-lines");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();

        reset_viewport_read_retries_for_test();
        engine.layout_frame_rust(&mut eval, frame_id);
        let live_retries = viewport_read_retries_for_test();
        let live_trace = selected_window_layout_trace(&eval, &engine, frame_id);

        reset_viewport_read_retries_for_test();
        let px_q = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Pixels {
                    start: LispCharPos1::new(1),
                    height: NonZeroUsize::new(768).unwrap(),
                },
            )
            .expect("tty pixels");
        let px_retries = viewport_read_retries_for_test();

        reset_viewport_read_retries_for_test();
        let rows_q = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::new(1),
                    count: NonZeroUsize::new(48).unwrap(),
                },
            )
            .expect("tty rows");
        let rows_retries = viewport_read_retries_for_test();

        reset_viewport_read_retries_for_test();
        let vp_q = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Viewport,
            )
            .expect("tty viewport");
        let vp_retries = viewport_read_retries_for_test();

        assert_eq!(
            (live_retries, px_retries, rows_retries, vp_retries),
            (0, 0, 0, 0),
            "TTY live + Pixels + Rows + Viewport must complete with zero horizon retries"
        );

        // Forced-retry control: assert full snapshot equality on ALL 4 paths!
        let _guard = AllowanceGuard::set(1024);
        let mut forced = crate::font::metrics::test_catalog::layout_engine();
        reset_viewport_read_retries_for_test();
        forced.layout_frame_rust(&mut eval, frame_id);
        assert!(viewport_read_retries_for_test() > 0);
        assert_eq!(
            selected_window_layout_trace(&eval, &forced, frame_id),
            live_trace,
            "TTY live layout trace must match forced-retry control"
        );
        let forced_px = forced
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Pixels {
                    start: LispCharPos1::new(1),
                    height: NonZeroUsize::new(768).unwrap(),
                },
            )
            .expect("forced tty pixels");
        let forced_rows = forced
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::new(1),
                    count: NonZeroUsize::new(48).unwrap(),
                },
            )
            .expect("forced tty rows");
        let forced_vp = forced
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Viewport,
            )
            .expect("forced tty viewport");
        assert_eq!(px_q.end(), forced_px.end());
        assert_eq!(rows_q.end(), forced_rows.end());
        assert_eq!(vp_q.end(), forced_vp.end());
    }

    // --- Arm B: GUI (Rows -> RowsWithinChars; Pixels & Viewport -> ViewportCharHorizon) ---
    {
        let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 1600, 800);
        eval.eval_str(
            "(progn \
               (set (make-local-variable 'truncate-lines) t) \
               (set (make-local-variable 'word-wrap) nil) \
               (set (make-local-variable 'mode-line-format) nil))",
        )
        .expect("truncate-lines gui");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame_id);

        reset_viewport_read_retries_for_test();
        let gui_px = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Pixels {
                    start: LispCharPos1::new(1),
                    height: NonZeroUsize::new(768).unwrap(),
                },
            )
            .expect("gui pixels");
        let gui_px_retries = viewport_read_retries_for_test();

        reset_viewport_read_retries_for_test();
        let gui_rows = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::new(1),
                    count: NonZeroUsize::new(48).unwrap(),
                },
            )
            .expect("gui rows");
        let gui_rows_retries = viewport_read_retries_for_test();

        reset_viewport_read_retries_for_test();
        let gui_vp = engine
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Viewport,
            )
            .expect("gui viewport");
        let gui_vp_retries = viewport_read_retries_for_test();

        assert_eq!(
            (gui_px_retries, gui_rows_retries, gui_vp_retries),
            (0, 0, 0),
            "GUI Pixels (ViewportCharHorizon) + Rows (RowsWithinChars) + Viewport (ViewportCharHorizon) must need 0 retries"
        );

        // Exercise ViewportCharHorizon's retry path explicitly via AllowanceGuard::set(1024)
        // and verify identical answers!
        let _guard = AllowanceGuard::set(1024);
        let mut forced = crate::font::metrics::test_catalog::layout_engine();
        reset_viewport_read_retries_for_test();
        let forced_gui_px = forced
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Pixels {
                    start: LispCharPos1::new(1),
                    height: NonZeroUsize::new(768).unwrap(),
                },
            )
            .expect("forced gui pixels");
        assert!(
            viewport_read_retries_for_test() > 0,
            "ViewportCharHorizon retry path must fire under 1024-char allowance"
        );
        reset_viewport_read_retries_for_test();
        let forced_gui_vp = forced
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Viewport,
            )
            .expect("forced gui viewport");
        assert!(
            viewport_read_retries_for_test() > 0,
            "GUI Viewport ViewportCharHorizon retry path must fire under 1024-char allowance"
        );
        let forced_gui_rows = forced
            .query_window_layout(
                &mut eval,
                frame_id,
                window_id,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::new(1),
                    count: NonZeroUsize::new(48).unwrap(),
                },
            )
            .expect("forced gui rows");
        assert_eq!(gui_px.end(), forced_gui_px.end());
        assert_eq!(gui_rows.end(), forced_gui_rows.end());
        assert_eq!(gui_vp.end(), forced_gui_vp.end());
    }
}

/// Issue #702 Round 2 (Wall 2): on short truncated lines with `hscroll = 100,000`,
/// `RowsWithinChars` stops at the `(body_read_rows + 2)`-th preserved newline
/// (~2 KB), never blowing up to megabytes with `hscroll`.
#[test]
fn short_truncated_lines_at_large_hscroll_stay_newline_bounded() {
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;

    let short_line = format!("{}\n", "x".repeat(39)); // 40 bytes/line
    let text = short_line.repeat(2_000); // 80,000 bytes
    let (mut eval, frame_id, _buf_id, window_id) = incr_editing_frame(&text, 1600, 800);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(None);
    eval.eval_str(
        "(progn \
           (set (make-local-variable 'truncate-lines) t) \
           (set (make-local-variable 'word-wrap) nil) \
           (set (make-local-variable 'mode-line-format) nil) \
           (set-window-hscroll (selected-window) 100000))",
    )
    .expect("hscroll 100k");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    reset_viewport_read_retries_for_test();
    let _ = engine
        .query_window_layout(
            &mut eval,
            frame_id,
            window_id,
            WindowLayoutQueryScope::Pixels {
                start: LispCharPos1::new(1),
                // On a TTY frame (`char_height = 1`), `height = 48` is 48 rows
                // -> `body_read_rows + 2 = 50` lines * 40 B = 2,000 bytes.
                height: NonZeroUsize::new(48).unwrap(),
            },
        )
        .expect("tty pixels at hscroll 100k");
    assert_eq!(viewport_read_retries_for_test(), 0);
    assert_eq!(
        engine.text_buf.len(),
        2_000,
        "40-byte truncated lines at hscroll=100k must stop at the 50th preserved newline (2,000 B), not copy 40 KB or scale with hscroll"
    );
}
