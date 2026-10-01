//! C6 integration checks require the process-wide row-storage knob. Run this
//! ignored family explicitly with NEOMACS_PRESENT_POINT_ROWS=on or verify.
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

fn check_fresh(eval: &mut Context, engine: &LayoutEngine, frame: neovm_core::window::FrameId) {
    let actual = selected_window_layout_trace(eval, engine, frame);
    let mut reference = LayoutEngine::new();
    reference.layout_frame_rust(eval, frame);
    assert_eq!(
        actual,
        selected_window_layout_trace(eval, &reference, frame)
    );
}

#[test]
#[ignore = "run with NEOMACS_PRESENT_POINT_ROWS=on or verify"]
fn cursor_replay_shares_every_point_row() {
    assert!(display_point_rows_mode().enabled());
    let (mut eval, frame, _, window) = incr_editing_frame(&LINE.repeat(200), 800, 600);
    let mut engine = LayoutEngine::new();
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
#[ignore = "run with NEOMACS_PRESENT_POINT_ROWS=on or verify"]
fn edited_row_is_new_and_unchanged_rows_share_cells() {
    assert!(display_point_rows_mode().enabled());
    let (mut eval, frame, _, window) = incr_editing_frame(&LINE.repeat(200), 800, 600);
    eval.eval_str(&format!("(goto-char {})", 8 * LINE.len() + 4))
        .unwrap();
    let mut engine = LayoutEngine::new();
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
#[ignore = "run with NEOMACS_PRESENT_POINT_ROWS=on or verify"]
fn scroll_and_prepared_page_replay_preserve_point_geometry() {
    assert!(display_point_rows_mode().enabled());
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
    let mut engine = LayoutEngine::new();
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
