//! Oracle parity tests for GNU `add-to-list` semantics.
//!
//! GNU implements `add-to-list` in `lisp/subr.el`.  Its runtime contract is
//! intentionally simple but user-visible: default membership uses `equal`,
//! explicit `eq`/`eql` comparison selects the matching primitive semantics,
//! custom predicates are called as `(COMPARE-FN ELEMENT EXISTING)`, APPEND
//! preserves the old list prefix, and LIST-VAR is resolved as a dynamic symbol
//! value rather than a lexical binding.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_add_to_list_default_uses_equal_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs nil)
      (first (copy-sequence "same"))
      (second (copy-sequence "same")))
  (list
   (add-to-list 'xs first)
   xs
   (add-to-list 'xs second)
   xs
   (length xs)
   (eq (car xs) first)
   (eq (car xs) second)
   (equal first second)))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_list_append_adds_at_end_only_when_absent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs '(a b)))
  (list
   (add-to-list 'xs 'c 'append)
   xs
   (add-to-list 'xs 'b 'append)
   xs
   (add-to-list 'xs 'head nil)
   xs))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_list_compare_function_selects_membership_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((eq-xs nil)
      (eql-xs nil)
      (a (copy-sequence "same"))
      (b (copy-sequence "same")))
  (add-to-list 'eq-xs a nil #'eq)
  (add-to-list 'eq-xs b nil #'eq)
  (add-to-list 'eql-xs 1.0 nil #'eql)
  (add-to-list 'eql-xs 1.0 nil #'eql)
  (add-to-list 'eql-xs 1 nil #'eql)
  (list
   eq-xs
   (length eq-xs)
   (eq (car eq-xs) b)
   (eq (cadr eq-xs) a)
   eql-xs
   (length eql-xs)))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable eq-xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_list_custom_compare_argument_order_and_short_circuit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs '((a . 1) (b . 2) (c . 3)))
      (calls nil))
  (list
   (add-to-list
    'xs
    '(b . 99)
    nil
    (lambda (element existing)
      (push (list element existing) calls)
      (eq (car element) (car existing))))
   xs
   (nreverse calls)
   (setq calls nil)
   (add-to-list
    'xs
    '(d . 4)
    'append
    (lambda (element existing)
      (push (list element existing) calls)
      (eq (car element) (car existing))))
   xs
   (nreverse calls)))
"#;

    let expect = expect_test::expect![r#""ERR (void-variable xs)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_list_resolves_symbol_value_not_lexical_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (let ((xs nil))
       (add-to-list 'xs 'value))
   (error (list (car err) (cdr err))))
 (progn
   (defvar neomacs--oracle-add-to-list-dynamic nil)
   (unwind-protect
       (let ((neomacs--oracle-add-to-list-dynamic '(old)))
         (list
          (add-to-list 'neomacs--oracle-add-to-list-dynamic 'new)
          neomacs--oracle-add-to-list-dynamic))
     (makunbound 'neomacs--oracle-add-to-list-dynamic))))
"#;

    let expect = expect_test::expect![r#""OK ((void-variable (xs)) ((new old) (new old)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
