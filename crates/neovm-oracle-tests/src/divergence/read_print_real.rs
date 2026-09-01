//! Divergence tests: real read/print behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_read_basic_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((42 . 2) (3.14 . 4) (\"hello\" . 7) (nil . 3) (t . 1) ((1 2 3) . 7) ([1 2 3] . 7) (65 . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (read-from-string \"42\")
  (read-from-string \"3.14\")
  (read-from-string \"\\\"hello\\\"\")
  (read-from-string \"nil\")
  (read-from-string \"t\")
  (read-from-string \"(1 2 3)\")
  (read-from-string \"[1 2 3]\")
  (read-from-string \"?A\")) ",
        expect,
    );
}

#[test]
fn divergence_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (\"#1=(a b . #1#)\" t)""##]];
    crate::common::assert_oracle_parity_expect(
        "(let ((print-circle t)
        (obj (list 'a 'b)))
  (nconc obj obj)
  (list (prin1-to-string obj)
        print-circle)) ",
        expect,
    );
}

#[test]
fn divergence_print_gensym_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function interned-p)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((sym (make-symbol \"temp\")))
  (list (symbolp sym)
        (symbol-name sym)
        (interned-p sym)
        (let ((print-gensym t))
          (prin1-to-string sym))
        (let ((print-gensym nil))
          (prin1-to-string sym)))) ",
        expect,
    );
}

#[test]
fn divergence_print_escape_newlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"hello\\nworld\" \"\\\"hello\\\\nworld\\\"\" \"\\\"hello\\nworld\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let ((s \"hello\\nworld\"))
  (list s
        (let ((print-escape-newlines t))
          (prin1-to-string s))
        (let ((print-escape-newlines nil))
          (prin1-to-string s)))) ",
        expect,
    );
}

#[test]
fn divergence_format_specifiers_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"count: 42\" \"pi: 3.14\" \"char: A\" \"% literal\" \"        hi\" \"hi        |\" \"00007\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (format \"hello %s\" \"world\")
  (format \"count: %d\" 42)
  (format \"pi: %.2f\" 3.14159)
  (format \"char: %c\" 65)
  (format \"%% literal\")
  (format \"%10s\" \"hi\")
  (format \"%-10s|\" \"hi\")
  (format \"%05d\" 7)) ",
        expect,
    );
}

#[test]
fn divergence_read_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 27 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((data '((1 (2 3)) [4 5] \"six\" :kw))
        (printed (prin1-to-string data))
        (re-read (read-from-string printed)))
  (list (equal data (car re-read))
        (cdr re-read)
        (= (cdr re-read) (length printed)))) ",
        expect,
    );
}

#[test]
fn divergence_print_length_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"(1 2 3 ...)\" \"[1 2 3 ...]\" \"\\\"abcdefghij\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((print-length 3))
  (list (prin1-to-string '(1 2 3 4 5))
        (prin1-to-string '[1 2 3 4 5])
        (prin1-to-string \"abcdefghij\"))) ",
        expect,
    );
}

#[test]
fn divergence_print_level_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(1 (2 ...))\" \"((a ...))\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((print-level 2))
  (list (prin1-to-string '(1 (2 (3 (4)))))
        (prin1-to-string '((a (b (c))))))) ",
        expect,
    );
}

#[test]
fn divergence_princ_vs_prin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"\\\"hello\\\"\" \"hello\" \"(a \\\"b\\\" 3)\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((s \"hello\"))
  (list (prin1-to-string s)
        (with-output-to-string (princ s))
        (substring (prin1-to-string '(a \"b\" 3)) 0))) ",
        expect,
    );
}

#[test]
fn divergence_print_escape_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t \"\\\"café\\\"\" 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((s \"caf\\u00e9\")
        (p1 (prin1-to-string s))
        (r1 (car (read-from-string p1))))
  (list (equal s r1)
        (string= s r1)
        p1
        (length s)
        (length r1))) ",
        expect,
    );
}
