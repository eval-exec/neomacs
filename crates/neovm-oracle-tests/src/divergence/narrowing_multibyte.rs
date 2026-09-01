//! Divergence tests: narrowing, region, multibyte, and buffer edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_narrow_insert_extends() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 10 \"cdefXYZ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (narrow-to-region 3 7)
  (goto-char 7)
  (insert "XYZ")
  (list (point-min) (point-max) (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_narrow_delete_shrinks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 5 \"ef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (narrow-to-region 3 7)
  (delete-region 3 5)
  (list (point-min) (point-max) (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_save_restriction_nested_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 7 \"cdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (narrow-to-region 3 7)
  (save-restriction
    (widen)
    (goto-char 10)
    (insert "Z"))
  (list (point-min) (point-max) (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_string_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"cdef\" \"cdef\" \"de\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (narrow-to-region 3 7)
  (list (buffer-string)
        (buffer-substring (point-min) (point-max))
        (buffer-substring-no-properties 4 6)))"#,
        expect,
    );
}

#[test]
fn divergence_position_bytes_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 3 4 5 8 6 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābc中d")
  (list (position-bytes 1)
        (position-bytes 2)
        (position-bytes 3)
        (position-bytes 4)
        (position-bytes 5)
        (point-max)
        (buffer-size)))"#,
        expect,
    );
}

#[test]
fn divergence_insert_char_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"āāā中中\" 5 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert-char ?ā 3)
  (insert-char ?中 2)
  (list (buffer-string) (buffer-size) (point)))"#,
        expect,
    );
}

#[test]
fn divergence_decode_encode_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ābc中\" \"ābc中\" 4 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "ābc中"))
  (list (encode-coding-string s 'utf-8)
        (decode-coding-string (encode-coding-string s 'utf-8) 'utf-8)
        (length s)
        (string-bytes s)))"#,
        expect,
    );
}

#[test]
fn divergence_char_syntax_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (119 119 32 62 40 34 92)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (char-syntax ?a)
              (char-syntax ?0)
              (char-syntax ? )
              (char-syntax ?\n)
              (char-syntax ?()
              (char-syntax ?\")
              (char-syntax ?\\))"#,
        expect,
    );
}

#[test]
fn divergence_upcase_downcase_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLD\" \"hello world\" \"Hello World\" \"Hello World\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (upcase "hello world")
              (downcase "HELLO WORLD")
              (upcase-initials "hello world")
              (capitalize "hello world"))"#,
        expect,
    );
}

#[test]
fn divergence_string_to_number_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 3.14 0 0 42 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (string-to-number \"42\")\n              (string-to-number \"3.14\")\n              (string-to-number \"0xff\")\n              (string-to-number \"abc\")\n              (string-to-number \"42abc\")\n              (string-to-number \"\"))",
        expect,
    );
}

#[test]
fn divergence_number_to_string_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"42\" \"-3\" \"3.14\" \"18446744073709551616\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (number-to-string 42)
              (number-to-string -3)
              (number-to-string 3.14)
              (number-to-string (expt 2 64)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (list (buffer-file-name)
        (buffer-file-name (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_kill_buffer_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf (get-buffer-create " *test-kill-restore*")))
  (with-current-buffer buf (insert "hello"))
  (kill-buffer buf)
  (list (get-buffer " *test-kill-restore*")
        (buffer-live-p buf)
        (buffer-name buf)))"#,
        expect,
    );
}

#[test]
fn divergence_get_buffer_create_vs_generate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\" *test-gbc*\" 0 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (get-buffer-create " *test-gbc*"))
        (b (generate-new-buffer " *test-gbc*")))
  (unwind-protect
      (list (buffer-name a)
            (string-match-p "\\` \\*test-gbc\\*-[0-9]+\\'" (buffer-name b))
            (not (eq a b))
            (eq a (get-buffer " *test-gbc*"))
            (eq b (get-buffer (buffer-name b))))
    (kill-buffer a)
    (kill-buffer b)))"#,
        expect,
    );
}
