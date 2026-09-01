//! Divergence tests: read/write objects, obarray operations, symbol shorthands.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_read_syntax_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((42 . 2) (-3 . 2) (1.5 . 3) (255 . 4))""#]];
    crate::common::assert_oracle_parity_expect(
        "(list\n  (read-from-string \"42\")\n  (read-from-string \"-3\")\n  (read-from-string \"1.5\")\n  (read-from-string \"#xFF\"))",
        expect,
    );
}

#[test]
fn divergence_read_syntax_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((10 . 6) (63 . 4))""#]];
    crate::common::assert_oracle_parity_expect(
        "(list\n  (read-from-string \"#b1010\")\n  (read-from-string \"#o77\"))",
        expect,
    );
}

#[test]
fn divergence_read_syntax_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((65 . 2) (92 . 3) (10 . 3) (9 . 3) (32 . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "?A")
  (read-from-string "?\\\\")
  (read-from-string "?\\n")
  (read-from-string "?\\t")
  (read-from-string "? "))"#,
        expect,
    );
}

#[test]
fn divergence_obarray_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ob (make-obarray 511)))
  (list (intern-soft "foo" ob)
        (intern "bar" ob)
        (intern "bar" ob)
        (intern-soft "bar" ob)
        (eq (intern "bar" ob) (intern-soft "bar" ob))))"#,
        expect,
    );
}

#[test]
fn divergence_obarray_count_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ob (make-obarray 31))
        count)
  (dotimes (i 5)
    (intern (format "sym%d" i) ob))
  (mapatoms (lambda (_) (setq count (1+ count))) ob)
  count)"#,
        expect,
    );
}

#[test]
fn divergence_symbol_function_interactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (commandp 'forward-char)
  (subrp (symbol-function 'forward-char))
  (commandp (symbol-function 'forward-char))
  (byte-code-function-p (symbol-function 'car)))"#,
        expect,
    );
}

#[test]
fn divergence_symbol_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"my-accessor-test\" 42 value nil (prop value) my-accessor-test)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (intern "my-accessor-test")))
  (set sym 42)
  (put sym 'prop 'value)
  (list (symbol-name sym)
        (symbol-value sym)
        (get sym 'prop)
        (symbol-function sym)
        (symbol-plist sym)
        (intern-soft "my-accessor-test"))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable my-kill-sym)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (set (intern "my-kill-sym") 99)
  (makunbound (intern "my-kill-sym"))
  (list (boundp (intern "my-kill-sym"))
        (intern-soft "my-kill-sym")
        (symbol-value (intern "my-kill-sym"))))"#,
        expect,
    );
}

#[test]
fn divergence_fmakunbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 42 my-fmakunbound-test nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-fmakunbound-test () 42)
  (list (fboundp 'my-fmakunbound-test)
        (my-fmakunbound-test)
        (fmakunbound 'my-fmakunbound-test)
        (fboundp 'my-fmakunbound-test)))"#,
        expect,
    );
}

#[test]
fn divergence_special_form_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (special-form-p (symbol-function 'if))
  (special-form-p (symbol-function 'let))
  (special-form-p (symbol-function 'let*))
  (special-form-p (symbol-function 'while))
  (special-form-p (symbol-function 'setq)))"#,
        expect,
    );
}
