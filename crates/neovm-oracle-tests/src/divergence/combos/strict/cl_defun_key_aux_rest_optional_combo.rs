//! Strict combo oracle probes, batch 304: cl-defun full arglist. &optional,
//! &key with defaults, &rest, and &aux binding, plus help-function-arglist
//! extraction of the parsed signature.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_defun_key_aux_rest_optional() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(cl-defun probe-keyfn (a b &optional c &key d (e 10) &rest f &aux (g (+ a b)))
  (list a b c d e f g))
(list (probe-keyfn 1 2)
      (probe-keyfn 1 2 3 :d 4 :e 5)
      (probe-keyfn 1 2 :d 9 :extra 1 :more 2)
      (help-function-arglist 'probe-keyfn)
      (fboundp 'probe-keyfn))
"##;
    let expect = expect_test::expect![[
        r#""ERR (error \"Malformed argument list ends with: (&rest f &aux (g (+ a b)))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_defun_destructuring_key_allow_other_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(cl-defun probe-destr ((list a b) &key (mode 'default) &allow-other-keys)
  (list a b mode))
(list (probe-destr '(1 2))
      (probe-destr '(x y) :mode 'special)
      (probe-destr '(p q) :mode 'extra :unknown 99))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments probe-destr 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_flet_key_labels_recursive_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-flet ((f (a &key (b 1)) (+ a b)))
         (list (f 5) (f 5 :b 10)))
      (cl-labels ((fact (n &optional (acc 1)) (if (= n 0) acc (fact (1- n) (* acc n)))))
        (list (fact 5) (fact 5 1)))
      (cl-flet ((g (&rest args) (length args)))
         (list (g 1 2 3) (g))))
"##;
    let expect = expect_test::expect![[r#""OK ((6 15) (120 120) (3 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
