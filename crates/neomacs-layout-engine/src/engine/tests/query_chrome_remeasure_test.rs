//! Position queries must remeasure point-dependent chrome before answering.
//!
//! GNU `pos_visible_p` evaluates and measures mode/header/tab lines on every
//! call, then walks the body under those measured text-area bounds
//! (src/xdisp.c:1732-1770). Logical `window-end` queries keep the accepted
//! partition instead. These tests exercise the production query path, not a
//! mocked `WindowLayoutQuery`.

use super::*;
use neovm_core::buffer::{EmacsBytePos, LispCharPos1};
use neovm_core::window::{FrameId, Window, WindowId, WindowLayoutQueryScope};

fn move_point(eval: &mut Context, frame: FrameId, buffer: BufferId, window: WindowId, pos: i64) {
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .goto_emacs_byte_pos(EmacsBytePos::new((pos - 1) as usize));
    match eval
        .frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .find_window_mut(window)
        .expect("window")
    {
        Window::Leaf { point, .. } => *point = LispCharPos1::new(pos),
        other => panic!("expected leaf window, got {other:?}"),
    }
}

fn query(
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame: FrameId,
    window: WindowId,
    target: i64,
) -> WindowDisplaySnapshot {
    engine
        .query_window_layout(
            eval,
            frame,
            window,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(target),
            },
        )
        .expect("position query converges")
        .into_geometry()
        .expect("query geometry")
}

#[test]
fn position_query_remeasures_point_dependent_header_height() {
    let text = "line\n".repeat(80);
    let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .set_window_system(Some(neovm_core::emacs_core::Value::symbol("neo")));
    eval.eval_str(
        "(progn
           (setq mode-line-format nil
                 tab-line-format nil
                 header-line-format
                 '(:eval (propertize \"header\" 'face
                                     (list :height (if (= (point) 1) 1.0 4.0)))))
           (goto-char 1))",
    )
    .expect("header format");

    let mut engine = LayoutEngine::new();
    let before = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .expect("viewport query converges")
        .into_geometry()
        .expect("query geometry");
    assert_eq!(
        before.header_line_height,
        before.regions.header_line.expect("header region").height as i64,
        "the short header must be one consistent row"
    );
    let short_header = before.header_line_height;
    assert!(short_header > 0, "header line must be present");
    let short_body_y = before.regions.text_body.y;
    let bottom = before
        .rows
        .iter()
        .rev()
        .find_map(|row| row.start_buffer_pos.filter(|pos| *pos > LispCharPos1::ONE))
        .expect("bottom source row");

    // Move point without redisplay. The header :eval now asks for 4x height.
    move_point(&mut eval, frame, buffer, window, 5);

    // `window-end` walks with the accepted partition; it must not evaluate the
    // chrome Lisp just because the caller asks for a query.
    let end_query = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .expect("window-end query converges")
        .into_geometry()
        .expect("query geometry");
    assert!(
        end_query.header_line_height < short_header * 2,
        "window-end must not evaluate point-dependent chrome \
         (accepted={short_header}, query={})",
        end_query.header_line_height
    );

    // `pos-visible-in-window-p`/`posn-at-point` must measure the new header and
    // relayout the body under it before answering. GNU evaluates the chrome
    // format once per query; the measured-metric retry must not re-evaluate it.
    crate::display_status_line::reset_mode_line_eval_count();
    let after = query(&mut eval, &mut engine, frame, window, 5);
    assert_eq!(
        crate::display_status_line::header_line_eval_count(),
        1,
        "a Position query must evaluate the header format exactly once"
    );
    assert!(
        after.header_line_height >= short_header * 3,
        "position query must report the freshly measured header height \
         (before={short_header}, after={})",
        after.header_line_height
    );
    assert_eq!(
        after.regions.header_line.expect("header region").height as i64,
        after.header_line_height,
        "the published region and metric must describe the same row"
    );
    assert!(
        after.regions.text_body.y > short_body_y,
        "the text area must move down by the measured header growth"
    );
    assert_eq!(
        after.regions.text_body.y as i64,
        after.header_line_height + after.tab_line_height,
        "text-area origin is tab + header height"
    );

    // The bottom-row regression: a source row that was visible under the short
    // header is outside the (shorter) body once the point-dependent header
    // grows, exactly as GNU `pos_visible_p` reports.
    let hidden = query(&mut eval, &mut engine, frame, window, bottom.as_i64());
    assert!(
        hidden.point_for_buffer_pos(bottom).is_none(),
        "the former bottom row must no longer be visible under the grown header"
    );
}

