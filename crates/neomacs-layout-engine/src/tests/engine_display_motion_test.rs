//! Lisp-visible motion through the real row producer, not a synthetic matrix.

use super::*;
use neovm_core::window::WindowLayoutQueryOutcome;

fn position_query_fixture(
    text: &str,
    width: u32,
    height: u32,
) -> (
    Context,
    neovm_core::window::FrameId,
    neovm_core::window::WindowId,
) {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(text);
    let frame = eval
        .frame_manager_mut()
        .create_frame("position-query", width, height, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 1) (set-window-start nil 1 t)").unwrap();
    (eval, frame, window)
}

#[test]
fn position_query_finishes_the_target_row_without_publishing_or_certifying_a_viewport() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let (mut eval, frame, window) = position_query_fixture(&"ordinary row\n".repeat(100), 400, 320);
    eval.eval_str("(put-text-property 9 12 'face '(:height 200))")
        .unwrap();
    let start_before = eval.eval_str("(window-start)").unwrap();
    let target = LispCharPos1::new(3);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let prefix = query
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Position { target },
        )
        .unwrap();
    let full = query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    let actual = prefix.geometry().unwrap();
    let expected = full.geometry().unwrap();
    assert_eq!(actual.rows.len(), 1, "target query walked later rows");
    assert!(
        expected.rows.len() > actual.rows.len(),
        "prefix was reused as a viewport"
    );
    assert_eq!(
        actual.rows[0], expected.rows[0],
        "late target-row metrics were omitted"
    );
    assert_eq!(
        actual.point_for_buffer_pos(target),
        expected.point_for_buffer_pos(target)
    );
    assert_eq!(actual.regions, expected.regions);
    assert!(prefix.end() < full.end());
    assert_eq!(eval.eval_str("(window-start)").unwrap(), start_before);
    assert!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .is_none()
    );
}

#[test]
fn position_queries_reuse_full_viewports_with_exact_inputs_only() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let (mut eval, frame, window) = position_query_fixture(&"ordinary row\n".repeat(100), 400, 240);
    eval.eval_str("(setq position-cache-face (list :height 100)) (put-text-property 1 30 'face position-cache-face)").unwrap();
    let target = LispCharPos1::new(50);
    let scope = WindowLayoutQueryScope::Position { target };
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let full = query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    for target in [target, LispCharPos1::new(1000)] {
        probe::reset();
        let actual = query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position { target },
            )
            .unwrap();
        assert_eq!(
            probe::max_depth(),
            0,
            "exact viewport was walked again for target {target:?}"
        );
        assert_eq!(actual.geometry(), full.geometry());
        let fresh = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position { target },
            )
            .unwrap();
        assert_eq!(
            actual.geometry().unwrap().point_for_buffer_pos(target),
            fresh.geometry().unwrap().point_for_buffer_pos(target)
        );
    }
    for mutation in [
        "(setcar (cdr position-cache-face) 200)",
        "(goto-char 20)",
        "(set-window-start nil 14 t)",
    ] {
        eval.eval_str(mutation).unwrap();
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "changed source reused a full viewport: {mutation}"
        );
        let fresh = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.geometry(), fresh.geometry(), "{mutation}");
    }
}

#[test]
fn position_prefix_reuses_earlier_complete_rows_with_exact_inputs() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let (mut eval, frame, window) = position_query_fixture(&"ordinary row\n".repeat(100), 400, 240);
    eval.eval_str("(goto-char 1000) (set-window-vscroll nil 2 t t)")
        .unwrap();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let prefix = query
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(60),
            },
        )
        .unwrap();
    for pixels in [2, 3, 7, 2] {
        eval.eval_str(&format!("(set-window-vscroll nil {pixels} t t)"))
            .unwrap();
        for position in [1, 3, 14, 39, 59, 60] {
            let target = LispCharPos1::new(position);
            let scope = WindowLayoutQueryScope::Position { target };
            probe::reset();
            let actual = query
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            assert_eq!(
                probe::max_depth(),
                0,
                "prefix walked again at {position}, vscroll {pixels}"
            );
            let expected = WindowLayoutQueryEngine::new_without_font_metrics()
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            assert_eq!(actual.end(), prefix.end());
            let actual = actual.geometry().unwrap();
            let expected = expected.geometry().unwrap();
            let point = expected.point_for_buffer_pos(target).unwrap();
            assert_eq!(actual.point_for_buffer_pos(target), Some(point));
            assert_eq!(
                actual.row_metrics(point.row),
                expected.row_metrics(point.row)
            );
            assert_eq!(actual.regions, expected.regions);
        }
    }
    for position in [61, 1000] {
        let scope = WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(position),
        };
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "prefix claimed an uncertified target {position}"
        );
        let expected = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.geometry(), expected.geometry());
    }
}

#[test]
fn position_prefix_reuse_preserves_wrap_inserted_strings_and_hidden_target_points() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for decoration in [
        "(setq word-wrap t) (put-text-property 1 150 'wrap-prefix \"p>\")",
        "(let ((o (make-overlay 5 12))) (overlay-put o 'before-string \"before\\nmore\\n\") (overlay-put o 'after-string \"after\\nend\")) (put-text-property 45 60 'display \"replace\\nnext\\nlast\")",
        "(setq buffer-invisibility-spec t) (put-text-property 15 60 'invisible t)",
    ] {
        let (mut eval, frame, window) = position_query_fixture(
            &"ab\twords around the wrapping edge and more\n".repeat(100),
            160,
            400,
        );
        eval.eval_str("(goto-char 3000)").unwrap();
        eval.eval_str(decoration).unwrap();
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        let prefix = query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(80),
                },
            )
            .unwrap();
        let covered: Vec<_> = (1..80)
            .map(LispCharPos1::new)
            .filter(|target| {
                prefix
                    .geometry()
                    .unwrap()
                    .point_for_buffer_pos(*target)
                    .is_some_and(|point| point.buffer_pos == *target)
            })
            .collect();
        assert!(!covered.is_empty(), "empty fixture: {decoration}");
        for target in covered {
            probe::reset();
            let scope = WindowLayoutQueryScope::Position { target };
            let actual = query
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            assert_eq!(
                probe::max_depth(),
                0,
                "covered target rewalked: {decoration}, {target:?}"
            );
            let expected = WindowLayoutQueryEngine::new_without_font_metrics()
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            assert_eq!(
                actual.geometry().unwrap().point_for_buffer_pos(target),
                expected.geometry().unwrap().point_for_buffer_pos(target),
                "{decoration}, {target:?}"
            );
        }
    }
}

#[test]
fn position_prefix_reuse_falls_back_for_hidden_neighbor_and_missing_points() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let (mut eval, frame, window) = position_query_fixture(&"ordinary row\n".repeat(100), 400, 240);
    eval.eval_str(
        "(goto-char 1000) (setq buffer-invisibility-spec t) (put-text-property 3 8 'invisible t)",
    )
    .unwrap();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let prefix = query
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(60),
            },
        )
        .unwrap();
    let hidden = LispCharPos1::new(5);
    assert_ne!(
        prefix
            .geometry()
            .unwrap()
            .point_for_buffer_pos(hidden)
            .unwrap()
            .buffer_pos,
        hidden,
        "fixture must resolve hidden position through a neighbor"
    );
    for target in [hidden, LispCharPos1::new(0)] {
        let scope = WindowLayoutQueryScope::Position { target };
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "prefix claimed absent exact point {target:?}"
        );
        let expected = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.geometry(), expected.geometry());
    }
}

#[test]
fn position_prefix_reuse_rejects_changed_collections_point_callbacks_and_clipped_cursor() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for mutation in [
        "(setcar (cdr position-prefix-face) 200)",
        "(goto-char 20)",
        "(setq fontification-functions position-prefix-hooks)",
        "(set-window-vscroll nil 80 t t)",
    ] {
        let (mut eval, frame, window) =
            position_query_fixture(&"ordinary row\n".repeat(100), 400, 240);
        eval.eval_str(
            r#"(goto-char 1000) (set-window-vscroll nil 2 t t)
            (setq position-prefix-face (list :height 100))
            (put-text-property 1 80 'face position-prefix-face)
            (setq fontification-functions nil position-prefix-hooks
                (list (lambda (start) (put-text-property start (point-max) 'fontified t))))"#,
        )
        .unwrap();
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(60),
                },
            )
            .unwrap();
        eval.eval_str(mutation).unwrap();
        let scope = WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(14),
        };
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "changed inputs reused prefix: {mutation}"
        );
        let expected = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.geometry(), expected.geometry(), "{mutation}");
    }
    for decoration in [
        "(goto-char 1)",
        r#"(goto-char 1000) (setq prefix-condition-calls 0)
            (put-text-property 1 2 'display
                '(when (progn (setq prefix-condition-calls (1+ prefix-condition-calls)) nil) . "unused"))"#,
    ] {
        let (mut eval, frame, window) =
            position_query_fixture(&"ordinary row\n".repeat(100), 400, 240);
        eval.eval_str(decoration).unwrap();
        eval.eval_str("(set-window-vscroll nil 2 t t)").unwrap();
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(60),
                },
            )
            .unwrap();
        eval.eval_str("(set-window-vscroll nil 3 t t)").unwrap();
        let scope = WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(14),
        };
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "callback/clipped cursor reused prefix: {decoration}"
        );
        let expected = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.geometry(), expected.geometry());
    }
}

#[test]
fn position_prefixes_reposition_complete_rows_with_variable_heights_and_overlays() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for decoration in [
        "nil",
        "(put-text-property 1 40 'face '(:height 150))",
        "(let ((o (make-overlay 14 25))) (overlay-put o 'before-string \"prefix\") (overlay-put o 'after-string \"suffix\") (overlay-put o 'face '(:height 125)))",
        "(put-text-property 1 40 'line-height 1.3) (put-text-property 18 21 'display '(raise 0.2))",
    ] {
        let (mut eval, frame, window) =
            position_query_fixture(&"ordinary row\n".repeat(200), 400, 240);
        eval.eval_str("(goto-char 1000) (set-window-vscroll nil 2 t t)")
            .unwrap();
        eval.eval_str(decoration).unwrap();
        let scope = WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(60),
        };
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        let mut reused = 0;
        for pixels in 3..16 {
            eval.eval_str(&format!("(set-window-vscroll nil {pixels} t t)"))
                .unwrap();
            probe::reset();
            let actual = query
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            reused += usize::from(probe::max_depth() == 0);
            let fresh = WindowLayoutQueryEngine::new_without_font_metrics()
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            assert_eq!(actual.end(), fresh.end(), "pixels={pixels}, {decoration}");
            assert_eq!(
                actual.geometry(),
                fresh.geometry(),
                "pixels={pixels}, {decoration}"
            );
        }
        if decoration.contains("line-height") {
            assert_eq!(reused, 0, "fractional noncontiguous rows were translated");
        } else {
            assert!(
                reused > 0,
                "complete target prefix was walked again: {decoration}"
            );
        }
        // Crossing the first-row edge requires a fresh canonical observation.
        eval.eval_str("(set-window-vscroll nil 80 t t)").unwrap();
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "first-row edge crossing reused a prefix"
        );
        let fresh = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.end(), fresh.end());
        assert_eq!(actual.geometry(), fresh.geometry());
    }
}

