//! Oracle parity tests for GNU `add-to-ordered-list` semantics.
//!
//! GNU implements this helper in `lisp/subr.el`.  The observable contract is
//! small but subtle: membership is by `eq`, ordering lives in a weak `eq` hash
//! table stored on the list symbol's `list-order` property, and non-numeric
//! ORDER removes an element's numeric priority.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_add_to_ordered_list_orders_by_symbol_property_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs nil))
  (unwind-protect
      (list
       (add-to-ordered-list 'xs 'middle 50)
       xs
       (add-to-ordered-list 'xs 'first 10)
       xs
       (add-to-ordered-list 'xs 'last 90)
       xs
       (hash-table-p (get 'xs 'list-order))
       (gethash 'first (get 'xs 'list-order))
       (gethash 'middle (get 'xs 'list-order))
       (gethash 'last (get 'xs 'list-order)))
    (put 'xs 'list-order nil)))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_ordered_list_nil_and_nonnumeric_order_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs nil))
  (unwind-protect
      (list
       (add-to-ordered-list 'xs 'unordered)
       xs
       (add-to-ordered-list 'xs 'ordered 1.5)
       xs
       (add-to-ordered-list 'xs 'tail 4)
       xs
       (add-to-ordered-list 'xs 'tail 'remove-order)
       xs
       (gethash 'tail (get 'xs 'list-order) :missing)
       (add-to-ordered-list 'xs 'tail 0)
       xs
       (gethash 'tail (get 'xs 'list-order) :missing))
    (put 'xs 'list-order nil)))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_ordered_list_membership_is_eq_not_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs nil)
      (a (copy-sequence "same"))
      (b (copy-sequence "same")))
  (unwind-protect
      (progn
        (add-to-ordered-list 'xs a 2)
        (add-to-ordered-list 'xs a 1)
        (add-to-ordered-list 'xs b 0)
        (list
         (length xs)
         (eq (car xs) b)
         (eq (cadr xs) a)
         (equal (car xs) (cadr xs))
         (gethash a (get 'xs 'list-order))
         (gethash b (get 'xs 'list-order))))
    (put 'xs 'list-order nil)))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_ordered_list_requires_symbol_value_and_not_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs nil))
  (list
   (condition-case err
       (add-to-ordered-list xs 'value 1)
     (error (list (car err) (cdr err))))
   (condition-case err
       (add-to-ordered-list 'missing-list-var 'value 1)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![
        r#""OK ((setting-constant (nil)) (void-variable (missing-list-var)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
