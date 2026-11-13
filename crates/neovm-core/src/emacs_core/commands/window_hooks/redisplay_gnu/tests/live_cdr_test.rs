//! Live cons iteration of the two gated window walkers.
//! A callback's `setcdr` is visible to the next step. Dying returns
//! before any callback.

use neovm_value::value::Value;

use crate::emacs_core::eval::ShutdownPhase;

fn context() -> crate::emacs_core::eval::Context {
    crate::test_utils::init_test_tracing();
    crate::test_utils::new_context()
}

#[test]
fn walk_reads_the_live_cdr_after_each_callback() {
    let mut eval = context();
    eval.eval_str(
        r#"(progn
  (setq log nil)
  (setq chain (list
    (lambda (_arg)
      (setq log (cons 'a log))
      (setcdr chain (list (lambda (_arg) (setq log (cons 'c log))))))
    (lambda (_arg) (setq log (cons 'b log))))))"#,
    )
    .expect("chain");
    let functions = eval.eval_str("chain").expect("chain value");
    let mut record = false;
    super::walk(&mut eval, &mut record, functions, Value::fixnum(1)).expect("walk");
    assert!(record);
    assert!(eval.eval_str("(equal log '(c a))").expect("log").is_t());
}

#[test]
fn walk_started_while_dying_calls_nothing() {
    let mut eval = context();
    eval.eval_str(
        r#"(progn
  (setq ran nil)
  (setq chain (list (lambda (_arg) (setq ran t)))))"#,
    )
    .expect("chain");
    let functions = eval.eval_str("chain").expect("chain value");
    eval.shutdown_phase = ShutdownPhase::Dying;
    let mut record = false;
    let result =
        super::walk(&mut eval, &mut record, functions, Value::fixnum(1)).expect("dying walk");
    assert!(result.is_nil());
    assert!(!record);
    assert!(eval.eval_str("ran").expect("ran").is_nil());
}

#[test]
fn run_funs_reads_the_live_cdr_after_each_callback() {
    let mut eval = context();
    eval.eval_str(
        r#"(progn
  (setq log nil)
  (setq chain (list
    (lambda ()
      (setq log (cons 'a log))
      (setcdr chain (list (lambda () (setq log (cons 'c log))))))
    (lambda () (setq log (cons 'b log))))))"#,
    )
    .expect("chain");
    let functions = eval.eval_str("chain").expect("chain value");
    super::run_funs(&mut eval, functions).expect("run_funs");
    assert!(eval.eval_str("(equal log '(c a))").expect("log").is_t());
}

#[test]
fn run_funs_started_while_dying_calls_nothing() {
    let mut eval = context();
    eval.eval_str(
        r#"(progn
  (setq ran nil)
  (setq chain (list (lambda () (setq ran t)))))"#,
    )
    .expect("chain");
    let functions = eval.eval_str("chain").expect("chain value");
    eval.shutdown_phase = ShutdownPhase::Dying;
    let result = super::run_funs(&mut eval, functions).expect("dying run_funs");
    assert!(result.is_nil());
    assert!(eval.eval_str("ran").expect("ran").is_nil());
}