#[test]
fn position_prefix_reposition_preserves_partial_bottom_target_row_clipping() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let (mut eval, frame, window) = position_query_fixture(&"ordinary row\n".repeat(200), 400, 160);
    eval.eval_str("(goto-char 1000) (set-window-vscroll nil 2 t t) (put-text-property 119 129 'face '(:height 150))").unwrap();
    let target = LispCharPos1::new(128);
    let scope = WindowLayoutQueryScope::Position { target };
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let first = query
        .query_window_layout(&mut eval, frame, window, scope)
        .unwrap();
    let snapshot = first.geometry().unwrap();
    let point = snapshot
        .point_for_buffer_pos(target)
        .expect("partial bottom target");
    let row = snapshot.row_metrics(point.row).unwrap();
    let top = (snapshot.regions.text_body.y - snapshot.regions.outer.y) as i64;
    let bottom = top + snapshot.regions.text_body.height as i64;
    assert!(
        row.y < bottom && row.y + row.height > bottom,
        "fixture target must begin partially clipped"
    );
    for pixels in 3..16 {
        eval.eval_str(&format!("(set-window-vscroll nil {pixels} t t)"))
            .unwrap();
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(
            probe::max_depth(),
            0,
            "partial target row was walked again at {pixels}"
        );
        let fresh = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.end(), fresh.end());
        assert_eq!(
            actual.geometry(),
            fresh.geometry(),
            "partial target at {pixels}"
        );
    }
}

#[test]
fn position_prefix_reposition_falls_back_for_missing_targets_and_clipped_cursors() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for (point, target) in [(1000, 2000), (1, 50)] {
        let (mut eval, frame, window) =
            position_query_fixture(&"ordinary row\n".repeat(200), 400, 240);
        eval.eval_str(&format!(
            "(goto-char {point}) (set-window-vscroll nil 2 t t)"
        ))
        .unwrap();
        let scope = WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(target),
        };
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        eval.eval_str("(set-window-vscroll nil 3 t t)").unwrap();
        probe::reset();
        let actual = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "ambiguous target/cursor placement reused rows: point={point}, target={target}"
        );
        let fresh = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.end(), fresh.end());
        assert_eq!(actual.geometry(), fresh.geometry());
    }
}

#[test]
fn position_queries_preserve_wrap_overlay_hidden_and_partial_row_geometry() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for decoration in [
        "nil",
        "(setq word-wrap t) (put-text-property 1 180 'wrap-prefix \"p>\")",
        "(put-text-property 1 20 'line-height 1.3) (put-text-property 8 10 'display '(raise 0.2)) (put-text-property 30 55 'face '(:height 175))",
        "(let ((o (make-overlay 5 12))) (overlay-put o 'before-string \"before\\nmore\\n\") (overlay-put o 'after-string \"after\\nend\")) (put-text-property 45 60 'display \"replace\\nnext\\nlast\")",
        "(setq buffer-invisibility-spec t) (put-text-property 15 60 'invisible t)",
        "(setq truncate-lines t) (set-window-hscroll nil 3)",
    ] {
        let (mut eval, frame, window) = position_query_fixture(
            &"ab\twords around the wrapping edge and more\n".repeat(100),
            160,
            150,
        );
        eval.eval_str(decoration).unwrap();
        for vscroll in [0, 3, 15] {
            eval.eval_str(&format!("(set-window-vscroll nil {vscroll} t t)"))
                .unwrap();
            let full = WindowLayoutQueryEngine::new_without_font_metrics()
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap()
                .into_geometry()
                .unwrap();
            let mut targets = vec![
                LispCharPos1::new(1),
                LispCharPos1::new(16),
                LispCharPos1::new(46),
                LispCharPos1::new(1000),
            ];
            // Row starts include exact wrap boundaries and the partial bottom row.
            targets.extend(full.rows.iter().filter_map(|row| row.start_buffer_pos));
            targets.sort();
            targets.dedup();
            for target in targets {
                let prefix = WindowLayoutQueryEngine::new_without_font_metrics()
                    .query_window_layout(
                        &mut eval,
                        frame,
                        window,
                        WindowLayoutQueryScope::Position { target },
                    )
                    .unwrap()
                    .into_geometry()
                    .unwrap();
                let actual = prefix.point_for_buffer_pos(target);
                let expected = full.point_for_buffer_pos(target);
                assert_eq!(
                    actual, expected,
                    "target={target:?}, vscroll={vscroll}, {decoration}"
                );
                assert_eq!(prefix.regions, full.regions);
                if let Some(point) = expected {
                    assert_eq!(
                        prefix.row_metrics(point.row),
                        full.row_metrics(point.row),
                        "target row incomplete: target={target:?}, {decoration}"
                    );
                }
            }
        }
    }
}

#[test]
fn position_queries_preserve_end_of_buffer_insertion_rows() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for text in ["", "last", "last\n"] {
        let (mut eval, frame, window) = position_query_fixture(text, 240, 160);
        let target = LispCharPos1::new(text.chars().count() as i64 + 1);
        let full = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
            .unwrap();
        let prefix = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Position { target },
            )
            .unwrap();
        assert_eq!(prefix.end(), full.end(), "{text:?}");
        assert_eq!(
            prefix.geometry().unwrap().rows,
            full.geometry().unwrap().rows,
            "{text:?}"
        );
        assert_eq!(
            prefix.geometry().unwrap().point_for_buffer_pos(target),
            full.geometry().unwrap().point_for_buffer_pos(target),
            "{text:?}"
        );
    }
}

