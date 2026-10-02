//! The AOT battery (P4.2 A7): GNU parity for forms that run loadup Lisp,
//! with Neomacs serving the dump-time preload (`NEOVM_AOT=1`) under a knob
//! matrix, and each run required to have served AOT leaves (`aot_loads > 0`
//! in its JIT statistics). Runs only with `NEOVM_ORACLE_AOT=1` against a
//! binary built with `cargo xtask fresh-build --aot-preload`; see
//! `common::assert_oracle_parity_under_aot_expect`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Required-only `subr.el` predicates and list/string helpers, the shape
/// the preload serves.
#[test]
fn oracle_aot_battery_subr_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (xor t nil) (xor 1 2) (ensure-list 3) (ensure-list '(3))
      (fixnump 5) (bignump (expt 2 70)) (booleanp nil) (zerop 0) (zerop 0.0)
      (remq 'a (list 'a 'b 'a 'c)) (remove 2 (list 1 2 3 2))
      (delete-dups (list 1 2 1 3)) (flatten-tree '(1 (2 (3 nil 4)) ((5))))
      (string-to-list "abc") (string-to-vector "ab") (copy-tree '(1 (2 3))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil (3) (3) t t t t t (b c) (1 3) (1 2 3) (1 2 3 4 5) (97 98 99) [97 98] (1 (2 3)))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// The same helpers called often enough to tier up under the JIT knobs, so
/// served AOT leaves and JIT leaves meet in one loop.
#[test]
fn oracle_aot_battery_hot_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((acc nil) (i 0))
  (while (< i 200)
    (push (list (xor (zerop (% i 3)) (zerop (% i 5)))
                (length (flatten-tree (list i (list i) nil))))
          acc)
    (setq i (1+ i)))
  (list (length acc) (car acc) (nth 199 acc)
        (length (delete-dups (mapcar #'car acc)))))
"#;

    let expect = expect_test::expect![[r#""OK (200 (nil 2) (nil 2) 2)""#]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// Signals raised inside served functions keep GNU's data.
#[test]
fn oracle_aot_battery_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (condition-case err (string-to-list 5) (error err))
      (condition-case err (flatten-tree 1) (error err))
      (condition-case err (zerop 'x) (error err)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument sequencep 5) (1) (wrong-type-argument number-or-marker-p x))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}
/// GNU Bcall reads the symbol's current function cell (src/bytecode.c:806);
/// Ffset replaces it (src/data.c:897). An old preload object remains callable
/// through an explicit saved reference after the name is redefined and GC runs.
#[test]
fn oracle_aot_battery_preloaded_function_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((original (symbol-function 'flatten-tree)))
  (unwind-protect
      (progn
        (dotimes (_ 64) (flatten-tree '(1 (2 3))))
        (let ((before (flatten-tree '(1 (2 . 3) nil 4))))
          (fset 'flatten-tree (lambda (tree) (list 'redefined tree)))
          (garbage-collect)
          (let ((after (flatten-tree '(1 (2 . 3) nil 4)))
                (saved (funcall original '(1 (2 . 3) nil 4))))
            (fset 'flatten-tree original)
            (list before after saved (flatten-tree '(1 (2 . 3) nil 4))))))
    (fset 'flatten-tree original)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4) (redefined (1 (2 . 3) nil 4)) (1 2 3 4) (1 2 3 4))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// GNU advice-add replaces the function cell with its wrapper (nadvice.el:509);
/// AOT must follow that definition and the restored preload object after remove.
#[test]
fn oracle_aot_battery_preloaded_function_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((original (symbol-function 'flatten-tree)) (calls 0))
  (let ((wrapper (lambda (function tree)
                   (setq calls (1+ calls))
                   (cons 'advised (funcall function tree)))))
    (unwind-protect
        (progn
          (dotimes (_ 64) (flatten-tree '(1 (2 3))))
          (let ((before (flatten-tree '(1 (2 3)))))
            (advice-add 'flatten-tree :around wrapper)
            (let ((during (flatten-tree '(1 (2 3))))
                  (installed (not (null (advice-member-p wrapper 'flatten-tree)))))
              (advice-remove 'flatten-tree wrapper)
              (list before during (flatten-tree '(1 (2 3))) calls installed
                    (not (null (advice-member-p wrapper 'flatten-tree)))
                    (eq (symbol-function 'flatten-tree) original)))))
      (advice-remove 'flatten-tree wrapper)
      (fset 'flatten-tree original))))
"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) (advised 1 2 3) (1 2 3) 1 t nil t)""#]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// GNU debug-on-entry is before advice at depth -100 (debug.el:689), which
/// calls the dynamically bound debugger with debug and a backtrace-base pair.
#[test]
fn oracle_aot_battery_preloaded_function_debug_on_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'debug)
  (let ((original (symbol-function 'flatten-tree)) (log nil))
    (let ((debugger (lambda (&rest arguments) (push arguments log) nil)))
      (unwind-protect
          (progn
            (dotimes (_ 64) (flatten-tree '(1 (2 3))))
            (let ((before (flatten-tree '(1 (2 3)))))
              (debug-on-entry 'flatten-tree)
              (let ((during (flatten-tree '(1 (2 3))))
                    (inhibited (let ((inhibit-debug-on-entry t))
                                 (flatten-tree '(1 (2 3))))))
                (cancel-debug-on-entry 'flatten-tree)
                (list before during inhibited (flatten-tree '(1 (2 3)))
                      (nreverse log)))))
        (cancel-debug-on-entry 'flatten-tree)
        (fset 'flatten-tree original)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (1 2 3) (1 2 3) (1 2 3) ((debug :backtrace-base (1 . debug--implement-debug-on-entry))))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// GNU records Bcall's called symbol and the call's original arguments before
/// executing it (src/bytecode.c:795). Inspect both the preload function's live
/// frame and its compiled caller while an error handler is running.
#[test]
fn oracle_aot_battery_preloaded_function_backtrace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defalias 'neovm--aot-frame-caller (byte-compile (lambda (x) (delete-dups x))))
  (dotimes (_ 64) (neovm--aot-frame-caller (list 1 2 1)))
  (let ((frames nil) (base nil))
    (list
     (condition-case err
         (handler-bind
             ((wrong-type-argument
               (lambda (_err)
                 (mapbacktrace
                  (lambda (_evaluated function arguments _flags)
                    (when (memq function '(delete-dups neovm--aot-frame-caller))
                      (push (list function arguments) frames))))
                 (setq base (backtrace-frame 0 'delete-dups)))))
           (neovm--aot-frame-caller 5))
       (error err))
     (nreverse frames) base)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument sequencep 5) ((delete-dups (5)) (neovm--aot-frame-caller (5))) (t delete-dups 5))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}
