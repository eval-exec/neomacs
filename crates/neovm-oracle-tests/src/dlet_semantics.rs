//! Oracle parity tests for GNU `dlet` dynamic binding semantics.
//!
//! GNU implements `dlet` in `lisp/subr.el` as a macro that locally emits
//! `defvar` declarations before an ordinary `let`.  Under lexical binding this
//! temporarily makes the listed variables special, so lambdas created inside
//! the `dlet` body read the dynamic binding while pre-existing lexical closures
//! keep their lexical captures.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_dlet_macroexpansion_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (macroexpand '(dlet ((neovm--dl-x 1)
                      neovm--dl-y)
                 (list neovm--dl-x neovm--dl-y)))
 (macroexpand '(dlet (neovm--dl-z)
                 neovm--dl-z)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((let (_) (defvar neovm--dl-x) (defvar neovm--dl-y) (let ((neovm--dl-x 1) neovm--dl-y) (list neovm--dl-x neovm--dl-y))) (let (_) (defvar neovm--dl-z) (let (neovm--dl-z) neovm--dl-z)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_dlet_temporarily_makes_lexical_variables_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((x 'outer)
      (after nil))
  (let ((reader (lambda () x)))
    (list
     (funcall reader)
     (dlet ((x 'dynamic))
       (let ((inner-reader (lambda () x)))
         (list (symbol-value 'x)
               (funcall reader)
               (funcall inner-reader)
               (let ((x 'nested-lexical))
                 (list x (symbol-value 'x))))))
     (setq after (funcall (lambda () x)))
     after)))
"#;

    let expect = expect_test::expect![[
        r#""OK (outer (dynamic outer outer (outer nested-lexical)) outer outer)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_dlet_omitted_initializers_and_unwind_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((x 'outer)
      (events nil))
  (list
   (dlet (x)
     (list x (symbol-value 'x)))
   (condition-case err
       (dlet ((x 'dynamic))
         (push (list 'before x (symbol-value 'x)) events)
         (error "boom"))
     (error (list (car err) x (symbol-value 'x) events)))
   (funcall (lambda () x))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