#[test]
fn position_queries_keep_full_sparse_fontification_callback_extent() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let hidden = "hidden\n".repeat(2000);
    let text = format!("first\n{hidden}TAIL\n");
    let tail = 7 + hidden.len();
    let mut observations = Vec::new();
    for scope in [
        WindowLayoutQueryScope::Viewport,
        WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(2),
        },
    ] {
        let (mut eval, frame, window) = position_query_fixture(&text, 360, 140);
        eval.eval_str(&format!(r#"(setq buffer-invisibility-spec t) (put-text-property 7 {tail} 'invisible t)
            (setq position-fontify-calls nil fontification-functions
                (list (lambda (start) (setq position-fontify-calls (cons start position-fontify-calls))
                    (put-text-property start (min (point-max) (+ start 80)) 'fontified t))))"#)).unwrap();
        let result = WindowLayoutQueryEngine::new_without_font_metrics()
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        let calls = eval
            .eval_str("(prin1-to-string position-fontify-calls)")
            .unwrap()
            .as_runtime_string_owned()
            .unwrap();
        let tail_fontified = eval
            .eval_str(&format!("(get-text-property {tail} 'fontified)"))
            .unwrap();
        assert!(
            tail_fontified.is_t(),
            "post-fold callback was omitted: {scope:?}"
        );
        observations.push((result.geometry().unwrap().rows.clone(), calls));
    }
    assert_eq!(observations[0], observations[1]);
}

#[test]
fn position_queries_disable_the_row_stop_when_display_conditions_install_fontification() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let (mut eval, frame, window) = position_query_fixture(&"ordinary row\n".repeat(100), 400, 240);
    eval.eval_str(r#"(setq fontification-functions nil position-fontify-calls nil)
        (put-text-property 1 2 'display
            '(when (progn
                (if fontification-functions nil
                    (setq fontification-functions
                        (list (lambda (start)
                            (setq position-fontify-calls (cons start position-fontify-calls))
                            (put-text-property start (min (point-max) (+ start 80)) 'fontified t)))))
                nil) . "unused"))"#).unwrap();
    let prefix = WindowLayoutQueryEngine::new_without_font_metrics()
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(3),
            },
        )
        .unwrap();
    let full = WindowLayoutQueryEngine::new_without_font_metrics()
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    assert!(
        prefix.geometry().unwrap().rows.len() > 1,
        "callbacks installed during preparation left a prefix stop"
    );
    assert_eq!(
        prefix.geometry().unwrap().rows,
        full.geometry().unwrap().rows
    );
    assert!(!eval.eval_str("position-fontify-calls").unwrap().is_nil());
}

#[test]
fn position_query_cache_observes_newly_enabled_existing_fontification_hooks() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let hidden = "hidden\n".repeat(2000);
    let tail = 7 + hidden.len();
    let scope = WindowLayoutQueryScope::Position {
        target: LispCharPos1::new(2),
    };
    for initial_scope in [scope, WindowLayoutQueryScope::Viewport] {
        let (mut eval, frame, window) =
            position_query_fixture(&format!("first\n{hidden}TAIL\n"), 360, 140);
        // Prepare the function before caching so enabling it does not change the
        // function epoch, buffer properties or any captured collection identity.
        eval.eval_str(&format!(
            r#"(setq buffer-invisibility-spec t) (put-text-property 7 {tail} 'invisible t)
        (setq fontification-functions nil position-fontify-calls nil)
        (setq prepared-position-fontify-hooks
            (list (lambda (start)
                (setq position-fontify-calls (cons start position-fontify-calls))
                (put-text-property start (min (point-max) (+ start 80)) 'fontified t))))"#
        ))
        .unwrap();
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        let first = query
            .query_window_layout(&mut eval, frame, window, initial_scope)
            .unwrap();
        if initial_scope == scope {
            assert_eq!(first.geometry().unwrap().rows.len(), 1);
        } else {
            assert!(first.geometry().unwrap().rows.len() > 1);
        }
        eval.eval_str("(setq fontification-functions prepared-position-fontify-hooks)")
            .unwrap();
        let second = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            second.geometry().unwrap().rows.len() > 1,
            "cached prefix bypassed newly installed hooks"
        );
        assert!(
            eval.eval_str(&format!("(get-text-property {tail} 'fontified)"))
                .unwrap()
                .is_t(),
            "cached prefix omitted the post-fold callback"
        );
        assert!(!eval.eval_str("position-fontify-calls").unwrap().is_nil());
    }
}

#[test]
fn redisplay_keeps_a_fully_visible_cursor_row_when_its_source_line_continues() {
    use neovm_core::window::WindowLayoutQueryScope;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&format!("head\n{}\n", "x".repeat(2000)));
    let frame = eval
        .frame_manager_mut()
        .create_frame("visible-wrap-cursor", 160, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 1) (set-window-start nil 1 t)").unwrap();
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let snapshot = query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap()
        .into_geometry()
        .unwrap();
    let body = snapshot.regions.text_body;
    let outer = snapshot.regions.outer;
    let row = snapshot
        .rows
        .iter()
        .rev()
        .find(|row| {
            row.start_buffer_pos.is_some()
                && row.y as f32 + outer.y >= body.y
                && (row.y + row.height) as f32 + outer.y <= body.bottom()
        })
        .unwrap();
    let point = row.start_buffer_pos.unwrap();
    assert!(point.as_i64() > 6 && point.as_i64() < 2000);
    eval.eval_str(&format!(
        "(goto-char {}) (set-window-start nil 1 t)",
        point.as_i64()
    ))
    .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        eval.eval_str("(window-start)").unwrap(),
        Value::fixnum(1),
        "a complete visible screen row must not scroll to expose the rest of its physical line"
    );
    assert_eq!(
        eval.eval_str("(point)").unwrap(),
        Value::fixnum(point.as_i64())
    );
}

#[test]
fn small_vertical_motion_measures_only_the_needed_rows() {
    use neovm_core::window::WindowLayoutQueryScope;
    use std::{cell::RefCell, rc::Rc};
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"row\n".repeat(1000));
    let frame = eval
        .frame_manager_mut()
        .create_frame("motion-budget", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let counts = Rc::new(RefCell::new(Vec::new()));
    let observed = counts.clone();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        if let WindowLayoutQueryScope::Rows { count, .. } = &scope {
            observed.borrow_mut().push(count.get());
        }
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval.eval_str("(progn (goto-char 2001) (let ((noninteractive nil)) (list (vertical-motion 1) (point) (vertical-motion -1) (point))))").unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(1 2005 -1 2001)"
    );
    let counts = counts.borrow();
    assert!(
        !counts.is_empty(),
        "must exercise offscreen row measurement"
    );
    assert!(
        counts.iter().all(|count| *count <= 4),
        "one-row motion overmeasured: {counts:?}"
    );
}

#[test]
fn small_backward_pixel_measurement_starts_with_bounded_rows() {
    use neovm_core::window::WindowLayoutQueryScope;
    use std::{cell::RefCell, rc::Rc};
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"row\n".repeat(1000));
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-budget", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let counts = Rc::new(RefCell::new(Vec::new()));
    let observed = counts.clone();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        if let WindowLayoutQueryScope::Rows { count, .. } = &scope {
            observed.borrow_mut().push(count.get());
        }
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval.eval_str("(progn (goto-char 2001) (cdr (window-text-pixel-size nil '(2001 . -1) 2001 nil nil nil t)))").unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(16 1997)"
    );
    assert!(!counts.borrow().is_empty());
    assert!(
        counts.borrow().iter().all(|count| *count <= 4),
        "one-pixel measurement overmeasured: {:?}",
        counts.borrow()
    );
}

#[test]
fn backward_pixel_measurement_uses_the_offscreen_rows_actual_height() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("row\nnext\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-height", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(progn (put-text-property 1 2 'display '(space :height 4)) (goto-char 5) (set-window-start nil 5 t))").unwrap();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval.eval_str("(list (mapcar (lambda (offset) (cdr (window-text-pixel-size nil (cons 5 offset) 5 nil nil nil t))) '(-1 -63 -64 -65)) (window-start) (point))").unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(((64 1) (64 1) (64 1) (64 1)) 5 5)"
    );
    let result = eval.eval_str("(progn (put-text-property 1 2 'display '(space :height 2)) (cdr (window-text-pixel-size nil '(5 . -1) 5 nil nil nil t)))").unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(32 1)"
    );
}

#[test]
fn backward_pixel_measurement_expands_past_wrapped_source_lines() {
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&format!("{}\nnext\n", "x".repeat(2000)));
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-wrap", 160, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let snapshot = query
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Rows {
                start: LispCharPos1::ONE,
                count: NonZeroUsize::new(256).unwrap(),
            },
        )
        .unwrap()
        .into_geometry()
        .unwrap();
    let rows: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|row| row.start_buffer_pos.is_some())
        .collect();
    let anchor = rows
        .iter()
        .position(|row| row.start_buffer_pos == Some(LispCharPos1::new(2002)))
        .unwrap();
    assert!(
        anchor > 16,
        "exercise expansion beyond the initial query budget"
    );
    let preceding = rows[anchor - 1];
    let expected = format!(
        "({} {})",
        rows[anchor].y - preceding.y,
        preceding.start_buffer_pos.unwrap().as_i64()
    );
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str("(cdr (window-text-pixel-size nil '(2002 . -1) 2002 nil nil nil t))")
        .unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        expected
    );
    // The same ambiguity occurs at a continuation boundary shared with the
    // preceding row's end, not only after the source newline.
    let origin = preceding.start_buffer_pos.unwrap().as_i64();
    let earlier = rows[anchor - 2];
    let result = eval
        .eval_str(&format!(
            "(cdr (window-text-pixel-size nil '({origin} . -1) {origin} nil nil nil t))"
        ))
        .unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        format!(
            "({} {})",
            preceding.y - earlier.y,
            earlier.start_buffer_pos.unwrap().as_i64()
        )
    );
}

#[test]
fn consecutive_scroll_commands_preserve_the_original_goal_past_short_rows() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"abcdefghij\nx\nabcdefghij\n".repeat(10));
    let frame = eval
        .frame_manager_mut()
        .create_frame("scroll-goal", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil)
      (scroll-preserve-screen-position 'always) (last-command nil))
      (put 'scroll-up 'scroll-command t)
      (goto-char 6) (set-window-start nil 1 t)
      (scroll-up 1)
      (let ((short-row (list (window-start) (point))))
        (setq last-command 'scroll-up)
        (scroll-up 1)
        (let ((long-row (list (window-start) (point))))
          (goto-char 16) (setq last-command 'forward-char)
          (scroll-up 1)
          (list short-row long-row (list (window-start) (point))))))"#,
        )
        .expect("retain the original pixel goal across scroll commands");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((12 13) (14 19) (25 27))"
    );
}

#[test]
fn graphical_scrolling_honors_screen_position_and_scroll_margin() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"row\n".repeat(40));
    let frame = eval
        .frame_manager_mut()
        .create_frame("scroll-policy", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
      (setq scroll-preserve-screen-position 'always scroll-margin 0)
      (goto-char 10) (set-window-start nil 1 t) (scroll-up 1)
      (let ((preserved (list (window-start) (point))))
        (setq scroll-preserve-screen-position nil scroll-margin 2 maximum-scroll-margin 0.5)
        (goto-char 1) (set-window-start nil 1 t) (scroll-up 1)
        (list preserved (list (window-start) (point)))))"#,
        )
        .expect("scroll policy");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((5 14) (5 13))"
    );
}

#[test]
fn motion_backtracking_does_not_reseat_inside_a_multiline_replacement() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert("AAA\n\nBBB\nCCC\n");
    eval.frame_manager_mut()
        .create_frame("source-boundary", 400, 160, buffer);
    eval.eval_str(r#"(put-text-property 1 7 'display "X")"#)
        .expect("replacement covers newlines");
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(progn (goto-char 8)
        (let ((noninteractive nil)) (list (vertical-motion 0) (point))))"#,
        )
        .expect("backtrack over replacement owner");
    assert_eq!(neovm_core::emacs_core::print::print_value(&result), "(0 1)");
}

#[test]
fn offscreen_row_queries_preserve_automatic_composition_metrics() {
    use neovm_core::window::{DisplayPointRole, WindowLayoutQueryScope};
    use std::num::NonZeroUsize;
    let mut eval = Context::new();
    crate::test_composition::install_rules(&mut eval);
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    let text = format!("{}👩‍💻Z\n", format!("{}\n", "a".repeat(40)).repeat(240));
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&text);
    let frame = eval
        .frame_manager_mut()
        .create_frame("offscreen-composition", 400, 160, buffer);
    let window = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .selected_window;
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let mut widths = Vec::new();
    for (start, count) in [(9841, 2), (1, 256)] {
        let snapshot = query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::new(start),
                    count: NonZeroUsize::new(count).expect("row budget"),
                },
            )
            .expect("canonical row query")
            .into_geometry()
            .expect("geometry");
        widths.push(
            snapshot
                .points
                .iter()
                .find(|point| {
                    point.role == DisplayPointRole::Glyph
                        && point.buffer_pos == LispCharPos1::new(9841)
                })
                .expect("emoji source position in measured rows")
                .width,
        );
    }
    assert_eq!(
        widths,
        vec![32, 32],
        "query distance must not change composition"
    );
}

