//! GNU Bcall records the called function and original arguments before running
//! the callee (src/bytecode.c:795-814). Signal handlers inspect those records
//! before signal_or_quit unwinds them (src/eval.c:2016-2066).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const FORM: &str = r#"
(progn
  (defalias 'delete-dups (byte-compile (symbol-function 'delete-dups)))
  (defalias 'neovm--gd-b-frame-caller (byte-compile (lambda (x) (delete-dups x))))
  (dotimes (_ 64) (neovm--gd-b-frame-caller (list 1 2 1)))
  (let ((frames nil) (base nil))
    (list
     (condition-case err
         (handler-bind
             ((wrong-type-argument
               (lambda (_err)
                 (mapbacktrace
                  (lambda (_evaluated function arguments _flags)
                    (when (memq function '(delete-dups neovm--gd-b-frame-caller))
                      (push (list function arguments) frames))))
                 (setq base (backtrace-frame 0 'delete-dups)))))
           (neovm--gd-b-frame-caller 5))
       (error err))
     (nreverse frames) base)))
"#;

#[test]
fn oracle_gd_b_backtrace_delete_dups_default_jit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument sequencep 5) ((delete-dups (5)) (neovm--gd-b-frame-caller (5))) (t delete-dups 5))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(
        FORM,
        &[("NEOVM_AOT", "0"), ("NEOVM_JIT", "1")],
        expect,
    );
}

#[test]
fn oracle_gd_b_backtrace_delete_dups_threshold_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument sequencep 5) ((delete-dups (5)) (neovm--gd-b-frame-caller (5))) (t delete-dups 5))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(
        FORM,
        &[
            ("NEOVM_AOT", "0"),
            ("NEOVM_JIT", "1"),
            ("NEOVM_JIT_THRESHOLD", "1"),
            ("NEOVM_JIT_BG", "sync"),
        ],
        expect,
    );
}

#[test]
fn oracle_gd_b_backtrace_delete_dups_vm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument sequencep 5) ((delete-dups (5)) (neovm--gd-b-frame-caller (5))) (t delete-dups 5))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(FORM, &[("NEOVM_JIT", "0")], expect);
}