#[test]
fn position_query_remeasures_point_dependent_mode_line_height() {
    let text = "line\n".repeat(80);
    let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .set_window_system(Some(neovm_core::emacs_core::Value::symbol("neo")));
    eval.eval_str(
        "(progn
           (setq header-line-format nil
                 tab-line-format nil
                 mode-line-format
                 '(:eval (propertize \"mode\" 'face
                                     (list :height (if (= (point) 1) 1.0 4.0)))))
           (goto-char 1))",
    )
    .expect("mode-line format");

    let mut engine = LayoutEngine::new();
    let before = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .expect("viewport query converges")
        .into_geometry()
        .expect("query geometry");
    let short_mode = before.mode_line_height;
    assert!(short_mode > 0, "mode line must be present");
    let before_body_height = before.regions.text_body.height;
    let bottom = before
        .rows
        .iter()
        .rev()
        .find_map(|row| row.start_buffer_pos.filter(|pos| *pos > LispCharPos1::ONE))
        .expect("bottom source row");

    move_point(&mut eval, frame, buffer, window, 5);
    let after = query(&mut eval, &mut engine, frame, window, 5);
    assert!(
        after.mode_line_height >= short_mode * 3,
        "position query must report the freshly measured mode-line height \
         (before={short_mode}, after={})",
        after.mode_line_height
    );
    assert!(
        after.regions.text_body.height < before_body_height,
        "a taller mode line must shrink the body without moving its origin"
    );
    assert_eq!(
        after.regions.text_body.y, before.regions.text_body.y,
        "the mode line is below the body; the origin does not move"
    );

    let hidden = query(&mut eval, &mut engine, frame, window, bottom.as_i64());
    assert!(
        hidden.point_for_buffer_pos(bottom).is_none(),
        "the former bottom row must no longer be visible under the grown mode line"
    );
}

#[test]
fn position_query_remeasures_chrome_when_retained_height_leaves_no_body() {
    let (mut eval, frame, buffer, window) = incr_editing_frame("line\nline\n", 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    eval.eval_str(
        "(setq mode-line-format nil tab-line-format nil
               header-line-format '(:eval (propertize \"header\" 'face
                 (list :height (if (= (point) 1) 4.0 1.0)))))",
    )
    .unwrap();
    move_point(&mut eval, frame, buffer, window, 1);
    let mut engine = LayoutEngine::new();
    let tall = query(&mut eval, &mut engine, frame, window, 1);
    let retained = crate::window_layout::WindowChromeMetrics::from_snapshot(&tall);
    // Reproduce the accepted partition carried by a presentation, then resize
    // the leaf below that height while leaving room for the shorter header.
    engine.retained_window_chrome_metrics.insert(
        neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64),
        retained,
    );
    let leaf = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap();
    let mut bounds = *leaf.bounds();
    bounds.height = retained.header_line_height;
    leaf.set_bounds(bounds);
    let empty = engine
        .query_window_layout(&mut eval, frame, window, WindowLayoutQueryScope::Viewport)
        .unwrap()
        .into_geometry()
        .unwrap();
    assert_eq!(empty.regions.text_body.height, 0.0);

    move_point(&mut eval, frame, buffer, window, 5);
    crate::display_status_line::reset_mode_line_eval_count();
    let recovered = query(&mut eval, &mut engine, frame, window, 1);
    assert_eq!(crate::display_status_line::header_line_eval_count(), 1);
    assert!(recovered.header_line_height < tall.header_line_height);
    assert!(recovered.regions.text_body.height > 0.0);
    assert!(recovered.point_for_buffer_pos(LispCharPos1::ONE).is_some());
}