#[test]
fn scrolling_restarts_after_fontification_moves_source_positions() {
    for window_system in [None, Some(Value::symbol("neomacs"))] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .expect("buffer")
            .insert(&"row\n".repeat(40));
        let frame = eval
            .frame_manager_mut()
            .create_frame("scroll-fontification", 400, 160, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .expect("frame")
            .window_system = window_system;
        eval.eval_str(
        "(progn (set-window-buffer nil (current-buffer)) (goto-char 5) (set-window-start nil 5 t))",
    )
    .expect("initial marker-backed viewport");
        let mut display = LayoutEngine::new_without_font_metrics();
        display.layout_frame_rust(&mut eval, frame);
        activate_last_engine_presentation(&mut eval, &display, frame);
        eval.eval_str(
            r#"(progn (setq motion-fontified nil)
        (setq fontification-functions
          (list (lambda (_start)
            (if motion-fontified nil
              (progn (setq motion-fontified t)
                (save-excursion (goto-char 1) (insert "new\n"))))
            (put-text-property (point-min) (point-max) 'fontified t)))))"#,
        )
        .expect("fontification edits source");
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        eval.install_window_layout_query(move |eval, frame, window, scope| {
            match query.query_window_layout(eval, frame, window, scope) {
                Ok(query) => WindowLayoutQueryOutcome::Ready(query),
                Err(error) => WindowLayoutQueryOutcome::Failed(error),
            }
        });
        let result = eval
            .eval_str(
                r#"(let ((noninteractive nil))
        (scroll-up 0) (list motion-fontified (window-start) (point)))"#,
            )
            .expect("scroll survives fontification");
        assert_eq!(
            neovm_core::emacs_core::print::print_value(&result),
            "(t 9 9)"
        );
    }
}

#[test]
fn measured_motion_distinguishes_overlay_insertions_from_replacements() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert("AAA\n\nBBB\n");
    eval.frame_manager_mut()
        .create_frame("overlay-motion", 400, 240, buffer);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval.eval_str(r#"(let ((noninteractive nil))
        (mapcar (lambda (property)
          (let ((overlay (make-overlay 1 2)))
            (overlay-put overlay property "X\nY\n")
            (prog1
              (list
                (mapcar (lambda (n) (goto-char 1) (list (vertical-motion n) (point))) '(0 1 2 3 4 5))
                (mapcar (lambda (n) (goto-char 10) (list (vertical-motion n) (point))) '(-1 -2 -3 -4 -5)))
              (delete-overlay overlay)))) '(before-string after-string)))"#).expect("motion through overlay insertions");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((((0 1) (1 5) (2 6) (3 10) (3 10) (3 10)) ((-1 6) (-2 5) (-3 1) (-4 1) (-5 1))) (((0 1) (2 2) (3 5) (4 6) (5 10) (5 10)) ((-1 6) (-2 5) (-3 2) (-4 2) (-5 1))))"
    );
}

#[test]
fn measured_motion_reaches_an_invisible_accessible_beginning() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"row\n".repeat(100));
    eval.frame_manager_mut()
        .create_frame("hidden-beginning", 400, 160, buffer);
    eval.eval_str(
        r#"(progn (setq buffer-invisibility-spec '(hidden))
        (put-text-property 1 3 'invisible 'hidden) (goto-char 101))"#,
    )
    .expect("hidden prefix");
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
        (list (vertical-motion -1000) (point)))"#,
        )
        .expect("back past beginning");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(-25 1)"
    );
}

#[test]
fn page_scrolling_back_to_a_tall_row_keeps_point_in_the_new_viewport() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"row\n".repeat(40));
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-paging", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        r#"(progn
        (setq auto-window-vscroll nil scroll-preserve-screen-position nil)
        (put-text-property 1 2 'display '(space :height 20))
        (goto-char 5) (set-window-start nil 5 t))"#,
    )
    .expect("start below a tall row");
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &display, frame);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
        (scroll-down)
        (list (window-start) (point) (window-vscroll nil t)))"#,
        )
        .expect("back over tall row");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(1 1 0)"
    );
    display.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &display, frame);
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
        (list (condition-case nil (scroll-down) (beginning-of-buffer 'at-start))
              (window-start) (point)))"#,
        )
        .expect("stay at beginning after redisplay");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(at-start 1 1)"
    );
}

#[test]
fn scrolling_promotes_a_bottom_clipped_point_row_before_pixel_scrolling() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"row\n".repeat(40));
    let frame = eval
        .frame_manager_mut()
        .create_frame("clipped-point-row", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        r#"(progn
      (setq auto-window-vscroll t scroll-preserve-screen-position nil)
      (put-text-property 9 10 'display '(space :height 20))
      (goto-char 9) (set-window-start nil 1 t))"#,
    )
    .expect("point on clipped third row");
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
      (scroll-up 1)
      (let ((promoted (list (window-start) (point) (window-vscroll nil t))))
        (scroll-up 1)
        (list promoted (window-start) (point) (> (window-vscroll nil t) 0))))"#,
        )
        .expect("promote then pixel-scroll the tall point row");
    // GNU window_scroll_pixel_based first moves start to the clipped point
    // row, keeping point; only the next scroll changes its pixel offset.
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((9 9 0) 9 9 t)"
    );
}

#[test]
fn page_scrolling_a_tall_first_row_uses_pixels_and_returns_to_the_same_viewport() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"row\n".repeat(40));
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-paging", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        r#"(progn
        (setq auto-window-vscroll t scroll-preserve-screen-position nil)
        (put-text-property 1 2 'display '(space :height 20))
        (goto-char 1) (set-window-start nil 1 t))"#,
    )
    .expect("tall row");
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &display, frame);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
        (scroll-up)
        (let ((forward (list (window-start) (> (window-vscroll nil t) 0))))
          (scroll-down)
          (list forward (window-start) (window-vscroll nil t) (point))))"#,
        )
        .expect("page inside a tall row and back");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((1 t) 1 0 1)"
    );
}

#[test]
fn vertical_motion_measures_replacements_outside_the_presented_viewport() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"line\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("offscreen-motion", 400, 80, buffer);
    eval.eval_str(
        r#"(progn
        (put-text-property 201 206 'display "X\nY\n")
        (goto-char 1) (set-window-start nil 1 t))"#,
    )
    .expect("offscreen replacement");
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    let presentation = activate_last_engine_presentation(&mut eval, &display, frame);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    // GNU -nw in a real PTY: the first step leaves both replacement rows;
    // the second reaches source line 43, with three physical rows crossed.
    let result = eval
        .eval_str(
            r#"(progn (goto-char 201)
        (let ((noninteractive nil))
          (list (vertical-motion 2) (point) (window-start))))"#,
        )
        .expect("offscreen motion");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(3 211 1)"
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .active_presentation(),
        Some(presentation)
    );
    let zero = eval
        .eval_str(
            r#"(progn (goto-char 201)
        (let ((noninteractive nil)) (list (vertical-motion 0) (point))))"#,
        )
        .expect("zero motion at a replacement following a buffer newline");
    assert_eq!(neovm_core::emacs_core::print::print_value(&zero), "(0 201)");
}

#[test]
fn vertical_motion_leaves_a_wrapped_replacement_before_taking_another_step() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert("ABC\n\nDEF\n");
    eval.frame_manager_mut()
        .create_frame("wrapped-display-motion", 400, 240, buffer);
    eval.eval_str(
        "(put-text-property 1 2 'display (make-string (1+ (* 2 (window-body-width))) ?X))",
    )
    .expect("wrapped replacement");
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
                 (mapcar (lambda (n)
                           (goto-char 1)
                           (list (vertical-motion n) (point)))
                         '(0 1 2)))"#,
        )
        .expect("motion leaves a wrapped replacement");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((0 5) (2 2) (3 5))"
    );
}

#[test]
fn vertical_motion_remeasures_display_rows_after_a_display_property_change() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert("AAA\n\nBBB\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("display-motion", 400, 240, buffer);
    eval.eval_str("(goto-char 1)").expect("initial point");
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    let presentation = activate_last_engine_presentation(&mut eval, &display, frame);
    let presented_frame = eval.frame_manager().get(frame).expect("frame");
    let window = presented_frame.selected_window;
    let retained = presented_frame.redisplay_snapshot(window).cloned();

    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(progn
                 (put-text-property 1 5 'display "X\n")
                 (let ((noninteractive nil))
                   (list (vertical-motion 2) (point) (window-start))))"#,
        )
        .expect("interactive motion through changed display text");

    // GNU's interactive iterator counts X, the blank line, then BBB.
    // The column-only scanner swallows the display-string newline and lands
    // at 10 instead. Changing point must not publish a speculative viewport.
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(2 6 1)"
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .active_presentation(),
        Some(presentation),
        "a motion query must not replace the renderer's presentation"
    );
    let presented_frame = eval.frame_manager().get(frame).expect("frame");
    assert_eq!(
        presented_frame.redisplay_snapshot(window),
        retained.as_ref()
    );
    assert!(!presented_frame.has_prepared_display_presentations());
    // These expectations come from a real GNU -nw session. Binding Lisp's
    // `noninteractive` under GNU --batch does not switch its C motion engine.
    let zero = eval
        .eval_str(
            "(progn (goto-char 1) (let ((noninteractive nil)) (list (vertical-motion 0) (point))))",
        )
        .expect("start of a row occupied by a replacement");
    assert_eq!(neovm_core::emacs_core::print::print_value(&zero), "(0 5)");
    let multi_line = eval
        .eval_str(
            r#"(progn
                 (put-text-property 1 5 'display "X\nY\n")
                 (let ((noninteractive nil))
                   (mapcar (lambda (n)
                             (goto-char 1)
                             (list (vertical-motion n) (point)))
                           '(0 1 2 3 4))))"#,
        )
        .expect("motion across multiple display-string newlines");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&multi_line),
        "((0 5) (2 5) (3 6) (4 10) (4 10))"
    );
    let backward = eval
        .eval_str(
            r#"(let ((noninteractive nil))
                 (mapcar (lambda (n)
                           (goto-char (point-max))
                           (list (vertical-motion n) (point)))
                         '(-1 -2 -3 -4 -5)))"#,
        )
        .expect("backward motion keeps physical display-row distances");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&backward),
        "((-1 6) (-2 5) (-3 1) (-4 1) (-4 1))"
    );
    let mixed_row_goals = eval
        .eval_str(
            r#"(progn
                 (put-text-property 1 5 'display "X\nY")
                 (let ((noninteractive nil))
                   (mapcar (lambda (goal)
                             (goto-char 1)
                             (list (vertical-motion goal) (point)))
                           '(1 (0 . 1) (1 . 1) (2 . 1)))))"#,
        )
        .expect("goal column cannot return inside a replacement just left");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&mixed_row_goals),
        "((1 5) (1 5) (1 5) (1 5))"
    );
    let mixed_row_zero = eval
        .eval_str(
            r#"(let ((noninteractive nil))
                 (mapcar (lambda (goal)
                           (goto-char 5)
                           (list (vertical-motion goal) (point)))
                         '(0 (0 . 0) (1 . 0) (2 . 0))))"#,
        )
        .expect("zero motion at the buffer text following a replacement");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&mixed_row_zero),
        "((0 5) (0 5) (0 5) (0 5))"
    );
}

#[test]
fn vertical_motion_counts_measured_rows_at_accessible_boundaries() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert("AAA\n\nBBB\n");
    eval.frame_manager_mut()
        .create_frame("display-motion", 400, 240, buffer);
    eval.eval_str(r#"(put-text-property 1 5 'display "X\n")"#)
        .expect("display replacement");
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
                 (list
                   (progn (goto-char 1)
                          (list (vertical-motion 4) (point)))
                   (progn (goto-char (point-max))
                          (list (vertical-motion -4) (point)))))"#,
        )
        .expect("motion through accessible boundaries");

    // GNU counts the rendered replacement newline in both directions, even
    // when fewer rows remain than were requested.
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((3 10) (-3 1))"
    );
    let goal = eval
        .eval_str(
            r#"(progn
                 (remove-text-properties (point-min) (point-max) '(display nil))
                 (goto-char (point-max))
                 (let ((noninteractive nil))
                   (list (vertical-motion '(2 . -4)) (point))))"#,
        )
        .expect("goal column at the accessible beginning");
    assert_eq!(neovm_core::emacs_core::print::print_value(&goal), "(-3 3)");
}

#[test]
fn vertical_motion_does_not_treat_measured_viewport_edges_as_buffer_boundaries() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"line\n".repeat(80));
    let frame = eval
        .frame_manager_mut()
        .create_frame("display-motion", 400, 80, buffer);
    // Start halfway through a buffer much taller than the measured viewport.
    eval.eval_str("(progn (goto-char 201) (set-window-start nil 201 t))")
        .expect("middle viewport");
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &display, frame);
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil))
                 (list (list (vertical-motion -20) (point))
                       (progn (goto-char 201)
                              (list (vertical-motion 20) (point)))
                       (window-start)))"#,
        )
        .expect("motion beyond measured rows");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((-20 101) (20 301) 201)"
    );
}

