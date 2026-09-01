//! Oracle parity tests for GNU `subr.el` `define-error` contracts.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_define_error_parent_and_property_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el builds `error-conditions` from parent conditions with
    // `delete-dups`, only sets `error-message` when MESSAGE is non-nil, and
    // only rejects an unknown parent when PARENT is a list of signals.
    let form = r#"(let ((syms '(neovm-define-error-parent-a
              neovm-define-error-parent-b
              neovm-define-error-child
              neovm-define-error-nil-message
              neovm-define-error-unknown-symbol
              neovm-define-error-unknown-list)))
  (unwind-protect
      (progn
        (define-error 'neovm-define-error-parent-a "A")
        (define-error 'neovm-define-error-parent-b "B")
        (define-error 'neovm-define-error-child
          "Child"
          '(neovm-define-error-parent-a neovm-define-error-parent-b))
        (define-error 'neovm-define-error-nil-message
          nil
          'neovm-define-error-parent-a)
        (define-error 'neovm-define-error-unknown-symbol
          "Unknown symbol parent"
          'neovm-define-error-missing-symbol-parent)
        (list
         (get 'neovm-define-error-parent-a 'error-conditions)
         (get 'neovm-define-error-parent-a 'error-message)
         (get 'neovm-define-error-child 'error-conditions)
         (get 'neovm-define-error-child 'error-message)
         (get 'neovm-define-error-nil-message 'error-conditions)
         (get 'neovm-define-error-nil-message 'error-message)
         (get 'neovm-define-error-unknown-symbol 'error-conditions)
         (get 'neovm-define-error-unknown-symbol 'error-message)
         (condition-case e (signal 'neovm-define-error-child '(payload))
           (neovm-define-error-parent-a (list 'caught-parent-a e))
           (neovm-define-error-parent-b (list 'caught-parent-b e))
           (error (list 'caught-error e)))
         (condition-case e
             (define-error 'neovm-define-error-unknown-list
               "Unknown list parent"
               '(neovm-define-error-missing-list-parent))
           (error (list 'unknown-list-parent-error
                        (car e)
                        (cadr e))))))
    (dolist (s syms)
      (put s 'error-conditions nil)
      (put s 'error-message nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((neovm-define-error-parent-a error) \"A\" (neovm-define-error-child neovm-define-error-parent-a error neovm-define-error-parent-b) \"Child\" (neovm-define-error-nil-message neovm-define-error-parent-a error) nil (neovm-define-error-unknown-symbol neovm-define-error-missing-symbol-parent) \"Unknown symbol parent\" (caught-parent-a (neovm-define-error-child payload)) (unknown-list-parent-error error \"Unknown signal ‘neovm-define-error-missing-list-parent’\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