#[test]
fn chrome_position_queries_do_not_reenter_chrome_or_signal_layout_busy() {
    use neovm_core::window::WindowLayoutQueryOutcome;

    // Exercise both entry points: a formatter calling a free query adapter,
    // and a query's formatter calling the already-running adapter.
    for outer_is_query in [false, true] {
        let (mut eval, frame, buffer, window) = incr_editing_frame("line\nline\n", 800, 600);
        eval.frame_manager_mut().get_mut(frame).unwrap().initial = false;
        eval.eval_str(
            "(setq mode-line-format nil tab-line-format nil
                   chrome-query-returned nil
                   header-line-format '(:eval (progn
                     (posn-at-point 1)
                     (setq chrome-query-returned t)
                     \"header\")))",
        )
        .unwrap();
        move_point(&mut eval, frame, buffer, window, 1);
        let mut engine = WindowLayoutQueryEngine::new();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = calls.clone();
        eval.install_window_layout_query(move |eval, frame, window, scope| {
            observed.set(observed.get() + 1);
            match engine.query_window_layout(eval, frame, window, scope) {
                Ok(query) => WindowLayoutQueryOutcome::Ready(query),
                Err(error) => WindowLayoutQueryOutcome::Failed(error),
            }
        });
        crate::display_status_line::reset_mode_line_eval_count();
        if outer_is_query {
            assert!(!eval.eval_str("(posn-at-point 1)").unwrap().is_nil());
            assert_eq!(crate::display_status_line::header_line_eval_count(), 1);
        } else {
            let format = eval
                .buffers
                .get(buffer)
                .expect("buffer")
                .buffer_local_value("header-line-format")
                .expect("header format");
            let rendered =
                neovm_core::emacs_core::xdisp::try_format_mode_line_for_display_with_sources(
                    &mut eval,
                    format,
                    Value::make_window(window.0),
                    Value::make_buffer(buffer),
                    80,
                )
                .unwrap();
            assert_eq!(
                rendered.value().as_runtime_string_owned().as_deref(),
                Some("header")
            );
            assert_eq!(crate::display_status_line::header_line_eval_count(), 0);
        }
        assert_eq!(calls.get(), 1);
        assert_eq!(
            eval.variable_value_by_name("chrome-query-returned"),
            Some(Value::T)
        );
        assert!(!neovm_core::emacs_core::xdisp::mode_line_display_active(
            &eval
        ));
    }
}

#[test]
fn position_query_results_are_not_retained_in_the_query_cache() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("line\nline\n", 800, 600);
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil)")
        .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let position = WindowLayoutQueryScope::Position {
        target: LispCharPos1::ONE,
    };
    engine
        .query_window_layout(&mut eval, frame, window, position)
        .unwrap();
    assert!(
        engine
            .query_cache
            .get(&eval, frame, window, position)
            .is_none(),
        "Position observations bypass cache reads and must not consume cache capacity"
    );

    // The cache still works for the scope that is allowed to read it.
    let viewport = WindowLayoutQueryScope::Viewport;
    engine
        .query_window_layout(&mut eval, frame, window, viewport)
        .unwrap();
    assert!(
        engine
            .query_cache
            .get(&eval, frame, window, viewport)
            .is_some(),
        "window-end observations must still be cached"
    );
}

#[test]
fn position_query_propagates_deferred_chrome_exit_to_lisp() {
    use neovm_core::window::WindowLayoutQueryOutcome;

    let (mut eval, frame, _buffer, _window) = incr_editing_frame("line\nline\n", 800, 600);
    eval.frame_manager_mut().get_mut(frame).unwrap().initial = false;
    eval.eval_str(
        "(progn
           (setq mode-line-format nil tab-line-format nil
                 header-line-format '(:eval (throw 'chrome-exit 42)))
           (goto-char 1))",
    )
    .unwrap();
    let mut engine = WindowLayoutQueryEngine::new();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match engine.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });

    assert_eq!(
        eval.eval_str("(catch 'chrome-exit (posn-at-point 1))")
            .unwrap(),
        Value::fixnum(42),
        "the chrome throw must reach the enclosing Lisp catch"
    );
    assert!(
        !eval.has_mode_line_display_flow(),
        "the deferred flow must be consumed at the position-query boundary"
    );
}
