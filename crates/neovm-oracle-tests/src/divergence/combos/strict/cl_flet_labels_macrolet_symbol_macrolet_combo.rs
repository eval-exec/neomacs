//! Strict combo oracle probes, batch 286: cl lexical binding macros. cl-flet
//! (local non-recursive), cl-labels (local recursive), cl-macrolet (local
//! macros), and cl-symbol-macrolet (symbol macros).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_flet_labels_local_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-flet ((double (x) (* x 2)))
         (double 5))
      (cl-labels ((fact (n) (if (= n 0) 1 (* n (fact (1- n))))))
        (fact 5))
      (cl-flet ((add (a b) (+ a b)))
        (cl-flet ((add2 (a) (add a 2)))
          (add2 5)))
      (cl-flet ((f (x) (cl-flet ((g (y) (* y y))) (g x))))
        (f 4)))
"##;
    let expect = expect_test::expect![[r#""OK (10 120 7 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_macrolet_symbol_macrolet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-macrolet ((when2 (c &rest body) `(if ,c (progn ,@body))))
         (when2 t 'yes 'extra))
      (cl-macrolet ((unless2 (c &rest body) `(if (not ,c) (progn ,@body))))
         (unless2 nil 'ran))
      (cl-symbol-macrolet ((x 42) (y (* 2 x)))
         (list x y))
      (cl-symbol-macrolet ((counter (progn (setq cs-counter (1+ cs-counter)) cs-counter)))
        (let ((cs-counter 0))
          (list counter counter counter))))
"##;
    let expect = expect_test::expect![[r#""OK (extra ran (42 84) (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_flet_shadowing_builtin_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-flet ((car (x) (cadr x)))
         (car '(1 2 3)))
      (car '(1 2 3))
      (cl-flet ((+ (a b) (- a b)))
         (+ 10 3))
      (+ 10 3)
      (let ((lst '(a b c)))
        (cl-flet ((first (l) (nth 1 l)))
          (first lst))))
"##;
    let expect = expect_test::expect![[r#""OK (2 1 7 13 b)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
