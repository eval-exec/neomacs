//! C6 integration checks require the process-wide row-storage knob.  The knob
//! is read once per process, so each check re-runs itself in a child process
//! with `NEOMACS_PRESENT_POINT_ROWS=on`; a run that already has the knob on
//! executes the body directly.
use super::*;
use neovm_core::window::{DisplayPointRows, display_point_rows_mode};

const LINE: &str = "(defun f (a b) (+ a b))\n";

fn snapshot(
    eval: &Context,
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
) -> WindowDisplaySnapshot {
    eval.frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .clone()
}

fn shared_count(before: &DisplayPointRows, after: &DisplayPointRows) -> usize {
    after
        .rows
        .iter()
        .filter(|row| before.rows.iter().any(|old| old.shares_cells_with(row)))
        .count()
}

/// Run one of the checks in the environment it requires: the row-storage mode
/// is process-wide and read once, so a process started without the knob
/// re-runs exactly this test in a child with `NEOMACS_PRESENT_POINT_ROWS=on`
/// and returns; the child runs the body.  Returns true when the body should
/// run in THIS process.
fn point_rows_mode_enabled(test: &str) -> bool {
    if display_point_rows_mode().enabled() {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([test, "--test-threads", "1"])
        .env("NEOMACS_PRESENT_POINT_ROWS", "on")
        .env_remove("RUST_LOG")
        .output()
        .expect("re-run the point-row test with NEOMACS_PRESENT_POINT_ROWS=on");
    assert!(
        output.status.success(),
        "point-row child failed under NEOMACS_PRESENT_POINT_ROWS=on: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    false
}

fn check_fresh(eval: &mut Context, engine: &LayoutEngine, frame: neovm_core::window::FrameId) {
    let actual = selected_window_layout_trace(eval, engine, frame);
    let mut reference = crate::font::metrics::test_catalog::layout_engine();
    reference.layout_frame_rust(eval, frame);
    assert_eq!(
        actual,
        selected_window_layout_trace(eval, &reference, frame)
    );
}

#[test]
fn cursor_replay_shares_every_point_row() {
    if !point_rows_mode_enabled("cursor_replay_shares_every_point_row") {
        return;
    }
    assert!(display_point_rows_mode().enabled());
    let (mut eval, frame, _, window) = incr_editing_frame(&LINE.repeat(200), 800, 600);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let before = snapshot(&eval, frame, window);
    assert!(
        before.points.is_empty(),
        "row storage replaces the flat vector"
    );
    eval.eval_str("(goto-char 2)").unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
    let after = snapshot(&eval, frame, window);
    let rows = after.point_rows.as_ref().unwrap();
    assert_eq!(
        shared_count(before.point_rows.as_ref().unwrap(), rows),
        rows.rows.len()
    );
    assert_eq!(
        before.iter_points().collect::<Vec<_>>(),
        after.iter_points().collect::<Vec<_>>()
    );
    check_fresh(&mut eval, &engine, frame);
}

#[test]
fn edited_row_is_new_and_unchanged_rows_share_cells() {
    if !point_rows_mode_enabled("edited_row_is_new_and_unchanged_rows_share_cells") {
        return;
    }
    assert!(display_point_rows_mode().enabled());
    let (mut eval, frame, _, window) = incr_editing_frame(&LINE.repeat(200), 800, 600);
    eval.eval_str(&format!("(goto-char {})", 8 * LINE.len() + 4))
        .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let before = snapshot(&eval, frame, window);
    eval.eval_str("(insert \"x\")").unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(engine.last_layout_stats().edit_windows, 1);
    let after = snapshot(&eval, frame, window);
    let rows = after.point_rows.as_ref().unwrap();
    let shared = shared_count(before.point_rows.as_ref().unwrap(), rows);
    assert!(shared > 10, "unchanged rows retain their immutable arrays");
    assert!(shared < rows.rows.len(), "the edited row gets fresh cells");
    assert!(after.points.is_empty());
    check_fresh(&mut eval, &engine, frame);
}

#[test]
fn scroll_and_prepared_page_replay_preserve_point_geometry() {
    if !point_rows_mode_enabled("scroll_and_prepared_page_replay_preserve_point_geometry") {
        return;
    }
    assert!(display_point_rows_mode().enabled());
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    scroll_window_to(&mut eval, frame, window, buffer, 1, 8 * LINE.len());
    engine.layout_frame_rust(&mut eval, frame);
    let first = snapshot(&eval, frame, window);
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        (6 * LINE.len()) as i64 + 1,
        14 * LINE.len(),
    );
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(engine.last_layout_stats().scroll_windows, 1);
    let shifted = snapshot(&eval, frame, window);
    assert!(
        shared_count(
            first.point_rows.as_ref().unwrap(),
            shifted.point_rows.as_ref().unwrap()
        ) > 10
    );
    check_fresh(&mut eval, &engine, frame);
    for start in [60, 0] {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (start * LINE.len()) as i64 + 1,
            (start + 8) * LINE.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
        if start == 0 {
            assert_eq!(engine.last_layout_stats().prepared_windows, 1);
            // Inspect this engine's publication before the independent fresh
            // layout replaces the evaluator's display snapshot.
            let returned = snapshot(&eval, frame, window);
            assert!(
                shared_count(
                    first.point_rows.as_ref().unwrap(),
                    returned.point_rows.as_ref().unwrap()
                ) > 10
            );
        }
        check_fresh(&mut eval, &engine, frame);
    }
}
