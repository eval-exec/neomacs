//! A completed body remains an ordinary interactive presentation after chrome
//! changes its source. The original mutation/Prepared REDs are retained; live
//! postconditions follow the GNU contract decided in lead13053/13066.
use super::*;
use std::{cell::Cell, rc::Rc};

fn changing_header_frame() -> (
    Context,
    neovm_core::window::FrameId,
    neovm_core::window::WindowId,
) {
    let (mut eval, frame, _buffer, window) =
        incr_editing_frame("agent shell body\nsecond line\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(progn
          (setq neo-policy-evals 0 neo-policy-overlay (make-overlay 1 6))
          (setq mode-line-format nil tab-line-format nil
                header-line-format
                '((:eval (progn
                    (setq neo-policy-evals (1+ neo-policy-evals))
                    (overlay-put neo-policy-overlay 'face
                      (if (= (% neo-policy-evals 2) 0) 'bold 'italic))
                    "Agent shell")))))"#,
    )
    .expect("install a genuinely changing late header callback");
    (eval, frame, window)
}

#[test]
fn exhausted_header_mutation_keeps_closed_paint_without_live_authority() {
    let (mut eval, frame, window) = changing_header_frame();
    let before_window_end = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap()
        .redisplay_state()
        .unwrap()
        .window_end;
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);

    // Keep the original first failing assertion and panic text for RED provenance.
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!(
            "a closed first presentation must survive exhaustion without claiming live authority"
        );
    };
    let state = output.state();
    let windows: std::collections::HashSet<_> = state
        .window_matrices
        .iter()
        .map(|entry| entry.window_id)
        .collect();
    assert_eq!(
        windows.len(),
        state.window_matrices.len(),
        "one matrix per presented window"
    );
    let matrix = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .expect("completed body window");
    let text_for = |role| {
        matrix
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == role)
            .map(|row| glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]))
            .collect::<Vec<_>>()
            .join("")
    };
    assert!(text_for(GlyphRowRole::Text).contains("agent shell body"));
    assert_eq!(text_for(GlyphRowRole::HeaderLine).trim_end(), "Agent shell");
    assert!(
        !state.faces.is_empty(),
        "drawing retains its resolved face owner"
    );
    assert!(
        state.validate_spatial_projections().is_ok(),
        "completed publication validates geometry"
    );
    assert!(
        !state.presented_hit_index.is_empty(),
        "normal presentation retains semantic hits"
    );
    assert_eq!(
        state.presented_pointer_source,
        crate::presentation::pointer::window_pointer_source_map(state).unwrap(),
        "normal pointer projection comes from the actual completed rows"
    );
    let cursor = state
        .phys_cursor
        .as_ref()
        .expect("GNU keeps the completed physical cursor");
    assert_eq!(cursor.window_id.get(), window.0 as i64);
    assert!(cursor.width > 0.0 && cursor.height > 0.0);
    assert_eq!(cursor.row, cursor.slot_id.row as usize);
    let row = &matrix.matrix.rows[cursor.row];
    assert_eq!(row.role, GlyphRowRole::Text);
    assert_eq!(row.cursor_col, Some(cursor.slot_id.col));
    assert!(
        state
            .cursors
            .iter()
            .all(|inactive| inactive.window_id.get() != window.0 as i64),
        "the selected physical cursor is not duplicated as an inactive artifact"
    );
    let retained = engine
        .retained_window_matrices
        .get(&DisplayWindowId::new(window.0 as i64))
        .expect("retain the completed matrix");
    let current_overlay_tick = eval
        .buffer_manager()
        .current_buffer()
        .unwrap()
        .overlay_modified_tick();
    assert!(
        retained.key.overlay_modified_tick < current_overlay_tick,
        "source reuse evidence must precede the header mutation, not certify it as row input"
    );
    assert!(
        !engine.retained_window_chrome_metrics.is_empty(),
        "normal chrome metrics remain available"
    );
    let evaluations = eval
        .eval_str("neo-policy-evals")
        .unwrap()
        .as_fixnum()
        .unwrap();
    assert!(
        evaluations > 0,
        "the callback must actually mutate the overlay"
    );
    assert!(
        evaluations <= (MAX_FRAME_LAYOUT_RETRIES + 1) as i64,
        "one exhausted request must terminate its callback work: {evaluations}"
    );

    let frame_state = eval.frame_manager().get(frame).unwrap();
    assert!(
        frame_state.redisplay_snapshot(window).is_some(),
        "publish the normal query matrix"
    );
    assert!(
        frame_state.completed_presentation_geometry().is_some(),
        "publish completed geometry"
    );
    assert_ne!(
        frame_state
            .find_window(window)
            .unwrap()
            .redisplay_state()
            .unwrap()
            .window_end,
        before_window_end,
        "publish the completed walk's end"
    );

    let presentation =
        neovm_core::window::geometry::PresentationId::new(state.presentation_id.get());
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .activate_display_presentation(presentation)
        .expect("activate the normal presentation");
    let frame_state = eval.frame_manager().get(frame).unwrap();
    assert!(frame_state.active_presentation_geometry().is_some());
    assert!(frame_state.redisplay_snapshot(window).is_some());
    assert!(
        eval.eval_str("(window-cursor-info)").unwrap().is_vector(),
        "the displayed cursor remains Lisp-visible after activation"
    );
}

