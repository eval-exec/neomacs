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
    let mut eval = Context::new();
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
