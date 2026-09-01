//! UTF-8 / multibyte *buffer region operations* divergence probes.
//!
//! Probes buffer-substring, insert-buffer-substring, delete/transpose chars,
//! and region coding over buffers that hold multibyte and eight-bit raw-byte
//! characters — the surfaces where the eight-bit handling bugs reappear.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- buffer-substring with eight-bit chars ----------------------------------

#[test]
fn div_utf8_buffer_substring_with_recovered_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"A\\310\\311B\" \"\\310\\311\" (65 4194248 4194249 66))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert (decode-coding-string (unibyte-string 65 200 201 66) 'utf-8))
  (list (buffer-substring 1 (point-max))
        (buffer-substring 2 4)
        (append (buffer-substring 1 (point-max)) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_buffer_substring_constructed_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((65 4194248 4194249 66) (4194248 4194249))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert (string-make-multibyte (unibyte-string 65 200 201 66)))
  (list (append (buffer-substring 1 (point-max)) nil)
        (append (buffer-substring 2 4) nil)))
"#,
        expect,
    );
}

// --- insert-buffer-substring ------------------------------------------------

#[test]
fn div_utf8_insert_buffer_substring_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\" 6 (99 97 102 233 19990 30028))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((src (generate-new-buffer " *s*"))
      (dst (generate-new-buffer " *d*")))
  (with-current-buffer src (insert "café世界"))
  (with-current-buffer dst (insert-buffer-substring src))
  (let ((r (with-current-buffer dst (buffer-string))))
    (kill-buffer src)
    (kill-buffer dst)
    (list r (length r) (append r nil))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_insert_buffer_substring_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((4194249) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((src (generate-new-buffer " *s*"))
      (dst (generate-new-buffer " *d*")))
  (with-current-buffer src
    (insert (decode-coding-string (unibyte-string 200 201 65) 'utf-8)))
  (with-current-buffer dst (insert-buffer-substring src 2 3))
  (let ((r (with-current-buffer dst (buffer-string))))
    (kill-buffer src)
    (kill-buffer dst)
    (list (append r nil) (length r))))
"#,
        expect,
    );
}

// --- delete / transpose over multibyte --------------------------------------

#[test]
fn div_utf8_delete_char_over_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a界b\" 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "a世界b")
  (goto-char 2)
  (delete-char 1)
  (list (buffer-string) (point)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_delete_backward_char_over_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a世b\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "a世界b")
  (goto-char 4)
  (delete-backward-char 1)
  (list (buffer-string) (point)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_transpose_chars_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"éabç\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "aébç")
  (goto-char 2)
  (transpose-chars 1)
  (list (buffer-string) (point)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_transpose_words_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"thé café world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café thé world")
  (goto-char 2)
  (transpose-words 1)
  (list (buffer-string)))
"#,
        expect,
    );
}

// --- region coding ----------------------------------------------------------

#[test]
fn div_utf8_encode_coding_region_utf16_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 (4194302 4194303 0 65 0 66))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "AB")
  (encode-coding-region (point-min) (point-max) 'utf-16)
  (list (length (buffer-string)) (append (buffer-string) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_encode_coding_region_with_signature_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 (4194287 4194235 4194239 97 98 99))""#]];
    // encode-coding-region 'utf-8-with-signature' — does it emit BOM?
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abc")
  (encode-coding-region (point-min) (point-max) 'utf-8-with-signature)
  (list (length (buffer-string)) (append (buffer-string) nil)))
"#,
        expect,
    );
}
