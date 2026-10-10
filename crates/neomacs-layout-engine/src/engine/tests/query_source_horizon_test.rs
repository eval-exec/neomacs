//! Query acquisition must preserve canonical geometry and complete source spans.
use super::*;
use neovm_core::window::{FrameId, WindowId, WindowLayoutQueryScope};

fn query_fixture(text: &str, setup: &str, start: usize) -> (Context, FrameId, WindowId) {
    let (mut eval, frame, buffer, window) = incr_editing_frame(text, 80, 24);
    eval.eval_str(setup).expect("source properties");
    {
        let frame = eval.frame_manager_mut().get_mut(frame).unwrap();
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(80, 24);
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut bounds = *mini.bounds();
            bounds.height = 1.0;
            mini.set_bounds(bounds);
        }
        if let neovm_core::window::Window::Leaf { window_start, .. } =
            frame.find_window_mut(window).unwrap()
        {
            *window_start = LispCharPos1::new(start as i64);
        }
    }
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .goto_emacs_byte_pos(EmacsBytePos::new(start - 1));
    (eval, frame, window)
}

fn viewport(text: &str, setup: &str, start: usize) -> WindowDisplaySnapshot {
    let (mut eval, frame, window) = query_fixture(text, setup, start);
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .expect("canonical query converges")
        .into_geometry()
        .expect("query geometry")
}

#[test]
fn truncated_position_target_stops_before_long_physical_tail() {
    for start in [1, 161] {
        for target in [
            start as i64,
            start as i64 + 40,
            start as i64 + 78,
            start as i64 + 79,
        ] {
            let mut observations = Vec::new();
            for tail in [50_000, 800_000] {
                let (mut eval, frame, window) = query_fixture(
                    &format!("{}λ", "x".repeat(tail)),
                    "(setq truncate-lines t)",
                    start,
                );
                let mut engine = LayoutEngine::new_without_font_metrics();
                let geometry = engine
                    .query_window_layout(
                        &mut eval,
                        frame,
                        window,
                        WindowLayoutQueryScope::Position {
                            target: LispCharPos1::new(target),
                        },
                    )
                    .expect("position query")
                    .into_geometry()
                    .unwrap();
                assert_eq!(engine.query_row_coverage, QueryRowCoverage::TargetPrefix);
                assert!(
                    engine.text_buf.len() < 4_096,
                    "acquisition must remain geometry bounded"
                );
                assert!(
                    geometry
                        .point_for_buffer_pos(LispCharPos1::new(target))
                        .is_some()
                );
                assert!(
                    geometry
                        .point_for_buffer_pos(LispCharPos1::new(tail as i64 + 2))
                        .is_none()
                );
                assert!(
                    geometry
                        .rows
                        .last()
                        .unwrap()
                        .end_buffer_pos
                        .unwrap()
                        .as_i64()
                        <= target + 1
                );
                observations.push(geometry);
            }
            assert_eq!(observations[0].rows, observations[1].rows);
            assert_eq!(
                observations[0].point_for_buffer_pos(LispCharPos1::new(target)),
                observations[1].point_for_buffer_pos(LispCharPos1::new(target)),
            );
        }
    }
}

#[test]
fn truncated_multibyte_target_keeps_absolute_source_positions() {
    let (mut eval, frame, window) =
        query_fixture(&"λ".repeat(800_000), "(setq truncate-lines t)", 1);
    let mut engine = LayoutEngine::new_without_font_metrics();
    let geometry = engine
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(41),
            },
        )
        .expect("multibyte target")
        .into_geometry()
        .unwrap();
    assert_eq!(engine.query_row_coverage, QueryRowCoverage::TargetPrefix);
    assert!(engine.text_buf.len() < 8_192);
    let target = geometry
        .point_for_buffer_pos(LispCharPos1::new(41))
        .unwrap();
    assert_eq!(target.buffer_pos, LispCharPos1::new(41));
    assert_eq!(target.col, 40);
}

