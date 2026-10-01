use super::*;

const LINE: &str = "(defun f (a b) (+ a b))\n";

#[test]
fn returning_to_a_prepared_page_reuses_body_and_matches_fresh_layout() {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
    let mut engine = LayoutEngine::new();
    for (step, start) in [0, 60, 0, 60, 0].into_iter().enumerate() {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (start * LINE.len()) as i64 + 1,
            (start + 8) * LINE.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
        if step >= 2 {
            assert_eq!(engine.last_layout_stats().prepared_windows, 1);
            let matrix = &engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .window_matrices
                .iter()
                .find(|entry| entry.window_id.get() == window.0 as i64)
                .unwrap()
                .matrix;
            assert!(
                (0..matrix.rows.len()).all(|row| matrix.row_damage(row) == RowDamage::New),
                "historical layout reuse must not skip repainting the current scene"
            );
            assert_eq!(
                crate::display_status_line::mode_line_eval_count(),
                1,
                "restoring body rows must still evaluate current chrome"
            );
            assert!(
                engine.last_layout_stats().reused_rows > 10,
                "returning to a prepared page must reuse its body: {:?}",
                engine.last_layout_stats()
            );
        }
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut reference = LayoutEngine::new();
        reference.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            actual,
            selected_window_layout_trace(&eval, &reference, frame)
        );
    }
}

#[test]
fn prepared_pages_are_invalidated_by_text_properties_and_layout_inputs() {
    for change in [
        "(progn (goto-char 2) (delete-region 2 3) (insert \"z\"))",
        "(put-text-property 1 12 'display \"replacement\")",
        "(setq line-spacing 3)",
        "(setq tab-width 3)",
    ] {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
        let mut engine = LayoutEngine::new();
        for start in [0, 60] {
            scroll_window_to(
                &mut eval,
                frame,
                window,
                buffer,
                (start * LINE.len()) as i64 + 1,
                (start + 8) * LINE.len(),
            );
            engine.layout_frame_rust(&mut eval, frame);
        }
        eval.eval_str(change).expect(change);
        scroll_window_to(&mut eval, frame, window, buffer, 1, 8 * LINE.len());
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(engine.last_layout_stats().prepared_windows, 0, "{change}");
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut reference = LayoutEngine::new();
        reference.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            actual,
            selected_window_layout_trace(&eval, &reference, frame),
            "{change}"
        );
    }
}

#[test]
fn prepared_page_storage_evicts_old_pages_and_keeps_recent_pages() {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(1200), 800, 600);
    let mut engine = LayoutEngine::new();
    for start in (0..10).map(|page| page * 60).chain([0, 480]) {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (start * LINE.len()) as i64 + 1,
            (start + 8) * LINE.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
        if start == 480 {
            // The second visit is cached; the first is a newly exposed page.
            if engine.last_layout_stats().prepared_windows > 0 {
                return;
            }
        }
        assert_eq!(engine.last_layout_stats().prepared_windows, 0);
    }
    panic!("a recent page should survive eviction of the oldest page");
}

#[test]
fn prepared_page_reconstructs_a_different_cursor_position() {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
    let mut engine = LayoutEngine::new();
    for (start, point) in [(0, 8), (60, 68), (0, 12)] {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (start * LINE.len()) as i64 + 1,
            point * LINE.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
    }
    assert_eq!(engine.last_layout_stats().prepared_windows, 1);
    let actual = selected_window_layout_trace(&eval, &engine, frame);
    let mut reference = LayoutEngine::new();
    reference.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        actual,
        selected_window_layout_trace(&eval, &reference, frame)
    );
}

#[test]
fn backward_scroll_into_prepared_coverage_reuses_rows_at_a_new_start() {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
    let mut engine = LayoutEngine::new();
    for (step, start) in [0, 40, 5, 3, 7].into_iter().enumerate() {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (start * LINE.len()) as i64 + 1,
            (start + 8) * LINE.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
        if step == 2 {
            assert_eq!(engine.last_layout_stats().prepared_windows, 1);
            assert!(
                engine.last_layout_stats().reused_shifted_rows > 20,
                "backward movement should consume prepared coverage: {:?}",
                engine.last_layout_stats()
            );
        }
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut reference = LayoutEngine::new();
        reference.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            actual,
            selected_window_layout_trace(&eval, &reference, frame),
            "start {start}"
        );
    }
}

#[test]
fn prepared_coverage_does_not_suppress_point_driven_recentering() {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&LINE.repeat(500), 800, 600);
    let mut engine = LayoutEngine::new();
    for start in [0, 10] {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (start * LINE.len()) as i64 + 1,
            (start + 8) * LINE.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
    }
    eval.eval_str(&format!("(goto-char {})", 100 * LINE.len() + 1))
        .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(engine.last_layout_stats().prepared_windows, 0);
    let actual = selected_window_layout_trace(&eval, &engine, frame);
    let mut reference = LayoutEngine::new();
    reference.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        actual,
        selected_window_layout_trace(&eval, &reference, frame)
    );
}
