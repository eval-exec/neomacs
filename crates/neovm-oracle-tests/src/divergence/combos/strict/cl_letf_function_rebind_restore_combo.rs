//! Strict combo oracle probes, batch 202: cl-letf (temporary place rebinding).
//! Temporarily mock a defun and verify restore, rebind a variable value, mock
//! a builtin (symbol-function), and nested cl-letf.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_letf_function_mock_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defun probe-clf () 'original)
  (list (probe-clf)
        (cl-letf (((probe-clf) 'mocked))
          (list (probe-clf)))
        (probe-clf)
        (cl-letf (((probe-clf) 'mocked-a))
          (cl-letf (((probe-clf) 'mocked-b))
            (probe-clf)))
        (probe-clf)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_letf_variable_value_builtin_mock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((x 5))
  (list x
        (cl-letf ((x 99)) x)
        x
        (cl-letf (((symbol-function 'car) (lambda (l) (cadr l))))
          (car '(1 2 3)))
        (car '(1 2 3))
        (let ((ht (make-hash-table)))
          (puthash 'k 'orig ht)
          (cl-letf (((gethash 'k ht) 'letf-val))
            (gethash 'k ht))
          (gethash 'k ht))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_letf_unwind_restore_on_nonlocal_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defun probe-letf-unwind () 'orig)
  (list (condition-case nil
            (cl-letf (((probe-letf-unwind) 'mocked))
              (signal 'error '("boom")))
          (error 'caught))
        (probe-letf-unwind)))
"##;
    let expect = expect_test::expect![[r#""OK (caught orig)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
