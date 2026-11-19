//! Oracle parity tests for GNU `subr.el` cXXr accessor contracts.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_cxxr_nested_car_cdr_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el defines each cXXr helper as the corresponding nested
    // `car`/`cdr` chain, so nil and dotted-tail behavior must fall out of
    // those primitive operations exactly.
    let form = r#"(let ((x '((((aaaa . aaad) . (aada . aadd))
            . ((adaa . adad) . (adda . addd)))
           . (((daaa . daad) . (dada . dadd))
              . ((ddaa . ddad) . (ddda . dddd))))))
  (list
   (mapcar (lambda (fn) (list fn (funcall fn x)))
           '(caar cadr cdar cddr
             caaar caadr cadar caddr cdaar cdadr cddar cdddr
             caaaar caaadr caadar caaddr cadaar cadadr caddar cadddr
             cdaaar cdaadr cdadar cdaddr cddaar cddadr cdddar cddddr))
   (mapcar (lambda (expr)
             (condition-case e (eval expr t)
               (error (list 'error (car e)))))
           '((caar nil)
             (cadr nil)
             (cddr '(a . b))
             (cadr '(a . b))
             (caar 42)
             (caddr '(a . b))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((caar ((aaaa . aaad) aada . aadd)) (cadr ((daaa . daad) dada . dadd)) (cdar ((adaa . adad) adda . addd)) (cddr ((ddaa . ddad) ddda . dddd)) (caaar (aaaa . aaad)) (caadr (daaa . daad)) (cadar (adaa . adad)) (caddr (ddaa . ddad)) (cdaar (aada . aadd)) (cdadr (dada . dadd)) (cddar (adda . addd)) (cdddr (ddda . dddd)) (caaaar aaaa) (caaadr daaa) (caadar adaa) (caaddr ddaa) (cadaar aada) (cadadr dada) (caddar adda) (cadddr ddda) (cdaaar aaad) (cdaadr daad) (cdadar adad) (cdaddr ddad) (cddaar aadd) (cddadr dadd) (cdddar addd) (cddddr dddd)) (nil nil (error wrong-type-argument) (error wrong-type-argument) (error wrong-type-argument) (error wrong-type-argument)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
