//! Divergence tests: reader and printer edge cases.
//!
//! Tests for read/print round-trip fidelity, symbol-with-pos,
//! character literal reading, and print representation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_read_from_string_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((a b) . 5) (42 . 2) (\"hello\" . 7))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (read-from-string "(a b) c")
              (read-from-string "42 99")
              (read-from-string "\"hello\" world"))"#,
        expect,
    );
}

#[test]
fn divergence_read_multibyte_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"ābc中def\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(car (read-from-string "\"ābc中def\""))"#,
        expect,
    );
}

#[test]
fn divergence_prin1_to_string_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((data '(a "hello world" 42 (nil . t) [1 2 3])))
  (equal data (car (read-from-string (prin1-to-string data)))))"#,
        expect,
    );
}

#[test]
fn divergence_print_symbol_with_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function sym)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((sym-with-pos (save-excursion
                (with-temp-buffer
                  (insert "(hello)")
                  (goto-char 2)
                  (read (current-buffer))))))
         (sym (if (symbol-with-pos-p sym-with-pos)
                  (symbol-with-pos-symbol sym-with-pos)
                sym-with-pos)))
    (list sym (symbol-name sym)
          (symbol-with-pos-p sym-with-pos)))"#,
        expect,
    );
}

#[test]
fn divergence_print_circle_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(1 2 #0)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x (list 1 2 3)))
  (setcar (nthcdr 2 x) x)
  (prin1-to-string x))"#,
        expect,
    );
}

#[test]
fn divergence_read_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect(r#"`(a ,(+ 1 2) ,@(list 4 5))"#, expect);
}

#[test]
fn divergence_read_special_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1.0e+INF -1.0e+INF 0.0e+NaN)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (car (read-from-string "1.0e+INF"))
              (car (read-from-string "-1.0e+INF"))
              (car (read-from-string "0.0e+NaN")))"#,
        expect,
    );
}

#[test]
fn divergence_print_string_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"\\\"hello \\\\\\\"world\\\\\\\" \\n tab\there\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(prin1-to-string "hello \"world\" \n tab\there")"#,
        expect,
    );
}

#[test]
fn divergence_read_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 two \"three\" (four))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((v (car (read-from-string "[1 two \"three\" (four)]"))))
  (list (aref v 0) (aref v 1) (aref v 2) (aref v 3)))"#,
        expect,
    );
}

#[test]
fn divergence_read_char_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 97 92 10 9 1 134217825)""#]];
    crate::common::assert_oracle_parity_expect(r#"(list ?A ?a ?\\ ?\n ?\t ?\C-a ?\M-a)"#, expect);
}

#[test]
fn divergence_format_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"number 42\" \"float 3.140000\" \"hex ff\" \"oct 10\" \"char A\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (format "hello %s" "world")
              (format "number %d" 42)
              (format "float %f" 3.14)
              (format "hex %x" 255)
              (format "oct %o" 8)
              (format "char %c" 65))"#,
        expect,
    );
}

#[test]
fn divergence_read_hash_table_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"val1\" 42 \"default\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((h (make-hash-table :test 'equal)))
  (puthash "key1" "val1" h)
  (puthash "key2" 42 h)
  (list (gethash "key1" h)
        (gethash "key2" h)
        (gethash "missing" h "default")))"#,
        expect,
    );
}

#[test]
fn divergence_read_cons_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a . b)""#]];
    crate::common::assert_oracle_parity_expect(r#"(car (read-from-string "(a . b)"))"#, expect);
}

#[test]
fn divergence_print_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (18446744073709551616 18446744073709551617 18446744073709551615)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((big (expt 2 64)))
  (list big (1+ big) (1- big)))"#,
        expect,
    );
}