#[test]
fn vertical_motion_preserves_a_labeled_accessible_region_during_measurement() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert("a\nb\nc\nd\ne\n");
    eval.frame_manager_mut()
        .create_frame("display-motion", 400, 240, buffer);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(progn
                 (goto-char 5)
                 (internal--labeled-narrow-to-region 5 7 'motion-test)
                 (let ((noninteractive nil))
                   (list (point-min) (point-max) (vertical-motion 2) (point))))"#,
        )
        .expect("motion within a labeled restriction");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(5 7 1 7)"
    );
}

/// A point past the right margin of a truncated line is on that line's row for
/// the scrolling planner too.
///
/// The planner asks the same "which row holds this position" question motion
/// does.  A position it cannot place reads as a point that has left the window:
/// the plan recenters the viewport around it, or -- when the measured rows reach
/// the end of the buffer -- fails with "Scroll origin is outside measured source
/// coverage".  GNU walks the display iterator from the start of the origin's
/// line and backtracks when the walk overshoots a line truncated on the right
/// (src/indent.c:2393-2400, the same walk `window_scroll_pixel_based` uses),
/// which puts such a point on the truncated row.
#[test]
fn scrolling_places_a_point_past_the_right_margin_on_the_truncated_row() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    // A long truncated line early, and enough lines after it that the window can
    // start well below point: the buffer's own end is then inside the rows the
    // measurement reaches, which is what turns the unplaced origin into a
    // failure rather than one more measurement.
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&format!(
            "line\n{}\n{}",
            "x".repeat(3000),
            "line\n".repeat(60)
        ));
    let frame = eval
        .frame_manager_mut()
        .create_frame("scroll-truncated-line", 400, 240, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil) (truncate-lines t))
                 (set-window-hscroll nil 50)
                 (goto-char 2500)
                 (set-window-start nil 307 t)
                 (scroll-down 1)
                 (list (window-start) (point)))"#,
        )
        .expect("scroll with a point past the right margin of a truncated line");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "(1 2500)"
    );
}

/// A position past the right margin of a truncated line belongs to that line's
/// row.
///
/// GNU never asks which row holds a position: `Fvertical_motion` reseats at the
/// start of the origin's line, walks forward to the origin, and when that walk
/// overshoots a line truncated on the right (`it.line_wrap == TRUNCATE &&
/// it.current_x >= it.last_visible_x`) backtracks one line, landing back on the
/// truncated row itself (src/indent.c:2393-2400).  Nothing implemented that for
/// rows, so the newline ending a truncated line was in no row at all, and
/// `C-e` -- whose `line-move-1` walks to `(line-end-position)` and then asks for
/// a screen line -- was answered with "Display motion origin is outside measured
/// source coverage" instead of reaching the end of its line.
#[test]
fn vertical_motion_reaches_the_end_of_a_truncated_line() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(&format!("{}\n", "x".repeat(300)));
    // Narrow enough that the line cannot fit at any plausible character width,
    // so redisplay truncates it -- which is the premise of this case.
    eval.frame_manager_mut()
        .create_frame("display-motion", 160, 240, buffer);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let result = eval
        .eval_str(
            r#"(let ((noninteractive nil) (truncate-lines t))
                 (list
                   (progn (goto-char (point-min))
                          (list (vertical-motion 1) (point)))
                   (progn (goto-char 301)
                          (list (vertical-motion 0) (point)))
                   (progn (goto-char 301)
                          (list (vertical-motion 1) (point)))))"#,
        )
        .expect("screen-line motion from inside a truncated line");

    // Measured against GNU Emacs 31 on the same buffer: one screen line down from
    // the beginning is the empty line after the newline, zero lines from the
    // newline answers the start of its own screen line, and one line from it
    // reaches that empty line.  Before the row lookup learned the overflow rule,
    // the zero-line probe signalled instead of answering.
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&result),
        "((1 302) (0 1) (1 302))"
    );
}

#[test]
fn graphical_posn_queries_follow_live_scroll_before_presentation() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    let text: String = (0..400)
        .map(|i| format!("Line {i:03} -- native scrolling diagnostic\n"))
        .collect();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&text);
    let frame = eval
        .frame_manager_mut()
        .create_frame("live-posn", 1000, 700, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        "(progn (goto-char 881) (set-window-start nil 801 t) (set-window-vscroll nil 12 t))",
    )
    .unwrap();
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &display, frame);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let observe = |eval: &mut Context| {
        let value = eval.eval_str(
            "(let ((noninteractive nil)) (list (nth 1 (posn-at-x-y 0 100 (selected-window))) (nth 2 (posn-at-point 881))))",
        ).unwrap();
        neovm_core::emacs_core::print::print_value(&value)
    };
    let old = observe(&mut eval);
    eval.eval_str("(progn (set-window-start nil 721 t) (set-window-vscroll nil 14 t))")
        .unwrap();
    let before_presentation = observe(&mut eval);
    let positions = eval
        .eval_str("(list (window-start) (window-vscroll nil t) (point))")
        .unwrap();
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&positions),
        "(721 14 881)"
    );
    display.layout_frame_rust(&mut eval, frame);
    let after_layout = observe(&mut eval);
    activate_last_engine_presentation(&mut eval, &display, frame);
    let after_presentation = observe(&mut eval);
    assert_ne!(old, after_presentation, "the viewport really moved");
    assert_eq!(
        after_layout, after_presentation,
        "completed redisplay rows must answer before renderer activation"
    );
    assert_eq!(
        before_presentation, after_presentation,
        "Lisp coordinate queries must follow live scrolling before renderer acknowledgement"
    );
}

#[test]
fn identical_geometry_queries_reuse_rows_and_mutations_force_a_new_walk() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    for scope in [
        WindowLayoutQueryScope::Viewport,
        WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(50),
        },
        WindowLayoutQueryScope::Rows {
            start: LispCharPos1::ONE,
            count: NonZeroUsize::new(8).unwrap(),
        },
    ] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert(&"ordinary row\n".repeat(100));
        let frame = eval
            .frame_manager_mut()
            .create_frame("query-cache", 400, 160, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        let initial = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        probe::reset();
        let repeated = query
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(initial.end(), repeated.end());
        assert_eq!(
            probe::max_depth(),
            0,
            "identical query walked the body again: {scope:?}"
        );
        for change in [
            "(put-text-property 1 8 'face '(:height 175))",
            r#"(overlay-put (make-overlay 1 20) 'display "replacement")"#,
            "(goto-char 20)",
            "(set-window-vscroll nil 3 t)",
            "(setq tab-width 3)",
            "(narrow-to-region 1 100)",
            "(progn (overlay-put (make-overlay 30 40) 'category 'query-face-category) (put 'query-face-category 'face '(:height 125)))",
            "(put 'query-face-category 'face '(:height 200))",
            r#"(progn (setq query-replacement (copy-sequence "a")) (put-text-property 50 51 'display query-replacement))"#,
            "(aset query-replacement 0 9)",
            "(progn (setq query-face (list :height 100)) (put-text-property 50 51 'face query-face))",
            "(setcar (cdr query-face) 200)",
        ] {
            eval.eval_str(change).unwrap();
            probe::reset();
            let actual = query
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            if matches!(scope, WindowLayoutQueryScope::Rows { .. })
                && change == "(set-window-vscroll nil 3 t)"
            {
                assert_eq!(
                    probe::max_depth(),
                    0,
                    "absolute rows moved with the viewport"
                );
            } else if !(matches!(scope, WindowLayoutQueryScope::Position { .. })
                && change == "(set-window-vscroll nil 3 t)")
            {
                assert!(
                    probe::max_depth() > 0,
                    "mutation reused stale rows: {change}"
                );
            }
            let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
            let expected = fresh
                .query_window_layout(&mut eval, frame, window, scope)
                .unwrap();
            assert_eq!(actual.end(), expected.end(), "{change}");
            let actual = actual.into_geometry().unwrap();
            let expected = expected.into_geometry().unwrap();
            assert_eq!(actual.rows, expected.rows, "{change}");
            assert_eq!(actual.points, expected.points, "{change}");
        }
    }
}

#[test]
fn geometry_queries_reevaluate_conditional_display_without_source_edits() {
    use neovm_core::window::WindowLayoutQueryScope;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("ordinary row\nnext row\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("query-conditions", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r#"(progn (setq query-condition t query-calls 0) (put-text-property 1 2 'display '(when (progn (setq query-calls (1+ query-calls)) query-condition) . "REPLACED")))"#).unwrap();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    let before = eval.eval_str("query-calls").unwrap().as_fixnum().unwrap();
    eval.eval_str("(setq query-condition nil)").unwrap();
    let actual = query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap()
        .into_geometry()
        .unwrap();
    let after = eval.eval_str("query-calls").unwrap().as_fixnum().unwrap();
    assert!(
        after > before,
        "query cache skipped Lisp display conditions"
    );
    let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
    let expected = fresh
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap()
        .into_geometry()
        .unwrap();
    assert_eq!(actual.rows, expected.rows);
    assert_eq!(actual.points, expected.points);
}

#[test]
fn pixel_only_queries_reuse_complete_row_geometry() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"ordinary row\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("query-pixel-placement", 400, 170, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 30) (set-window-vscroll nil 2 t t)").unwrap();
    let mut query = LayoutEngine::new_without_font_metrics();
    query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    eval.eval_str("(set-window-vscroll nil 3 t t)").unwrap();
    probe::reset();
    let actual = query
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    let walks = probe::max_depth();
    let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
    let expected = fresh
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    assert_eq!(actual.end(), expected.end());
    assert_eq!(actual.geometry(), expected.geometry());
    assert_eq!(walks, 0, "pixel placement rewalked unchanged complete rows");
}

