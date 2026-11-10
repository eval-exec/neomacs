//! GNU-observable rows for inlining a byte-compiled callee.
//!
//! Each expectation is the outcome printed by GNU Emacs 31.1
//! (`a360712c9d272d950d8d8255ef74570f7e90b7d9`) for the same form, probed
//! 2026-11-10 with `emacs -Q --batch -l` under `lexical-binding: t`.
//!
//! GNU charges the frame from the bytecode object's own depth at
//! `setup_frame` (`src/bytecode.c:510-515`) against the current stack
//! position, and `Bcall` counts the call in `lisp_eval_depth`
//! (`src/bytecode.c:783-788`). `make-byte-code` stores the depth and does
//! not check it (`src/alloc.c:3539`). `BC_STACK_SIZE` is
//! `512 * 1024 * sizeof (Lisp_Object)` (`src/bytecode.c:337`), so a depth
//! of 600000 does not fit in one frame. A depth of 200000 does fit alone
//! and still overflows on top of a caller frame of 400000.
//!
//! The inliner refuses only a callee whose frame does not fit at position
//! 0. It does not add that callee's `max_stack` to the caller's frame and
//! it does not repeat the positional compare or the `lisp_eval_depth`
//! increment at the splice. Rows (a) and (b) are that refusal. The
//! positional case and the nesting case are the next member, not rows of
//! this commit. Row (c) is the `debug-on-error` backtrace of a warmed call.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Every function compiled at its first call. GNU ignores the variable.
const JIT_ENV: &[(&str, &str)] = &[("NEOVM_JIT_THRESHOLD", "1")];

/// (a) After a compiled caller has been calling a small callee, `defalias`
/// of that callee to the same code at depth 600000 makes the next call
/// signal GNU's bytecode-stack error.
#[test]
fn oracle_eieio_redefined_max_stack_refuses_at_the_new_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neo-eieio-redef (byte-compile (lambda (x) x)))
  (defalias 'neo-eieio-redef-call (byte-compile (lambda (x) (neo-eieio-redef x))))
  (dotimes (_ 8) (neo-eieio-redef-call 3))
  (let* ((fn (symbol-function 'neo-eieio-redef))
         (huge (make-byte-code (aref fn 0) (aref fn 1) (aref fn 2) 600000)))
    (defalias 'neo-eieio-redef huge)
    (condition-case err (neo-eieio-redef-call 3) (error err))))"#;
    let expect = expect_test::expect![[r#""OK (error \"Bytecode stack overflow\")""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// (b) A frame whose declared depth does not fit signals
/// `(error "Bytecode stack overflow")`. The same code at depth 8 returns
/// its argument, so the refusal is the depth, not the body.
#[test]
fn oracle_eieio_armed_byte_limit_refuses_with_gnu_overflow_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((small (byte-compile (lambda (x) x)))
       (huge (make-byte-code (aref small 0) (aref small 1) (aref small 2) 600000))
       (fit (make-byte-code (aref small 0) (aref small 1) (aref small 2) 8)))
  (defalias 'neo-eieio-limit huge)
  (defalias 'neo-eieio-limit-call (byte-compile (lambda (x) (neo-eieio-limit x))))
  (list (funcall fit 4)
        (condition-case err (neo-eieio-limit-call 4) (error err))))"#;
    let expect = expect_test::expect![[r#""OK (4 (error \"Bytecode stack overflow\"))""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// (c) `debug-on-error` on a warmed compiled call records the callee and
/// caller symbols with the call's argument. `condition-case-unless-debug`
/// is what lets the debugger run inside the harness's own `error` handler.
#[test]
fn oracle_eieio_debug_on_error_backtrace_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neo-eieio-dbg
    (byte-compile (lambda (x) (if x (signal 'error (list "shim-boom")) x))))
  (defalias 'neo-eieio-caller (byte-compile (lambda (x) (neo-eieio-dbg x))))
  (dotimes (_ 4) (neo-eieio-caller nil))
  (setq debug-on-error t)
  (setq debugger
        (lambda (&rest args)
          (let (fs)
            (mapbacktrace
             (lambda (_evald f a _flags)
               (when (memq f '(neo-eieio-dbg neo-eieio-caller))
                 (push (list f a) fs))))
            (throw 'neo-eieio-got (list args (nreverse fs))))))
  (catch 'neo-eieio-got
    (condition-case-unless-debug err
        (neo-eieio-caller t)
      (error (list :caught err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error (error \"shim-boom\")) ((neo-eieio-dbg (t)) (neo-eieio-caller (t))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}
