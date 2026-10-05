//! GNU eval.c:1676-1685 and bytecomp.el:4990-5000 bind condition-case VAR
//! lexically whenever a lexical environment exists, including special VARs.
//! Refresh expectations from GNU 31.1 with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const ENGINE_ENVS: &[&[(&str, &str)]] = &[
    &[("NEOVM_JIT", "0"), ("NEOVM_TIER_I", "off")],
    &[],
    &[
        ("NEOVM_JIT", "1"),
        ("NEOVM_JIT_THRESHOLD", "1"),
        ("NEOVM_AOT", "0"),
        ("NEOVM_JIT_BG", "sync"),
    ],
];

#[test]
fn oracle_condition_case_special_error_var_is_lexical_and_captured() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b5-special 'global)
  (eval
   '(let ((reader
           (condition-case gd-b5-special (signal 'error '(bad))
             (error
              (setq gd-b5-special (list 'caught (car gd-b5-special)))
              (lambda () (list gd-b5-special (symbol-value 'gd-b5-special)))))))
      (garbage-collect)
      (list (funcall reader) gd-b5-special))
   t))"#;
    let expected = expect_test::expect![[r#""OK (((caught error) global) global)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}

#[test]
fn oracle_condition_case_special_success_var_preserves_outer_dynamic_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b5-special 'global)
  (eval
   '(let ((gd-b5-special 'outer))
      (list
       (condition-case gd-b5-special 42
         (:success
          (list gd-b5-special (symbol-value 'gd-b5-special)
                (funcall (lambda () gd-b5-special)))))
       gd-b5-special))
   t))"#;
    let expected = expect_test::expect![[r#""OK ((42 outer 42) outer)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}

#[test]
fn oracle_condition_case_locally_special_var_is_lexical_in_both_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (set 'gd-b5-local 'global)
  (eval
   '(let ((gd-b5-local 'outer))
      (defvar gd-b5-local)
      (list
       (condition-case gd-b5-local (signal 'error '(bad))
         (error (list gd-b5-local (symbol-value 'gd-b5-local))))
       (condition-case gd-b5-local 42
         (:success (list gd-b5-local (symbol-value 'gd-b5-local))))
       gd-b5-local))
   t))"#;
    let expected = expect_test::expect![[r#""OK (((error bad) global) (42 global) outer)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}

#[test]
fn oracle_condition_case_dynamic_var_binds_dynamic_cell_temporarily() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b5-special 'global)
  (eval
   '(list
     (condition-case gd-b5-special (signal 'error '(bad))
       (error (list gd-b5-special (symbol-value 'gd-b5-special))))
     (condition-case gd-b5-special 42
       (:success (list gd-b5-special (symbol-value 'gd-b5-special))))
     gd-b5-special)
   nil))"#;
    let expected = expect_test::expect![[r#""OK (((error bad) (error bad)) (42 42) global)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}

#[test]
fn oracle_condition_case_compiled_special_var_matches_interpreted_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b5-special 'global)
  (let ((source
         '(lambda (value)
            (list
             (condition-case gd-b5-special (signal 'error (list value))
               (error (list gd-b5-special (symbol-value 'gd-b5-special))))
             (condition-case gd-b5-special value
               (:success (list gd-b5-special (symbol-value 'gd-b5-special))))
             gd-b5-special))))
    (let ((interpreted (eval source t))
          (compiled (eval `(byte-compile (function ,source)) t)))
      (list (funcall interpreted 42)
            (funcall compiled 42)
            (funcall compiled 43)))))"#;
    let expected = expect_test::expect![[
        r#""OK ((((error 42) global) (42 global) global) (((error 42) global) (42 global) global) (((error 43) global) (43 global) global))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}

#[test]
fn oracle_condition_case_special_var_restores_binding_after_handler_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b5-special 'global)
  (eval
   '(let ((gd-b5-special 'outer))
      (list
       (catch 'gd-b5-exit
         (condition-case gd-b5-special (signal 'error '(bad))
           (error
            (throw 'gd-b5-exit
                   (list gd-b5-special (symbol-value 'gd-b5-special))))))
       (catch 'gd-b5-exit
         (condition-case gd-b5-special 42
           (:success
            (throw 'gd-b5-exit
                   (list gd-b5-special (symbol-value 'gd-b5-special))))))
       gd-b5-special))
   t))"#;
    let expected = expect_test::expect![[r#""OK (((error bad) outer) (42 outer) outer)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}

#[test]
fn oracle_condition_case_compiled_special_var_closures_survive_both_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b5-special 'global)
  (defalias 'gd-b5-maker
    (eval '(byte-compile
            (lambda (fail)
              (condition-case gd-b5-special
                  (if fail (signal 'error '(bad)) 42)
                (error
                 (setq gd-b5-special (list 'caught (car gd-b5-special)))
                 (lambda ()
                   (list gd-b5-special (symbol-value 'gd-b5-special))))
                (:success
                 (lambda ()
                   (list gd-b5-special (symbol-value 'gd-b5-special)))))))
          t))
  (let ((error-reader (gd-b5-maker t))
        (success-reader (gd-b5-maker nil)))
    (garbage-collect)
    (list (funcall error-reader) (funcall success-reader) gd-b5-special)))"#;
    let expected = expect_test::expect![[r#""OK (((caught error) global) (42 global) global)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, ENGINE_ENVS, expected);
}
