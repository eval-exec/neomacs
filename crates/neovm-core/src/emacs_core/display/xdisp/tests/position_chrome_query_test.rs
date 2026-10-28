//! Chrome-presence gating for point-only retained-row reuse.
//!
//! GNU `pos_visible_p` measures chrome on every query, so retained rows may
//! answer a point-only move only when no chrome line can change the text area.

use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::value::Value;
use crate::window::{Window, WindowLayoutQuery, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

fn assert_point_only_warm_query_reuse(setup: &str, reuse_after_move: bool) {
    use crate::window::{WindowLayoutQuery, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().unwrap().id;
    eval.buffers.get_mut(buffer).unwrap().insert("a\nb\n");
    let frame = eval
        .frames
        .create_frame("point-only-row-query", 160, 96, buffer);
    let window = eval.frames.get(frame).unwrap().selected_window;
    eval.frames.get_mut(frame).unwrap().initial = false;
    eval.set_variable("point-query-window", Value::make_window(window.0));
    eval.eval_str(
        "(progn
           (set-default 'display-line-numbers nil)
           (set-default 'mode-line-format nil)
           (set-default 'header-line-format nil)
           (set-default 'tab-line-format nil)
           (goto-char 1))",
    )
    .unwrap();
    eval.eval_str(setup).unwrap();
    match eval
        .frames
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        crate::window::Window::Leaf { point, .. } => *point = LispCharPos1::ONE,
        other => panic!("expected leaf window, got {other:?}"),
    }

    // Deliberately give the current buffer the opposite admission conditions.
    // The gate must inspect the queried window's buffer, not this buffer.
    let other_buffer = eval.buffers.create_buffer("*point-query-other*");
    eval.set_current_buffer_unrecorded(other_buffer).unwrap();
    let other_value = if reuse_after_move {
        Value::T
    } else {
        Value::NIL
    };
    for name in [
        "display-line-numbers",
        "mode-line-format",
        "header-line-format",
        "tab-line-format",
    ] {
        eval.buffers
            .set_buffer_local_property(other_buffer, name, other_value)
            .unwrap();
    }

    let freshness = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    let mut warm = crate::window::WindowDisplaySnapshot {
        window_id: window,
        regions_materialized: true,
        points: vec![crate::window::DisplayPointSnapshot {
            role: crate::window::DisplayPointRole::Glyph,
            buffer_pos: LispCharPos1::new(3),
            x: 0,
            y: 16,
            width: 8,
            height: 16,
            row: 1,
            col: 0,
        }],
        rows: vec![crate::window::DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_buffer_pos: Some(LispCharPos1::new(3)),
            end_buffer_pos: Some(LispCharPos1::new(4)),
            ..Default::default()
        }],
        buffer_modiff: Some(eval.buffers.get(buffer).unwrap().modified_tick()),
        layout_freshness: Some(freshness.clone()),
        ..Default::default()
    };
    warm.regions.text_body.width = 160.0;
    warm.regions.text_body.height = 32.0;
    eval.frames
        .get_mut(frame)
        .unwrap()
        .commit_redisplay_cache_for_test(vec![warm]);

    // Mock canonical bounds, not a font/chrome metric engine: the old body
    // admitted the target at y=16 in [0,32); the new visible body is [0,16),
    // with no target point. Header growth or current-line number face metrics
    // can invalidate that old admission without any display-variable mutation.
    let mut refreshed = crate::window::WindowDisplaySnapshot {
        window_id: window,
        regions_materialized: true,
        rows: vec![crate::window::DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_buffer_pos: Some(LispCharPos1::ONE),
            end_buffer_pos: Some(LispCharPos1::new(2)),
            ..Default::default()
        }],
        ..Default::default()
    };
    refreshed.regions.text_body.width = 160.0;
    refreshed.regions.text_body.height = 16.0;
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    eval.install_window_layout_query(move |_, fid, wid, scope| {
        assert_eq!((fid, wid), (frame, window));
        assert_eq!(
            scope,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(3)
            }
        );
        observed.set(observed.get() + 1);
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(
            LispCharPos1::ONE,
            Some(refreshed.clone()),
        ))
    });
    let query = |eval: &mut Context| {
        builtin_posn_at_point(eval, vec![Value::fixnum(3), Value::make_window(window.0)]).unwrap()
    };
    assert!(
        !query(&mut eval).is_nil(),
        "exact-fresh rows must answer: {setup}"
    );
    assert_eq!(calls.get(), 0, "exact-fresh rows must not query: {setup}");

    eval.buffers
        .get_mut(buffer)
        .unwrap()
        .goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
    match eval
        .frames
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        crate::window::Window::Leaf { point, .. } => {
            assert_eq!(*point, LispCharPos1::ONE);
            *point = LispCharPos1::new(3);
        }
        other => panic!("expected leaf window, got {other:?}"),
    }
    let moved = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    assert_ne!(
        freshness, moved,
        "saved window point must change freshness: {setup}"
    );
    assert!(
        freshness.same_position_rows_ignoring_point(&moved),
        "only point may change: {setup}"
    );
    assert!(
        eval.fresh_window_display_snapshot(frame, window, buffer)
            .is_none()
    );
    assert_eq!(query(&mut eval).is_nil(), !reuse_after_move, "{setup}");
    assert_eq!(calls.get(), usize::from(!reuse_after_move), "{setup}");
    assert_eq!(eval.buffers.current_buffer_id(), Some(other_buffer));
}

#[test]
fn posn_at_point_requeries_warm_rows_for_chrome_after_point_moves() {
    for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
        for setup in [
            format!("(set-default '{name} \"static chrome\")"),
            format!("(setq {name} \"local chrome\")"),
            format!("(set-window-parameter point-query-window '{name} \"window chrome\")"),
            format!(
                "(progn (set-default '{name} \"default chrome\") (set-window-parameter point-query-window '{name} nil))"
            ),
        ] {
            assert_point_only_warm_query_reuse(&setup, false);
        }
    }
    for setup in [
        "(setq header-line-format '(:eval (propertize \"header\" 'face (list :height (if (= (point) 1) 1.0 4.0)))))",
        "(set-window-parameter point-query-window 'header-line-format '(:eval (propertize \"header\" 'face (list :height (if (= (point) 1) 1.0 4.0)))))",
    ] {
        assert_point_only_warm_query_reuse(setup, false);
    }
}

#[test]
fn posn_at_point_reuses_point_only_warm_rows_with_effective_nil_chrome_and_numbers() {
    assert_point_only_warm_query_reuse("nil", true);
    for name in [
        "display-line-numbers",
        "mode-line-format",
        "header-line-format",
        "tab-line-format",
    ] {
        assert_point_only_warm_query_reuse(
            &format!("(progn (set-default '{name} t) (set (make-local-variable '{name}) nil))"),
            true,
        );
    }
    assert_point_only_warm_query_reuse(
        "(set-window-parameter point-query-window 'header-line-format nil)",
        true,
    );
    // GNU `window_wants_header_line` treats a `none` parameter as absence
    // (src/window.c:6067-6081); the buffer format behind it is irrelevant.
    for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
        assert_point_only_warm_query_reuse(
            &format!(
                "(progn (set-default '{name} \"buffer chrome\") (set-window-parameter point-query-window '{name} 'none))"
            ),
            true,
        );
    }
}
