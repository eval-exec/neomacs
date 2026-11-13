//! `NEOVM_PPS_PROPERTIZE`: on by default (GNU's behaviour); `0` restores the
//! scan that never propertizes, for attribution only.

use super::*;

use neovm_edit::buffer::BufferId;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::test_utils::runtime_startup_eval_one;

use neovm_text::emacs_core::syntax::*;

/// The call count GNU makes for `(parse-partial-sexp 1 30000)` on this text
/// (the oracle form `pps_propertize_gnu_tests::..._in_chunks`: 15 calls), and
/// none with the knob off.
#[test]
fn switching_the_knob_off_skips_every_call() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let ((log nil))
  (with-temp-buffer
    (dotimes (i 600)
      (insert (format "(defun f%d (x) \"doc %d ;; not a comment\" ; c %d\n  (list x '(a . b) ?\\( \"s\\\"t\" #'car))\n" i i i)))
    (emacs-lisp-mode)
    (setq-local syntax-propertize-function
                (let ((orig syntax-propertize-function))
                  (lambda (s e) (push (list s e) log) (funcall orig s e))))
    (setq syntax-propertize--done 1)
    (list (car (parse-partial-sexp 1 30000)) (length log))))
"#;
    let run = |on| {
        PPS_PROPERTIZE_OVERRIDE.with(|cell| cell.set(Some(on)));
        let out = runtime_startup_eval_one(form);
        PPS_PROPERTIZE_OVERRIDE.with(|cell| cell.set(None));
        out
    };
    assert_eq!(run(true), "OK (2 15)");
    assert_eq!(run(false), "OK (2 0)");
}
