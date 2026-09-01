//! Strict combo oracle probes, batch 305: cl-locally / cl-remf / cl-symbol-
//! macrolet / cl-multiple-value-setq deep.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_locally_destructuring_return_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-locally (defun probe-loc () 'local) (probe-loc))
      (cl-multiple-value-setq (a b c) (values 1 2 3))
      (list a b c)
      (cl-destructuring-bind (x y &rest z) '(1 2 3 4)
        (list x y z))
      (cl-multiple-value-bind (q r) (values 10 20)
        (list q r)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function values)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_remf_remprop_getf_symbol_macrolet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((p (copy-list '(a 1 b 2 c 3))))
  (list (cl-getf p 'b)
        (cl-remf p 'b)
        p
        (cl-getf p 'b 'gone)
        (cl-symbol-macrolet ((slot (cl-getf p 'c)))
          (setf slot 99)
          p)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function copy-list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_the_check_type_assert_with_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-the integer 5)
      (cl-the string "hi")
      (condition-case err (cl-the integer "not-int") (wrong-type-argument (cons 'caught (cadr err))))
      (let ((n 5)) (cl-check-type n integer) 'ok)
      (cl-assert (= 2 2) t "probe-assert")
      'done)
"##;
    let expect = expect_test::expect![[r#""OK (5 \"hi\" (caught . integer) ok nil done)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