#[test]
fn exhausted_request_quiesces_but_a_new_point_change_runs_the_real_producer() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, _window) = changing_header_frame();
    let attempts = Rc::new(Cell::new(0));
    let observed = attempts.clone();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(eval, frame)
        else {
            panic!("exhausted real producer must yield its completed paint");
        };
        let id = neovm_core::window::geometry::PresentationId::new(output.presentation_id.get());
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .activate_display_presentation(id)
            .expect("activate real owned output");
    }));
    eval.redisplay_with_force(false).expect("initial redisplay");
    assert_eq!(attempts.get(), 1);
    let evaluations = eval.eval_str("neo-policy-evals").unwrap().as_fixnum();
    for _ in 0..3 {
        eval.redisplay_with_force(false)
            .expect("idle redisplay request");
    }
    assert_eq!(
        attempts.get(),
        1,
        "accepted callback mutations settle before idle requests"
    );
    eval.eval_str(
        "(let ((cell (list 1)) (table (make-hash-table))) (setcar cell 2) (puthash 'idle 3 table))",
    )
    .expect("unrelated real collection mutations");
    eval.redisplay_with_force(false)
        .expect("idle after unrelated mutation");
    assert_eq!(
        attempts.get(),
        1,
        "unrelated collection writes do not wake redisplay"
    );
    eval.eval_str(
        "(progn (fset 'neo-policy-unrelated (lambda () 17)) (put 'neo-policy-unrelated 'idle 9))",
    )
    .expect("unrelated real function and symbol-property mutations");
    eval.redisplay_with_force(false)
        .expect("idle after unrelated definition/property mutation");
    assert_eq!(
        attempts.get(),
        1,
        "no self-dirty retry loop after exhaustion"
    );
    assert_eq!(
        eval.eval_str("neo-policy-evals").unwrap().as_fixnum(),
        evaluations
    );
    eval.eval_str("(goto-char 2)")
        .expect("new externally owned point change");
    eval.redisplay_with_force(false)
        .expect("redisplay after new work");
    assert_eq!(
        attempts.get(),
        2,
        "quiescence must not suppress subsequent work"
    );
    eval.gnu_mark_frame_redisplay(frame);
    eval.redisplay_with_force(false)
        .expect("explicit new frame display request");
    assert_eq!(
        attempts.get(),
        3,
        "an idempotent target mark is still a new display request"
    );
    // GNU ignores redisplay's historical FORCE argument (dispnew.c:7021-7034).
    // Test the existing explicit off policy for a truly forced native layout.
    struct RestoreIdleSkip;
    impl Drop for RestoreIdleSkip {
        fn drop(&mut self) {
            neovm_display::test_support::authority::set_redisplay_idle_skip_for_test(None);
        }
    }
    neovm_display::test_support::authority::set_redisplay_idle_skip_for_test(Some(false));
    let _restore_idle_skip = RestoreIdleSkip;
    eval.redisplay_with_force(true)
        .expect("explicit forced redisplay");
    assert_eq!(attempts.get(), 4);
}

#[test]
fn later_chrome_buffer_reassociation_retries_before_publishing_earlier_rows() {
    let (mut eval, frame, buffer, earlier) =
        incr_editing_frame("original earlier body\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let later = eval
        .frame_manager_mut()
        .split_window(
            frame,
            earlier,
            neovm_core::window::SplitDirection::Horizontal,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    eval.set_variable("neo-owner-later-window", Value::make_window(later.0));
    let replacement = eval.buffer_manager_mut().create_buffer("*832-replacement*");
    let later_buffer = eval.buffer_manager_mut().create_buffer("*832-later*");
    eval.buffer_manager_mut()
        .get_mut(replacement)
        .unwrap()
        .insert("replacement earlier body\n");
    eval.buffer_manager_mut()
        .get_mut(later_buffer)
        .unwrap()
        .insert("later body\n");
    for id in [buffer, replacement, later_buffer] {
        let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
        for name in ["mode-line-format", "tab-line-format", "header-line-format"] {
            buffer.set_buffer_local(name, Value::NIL);
        }
    }
    eval.set_variable("neo-owner-earlier-window", Value::make_window(earlier.0));
    eval.set_variable("neo-owner-replacement", Value::make_buffer(replacement));
    let header = eval
        .eval_str(
            r#"'((:eval (progn
        (if (eq (window-buffer neo-owner-earlier-window) neo-owner-replacement) nil
            (set-window-buffer neo-owner-earlier-window neo-owner-replacement))
        "Later")))"#,
        )
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(later_buffer)
        .unwrap()
        .set_buffer_local("header-line-format", header);
    eval.eval_str(r#"(set-window-buffer neo-owner-later-window "*832-later*")"#)
        .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("reassociated live owners must converge");
    };
    assert_ne!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(earlier)
            .unwrap()
            .buffer_id(),
        Some(buffer)
    );
    let earlier_matrix = output
        .state()
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == earlier.0 as i64)
        .unwrap();
    let text = earlier_matrix
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .map(|row| glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]))
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("replacement earlier body"),
        "live owner must match produced body: {text:?}"
    );
    assert!(!text.contains("original earlier body"));
}
