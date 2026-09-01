//! Divergence tests: multibyte deep - char composition, bidi, coding conversions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_bidi_direction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bidi-direction)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (bidi-direction ?A)
  (bidi-direction ?a)
  (bidi-direction ?0)
  (bidi-direction ?\x05D0)
  (bidi-direction ? ))"#,
        expect,
    );
}

#[test]
fn divergence_bidi_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (16 16 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "Hello \x05E9\x05DC\x05D5\x05DD World"))
  (list (string-width s)
        (length s)
        (multibyte-string-p s)))"#,
        expect,
    );
}

#[test]
fn divergence_char_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'compose-region)
  (fboundp 'compose-string)
  (fboundp 'decompose-region)
  (fboundp 'decompose-string))"#,
        expect,
    );
}

#[test]
fn divergence_normalize_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 233 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "café")
  (let ((len (length (buffer-string))))
    (list len
          (aref (buffer-string) (1- len))
          (= (aref (buffer-string) (1- len)) ?é)
          (multibyte-string-p (buffer-string)))))"#,
        expect,
    );
}

#[test]
fn divergence_unicode_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (Lu Ll Nd Zs)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (get-char-code-property ?A 'general-category)
  (get-char-code-property ?a 'general-category)
  (get-char-code-property ?0 'general-category)
  (get-char-code-property ?  'general-category))"#,
        expect,
    );
}

#[test]
fn divergence_char_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (char-equal ?A ?a)
  (char-equal ?A ?A)
  (char-equal ?A ?b)
  (= (downcase ?A) ?a)
  (= (upcase ?a) ?A))"#,
        expect,
    );
}

#[test]
fn divergence_string_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Invalid argument 1 of operation ‘insert-file-contents’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "abc"))
  (list (coding-system-p (find-operation-coding-system 'insert-file-contents (list s)))
        (consp (find-operation-coding-system 'write-region s))))"#,
        expect,
    );
}

#[test]
fn divergence_decode_coding_string_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"é\" 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((raw "\xC3\xA9")
         (decoded (decode-coding-string raw 'utf-8)))
  (list decoded
        (length decoded)
        (= (aref decoded 0) ?é)))"#,
        expect,
    );
}

#[test]
fn divergence_encode_coding_string_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 2 195 169)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((str "é")
         (encoded (encode-coding-string str 'utf-8)))
  (list (length encoded)
        (string-bytes encoded)
        (aref encoded 0)
        (aref encoded 1)))"#,
        expect,
    );
}

#[test]
fn divergence_char_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (= ?A 65)
  (= ?a 97)
  (= ?0 48)
  (> ?é 127)
  (> ?中 #x4E2D))"#,
        expect,
    );
}
