//! Oracle parity tests for GNU `function-equal` semantics.
//!
//! GNU implements `Ffunction_equal` in `src/profiler.c`: identical objects are
//! equal, closures compare by shared closure code object, and unrelated
//! functions are not treated as equal.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_function_equal_closure_and_symbol_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((maker (lambda (y) (lambda (x) (+ x y))))
       (a (funcall maker 1))
       (b (funcall maker 2))
       (same a)
       (different-source (lambda (x) (+ x 1))))
  (list
   (function-equal a b)
   (function-equal a same)
   (function-equal a different-source)
   (function-equal 'car 'car)
   (function-equal 'car 'cdr)
   (function-equal '(lambda (x) x) '(lambda (x) x))))
"#;

    let expect = expect_test::expect![[r#""OK (nil t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
