//! Negative safety policy; this does not claim GNU's physical-toggle behavior.
use super::*;
use std::{cell::Cell, rc::Rc};

#[test]
fn no_safe_physical_retry_settles_without_accepting_and_preserves_new_work() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, _, window) =
        incr_editing_frame("abcdefghij\nsecond complete body row\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.set_variable("nosafe-window", Value::make_window(window.0));
    eval.eval_str(
        r#"(progn
      (goto-char 3)
      (setq nosafe-evals 0 nosafe-scrolls nil
            mode-line-format nil tab-line-format nil
            header-line-format
            '((:eval (progn
              (setq nosafe-evals (1+ nosafe-evals))
              (let ((before (window-hscroll nosafe-window)))
                (set-window-hscroll nosafe-window (if (= before 0) 1 0))
                (setq nosafe-scrolls
                  (cons (list before (window-hscroll nosafe-window)) nosafe-scrolls)))
              "NO SAFE")))))"#,
    )
    .expect("install real late physical-input toggling");
    let leaf = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap();
    let before_end = leaf.window_end_state();
    let before_cursor = leaf.display().unwrap().cursor;
    let before_physical = leaf.display().unwrap().phys_cursor.clone();
    let before_output_cursor = leaf.display().unwrap().output_cursor;
    let accepted_before = eval.gnu_redisplay_hooks_accepted_serial();
    assert_eq!(accepted_before, 0, "genuine first-frame NoSafe control");
    assert!(leaf.display().unwrap().redisplay_output().is_none());

    let requests = Rc::new(Cell::new(0));
    let observed = requests.clone();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        let prior = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
        let outcome = engine.redisplay_frame_attempt(eval, frame);
        let after = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
        // Prove repeated physical refusal BEFORE testing scheduler settlement.
        assert!(
            after - prior >= 2,
            "actual repeated header callback: {prior}..{after}"
        );
        assert!(
            after - prior <= (MAX_FRAME_LAYOUT_RETRIES + 1) as i64,
            "one physical-refusal episode must terminate: {prior}..{after}"
        );
        let scrolls =
            neovm_core::emacs_core::value::list_to_vec(&eval.eval_str("nosafe-scrolls").unwrap())
                .unwrap();
        let episode: Vec<_> = scrolls
            .iter()
            .take((after - prior) as usize)
            .map(|entry| neovm_core::emacs_core::value::list_to_vec(entry).unwrap())
            .collect();
        assert!(
            episode
                .iter()
                .all(|pair| pair.len() == 2 && pair[0] != pair[1]),
            "each callback really changes strict hscroll input: {episode:?}"
        );
        assert!(
            episode.iter().any(|pair| pair[1] == Value::fixnum(0))
                && episode.iter().any(|pair| pair[1] == Value::fixnum(1)),
            "one bounded episode exercises both actual physical values: {episode:?}"
        );
        assert!(
            matches!(outcome, FrameLayoutAttempt::Aborted),
            "no owned physically consistent whole candidate exists"
        );
        assert!(engine.last_frame_display_state.is_none());
        assert!(
            engine.retained_window_matrices.is_empty(),
            "no fake reusable rows"
        );
        assert_eq!(
            eval.gnu_redisplay_hooks_accepted_serial(),
            accepted_before,
            "settlement cannot masquerade as accepted output"
        );
        let frame_state = eval.frame_manager().get(frame).unwrap();
        assert!(frame_state.redisplay_snapshot(window).is_none());
        assert!(frame_state.completed_presentation_geometry().is_none());
        assert!(!frame_state.has_prepared_display_presentations());
        assert!(frame_state.active_presentation().is_none());
        let leaf = frame_state.find_window(window).unwrap();
        assert_eq!(
            leaf.window_end_state(),
            before_end,
            "no speculative end acceptance"
        );
        let display = leaf.display().unwrap();
        assert!(
            display.redisplay_output().is_none(),
            "no output or GNU accuracy stamp"
        );
        assert_eq!(
            display.cursor, before_cursor,
            "no speculative logical cursor"
        );
        assert_eq!(
            display.phys_cursor, before_physical,
            "no speculative physical cursor"
        );
        assert_eq!(
            display.output_cursor, before_output_cursor,
            "no output progress"
        );
    }));

    eval.redisplay_with_force(false)
        .expect("first bounded NoSafe episode");
    assert_eq!(requests.get(), 1);
    assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), 0);
    let callbacks = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
    for _ in 0..3 {
        eval.redisplay_with_force(false).expect("idle after NoSafe");
    }
    assert_eq!(
        requests.get(),
        1,
        "NoSafe must settle its own request even while accepted_serial stays zero"
    );
    assert_eq!(
        eval.eval_str("nosafe-evals").unwrap().as_fixnum(),
        Some(callbacks)
    );

    // Point remains within both hscroll0 and hscroll1 viewports. The new work
    // must be serviced, while the same adversarial physical guard stays strict.
    eval.eval_str("(goto-char 4)").unwrap();
    eval.redisplay_with_force(false)
        .expect("real new point work");
    assert_eq!(requests.get(), 2);
    let after_point = eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap();
    assert!(after_point > callbacks);
    eval.redisplay_with_force(false)
        .expect("idle after second NoSafe episode");
    assert_eq!(requests.get(), 2);

    eval.gnu_mark_frame_redisplay(frame);
    eval.redisplay_with_force(false)
        .expect("explicit externally marked frame work");
    assert_eq!(requests.get(), 3);
    assert!(eval.eval_str("nosafe-evals").unwrap().as_fixnum().unwrap() > after_point);
    eval.redisplay_with_force(false)
        .expect("idle after third NoSafe episode");
    assert_eq!(requests.get(), 3);
    assert_eq!(eval.gnu_redisplay_hooks_accepted_serial(), 0);
}
