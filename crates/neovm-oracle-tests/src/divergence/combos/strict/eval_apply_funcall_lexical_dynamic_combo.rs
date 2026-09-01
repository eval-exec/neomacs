//! Strict combo oracle probes, batch 306: eval / apply / funcall deep. apply
//! with leading + trailing args, funcall on closures and symbol-function, eval
//! lexical vs dynamic, and apply-partially.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_apply_funcall_trailing_leading_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (apply #'+ 1 2 '(3 4))
      (apply #'max '(1 5 3 2))
      (apply #'+ '(1 2 3))
      (apply #'+ 0 nil)
      (funcall (lambda (x) (* x 2)) 5)
      (funcall (lambda (&rest args) (length args)) 1 2 3 4)
      (funcall (symbol-function 'car) '(a b c))
      (funcall '+ 1 2 3))
"##;
    let expect = expect_test::expect![[r#""OK (10 5 6 0 10 4 a 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_eval_lexical_dynamic_apply_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (eval '(+ 1 2) t)
      (eval '(+ 1 2) nil)
      (eval (list '* 3 4))
      (let ((x 5))
        (eval 'x))   ;; dynamic binding (let is dynamic) -> 5
      (funcall (apply-partially #'+ 10) 5)
      (funcall (apply-partially #'list 'a 'b) 'c)
      (mapcar (apply-partially #'+ 100) '(1 2 3)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_funcall_macro_special_form_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (condition-case err (funcall 'if t 'a) (error 'caught-if))
      (condition-case err (funcall 'and 1 2) (error 'caught-and))
      (condition-case err (funcall 'setq x 1) (error 'caught-setq))
      (condition-case err (funcall 'let) (error 'caught-let))
      (condition-case err (apply 'quote '(x)) (error 'caught-quote))
      (functionp 'car)
      (functionp (lambda () nil)))
"##;
    let expect = expect_test::expect![[
        r#""OK (caught-if caught-and caught-setq caught-let caught-quote t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
