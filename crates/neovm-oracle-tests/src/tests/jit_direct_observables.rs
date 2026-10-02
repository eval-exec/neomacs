//! GNU oracle pins for debugger advice and quit after named callees have
//! warmed their native entries. Expectations are refreshed from GNU only.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Match `jit_call_frames`' first-call tiering and ensure a call-heavy
/// callee's profitability deferral has expired during the warm-up. Keep
/// calls explicit so these pins exercise native call frames and exits.
const JIT_ENV: &[(&str, &str)] = &[
    ("NEOVM_JIT_THRESHOLD", "1"),
    ("NEOVM_JIT_PROFIT_DEFER", "1"),
    ("NEOVM_JIT_INLINE", "off"),
];

/// GNU `debug.el` implements debug-on-entry with :before advice, so advice
/// added after warm-up must invalidate the direct callee binding. The
/// recorder observes the named caller's frame and cancels the advice from
/// inside the first debugger invocation; recursion and later calls resume.
#[test]
fn oracle_jit_warmed_direct_callee_obeys_debug_on_entry_and_cancellation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (require 'debug)
  (defvar neovm--jdo-log nil)
  (defalias 'neovm--jdo-rec
    (byte-compile
     (lambda (n) (if (= n 0) 0 (1+ (neovm--jdo-rec (1- n)))))))
  (defalias 'neovm--jdo-caller
    (byte-compile (lambda (n) (+ (neovm--jdo-rec n) 2))))
  (dotimes (_ 6000) (neovm--jdo-caller 4))
  (let ((debugger
         (lambda (&rest args)
           (push (list (car args) (backtrace-frame 0 'neovm--jdo-caller))
                 neovm--jdo-log)
           (cancel-debug-on-entry 'neovm--jdo-rec)
           nil)))
    (debug-on-entry 'neovm--jdo-rec)
    (list (neovm--jdo-caller 5) (nreverse neovm--jdo-log)
          (neovm--jdo-caller 3))))"#;
    let expect = expect_test::expect![[r#""OK (7 ((debug (t neovm--jdo-caller 5))) 5)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// GNU `Bcall` polls quit before the next frame is pushed, and
/// `process_quit_flag` clears the request before signaling. A quit armed
/// inside a warmed callee must unwind its native recursion to the outer
/// handler, leave the flag clear and allow the next call to finish.
#[test]
fn oracle_jit_quit_inside_a_warmed_direct_callee_unwinds_and_recovers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--jdq-arm nil)
  (defalias 'neovm--jdq-value
    (byte-compile (lambda (x) (if (= x 0) 7 (1+ x)))))
  (defalias 'neovm--jdq-callee
    (byte-compile
     (lambda (x)
       (when neovm--jdq-arm (setq quit-flag t))
       (neovm--jdq-value x))))
  (defalias 'neovm--jdq-rec
    (byte-compile
     (lambda (n)
       (if (= n 0) (neovm--jdq-callee n)
         (1+ (neovm--jdq-rec (1- n)))))))
  (dotimes (_ 6000) (neovm--jdq-rec 4))
  (list (let ((neovm--jdq-arm t))
          (condition-case err (neovm--jdq-rec 4)
            (quit (list 'caught err))))
        quit-flag (neovm--jdq-rec 3)))"#;
    let expect = expect_test::expect![[r#""OK ((caught (quit)) nil 10)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}
