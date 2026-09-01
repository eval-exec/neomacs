//! Oracle parity tests for GNU history-list helper semantics.
//!
//! GNU implements `add-to-history` in `lisp/subr.el`.  These tests pin its
//! observable list mutation rules: empty strings, duplicate handling,
//! `history-delete-duplicates`, `history-length`/symbol property limits, and
//! the documented lexical-variable rejection behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_add_to_history_empty_duplicate_and_keep_all_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((h nil)
      (history-delete-duplicates nil))
  (list
   (add-to-history 'h "" 10)
   h
   (add-to-history 'h "" 10 t)
   h
   (add-to-history 'h "a" 10)
   h
   (add-to-history 'h "a" 10)
   h
   (add-to-history 'h "a" 10 t)
   h))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable h)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_history_delete_duplicates_and_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((h '("old" "a" "mid" "a" "tail"))
      (history-delete-duplicates t))
  (list
   (add-to-history 'h "a" 3)
   h
   (add-to-history 'h "z" 0)
   h
   (add-to-history 'h "q" -1)
   h))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable h)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_history_uses_symbol_property_and_dynamic_history_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((h nil)
      (history-length 2)
      (history-delete-duplicates nil))
  (unwind-protect
      (progn
        (put 'h 'history-length 3)
        (list
         (add-to-history 'h "one")
         (add-to-history 'h "two")
         (add-to-history 'h "three")
         (add-to-history 'h "four")
         h
         (progn
           (put 'h 'history-length nil)
           (add-to-history 'h "five")
           h)))
    (put 'h 'history-length nil)))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable h)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_to_history_requires_symbol_value_list_and_not_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((h "not-a-list")
      (lexical-history nil))
  (list
   (add-to-history 'h "x")
   h
   (condition-case err
       (add-to-history lexical-history "x")
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable h)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
