//! Strict combo oracle probes, batch 114: print-circle with complex shared
//! structure, print-gensym (uninterned symbol printing), read-circle
//! backreferences (#N= and #N#), and cl-assert/cl-check-type/cl-etypecase.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s8_print_circle_complex_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let* ((shared (list 1 2))
       (tree (list shared shared shared)))
  (list (let ((print-circle nil)) (prin1-to-string tree))
        (let ((print-circle t)) (prin1-to-string tree))))
"####,
    );
}

#[test]
fn div_s8_print_gensym_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((s (make-symbol "probe-gensym")))
  (list (let ((print-gensym t)) (prin1-to-string s))
        (let ((print-gensym nil)) (prin1-to-string s))
        (let ((print-circle t) (print-gensym t)) (prin1-to-string (list s s)))))
"####,
    );
}

#[test]
fn div_s8_read_circle_backreferences() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (read "#1=(a b . #1#)")
      (read "(#1=(x y) #1#)")
      (read "#1=[1 2 #1#]")
      (let ((obj (read "#1=(a . b) (test . #1#)")))
        obj))
"####,
    );
}

#[test]
fn div_s8_cl_assert_check_type_etypecase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (condition-case err (cl-assert t) (error (car err)))
      (condition-case err (cl-assert nil nil "probe-fail") (error (cadr err)))
      (condition-case err (cl-check-type 42 integer) (error (car err)))
      (condition-case err (cl-check-type "x" integer) (error (car err)))
      (cl-typecase 42
        (integer 'int)
        (string 'str))
      (cl-etypecase 42
        (integer 'int)
        (string 'str))
      (cl-etypecase "x"
        (integer 'int)
        (string 'str)))
"####,
    );
}

#[test]
fn div_s8_backquote_nested_splicing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((items '(a b c))
      (more '(d e)))
  (list `(start ,@items ,@more end)
        `(1 ,@(mapcar #'1+ '(1 2 3)) 4)
        `(outer ,`(inner ,(+ 1 2)))
        `(head ,@items tail ,@more)
        `(deep ,@(list 1 `(nested ,@more) 3))))
"####,
    );
}