#[test]
fn truncated_target_edge_keeps_the_real_end_distinct() {
    for length in [80, 5_000] {
        let text = "x".repeat(length);
        let reference = viewport(&text, "(setq truncate-lines t)", 1);
        let (mut eval, frame, window) = query_fixture(&text, "(setq truncate-lines t)", 1);
        let mut engine = LayoutEngine::new_without_font_metrics();
        let edge = engine
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(80),
                },
            )
            .unwrap()
            .into_geometry()
            .unwrap();
        let point = edge.point_for_buffer_pos(LispCharPos1::new(80)).unwrap();
        assert_eq!((point.col, point.row), (79, 0));
        assert_eq!(engine.query_row_coverage, QueryRowCoverage::TargetPrefix);
        assert!(engine.text_buf.len() < 4_096);
        let after = engine
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(81),
                },
            )
            .unwrap()
            .into_geometry()
            .unwrap();
        assert_eq!(engine.query_row_coverage, QueryRowCoverage::Complete);
        assert_eq!(
            after.point_for_buffer_pos(LispCharPos1::new(81)),
            reference.point_for_buffer_pos(LispCharPos1::new(81)),
        );
        assert_eq!(
            after.point_for_buffer_pos(LispCharPos1::new(81)).is_some(),
            length == 80,
        );
    }
}

#[test]
fn truncated_target_prefix_does_not_include_later_tall_face_or_reuse_its_coverage() {
    let (mut eval, frame, window) =
        query_fixture(&"x".repeat(50_000), "(setq truncate-lines t)", 1);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        "(progn
        (internal-set-lisp-face-attribute 'query-tall-test :height 200 (selected-frame))
        (put-text-property 3 4 'face 'query-tall-test))",
    )
    .unwrap();
    let mut engine = LayoutEngine::new();
    let early_scope = WindowLayoutQueryScope::Position {
        target: LispCharPos1::new(1),
    };
    let later_scope = WindowLayoutQueryScope::Position {
        target: LispCharPos1::new(5),
    };
    let early = engine
        .query_window_layout(&mut eval, frame, window, early_scope)
        .unwrap()
        .into_geometry()
        .unwrap();
    assert_eq!(engine.query_row_coverage, QueryRowCoverage::TargetPrefix);
    assert!(
        engine
            .query_cache
            .get(&eval, frame, window, early_scope)
            .is_some()
    );
    let later = engine
        .query_window_layout(&mut eval, frame, window, later_scope)
        .unwrap()
        .into_geometry()
        .unwrap();
    assert!(later.rows[0].height > early.rows[0].height);
    engine.query_cache.clear();
    engine
        .query_window_layout(&mut eval, frame, window, later_scope)
        .unwrap();
    assert!(
        engine
            .query_cache
            .get(&eval, frame, window, early_scope)
            .is_none(),
        "a later partial row must not certify an earlier target's metrics"
    );
    let recomputed = engine
        .query_window_layout(&mut eval, frame, window, early_scope)
        .unwrap()
        .into_geometry()
        .unwrap();
    assert_eq!(recomputed.rows[0].height, early.rows[0].height);
    engine.query_cache.clear();
    engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    assert!(
        engine
            .query_cache
            .get(&eval, frame, window, early_scope)
            .is_none(),
        "a completed viewport's later metrics must not substitute for target-prefix metrics"
    );
    engine
        .query_window_layout(&mut eval, frame, window, early_scope)
        .unwrap();
    if let neovm_core::window::Window::Leaf { vscroll, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *vscroll = -1;
    }
    assert!(
        engine
            .query_cache
            .get(&eval, frame, window, early_scope)
            .is_none(),
        "a partial row cannot certify repositioning after vscroll"
    );
    let moved = engine
        .query_window_layout(&mut eval, frame, window, early_scope)
        .unwrap()
        .into_geometry()
        .unwrap();
    assert_eq!(moved.rows[0].y, early.rows[0].y - 1);
}

