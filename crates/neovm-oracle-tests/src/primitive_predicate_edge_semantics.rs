//! Oracle parity tests for low-level GNU primitive predicate semantics.
//!
//! GNU implements these in `src/data.c`, `src/eval.c`, and `src/fns.c`.
//! They are simple predicates at the Elisp level, but they exercise distinct
//! object tags: records, closures, special symbols, booleans, and symbols with
//! source positions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_primitive_predicates_records_closures_booleans_and_not() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((rec (record 'neovm--oracle-record 'a 'b))
      (closure (lambda (x) (+ x 1))))
  (list
   (recordp rec)
   (recordp (vector 'neovm--oracle-record 'a 'b))
   (recordp '(neovm--oracle-record a b))
   (closurep closure)
   (closurep '(lambda (x) (+ x 1)))
   (closurep (symbol-function 'car))
   (booleanp t)
   (booleanp nil)
   (booleanp 0)
   (booleanp 't)
   (not nil)
   (not t)
   (not 0)
   (not "")))
"#;

    let expect = expect_test::expect![[r#""OK (t nil nil t nil nil t t nil t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_primitive_predicates_symbol_positions_and_specials() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--oracle-special-var 42)
  (unwind-protect
      (let* ((sp (position-symbol 'neovm--oracle-symbol 123))
             (sp2 (position-symbol sp 456)))
        (list
         (special-variable-p 'neovm--oracle-special-var)
         (special-variable-p 'neovm--ordinary-var)
         (condition-case err
             (special-variable-p "not-a-symbol")
           (error (list (car err) (cdr err))))
         (symbol-with-pos-p sp)
         (symbol-with-pos-p 'neovm--oracle-symbol)
         (bare-symbol-p sp)
         (bare-symbol-p 'neovm--oracle-symbol)
         (bare-symbol sp)
         (bare-symbol sp2)
         (symbol-with-pos-pos sp)
         (symbol-with-pos-pos sp2)
         (remove-pos-from-symbol sp)
         (remove-pos-from-symbol "not-a-symbol")
         (condition-case err
             (symbol-with-pos-pos 'neovm--oracle-symbol)
           (error (list (car err) (cdr err))))
         (condition-case err
             (position-symbol "not-a-symbol" 1)
           (error (list (car err) (cdr err))))
         (condition-case err
             (position-symbol 'neovm--oracle-symbol "not-a-position")
           (error (list (car err) (cdr err))))))
    (makunbound 'neovm--oracle-special-var)))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil (wrong-type-argument (symbolp \"not-a-symbol\")) t nil nil t neovm--oracle-symbol neovm--oracle-symbol 123 456 neovm--oracle-symbol \"not-a-symbol\" (wrong-type-argument (symbol-with-pos-p neovm--oracle-symbol)) (wrong-type-argument ((symbolp symbol-with-pos-p) \"not-a-symbol\")) (wrong-type-argument (fixnum-or-symbol-with-pos-p \"not-a-position\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
