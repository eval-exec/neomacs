use crate::emacs_core::value::Value;

fn unlet_case(prior: &str, exit: &str) {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(&format!(
        "(progn
           (setq inhibit-quit {prior} unlet-count 0 unlet-side-effect nil)
           (defun inhibition-watcher (_symbol proposed operation _where)
             (when (eq operation 'unlet)
               (remove-variable-watcher 'inhibit-quit #'inhibition-watcher)
               (setq unlet-count (1+ unlet-count)
                     unlet-proposed proposed unlet-live inhibit-quit
                     unlet-side-effect 'retained)
               (garbage-collect)
               {exit})))"
    ))
    .unwrap();
    let expected = eval.eval_str("inhibit-quit").unwrap();
    let callback = eval
        .eval_str(
            "(function (lambda ()
               (setq unlet-body inhibit-quit)
               (add-variable-watcher 'inhibit-quit #'inhibition-watcher)))",
        )
        .unwrap();
    let specpdl = eval.specpdl.len();
    let roots = eval.save_vm_roots();
    let frame_roots = eval.save_vm_frame_roots();
    eval.run_timer_callback_preserving_state(callback, vec![])
        .unwrap();
    assert_eq!(eval.specpdl.len(), specpdl);
    assert_eq!(eval.save_vm_frame_roots(), frame_roots);
    eval.restore_vm_roots(roots);
    assert_eq!(eval.eval_str("unlet-count").unwrap(), Value::fixnum(1));
    assert_eq!(eval.eval_str("unlet-proposed").unwrap(), expected);
    assert_eq!(eval.eval_str("unlet-body").unwrap(), Value::T);
    assert_eq!(eval.eval_str("unlet-live").unwrap(), Value::T);
    assert_eq!(
        eval.eval_str("unlet-side-effect").unwrap(),
        Value::symbol("retained")
    );
    assert_eq!(
        eval.eval_str("(get-variable-watchers 'inhibit-quit)")
            .unwrap(),
        Value::NIL
    );
    assert!(eval.active_variable_watchers.is_empty());
    // Exercise quit before asserting the restored variable. This must catch
    // suppression, not merely a mismatch of two Lisp values.
    let quit = eval
        .eval_str(
            "(condition-case nil
               (let ((quit-flag t)) (sleep-for 0.001) 'suppressed)
               (quit 'interrupted))",
        )
        .unwrap();
    assert_eq!(
        quit,
        Value::symbol(if expected.is_nil() {
            "interrupted"
        } else {
            "suppressed"
        })
    );
    assert_eq!(eval.eval_str("inhibit-quit").unwrap(), expected);
    if prior == "'(heap prior)" {
        assert_eq!(
            eval.eval_str("(equal inhibit-quit '(heap prior))").unwrap(),
            Value::T
        );
    }
}

#[test]
fn timer_unlet_signal_restores_nil_and_pending_quit() {
    unlet_case("nil", "(error \"timer unlet signal\")");
}

#[test]
fn timer_unlet_signal_preserves_deliberate_prior_inhibition() {
    unlet_case("t", "(error \"timer unlet signal\")");
}

#[test]
fn timer_unlet_signal_roots_exact_heap_prior_inhibition() {
    unlet_case("'(heap prior)", "(error \"timer unlet signal\")");
}

#[test]
fn timer_unlet_normal_preserves_nil_and_true_quit_controls() {
    for prior in ["nil", "t"] {
        unlet_case(prior, "nil");
    }
}
