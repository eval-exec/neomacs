//! Oracle parity tests for print/eval: `prin1-to-string`, `eval`,
//! `identity`, `number-to-string`, `string-to-number`.
//!
//! GNU src/print.c, src/eval.c, src/editfns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prin1_to_string_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(prin1-to-string 42)"#, expect);
    assert_ok_eq("\"42\"", &o, &n);
}

#[test]
fn oracle_prin1_to_string_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\"hello\\\"\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(prin1-to-string "hello")"#, expect);
    assert_ok_eq("\"\\\"hello\\\"\"", &o, &n);
}

#[test]
fn oracle_prin1_to_string_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(prin1-to-string 'hello)"#, expect);
    assert_ok_eq("\"hello\"", &o, &n);
}

#[test]
fn oracle_printer_quit_during_traversal_and_inhibit_quit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // A quit requested by an unreadable-object callback must be delivered
    // inside the surrounding print call, before its following object is
    // visited. Cover ordinary/circle printing and interpreted/compiled callers;
    // inhibiting quit must preserve the complete output and pending flag.
    let form = r#"(progn
      (defun neovm--issue590-print-with-pending-quit (circle inhibit)
        (let ((print-circle circle)
              (print-level nil)
              (print-length nil)
              (quit-flag nil)
              (inhibit-quit inhibit)
              (print-unreadable-function
               (lambda (_object _escape) (setq quit-flag t) "Q")))
          (if inhibit
              (let* ((text (prin1-to-string (vector (make-marker) '(after))))
                     (pending quit-flag))
                (setq quit-flag nil)
                (list text pending))
            (condition-case nil
                (prin1-to-string (vector (make-marker) '(after)))
              (quit 'caught)))))
      (let ((interpreted
             (mapcar (lambda (circle)
                       (list (neovm--issue590-print-with-pending-quit circle nil)
                             (neovm--issue590-print-with-pending-quit circle t)))
                     '(nil t))))
        (byte-compile 'neovm--issue590-print-with-pending-quit)
        (list interpreted
              (mapcar (lambda (circle)
                        (list (neovm--issue590-print-with-pending-quit circle nil)
                              (neovm--issue590-print-with-pending-quit circle t)))
                      '(nil t)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((caught (\"[Q (after)]\" t)) (caught (\"[Q (after)]\" t))) ((caught (\"[Q (after)]\" t)) (caught (\"[Q (after)]\" t))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_eval_self_evaluating() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_eval_quoted_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval '(+ 1 2))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_identity_returns_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 nil sym)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (identity 42) (identity nil) (identity 'sym))"#,
        expect,
    );
    assert_ok_eq("(42 nil sym)", &o, &n);
}

#[test]
fn oracle_number_to_string_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string 42)"#, expect);
    assert_ok_eq("\"42\"", &o, &n);
}

#[test]
fn oracle_number_to_string_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"-99\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string -99)"#, expect);
    assert_ok_eq("\"-99\"", &o, &n);
}

#[test]
fn oracle_string_to_number_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-to-number (number-to-string 42))"#,
        expect,
    );
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_number_to_string_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument numberp sym)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string 'sym)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
