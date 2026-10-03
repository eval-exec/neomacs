//! Accepted layout, live split, and Lisp query share GNU's numeric frame pool.
//! Run in a fresh nextest child with NEOMACS_POSN_OBJECT_EXTENT=on. The policy
//! belongs to process startup; this test never mutates environment or OnceLock.

use super::*;
use neovm_core::window::{SplitDirection, SplitPlacement, WindowLayoutQueryOutcome};

#[test]
fn accepted_terminal_frame_pool_answers_a_split_child_before_redisplay() {
    assert!(
        neovm_core::window::posn_object_extent_mode().enabled(),
        "isolate this test with NEOMACS_POSN_OBJECT_EXTENT=on before child startup"
    );
    let (mut eval, frame_id, buffer_id, old_window) = incr_editing_frame("", 80, 24);
    eval.set_variable("noninteractive", Value::NIL);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.set_buffer_local("mode-line-format", Value::NIL);
        buffer.set_buffer_local("header-line-format", Value::NIL);
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.initial = false;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(80, 24);
    }
    let mut engine = LayoutEngine::new_without_font_metrics();
    // The real accepted renderer path prepares and seals the presentation;
    // its empty-buffer producer owns one numeric nil glyph per body row.
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(old_window)
            .is_some()
    );

    // Change source and split without another renderer pass. GNU repartitions
    // the old accepted frame pool even though this new leaf has no output.
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd\n");
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let child = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            old_window,
            SplitDirection::Vertical,
            buffer_id,
            Some(10),
            SplitPlacement::AfterTarget,
        )
        .expect("live vertical split");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            frame_id,
            child,
            Some(10),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("apply live split geometry");
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(child)
            .is_none()
    );

    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    query.synchronize(engine.window_layout_query_seed());
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    eval.set_variable("accepted-pool-child", Value::make_window(child.0));
    // Expectations are the cold-second-child TEXT fields captured from GNU
    // 31.1 by posn_object_extent_oracle's split-cold fixture: the first stored
    // cell is (1 . 0), untouched allocated cells remain (0 . 0), and the EOB
    // iterator's hpos remains zero even for a click after the source boundary.
    for (x, y, expected) in [
        (0, 0, "(1 . 0)"),
        (1, 0, "(0 . 0)"),
        (3, 0, "(0 . 0)"),
        (7, 1, "(1 . 0)"),
    ] {
        let position = eval
            .eval_str(&format!("(posn-at-x-y {x} {y} accepted-pool-child)"))
            .expect("real Lisp posn-at-x-y");
        let fields = neovm_core::emacs_core::value::list_to_vec(&position)
            .expect("ten-field position tuple");
        assert_eq!(fields.len(), 10);
        assert_eq!(fields[0].as_window_id(), Some(child.0));
        assert_eq!(
            neovm_core::emacs_core::print::print_value(&fields[9]),
            expected,
            "GNU accepted frame-pool numeric cells at ({x}, {y})"
        );
    }
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(child)
            .is_none(),
        "query-only layout must not accept a new current matrix"
    );
}

#[test]
fn accepted_terminal_frame_pool_fallback_reads_eob_iterator_cell_before_reported_columns() {
    assert!(
        neovm_core::window::posn_object_extent_mode().enabled(),
        "isolate this test with NEOMACS_POSN_OBJECT_EXTENT=on before child startup"
    );
    let (mut eval, frame_id, buffer_id, old_window) = incr_editing_frame("", 80, 24);
    eval.set_variable("noninteractive", Value::NIL);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.set_buffer_local("mode-line-format", Value::NIL);
        buffer.set_buffer_local("header-line-format", Value::NIL);
        buffer.set_buffer_local("tab-line-format", Value::NIL);
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.initial = false;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(80, 24);
    }
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    // Use the already proven empty accepted producer to make the cold
    // backing explicit: one numeric EOB append followed by zero-space fill.
    // Change LIVE source to the exact GNU split-cold text without accepting
    // another frame. The query contract still uses the same six GNU values.
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcdef\nghijkl\n");
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let child = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            old_window,
            SplitDirection::Vertical,
            buffer_id,
            Some(10),
            SplitPlacement::AfterTarget,
        )
        .expect("live vertical split");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            frame_id,
            child,
            Some(10),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("apply live split geometry");
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(child)
            .is_none()
    );
    // Verify the actual producer backing before exercising the new query
    // read. These are immutable accepted observations, not a hand-written
    // matrix, expected source rectangle, or an inferred erase-to-EOL tail.
    {
        let frame = eval.frame_manager().get(frame_id).unwrap();
        let window = frame.find_window(child).unwrap();
        let first = usize::try_from(window.top_line()).expect("nonnegative cold partition");
        let pool = frame
            .tty_posn_pool()
            .expect("real accepted empty frame pool");
        for (local_row, column) in [(0, 2), (1, 3)] {
            let row = pool.rows.get(first + local_row).expect("accepted cold row");
            assert!(row.enabled, "empty accepted pool owns enabled cold backing");
            assert_eq!(
                row.cells
                    .get(column)
                    .expect("accepted allocated cell")
                    .dimensions(),
                (0, 0),
                "empty accepted fixture owns GNU zero-space backing before the query"
            );
        }
    }
    // The deliberate unavailable-producer seam is the same dynamic
    // noninteractive binding as the live split-cold GNU oracle. The accepted
    // frame pool remains available, and the query cannot accept any new rows.
    eval.set_variable("noninteractive", Value::T);
    eval.set_variable("accepted-pool-fallback-child", Value::make_window(child.0));
    // GNU dispnew.c reads current_matrix at iterator hpos/vpos before adding
    // after-EOL columns. These six exact unavailable queries/expectations
    // come from the fresh split-cold GNU capture. Live EOB is row2/column0;
    // the cold child's accepted backing is an EOB filler partition with
    // (1,0) at column0 and explicit (0,0) zero-space fill after it. Reported
    // clicked row/column must not become numeric matrix read authority.
    for (x, y, expected_point, expected_extent) in [
        (2, 0, "3", "(0 . 0)"),
        (3, 1, "11", "(0 . 0)"),
        (4, 2, "15", "(1 . 0)"),
        (5, 3, "15", "(1 . 0)"),
        (8, 2, "15", "(1 . 0)"),
        (9, 3, "15", "(1 . 0)"),
    ] {
        let position = eval
            .eval_str(&format!(
                "(posn-at-x-y {x} {y} accepted-pool-fallback-child)"
            ))
            .expect("fallback Lisp posn-at-x-y");
        let fields = neovm_core::emacs_core::value::list_to_vec(&position)
            .expect("ten-field position tuple");
        assert_eq!(fields.len(), 10);
        assert_eq!(fields[0].as_window_id(), Some(child.0));
        assert_eq!(
            neovm_core::emacs_core::print::print_value(&fields[5]),
            expected_point
        );
        assert_eq!(
            neovm_core::emacs_core::print::print_value(&fields[9]),
            expected_extent,
            "GNU fallback current-matrix extent at ({x}, {y})"
        );
    }
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(child)
            .is_none(),
        "fallback query must not accept source rows"
    );
}
