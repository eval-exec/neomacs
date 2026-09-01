//! Oracle parity tests for GNU `subr.el` primitive function predicates.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_subr_primitive_and_primitive_function_p_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:subr-primitive-p accepts primitive function and special-form
    // subrs, while primitive-function-p excludes unevalled special forms.
    let form = r#"(mapcar
 (lambda (entry)
   (let ((label (car entry))
         (obj (cdr entry)))
     (list label
           (subr-primitive-p obj)
           (primitive-function-p obj)
           (subrp obj))))
 (list (cons '+-function (symbol-function '+))
       (cons 'if-special-form (symbol-function 'if))
       (cons 'lambda-macro (symbol-function 'lambda))
       (cons 'lambda-closure (lambda (x) x))
       (cons '+-symbol '+)
       (cons 'if-symbol 'if)
       (cons 'nil-object nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+-function t t t) (if-special-form t nil t) (lambda-macro nil nil nil) (lambda-closure nil nil nil) (+-symbol nil nil nil) (if-symbol nil nil nil) (nil-object nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
