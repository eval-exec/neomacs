//! GNU bytecode.c:795 records the callee and original arguments before
//! execution; eval.c:1974 runs signal-hook-function before unwinding them.
//! The wide callback collects before reading borrowed heap arguments.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gd_b_backtrace_raw_signal_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar neovm--gd-b-raw-seen nil)
  (defvar neovm--gd-b-raw-wide-seen nil)
  (defalias 'neovm--gd-b-raw-leaf
    (eval '(byte-compile (lambda (x) (length x))) t))
  (defalias 'neovm--gd-b-raw-caller
    (eval '(byte-compile (lambda (x) (neovm--gd-b-raw-leaf x))) t))
  (defalias 'neovm--gd-b-raw-wide-leaf
    (eval '(byte-compile (lambda (_a _b x) (length x))) t))
  (defalias 'neovm--gd-b-raw-wide-caller
    (eval '(byte-compile
            (lambda (x) (neovm--gd-b-raw-wide-leaf (list 'a) (concat "heap" "-b") x))) t))
  (dotimes (_ 6000)
    (neovm--gd-b-raw-caller '(a))
    (neovm--gd-b-raw-wide-caller '(a)))
  (list
   (let ((signal-hook-function
          (lambda (_kind _data)
            (setq neovm--gd-b-raw-seen
                  (backtrace-frame 0 'neovm--gd-b-raw-leaf)))))
     (condition-case nil (neovm--gd-b-raw-caller 5)
       (error neovm--gd-b-raw-seen)))
   (let ((signal-hook-function
          (lambda (_kind _data)
            (garbage-collect)
            (setq neovm--gd-b-raw-wide-seen
                  (backtrace-frame 0 'neovm--gd-b-raw-wide-leaf)))))
     (condition-case nil (neovm--gd-b-raw-wide-caller 5)
       (error neovm--gd-b-raw-wide-seen)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t neovm--gd-b-raw-leaf 5) (t neovm--gd-b-raw-wide-leaf (a) \"heap-b\" 5))""#
    ]];
    // Preserve the native call boundary; fusing this tiny callee exercises
    // inline deoptimization instead of the raw callee's signal exit.
    let modes: &[&[(&str, &str)]] = &[
        &[("NEOVM_JIT", "0"), ("NEOVM_AOT", "0")],
        &[
            ("NEOVM_JIT", "1"),
            ("NEOVM_AOT", "0"),
            ("NEOVM_JIT_INLINE", "off"),
            ("NEOVM_JIT_BG", "sync"),
        ],
        &[
            ("NEOVM_JIT", "1"),
            ("NEOVM_AOT", "0"),
            ("NEOVM_JIT_INLINE", "off"),
            ("NEOVM_JIT_THRESHOLD", "1"),
            ("NEOVM_JIT_BG", "sync"),
        ],
    ];
    crate::common::assert_oracle_parity_under_envs_expect(form, modes, expect);
}
