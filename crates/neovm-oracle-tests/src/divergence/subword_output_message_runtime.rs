//! subword-mode forward-word (find-word-boundary-function-table), output
//! functions (with-output-to-string, princ/prin1/terpri/print to a buffer,
//! prin1 to a function stream, pp-to-string), and format-message quoting
//! (grave/curve); plus the batch current-message divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn sw_find_word_boundary_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (11 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "helloWorld test")
  (goto-char (point-min))
  (list (progn (forward-word) (point)) (boundp 'find-word-boundary-function-table)))"##,
        expect,
    );
}

#[test]
fn sw_format_message_curve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"type ‘C-x C-c’ to quit\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((text-quoting-style 'curve))
  (format-message "type `C-x C-c' to quit"))"##,
        expect,
    );
}

#[test]
fn sw_format_message_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"use `foo'\" \"\\\\`a\\\\'\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((text-quoting-style 'grave))
  (list (format-message "use `foo'") (substitute-command-keys "\\`a\\'")))"##,
        expect,
    );
}

#[test]
fn sw_output_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello 42\" \"(1 2 3)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (with-output-to-string (princ "hello") (princ " ") (princ 42))
        (with-output-to-string (prin1 '(1 2 3))))"##,
        expect,
    );
}

#[test]
fn sw_pp_to_string_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(a (b c) d)\" \"[1 2 3]\" \"42\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-trim (pp-to-string '(a (b c) d)))
        (string-trim (pp-to-string [1 2 3]))
        (string-trim (pp-to-string 42)))"##,
        expect,
    );
}

#[test]
fn sw_prin1_to_string_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"symbol\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc nil))
  (prin1 'symbol (lambda (c) (push c acc)))
  (concat (nreverse acc)))"##,
        expect,
    );
}

#[test]
fn sw_princ_prin1_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"abc(x y)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (princ "abc" (current-buffer))
  (prin1 '(x y) (current-buffer))
  (terpri (current-buffer))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn sw_print_to_buffer_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"X\\n\\\"inserted\\\"\\nY\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "XY")
  (goto-char 2)
  (print "inserted" (current-buffer))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn sw_subword_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 7 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'subword)
  (with-temp-buffer
    (insert "fooBarBaz quxQux")
    (subword-mode 1)
    (goto-char (point-min))
    (list (progn (forward-word) (point)) (progn (forward-word) (point)) (progn (forward-word) (point))))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn divergence_current_message_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (message "hello %d" 42)
       (list (current-message) (booleanp (current-message))))"##,
        expect,
    );
}