#[test]
fn truncated_query_retains_complete_element_and_real_eob_fallbacks() {
    for (text, setup, target) in [
        ("abc\n", "(setq truncate-lines t)", 5),
        (
            "abc\n",
            "(progn (setq truncate-lines t) (setq word-wrap t))",
            1,
        ),
        (
            "abtail\n",
            "(progn (setq truncate-lines t) (put-text-property 1 3 'composition '(0 2 [9673])))",
            1,
        ),
        (
            "abtail\n",
            "(progn (setq truncate-lines t) (put-text-property 1 3 'display \"hello\"))",
            1,
        ),
        (
            "atail\n",
            "(progn (setq truncate-lines t) (put-text-property 1 2 'display \"hello\"))",
            1,
        ),
    ] {
        let (mut eval, frame, window) = query_fixture(text, setup, 1);
        let mut engine = LayoutEngine::new_without_font_metrics();
        let geometry = engine
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(target),
                },
            )
            .unwrap()
            .into_geometry()
            .unwrap();
        assert_eq!(engine.query_row_coverage, QueryRowCoverage::Complete);
        if target == 5 {
            assert!(
                geometry
                    .point_for_buffer_pos(LispCharPos1::new(5))
                    .is_some()
            );
        }
    }
}

#[test]
fn query_long_wrapped_tail_does_not_change_visible_rows_or_points() {
    for start in [1, 161] {
        let short = viewport(&"x".repeat(5_000), "nil", start);
        let long = viewport(&"x".repeat(800_000), "nil", start);
        assert_eq!(short.rows, long.rows);
        for pos in [start as i64, start as i64 + 40] {
            let pos = LispCharPos1::new(pos);
            assert_eq!(
                short.point_for_buffer_pos(pos),
                long.point_for_buffer_pos(pos)
            );
            assert!(long.point_for_buffer_pos(pos).is_some());
        }
        assert!(long.rows.last().unwrap().end_buffer_pos.unwrap().as_i64() < 5_000);
    }
}

#[test]
fn query_horizon_retry_preserves_hidden_span_and_visible_suffix() {
    let hidden = viewport(
        &format!("{}{}", "x".repeat(4_000), "abc\n".repeat(40)),
        "(put-text-property 1 4001 'invisible t)",
        1,
    );
    assert!(
        hidden
            .rows
            .iter()
            .any(|row| row.end_buffer_pos.is_some_and(|p| p.as_i64() > 4_001))
    );
    assert!(
        hidden
            .point_for_buffer_pos(LispCharPos1::new(4_002))
            .is_some()
    );
}

#[test]
fn final_truncated_query_row_requires_a_real_physical_line_end() {
    let (mut eval, frame, window) = query_fixture(
        &format!("{}\nvisible\n", "x".repeat(10_000)),
        "(setq truncate-lines t)",
        1,
    );
    let mut engine = LayoutEngine::new_without_font_metrics();
    let geometry = engine
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Rows {
                start: LispCharPos1::ONE,
                count: std::num::NonZeroUsize::new(1).unwrap(),
            },
        )
        .unwrap()
        .into_geometry()
        .unwrap();
    assert_eq!(geometry.rows.len(), 1);
    assert_eq!(
        geometry.rows[0].truncated_end_buffer_pos,
        Some(LispCharPos1::new(10_001))
    );
    assert_eq!(engine.query_row_coverage, QueryRowCoverage::Complete);
}

#[test]
fn selective_query_classifies_complete_indentation_and_carriage_return_tails() {
    for (text, setup, visible) in [
        (
            format!("header\n{}hidden\nvisible\n", " ".repeat(5_000)),
            "(setq selective-display 4000)",
            5_015,
        ),
        (
            format!("header\r{}\nvisible\n", "x".repeat(5_000)),
            "(setq selective-display t)",
            5_009,
        ),
    ] {
        let geometry = viewport(&text, setup, 1);
        let point = geometry
            .point_for_buffer_pos(LispCharPos1::new(visible))
            .unwrap();
        assert_eq!(
            point.row, 1,
            "the long hidden source must not occupy visible rows"
        );
        assert_eq!(point.col, 0);
    }
}