#[test]
fn pixel_query_placement_matches_fresh_wrapped_and_decorated_rows() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    for decoration in [
        "nil",
        "(put-text-property 1 121 'face '(:height 1.5))",
        r#"(progn (put-text-property 1 121 'line-height 1.3)
            (put-text-property 31 35 'display '(raise 0.2))
            (let ((o (make-overlay 90 110)))
              (overlay-put o 'before-string "prefix")
              (overlay-put o 'after-string "suffix")
              (overlay-put o 'face '(:height 1.2))))"#,
    ] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert(&format!("{}\n", "abc def ghi ".repeat(20)).repeat(30));
        let frame =
            eval.frame_manager_mut()
                .create_frame("query-pixel-differential", 180, 200, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil word-wrap t) (goto-char 55)").unwrap();
        eval.eval_str(decoration).unwrap();
        let mut query = LayoutEngine::new_without_font_metrics();
        let mut reused = 0;
        for pixels in (0..32).chain((0..32).rev()) {
            eval.eval_str(&format!("(set-window-vscroll nil {pixels} t t)"))
                .unwrap();
            probe::reset();
            let actual = query
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            reused += usize::from(probe::max_depth() == 0);
            let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
            let expected = fresh
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            assert_eq!(actual.end(), expected.end(), "{decoration} pixels={pixels}");
            assert_eq!(
                actual.geometry(),
                expected.geometry(),
                "{decoration} pixels={pixels}"
            );
        }
        assert!(
            reused > 4,
            "no pixel placement reuse: {decoration}, reused={reused}"
        );
    }
}

#[test]
fn ordinary_query_reuse_ignores_unread_lisp_collection_writes() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"row\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("query-collections", 400, 170, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(setq unrelated-scroll-state (vector 0 (list 1 2)))")
        .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let initial = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    eval.eval_str("(aset unrelated-scroll-state 0 1) (setcar (aref unrelated-scroll-state 1) 3)")
        .unwrap();
    probe::reset();
    let actual = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    assert_eq!(initial.geometry(), actual.geometry());
    assert_eq!(
        probe::max_depth(),
        0,
        "unread collection writes forced another row walk"
    );
}

#[test]
fn ordinary_query_reuse_ignores_unrelated_buffer_local_value_writes() {
    query_reuse_after_unrelated_local_write(false);
}

#[test]
fn ordinary_query_reuse_ignores_local_writes_after_hook_cache_invalidation() {
    query_reuse_after_unrelated_local_write(true);
}

fn query_reuse_after_unrelated_local_write(invalidate_hook_cache: bool) {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let buf = eval.buffer_manager_mut().get_mut(buffer).unwrap();
    buf.insert(&"row\n".repeat(100));
    buf.set_buffer_local("deactivate-mark", Value::NIL);
    // The runtime's index is already warm during native input. Its cold
    // construction can conservatively observe the whole binding list once.
    assert_eq!(buf.buffer_local_value("deactivate-mark"), Some(Value::NIL));
    let frame = eval
        .frame_manager_mut()
        .create_frame("query-local-dependencies", 400, 170, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let mut engine = LayoutEngine::new_without_font_metrics();
    // Native scrolling starts after an ordinary redisplay has resolved the
    // current buffer's localized hook cells and variable index.
    engine.layout_frame_rust(&mut eval, frame);
    if invalidate_hook_cache {
        // Timer callbacks commonly create temporary buffers and locals.
        // This invalidates the runtime's global localized-binding epoch.
        let observer = eval.buffer_manager_mut().create_buffer("observer");
        eval.buffer_manager_mut()
            .get_mut(observer)
            .unwrap()
            .set_buffer_local("observer-local", Value::T);
    }
    let initial = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap();
    for value in [Value::T, Value::NIL] {
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .set_buffer_local("deactivate-mark", value);
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
            .unwrap();
        assert_eq!(initial.geometry(), actual.geometry());
        assert_eq!(
            probe::max_depth(),
            0,
            "unrelated local binding forced another row walk"
        );
    }
}

#[test]
fn repeated_query_points_keep_bounded_independent_observations() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    for decoration in [
        "nil",
        r#"(progn (put-text-property 1 121 'face '(:height 1.5))
            (put-text-property 31 35 'display '(raise 0.2))
            (let ((o (make-overlay 90 110)))
              (overlay-put o 'before-string "prefix")
              (overlay-put o 'after-string "suffix")
              (overlay-put o 'face '(:height 1.2))))"#,
    ] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert(&format!("{}\n", "abc def ghi ".repeat(20)).repeat(30));
        let frame =
            eval.frame_manager_mut()
                .create_frame("query-alternating-points", 180, 200, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str(
            "(setq mode-line-format nil header-line-format nil tab-line-format nil word-wrap t)",
        )
        .unwrap();
        eval.eval_str(decoration).unwrap();
        let mut engine = LayoutEngine::new_without_font_metrics();
        for (step, point) in [30, 55, 30, 55, 30].into_iter().enumerate() {
            eval.eval_str(&format!("(goto-char {point})")).unwrap();
            probe::reset();
            let actual = engine
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            let walks = probe::max_depth();
            let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
            let expected = fresh
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            assert_eq!(actual.end(), expected.end());
            assert_eq!(actual.geometry(), expected.geometry());
            if step >= 2 {
                assert_eq!(
                    walks, 0,
                    "returning to point {point} evicted an unchanged observation: {decoration}"
                );
            }
        }
    }
}

#[test]
fn backward_page_does_not_treat_bounded_wrapped_rows_as_covering_the_origin() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&format!("{}\n", "x".repeat(200)).repeat(120));
    let frame = eval
        .frame_manager_mut()
        .create_frame("bounded-page", 160, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let start = 201 * 90 + 1;
    eval.eval_str(&format!("(setq mode-line-format nil auto-window-vscroll nil scroll-preserve-screen-position nil) (put-text-property 1 200 'face '(:height 2.0)) (goto-char {start}) (set-window-start nil {start} t)")).unwrap();
    let measured = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = measured.clone();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => {
                observed
                    .borrow_mut()
                    .push(query.geometry().map_or(0, |snapshot| snapshot.rows.len()));
                WindowLayoutQueryOutcome::Ready(query)
            }
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let actual = eval
        .eval_str("(let ((noninteractive nil)) (scroll-down) (window-start))")
        .unwrap()
        .as_fixnum()
        .unwrap();
    assert!(
        actual < start && actual > start - 201 * 3,
        "one wrapped screen page must stay near its origin: {start} -> {actual}"
    );
    assert!(
        measured.borrow().iter().sum::<usize>() < 128,
        "a nearby page should not measure dozens of physical lines: {:?}",
        measured.borrow()
    );
}

#[test]
fn bounded_truncated_rows_publish_only_their_consumed_line_tail() {
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for newline in [false, true] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        let text = format!(
            "{}{}",
            "x".repeat(300),
            if newline { "\nlater\n" } else { "" }
        );
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert(&text);
        let frame = eval
            .frame_manager_mut()
            .create_frame("truncated-tail", 160, 160, buffer);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str("(setq truncate-lines t mode-line-format nil) (goto-char 1)")
            .unwrap();
        let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
        let result = query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::ONE,
                    count: std::num::NonZeroUsize::new(1).unwrap(),
                },
            )
            .unwrap();
        let row = result
            .geometry()
            .unwrap()
            .rows
            .iter()
            .find(|row| row.start_buffer_pos.is_some())
            .unwrap();
        assert_eq!(
            row.truncated_end_buffer_pos,
            Some(LispCharPos1::new(301)),
            "newline={newline}"
        );
    }
}

#[test]
fn absolute_row_queries_survive_viewport_placement_but_not_source_changes() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"short row\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("absolute-query", 400, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 51) (setq mode-line-format nil)")
        .unwrap();
    let scope = WindowLayoutQueryScope::Rows {
        start: LispCharPos1::ONE,
        count: std::num::NonZeroUsize::new(8).unwrap(),
    };
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine
        .query_window_layout(&mut eval, frame, window, scope)
        .unwrap();
    eval.eval_str("(set-window-start nil 41 t) (set-window-vscroll nil 3 t)")
        .unwrap();
    probe::reset();
    let actual = engine
        .query_window_layout(&mut eval, frame, window, scope)
        .unwrap();
    assert_eq!(
        probe::max_depth(),
        0,
        "absolute rows do not depend on live viewport placement"
    );
    let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
    let expected = fresh
        .query_window_layout(&mut eval, frame, window, scope)
        .unwrap();
    assert_eq!(actual.geometry(), expected.geometry());
    eval.eval_str("(put-text-property 1 5 'display \"changed\")")
        .unwrap();
    probe::reset();
    engine
        .query_window_layout(&mut eval, frame, window, scope)
        .unwrap();
    assert!(probe::max_depth() > 0);
}

#[test]
fn page_viewports_measure_their_pixel_extent_in_one_walk() {
    use std::{cell::Cell, rc::Rc};
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"ordinary row\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("page-pixel-budget", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 1) (set-window-start nil 1 t)").unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        observed.set(observed.get() + 1);
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    eval.eval_str("(let ((noninteractive nil)) (scroll-up))")
        .unwrap();
    assert!(
        eval.eval_str("(window-start)")
            .unwrap()
            .as_fixnum()
            .unwrap()
            > 1
    );
    assert!(
        calls.get() <= 2,
        "page plan remeasured covered pixels: {}",
        calls.get()
    );
}

