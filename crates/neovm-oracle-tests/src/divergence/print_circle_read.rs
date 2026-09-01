//! Divergence tests: print-circle, read-circle, circular structure deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_print_circle_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(#1=(1 2 3) #1#)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-circle t)
        (shared (list 1 2 3)))
  (prin1-to-string (list shared shared)))"#,
        expect,
    );
}

#[test]
fn divergence_print_circle_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"((1 2 3) (1 2 3))\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-circle nil)
        (shared (list 1 2 3)))
  (prin1-to-string (list shared shared)))"#,
        expect,
    );
}

#[test]
fn divergence_read_circle_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"[#1=(1 2) #1#]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-circle t)
        (x (list 1 2))
        (y (vector 'a 'b)))
  (aset y 0 x)
  (aset y 1 x)
  (let* ((s (prin1-to-string y))
         (r (read s)))
    (list (eq (aref r 0) (aref r 1))
          s)))"#,
        expect,
    );
}

#[test]
fn divergence_print_length_ellipsis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(1 2 3 ...)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-length 3))
  (prin1-to-string '(1 2 3 4 5)))"#,
        expect,
    );
}

#[test]
fn divergence_print_level_ellipsis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(a (b ...) e)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-level 2))
  (prin1-to-string '(a (b (c (d))) e)))"#,
        expect,
    );
}

#[test]
fn divergence_print_escape_newlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\\\"line1\\\\nline2\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-escape-newlines t))
  (prin1-to-string "line1\nline2"))"#,
        expect,
    );
}

#[test]
fn divergence_print_escape_nonascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\\\"café\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-escape-nonascii t))
  (prin1-to-string "café"))"#,
        expect,
    );
}

#[test]
fn divergence_print_quoted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"'foo\" \"(lambda (x) x)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-quoted t))
  (list (prin1-to-string ''foo)
        (prin1-to-string '(lambda (x) x))))"#,
        expect,
    );
}

#[test]
fn divergence_print_gensym_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (test \"test\" \"#:test\")""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-gensym t)
        (sym (make-symbol "test")))
  (list (intern-soft "test")
        (symbol-name sym)
        (prin1-to-string sym)))"#,
        expect,
    );
}

#[test]
fn divergence_print_float_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"3.14\" \"1.00\" \"0.50\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((float-output-format "%.2f"))
  (list (prin1-to-string 3.14159)
        (prin1-to-string 1.0)
        (prin1-to-string 0.5)))"#,
        expect,
    );
}
