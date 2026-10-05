//! GNU signal-time dynamic extent and reserved evaluator depth.
//! Expectations are populated by GNU 31.1 with UPDATE_EXPECT=1.

fn check(form: &str, expected: expect_test::Expect) {
    crate::common::assert_oracle_parity_under_envs_expect(
        form,
        &[
            &[("NEOVM_JIT", "0"), ("NEOVM_TIER_I", "off")],
            &[("NEOVM_JIT", "1")],
            &[
                ("NEOVM_JIT", "1"),
                ("NEOVM_JIT_THRESHOLD", "1"),
                ("NEOVM_AOT", "0"),
                ("NEOVM_JIT_BG", "sync"),
            ],
        ],
        expected,
    );
}

#[test]
fn compiled_signal_observers_see_live_dynamic_bindings() {
    check(
        r#"(progn
  (defvar gd-b-probe 'outer)
  (defvar gd-b-seen nil)
  (defalias 'gd-b-bound-leaf
    (byte-compile
     (lambda (x) (let ((gd-b-probe 'inner)) (length x)))))
  (dotimes (_ 1000) (gd-b-bound-leaf '(a)))
  (let ((signal-hook-function
         (lambda (_kind _data) (push (list 'hook gd-b-probe) gd-b-seen))))
    (condition-case err
        (handler-bind
         ((wrong-type-argument
           (lambda (_error) (push (list 'handler gd-b-probe) gd-b-seen))))
         (gd-b-bound-leaf 5))
      (error (list (car err) (nreverse gd-b-seen) gd-b-probe)))))"#,
        expect_test::expect![[
            r#""OK (wrong-type-argument ((hook inner) (handler inner)) outer)""#
        ]],
    );
}

#[test]
fn compiled_signal_uses_leaf_debugger_binding() {
    check(
        r#"(progn
  (defvar gd-b-probe 'outer)
  (defvar gd-b-seen nil)
  (defalias 'gd-b-debug-leaf
    (byte-compile
     (lambda (x)
       (let ((gd-b-probe 'inner) (debug-on-error t)) (length x)))))
  (dotimes (_ 1000) (gd-b-debug-leaf '(a)))
  (let ((debugger (lambda (&rest args) (setq gd-b-seen (list gd-b-probe args))))
        (debug-ignored-errors nil))
    (condition-case err (gd-b-debug-leaf 5)
      ((debug error) (list (car err) gd-b-seen gd-b-probe)))))"#,
        expect_test::expect![[
            r#""OK (wrong-type-argument (inner (error (wrong-type-argument sequencep 5))) outer)""#
        ]],
    );
}