#[test]
fn pixel_extent_queries_match_complete_rows_with_mixed_heights_and_overlays() {
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"mixed\twords wrap around the edge\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-extent-parity", 160, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(put-text-property 1 6 'face '(:height 250)) (put-text-property 20 80 'face '(:height 75)) (overlay-put (make-overlay 80 100) 'before-string (propertize \"inserted\" 'face '(:height 175)))").unwrap();
    for start in [1, 4, 33] {
        for height in [1, 17, 80, 350] {
            let start = LispCharPos1::from_one_based_usize(start);
            let mut query = WindowLayoutQueryEngine::new();
            let pixels = query
                .query_window_layout(
                    &mut eval,
                    frame,
                    window,
                    WindowLayoutQueryScope::Pixels {
                        start,
                        height: NonZeroUsize::new(height).unwrap(),
                    },
                )
                .unwrap()
                .into_geometry()
                .unwrap();
            let mut fresh = WindowLayoutQueryEngine::new();
            let rows = fresh
                .query_window_layout(
                    &mut eval,
                    frame,
                    window,
                    WindowLayoutQueryScope::Rows {
                        start,
                        count: NonZeroUsize::new(128).unwrap(),
                    },
                )
                .unwrap()
                .into_geometry()
                .unwrap();
            let bottom = rows.rows[0].y + height as i64;
            let expected: Vec<_> = rows
                .rows
                .iter()
                .filter(|row| row.y < bottom)
                .cloned()
                .collect();
            assert_eq!(pixels.rows, expected, "start={start:?}, height={height}");
            let last = pixels.rows.last().unwrap().row;
            let expected: Vec<_> = rows
                .points
                .into_iter()
                .filter(|point| point.row <= last)
                .collect();
            assert_eq!(pixels.points, expected, "start={start:?}, height={height}");
        }
    }
}

#[test]
fn forward_page_uses_the_visible_wrap_context_with_tabs_and_overlays() {
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    for start in [17, 33, 74] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut().get_mut(buffer).unwrap().insert(
            &"words\twith tabs and wrapping words repeated many times within a single physical line\n".repeat(100));
        let frame = eval
            .frame_manager_mut()
            .create_frame("page-wrap-context", 190, 240, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str(&format!("(setq mode-line-format nil header-line-format nil tab-line-format nil word-wrap t) (goto-char {start}) (set-window-start nil {start} t) (overlay-put (make-overlay 85 100) 'before-string (propertize \"prefix\" 'face '(:height 1.5)))")).unwrap();
        let height = eval
            .eval_str("(window-body-height nil t)")
            .unwrap()
            .as_fixnum()
            .unwrap();
        let mut query = WindowLayoutQueryEngine::new();
        let snapshot = query
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Pixels {
                    start: LispCharPos1::from_one_based_usize(start),
                    height: NonZeroUsize::new(height as usize).unwrap(),
                },
            )
            .unwrap()
            .into_geometry()
            .unwrap();
        let line_height = eval.frame_manager().get(frame).unwrap().char_height as i64;
        let delta = (height / line_height - 2).max(1) * line_height;
        let goal = snapshot.rows[0].y + delta;
        let expected = snapshot
            .rows
            .iter()
            .rev()
            .find(|row| row.y <= goal && row.start_buffer_pos.is_some())
            .unwrap()
            .start_buffer_pos
            .unwrap();
        assert!(expected.as_i64() > start as i64);
        eval.install_window_layout_query(move |eval, frame, window, scope| {
            match query.query_window_layout(eval, frame, window, scope) {
                Ok(query) => WindowLayoutQueryOutcome::Ready(query),
                Err(error) => WindowLayoutQueryOutcome::Failed(error),
            }
        });
        eval.eval_str("(let ((noninteractive nil)) (scroll-up))")
            .unwrap();
        assert_eq!(
            eval.eval_str("(window-start)").unwrap().as_fixnum(),
            Some(expected.as_i64()),
            "forward page must use the measured viewport at start {start}"
        );
    }
}

#[test]
fn backward_page_grows_pixel_coverage_without_guessing_row_counts() {
    use std::{cell::Cell, rc::Rc};
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&format!("{}\n", "x".repeat(200)).repeat(120));
    let frame = eval
        .frame_manager_mut()
        .create_frame("page-pixel-budget", 160, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 18191) (set-window-start nil 18191 t)").unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        observed.set(observed.get() + 1);
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    eval.eval_str("(let ((noninteractive nil)) (scroll-down))")
        .unwrap();
    let actual = eval
        .eval_str("(window-start)")
        .unwrap()
        .as_fixnum()
        .unwrap();
    assert!(
        (17000..18191).contains(&actual),
        "page must move backward by a nearby viewport: {actual}"
    );
    assert!(
        calls.get() <= 4,
        "backward page restarted row-count guesses: {}",
        calls.get()
    );
}

#[test]
fn pixel_coverage_reuses_a_larger_certified_observation() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"mixed words\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-coverage-cache", 400, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 1) (setq mode-line-format nil) (put-text-property 20 60 'face '(:height 1.5)) (overlay-put (make-overlay 80 100) 'before-string \"prefix\")").unwrap();
    let scope = |height| WindowLayoutQueryScope::Pixels {
        start: LispCharPos1::ONE,
        height: NonZeroUsize::new(height).unwrap(),
    };
    let mut engine = WindowLayoutQueryEngine::new();
    let large = engine
        .query_window_layout(&mut eval, frame, window, scope(400))
        .unwrap();
    probe::reset();
    let covered = engine
        .query_window_layout(&mut eval, frame, window, scope(160))
        .unwrap();
    assert_eq!(
        probe::max_depth(),
        0,
        "smaller coverage rewalked its certified prefix"
    );
    assert_eq!(covered.end(), large.end());
    assert_eq!(covered.geometry(), large.geometry());
    eval.eval_str("(put-text-property 1 10 'face '(:height 2.0))")
        .unwrap();
    probe::reset();
    let changed = engine
        .query_window_layout(&mut eval, frame, window, scope(160))
        .unwrap();
    assert!(
        probe::max_depth() > 0,
        "source mutation must invalidate coverage"
    );
    let fresh = WindowLayoutQueryEngine::new()
        .query_window_layout(&mut eval, frame, window, scope(160))
        .unwrap();
    assert_eq!(changed.geometry(), fresh.geometry());
}

// Pixels may return a larger complete observation than requested. Compare
// that observation to the canonical producer at its own returned extent,
// including all row numbers, points, cursor coordinates and end records.
fn assert_cross_start_pixel_observation(
    eval: &mut Context,
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
    actual: &neovm_core::window::WindowLayoutQuery,
    start: usize,
    requested_height: usize,
    case: &str,
) {
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    let geometry = actual.geometry().unwrap();
    let first = geometry.rows.first().unwrap();
    let last = geometry.rows.last().unwrap();
    let extent = usize::try_from(last.y + last.height - first.y).unwrap();
    let height = extent.max(requested_height);
    let expected = WindowLayoutQueryEngine::new()
        .query_window_layout(
            eval,
            frame,
            window,
            WindowLayoutQueryScope::Pixels {
                start: LispCharPos1::from_one_based_usize(start),
                height: NonZeroUsize::new(height).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(actual.end(), expected.end(), "{case}: translated end");
    assert_eq!(
        actual.geometry(),
        expected.geometry(),
        "{case}: translated rows, points, cursors and end record"
    );
}

#[test]
fn cross_start_pixels_reuse_complete_physical_line_suffixes() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    for decoration in [
        "nil",
        r#"(progn
            (put-text-property 27 40 'face '(:family "monospace" :height 2.0 :weight bold))
            (put-text-property 40 53 'face '(:family "serif" :height 0.75 :slant italic))
            (put-text-property 53 66 'face '(:family "sans-serif" :height 1.5))
            (overlay-put (make-overlay 69 74) 'face '(:height 1.25 :underline t)))"#,
    ] {
        for point in [1, 1000] {
            let (mut eval, frame, window) =
                position_query_fixture(&"ordinary row\n".repeat(100), 400, 200);
            eval.eval_str(decoration).unwrap();
            eval.eval_str(&format!("(goto-char {point})")).unwrap();
            let before = eval
                .eval_str("(list (window-start) (window-vscroll nil t) (point))")
                .unwrap();
            let scope = |start, height| WindowLayoutQueryScope::Pixels {
                start: LispCharPos1::from_one_based_usize(start),
                height: NonZeroUsize::new(height).unwrap(),
            };
            let mut engine = WindowLayoutQueryEngine::new();
            let original = engine
                .query_window_layout(&mut eval, frame, window, scope(1, 500))
                .unwrap();
            probe::reset();
            let shifted = engine
                .query_window_layout(&mut eval, frame, window, scope(27, 120))
                .unwrap();
            let depth = probe::max_depth();
            assert_cross_start_pixel_observation(
                &mut eval, frame, window, &shifted, 27, 120, decoration,
            );
            assert_eq!(
                eval.eval_str("(list (window-start) (window-vscroll nil t) (point))")
                    .unwrap(),
                before,
                "measurement changed the live viewport"
            );
            assert!(
                original.geometry().unwrap().rows.len() > shifted.geometry().unwrap().rows.len(),
                "discarded leading rows were retained"
            );
            assert_eq!(
                depth, 0,
                "overlapping clean source rows were measured again: {decoration}, point={point}"
            );
        }
    }
}

#[test]
fn cross_start_pixels_reject_changed_source_point_collections_and_callbacks() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    for mutation in [
        "(goto-char 44)",
        "(put-text-property 30 35 'face '(:height 2.0))",
        "(overlay-put cross-start-overlay 'face '(:height 1.5))",
        "(setcar (cdr cross-start-face) 200)",
        "(setq fontification-functions cross-start-hooks)",
        "(setq tab-width 3)",
        "(narrow-to-region 1 200)",
    ] {
        let (mut eval, frame, window) =
            position_query_fixture(&"ordinary row\n".repeat(100), 400, 200);
        eval.eval_str(
            r#"(setq cross-start-face (list :height 100))
            (put-text-property 27 80 'face cross-start-face)
            (setq cross-start-overlay (make-overlay 80 100))
            (overlay-put cross-start-overlay 'face '(:height 1.25))
            (setq fontification-functions nil cross-start-hooks
                (list (lambda (start) (put-text-property start (point-max) 'fontified t))))"#,
        )
        .unwrap();
        let scope = |start, height| WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::from_one_based_usize(start),
            height: NonZeroUsize::new(height).unwrap(),
        };
        let mut engine = WindowLayoutQueryEngine::new();
        engine
            .query_window_layout(&mut eval, frame, window, scope(1, 500))
            .unwrap();
        eval.eval_str(mutation).unwrap();
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, scope(27, 120))
            .unwrap();
        let depth = probe::max_depth();
        let expected = WindowLayoutQueryEngine::new()
            .query_window_layout(&mut eval, frame, window, scope(27, 120))
            .unwrap();
        assert_eq!(actual.end(), expected.end(), "{mutation}");
        assert_eq!(actual.geometry(), expected.geometry(), "{mutation}");
        assert!(
            depth > 0,
            "changed inputs reused a cross-start suffix: {mutation}"
        );
    }
}

#[test]
fn cross_start_pixels_walk_for_retained_cursor_and_missing_coverage() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    for (point, start, height) in [(44, 27, 120), (1, 27, 600), (1, 1, 120)] {
        let (mut eval, frame, window) =
            position_query_fixture(&"ordinary row\n".repeat(100), 400, 200);
        eval.eval_str(&format!("(goto-char {point})")).unwrap();
        let scope = |start, height| WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::from_one_based_usize(start),
            height: NonZeroUsize::new(height).unwrap(),
        };
        let mut engine = WindowLayoutQueryEngine::new();
        // Starting after the requested source start cannot certify its prefix.
        let cached_start = if start == 1 { 27 } else { 1 };
        engine
            .query_window_layout(&mut eval, frame, window, scope(cached_start, 500))
            .unwrap();
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, scope(start, height))
            .unwrap();
        let depth = probe::max_depth();
        let expected = WindowLayoutQueryEngine::new()
            .query_window_layout(&mut eval, frame, window, scope(start, height))
            .unwrap();
        assert_eq!(actual.end(), expected.end());
        assert_eq!(actual.geometry(), expected.geometry());
        assert!(
            depth > 0,
            "uncertified suffix reused: point={point}, start={start}, height={height}"
        );
    }
}

