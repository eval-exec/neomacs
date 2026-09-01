//! Coding encodability/query parity: coding-system-change-eol/text-conversion,
//! charset-list/mnemonic, find-coding-systems-for-charsets, terminal/keyboard
//! coding, decode-coding-region; plus four encodability divergences
//! (unencodable-char-position, find-coding-systems-string, check-coding-//! systems-region, define-coding-system-alias round-trip).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn coding_change_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8-dos utf-8-unix utf-8-mac utf-8-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-change-eol-conversion 'utf-8 'dos)
        (coding-system-change-eol-conversion 'utf-8 'unix)
        (coding-system-change-eol-conversion 'utf-8 'mac)
        (coding-system-change-eol-conversion 'utf-8-dos 'unix))"##,
        expect,
    );
}

#[test]
fn coding_change_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (iso-latin-1-dos utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-change-text-conversion 'utf-8-dos 'latin-1)
        (coding-system-base (coding-system-change-text-conversion 'undecided-unix 'utf-8)))"##,
        expect,
    );
}

#[test]
fn coding_charset_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil (iso-8859-1) 85)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (memq 'ascii (coding-system-charset-list 'utf-8))
        (coding-system-charset-list 'iso-8859-1)
        (coding-system-mnemonic 'utf-8))"##,
        expect,
    );
}

#[test]
fn decode_coding_region_dest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"héllo\" 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (encode-coding-string "héllo" 'utf-8))
  (decode-coding-region (point-min) (point-max) 'utf-8)
  (list (buffer-string) (buffer-size)))"##,
        expect,
    );
}

#[test]
fn find_cs_for_charsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (listp (find-coding-systems-for-charsets '(ascii)))
        (booleanp (and (memq 'iso-8859-1 (find-coding-systems-for-charsets '(latin-iso8859-1))) t)))"##,
        expect,
    );
}

#[test]
fn terminal_keyboard_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-p (or (keyboard-coding-system) 'undecided))
        (coding-system-p (or (terminal-coding-system) 'undecided)))"##,
        expect,
    );
}

#[test]
fn divergence_unencodable_char_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc日本def")
  (list (unencodable-char-position (point-min) (point-max) 'us-ascii)
        (unencodable-char-position (point-min) (point-max) 'utf-8)
        (unencodable-char-position (point-min) (point-max) 'iso-8859-1)))"##,
        expect,
    );
}

#[test]
fn divergence_find_coding_systems_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (> (length (find-coding-systems-string "héllo 日本語")) 10)
      (and (memq 'iso-2022-jp (find-coding-systems-string "日本語")) t)
      (and (memq 'chinese-gbk (find-coding-systems-string "中文")) t))"##,
        expect,
    );
}

#[test]
fn divergence_check_coding_systems_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((us-ascii 4) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "café")
  (let ((r (check-coding-systems-region (point-min) (point-max) '(us-ascii utf-8))))
    (list (assq 'us-ascii r) (assq 'utf-8 r))))"##,
        expect,
    );
}

#[test]
fn divergence_define_coding_system_alias_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t utf-8 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(define-coding-system-alias 'neo-utf8-alias-xyz 'utf-8)
(list (coding-system-p 'neo-utf8-alias-xyz)
      (coding-system-base 'neo-utf8-alias-xyz)
      (let ((s "tëst")) (string= s (decode-coding-string (encode-coding-string s 'neo-utf8-alias-xyz) 'neo-utf8-alias-xyz))))"##,
        expect,
    );
}
