//! Class-4 physical-retry publish-at-exhaustion and true unpublishable NoSafe settlement.
use super::*;
use std::{cell::Cell, rc::Rc};

/// Attested GNU 31.1 oracle (`a360712c9d`, linux-gtk3, Xvfb) publishes when a
/// header-line `:eval` toggles `window-hscroll` 0<->1 on every call:
/// `(:case header-toggles-hscroll :ph-calls 3 :ph-inhibit (t t t) :hscroll 0 :win-end-cached 38)`
#[test]
fn class4_physical_hscroll_retry_publishes_with_last_header_and_quiesces() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, _, window) =
        incr_editing_frame("abcdefghij\nsecond complete body row\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.set_variable("nosafe-window", Value::make_window(window.0));
    eval.eval_str(
        r#"(progn
      (goto-char 3)
      (setq inhibit-redisplay nil nosafe-evals 0 nosafe-scrolls nil
            mode-line-format nil tab-line-format nil
            header-line-format
            '((:eval (progn
              (setq nosafe-evals (1+ nosafe-evals))
              (let ((before (window-hscroll nosafe-window)))
                (set-window-hscroll nosafe-window (if (= before 0) 1 0))
                (setq nosafe-scrolls
                  (cons (list before (window-hscroll nosafe-window)) nosafe-scrolls)))
              (format "NO SAFE %d" nosafe-evals))))))"#,
    )
    .expect("install real late physical-input toggling");
    assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), 0);

    let requests = Rc::new(Cell::new(0));
    let observed = requests.clone();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        let prior = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
        let outcome = engine.redisplay_frame_attempt(eval, frame);
        let after = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
        assert_eq!(
            after - prior,
            2,
            "hscroll-toggling header exhaustion must stop after one retry (not 13x): {prior}..{after}"
        );
        let FrameLayoutAttempt::Prepared(state) = outcome else {
            panic!("hscroll-toggling header must publish at exhaustion via ExhaustionSnapshot");
        };
        assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 1);
        let header = state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::HeaderLine)
            .map(|row| glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]))
            .collect::<Vec<_>>()
            .join("");
        let expected = format!("NO SAFE {after}");
        assert!(
            header.contains(&expected),
            "published header must show last evaluated string {expected:?}: {header:?}"
        );
        let id = neovm_core::window::geometry::PresentationId::new(state.presentation_id.get());
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .activate_display_presentation(id)
            .unwrap();
    }));

    eval.redisplay_with_force(false)
        .expect("first bounded exhaustion publish");
    assert_eq!(requests.get(), 1);
    assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), 1);
    let callbacks = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
    for _ in 0..3 {
        eval.redisplay_with_force(false)
            .expect("idle after exhaustion publish");
    }
    assert_eq!(
        requests.get(),
        1,
        "published exhaustion frame must quiesce idle redisplay"
    );
    assert_eq!(
        eval.eval_str("nosafe-evals").unwrap().as_fixnum(),
        Some(callbacks)
    );

    // New real work still wakes the producer.
    eval.eval_str("(goto-char 4)").unwrap();
    eval.redisplay_with_force(false)
        .expect("real new point work");
    assert_eq!(requests.get(), 2);
    assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), 2);
    assert!(eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap() > callbacks);
}

/// Genuinely unpublishable seam: a callback deletes the frame being laid out
/// mid-attempt. `redisplay_frame_attempt` must abort without accepting, settle
/// the deleted frame's pending demand, and preserve the sibling live frame's.
#[test]
fn no_safe_deleted_frame_settles_without_accepting_and_preserves_other_frame() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, doomed_frame, buffer, _window) =
        incr_editing_frame("doomed frame body\n", 800, 240);
    realize_test_gui_frame(&mut eval, doomed_frame);
    let survivor_frame = eval
        .frame_manager_mut()
        .create_frame("survivor", 800, 240, buffer);
    realize_test_gui_frame(&mut eval, survivor_frame);
    eval.set_variable("doomed-frame", Value::make_frame(doomed_frame.0));
    eval.eval_str(
        r#"(setq mode-line-format nil tab-line-format nil
                 delete-calls 0
                 header-line-format
                 '((:eval (progn
                            (setq delete-calls (1+ delete-calls))
                            (if (frame-live-p doomed-frame)
                                (delete-frame doomed-frame t))
                            "BYE"))))"#,
    )
    .expect("install frame-deleting header-line");
    let accepted_before = eval.gnu_redisplay_hooks_accepted_serial();

    let requests = Rc::new(Cell::new(0));
    let observed = requests.clone();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        let outcome = engine.redisplay_frame_attempt(eval, doomed_frame);
        assert!(
            matches!(outcome, FrameLayoutAttempt::Aborted),
            "deleted frame cannot be published even by ExhaustionSnapshot"
        );
        assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 0);
        assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), accepted_before);
    }));

    eval.gnu_mark_windows_all();
    eval.redisplay_with_force(false)
        .expect("deleted-frame NoSafe episode");
    assert_eq!(requests.get(), 1);
    assert_eq!(eval.eval_str("delete-calls").unwrap().as_fixnum(), Some(1));
    assert!(
        eval.frame_manager().get(doomed_frame).is_none(),
        "frame really was deleted mid-attempt"
    );
    assert!(
        eval.frame_redisplay_pending(survivor_frame),
        "NoSafe settlement must preserve PendingScope::All for surviving live frames"
    );
    assert!(
        !eval.frame_redisplay_pending(doomed_frame),
        "deleted frame's own mark must be cleared"
    );
    assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), accepted_before);
}
