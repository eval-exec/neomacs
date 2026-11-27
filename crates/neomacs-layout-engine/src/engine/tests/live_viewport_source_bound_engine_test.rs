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
