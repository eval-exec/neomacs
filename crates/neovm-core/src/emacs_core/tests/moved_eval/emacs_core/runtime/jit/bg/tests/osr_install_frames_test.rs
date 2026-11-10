//! T-O2: a hot loop that calls a frame-watching callee shows the same
//! frames before its OSR leaf is installed (interpreted) and after (native).
//!
//! `runtime_startup_context` is minted in this crate. The transfer counter
//! is the existing authority read.

use crate::emacs_core::print::print_value;
use crate::test_utils::{init_test_tracing, runtime_startup_context};
use neovm_eval::test_support::authority::{
    force_deopt_for_test, force_mode_for_test, force_osr_for_test, osr_transfer_count_for_test,
};
use neovm_value::value::Value;

/// T-O2: a hot loop that calls a frame-watching callee shows the same
/// frames before its OSR leaf is installed (interpreted) and after (native).
#[test]
fn jit_bg_osr_install_keeps_the_frames() {
    init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        r#"(progn
  (defvar neovm--bgo-seen nil)
  (defun neovm--bgo-probe (i)
    (when (memq i '(3 1999997))
      (push (list i (backtrace-frame 1 'neovm--bgo-probe)) neovm--bgo-seen))
    1)
  (defun neovm--bgo-loop (n)
    (let ((i 0) (s 0))
      (while (< i n) (setq s (+ s (neovm--bgo-probe i))) (setq i (1+ i)))
      s))
  (byte-compile 'neovm--bgo-probe)
  (byte-compile 'neovm--bgo-loop))"#,
    )
    .expect("defined");
    force_mode_for_test(Some(crate::emacs_core::jit::BgMode::Threaded));
    force_osr_for_test(true);
    force_deopt_for_test(false);
    let before = osr_transfer_count_for_test();
    let got = ev.eval_str("(neovm--bgo-loop 2000000)").expect("runs");
    assert_eq!(got, Value::make_int(2_000_000));
    assert_eq!(
        osr_transfer_count_for_test() - before,
        1,
        "the loop moved to native code"
    );
    let seen = ev.eval_str("(reverse neovm--bgo-seen)").expect("seen");
    assert_eq!(
        print_value(&seen),
        "((3 (t neovm--bgo-loop 2000000)) (1999997 (t neovm--bgo-loop 2000000)))",
        "the same caller frame interpreted and native"
    );
    force_osr_for_test(false);
    force_mode_for_test(None);
}