#[test]
fn cross_start_pixels_walk_when_a_cached_wrap_row_has_no_restart_proof() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    let text =
        "words with tabs\tand wrapping repeated many times on one physical line\n".repeat(100);
    let (mut eval, frame, window) = position_query_fixture(&text, 180, 200);
    eval.eval_str("(setq word-wrap t) (put-text-property 1 (point-max) 'wrap-prefix \"w>\")")
        .unwrap();
    let scope = |start, height| WindowLayoutQueryScope::Pixels {
        start: LispCharPos1::from_one_based_usize(start),
        height: NonZeroUsize::new(height).unwrap(),
    };
    let mut engine = WindowLayoutQueryEngine::new();
    let original = engine
        .query_window_layout(&mut eval, frame, window, scope(1, 500))
        .unwrap();
    let start = original
        .geometry()
        .unwrap()
        .rows
        .iter()
        .filter_map(|row| row.start_buffer_pos)
        .map(|position| position.as_i64() as usize)
        .find(|start| *start > 1 && text.as_bytes()[start - 2] != b'\n')
        .expect("fixture must materialize a continuation with an exact source anchor");
    probe::reset();
    let actual = engine
        .query_window_layout(&mut eval, frame, window, scope(start, 120))
        .unwrap();
    let depth = probe::max_depth();
    let expected = WindowLayoutQueryEngine::new()
        .query_window_layout(&mut eval, frame, window, scope(start, 120))
        .unwrap();
    assert_eq!(actual.end(), expected.end());
    assert_eq!(actual.geometry(), expected.geometry());
    assert!(depth > 0, "a matching wrap row is not a restart proof");
}

#[test]
fn cross_start_pixels_preserve_multibyte_end_records_and_eob_rows() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    for lines in [100, 5] {
        let (mut eval, frame, window) =
            position_query_fixture(&"αβ🙂 row\n".repeat(lines), 400, 200);
        let scope = |start, height| WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::from_one_based_usize(start),
            height: NonZeroUsize::new(height).unwrap(),
        };
        let mut engine = WindowLayoutQueryEngine::new();
        engine
            .query_window_layout(&mut eval, frame, window, scope(1, 500))
            .unwrap();
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, scope(17, 120))
            .unwrap();
        let depth = probe::max_depth();
        if depth == 0 {
            assert_cross_start_pixel_observation(
                &mut eval,
                frame,
                window,
                &actual,
                17,
                120,
                "multibyte end record and EOB",
            );
        } else {
            let expected = WindowLayoutQueryEngine::new()
                .query_window_layout(&mut eval, frame, window, scope(17, 120))
                .unwrap();
            assert_eq!(actual.end(), expected.end());
            assert_eq!(actual.geometry(), expected.geometry());
        }
    }
}

#[test]
fn cross_start_pixels_match_canonical_context_sensitive_rows() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    let wrapped_line = "words with tabs\tand wrapping repeated many times on one physical line\n";
    for (text, width, decoration, start) in [
        (
            "words with tabs\tand wrapping repeated many times on one physical line\n".repeat(100),
            180,
            "(setq word-wrap t)",
            12,
        ),
        (
            wrapped_line.repeat(100),
            180,
            "(setq word-wrap t)",
            wrapped_line.len() + 1,
        ),
        (
            wrapped_line.repeat(100),
            180,
            "(setq truncate-lines t) (set-window-hscroll nil 3)",
            wrapped_line.len() + 1,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(set-window-hscroll nil 3)",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(put-text-property 20 50 'face '(:box (:line-width 2 :color \"red\") :height 1.5))",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(put-text-property 1 27 'line-height 3.0) (put-text-property 27 40 'line-height 1.5)",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(setq line-spacing 3) (put-text-property 1 27 'line-spacing 8) (put-text-property 27 40 'line-spacing 1)",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(setq header-line-format '(\"header\") tab-line-format '(\"tabs\"))",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(put-text-property 1 (point-max) 'line-prefix \"p>\")",
            27,
        ),
        (
            "words with tabs\tand wrapping repeated many times on one physical line\n".repeat(100),
            180,
            "(setq word-wrap t) (put-text-property 1 (point-max) 'wrap-prefix \"w>\")",
            12,
        ),
        (
            "abc אבג مرحبا tail\n".repeat(100),
            400,
            "(setq bidi-display-reordering t bidi-paragraph-direction 'right-to-left)",
            20,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(setq buffer-invisibility-spec t) (put-text-property 20 35 'invisible t)",
            40,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(put-text-property 20 35 'display \"replacement\")",
            40,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(overlay-put (make-overlay 27 40) 'before-string \"one\\ntwo\\nthree\")",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(overlay-put (make-overlay 14 27) 'after-string \"one\\ntwo\\nthree\")",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(overlay-put (make-overlay 27 40) 'before-string \"inline-prefix\")",
            27,
        ),
        (
            "ordinary row\n".repeat(100),
            400,
            "(overlay-put (make-overlay 14 27) 'after-string \"inline-suffix\")",
            27,
        ),
    ] {
        let (mut eval, frame, window) = position_query_fixture(&text, width, 200);
        eval.eval_str(decoration).unwrap();
        let scope = |start, height| WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::from_one_based_usize(start),
            height: NonZeroUsize::new(height).unwrap(),
        };
        let mut engine = WindowLayoutQueryEngine::new();
        engine
            .query_window_layout(&mut eval, frame, window, scope(1, 500))
            .unwrap();
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, scope(start, 120))
            .unwrap();
        let walked = probe::max_depth() > 0;
        if walked {
            let expected = WindowLayoutQueryEngine::new()
                .query_window_layout(&mut eval, frame, window, scope(start, 120))
                .unwrap();
            assert_eq!(actual.end(), expected.end(), "{decoration}");
            assert_eq!(actual.geometry(), expected.geometry(), "{decoration}");
        } else {
            assert_cross_start_pixel_observation(
                &mut eval, frame, window, &actual, start, 120, decoration,
            );
        }
    }
}

#[test]
fn backward_page_growth_handles_different_physical_line_lengths() {
    use std::{cell::Cell, rc::Rc};
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&format!(
            "{}{}{}",
            format!("{}\n", "x".repeat(200)).repeat(100),
            format!("{}\n", "s".repeat(100)),
            format!("{}\n", "x".repeat(200)).repeat(100)
        ));
    let frame = eval
        .frame_manager_mut()
        .create_frame("page-pixel-budget", 160, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 20202) (set-window-start nil 20202 t)").unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        observed.set(observed.get() + 1);
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    eval.eval_str("(let ((noninteractive nil)) (scroll-down))")
        .unwrap();
    let actual = eval
        .eval_str("(window-start)")
        .unwrap()
        .as_fixnum()
        .unwrap();
    assert!(
        (18000..20202).contains(&actual),
        "page must move backward by a nearby viewport: {actual}"
    );
    assert!(
        calls.get() <= 4,
        "backward page restarted row-count guesses: {}",
        calls.get()
    );
}

#[test]
fn pixel_only_queries_reuse_rows_when_point_is_outside_the_viewport() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::window::WindowLayoutQueryScope;
    for decoration in [
        "nil",
        "(put-text-property 1 121 'face '(:height 1.5))",
        r#"(progn (put-text-property 1 121 'line-height 1.3)
            (put-text-property 31 35 'display '(raise 0.2))
            (let ((o (make-overlay 90 110)))
              (overlay-put o 'before-string "prefix")
              (overlay-put o 'after-string "suffix")
              (overlay-put o 'face '(:height 1.2))))"#,
    ] {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert(&"ordinary row\n".repeat(200));
        let frame = eval
            .frame_manager_mut()
            .create_frame("query-outside-point", 400, 170, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil) (goto-char 1000) (set-window-vscroll nil 2 t t)").unwrap();
        eval.eval_str(decoration).unwrap();
        let mut query = LayoutEngine::new_without_font_metrics();
        let initial = query
            .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
            .unwrap();
        let snapshot = initial.geometry().unwrap();
        assert!(
            snapshot.logical_cursor.is_none(),
            "fixture has visible logical cursor: {decoration}"
        );
        assert!(
            snapshot.phys_cursor.is_none(),
            "fixture has visible physical cursor: {decoration}"
        );
        assert!(
            snapshot
                .point_for_buffer_pos(neovm_core::buffer::LispCharPos1::from_one_based_usize(1000))
                .is_none()
        );
        let mut reused = 0;
        for pixels in 3..16 {
            eval.eval_str(&format!("(set-window-vscroll nil {pixels} t t)"))
                .unwrap();
            probe::reset();
            let actual = query
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            reused += usize::from(probe::max_depth() == 0);
            let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
            let expected = fresh
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            assert_eq!(actual.end(), expected.end(), "{decoration} pixels={pixels}");
            assert_eq!(
                actual.geometry(),
                expected.geometry(),
                "{decoration} pixels={pixels}"
            );
        }
        if decoration.contains("line-height") {
            // Fractional line heights round adjacent rows differently; the
            // unchanged-row placement guard intentionally stays conservative.
            assert_eq!(reused, 0, "noncontiguous rows reused: {decoration}");
        } else {
            assert!(
                reused > 0,
                "unchanged cursorless rows walked again: {decoration}"
            );
        }
        for change in [
            "(goto-char 1001)",
            "(put-text-property 1 8 'face '(:height 175))",
            r#"(overlay-put (make-overlay 1 20) 'display "replacement")"#,
        ] {
            eval.eval_str(change).unwrap();
            probe::reset();
            let actual = query
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            assert!(
                probe::max_depth() > 0,
                "cursorless query reused changed inputs: {change}"
            );
            let mut fresh = WindowLayoutQueryEngine::new_without_font_metrics();
            let expected = fresh
                .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
                .unwrap();
            assert_eq!(actual.end(), expected.end(), "{change}");
            assert_eq!(actual.geometry(), expected.geometry(), "{change}");
        }
    }
}
