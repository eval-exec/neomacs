//! Strict combo oracle probes, batch 241: cl type system. cl-typep with
//! compound types (and/or/satisfies/member), cl-typecase/cl-etypecase dispatch,
//! cl-check-type signal, and cl-assert with condition.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_typep_compound_and_or_satisfies_member() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-typep 5 'integer)
      (cl-typep "x" 'string)
      (cl-typep '(1 2) 'list)
      (cl-typep 5 '(or string integer))
      (cl-typep "x" '(or string integer))
      (cl-typep 3 '(member 1 2 3))
      (cl-typep 9 '(member 1 2 3))
      (cl-typep 5 '(satisfies cl-evenp))
      (cl-typep 4 '(satisfies cl-evenp))
      (cl-typep '(1 2) '(and list (satisfies (lambda (l) (> (length l) 0)))))))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 79)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_typecase_etypecase_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-typecase "x" (string 'str) (integer 'int) (t 'other))
      (cl-typecase 5 (string 'str) (integer 'int) (t 'other))
      (cl-typecase '(1 2) (string 'str) (list 'lst) (t 'other))
      (cl-typecase [1 2] (string 'str) (vector 'vec) (t 'other))
      (condition-case err
          (cl-etypecase 5 (string 'str))
        (cl-ecase-error-type 'caught-ecase)
        (wrong-number-of-arguments 'caught-wrong-arg)
        (error (cons 'caught (car err))))
      (cl-typecase nil (null 'null) (list 'list))))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 51)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_check_type_assert_signal_messages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((x "not-int"))
  (list (condition-case err (cl-check-type x integer) (wrong-type-argument (cons 'caught (car err))))
        (let ((n 5)) (cl-check-type n integer) 'ok)
        (condition-case err (cl-assert (= 1 2) t "probe-assert-failed") (error 'caught))
        (let ((n 0)) (cl-assert (= n 0)) 'assert-ok)
        (condition-case err (cl-assert nil) (error 'caught))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((caught . wrong-type-argument) ok caught assert-ok caught)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
