//! md5/secure-hash/sha1, base64/base64url encode-decode, hex/radix number
//! conversions, color-name-to-rgb / rgb-to-hex, subst-char/translate-region,
//! and character predicate parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn base64_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"SGVsbG8sIFdvcmxkIQ==\" \"Hello\" \"c3ViamVjdHM_X2Q\" \"Y2Fmw6k=\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (base64-encode-string "Hello, World!")
        (base64-decode-string "SGVsbG8=")
        (base64url-encode-string "subjects?_d" t)
        (base64-encode-string (string-to-unibyte (encode-coding-string "café" 'utf-8))))"##,
        expect,
    );
}

#[test]
fn hex_string_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ff\" 255 10 \"255\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%x" 255) (string-to-number "ff" 16)
        (string-to-number "1010" 2) (number-to-string 255))"##,
        expect,
    );
}

#[test]
fn md5_sha() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"5d41402abc4b2a76b9719d911017c592\" \"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\" \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\" \"a94a8fe5ccb19ba61c4c0873d391e987982fbbd3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (md5 "hello") (secure-hash 'sha1 "hello")
        (secure-hash 'sha256 "abc") (sha1 "test"))"##,
        expect,
    );
}

#[test]
fn secure_hash_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"594c0da6edd43f7538a8293b817f829b\" \"2af0f99d93bd34c5cc387434301f36e37cd834a5113a084e112861f755aab85c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "content here")
  (list (secure-hash 'md5 (current-buffer))
        (secure-hash 'sha256 (current-buffer) (point-min) 7)))"##,
        expect,
    );
}

#[test]
fn char_displayable_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 4194303 t 65 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (characterp ?A) (characterp 9999999)
        (max-char) (characterp (max-char))
        (logand ?A #xff) (ash ?A -4))"##,
        expect,
    );
}

#[test]
fn color_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((1.0 1.0 1.0) (0.0 0.0 0.0) \"#ffff00000000\" \"#7f7f7f\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-name-to-rgb "white")
        (color-name-to-rgb "black")
        (color-rgb-to-hex 1.0 0.0 0.0)
        (color-rgb-to-hex 0.5 0.5 0.5 2))"##,
        expect,
    );
}

#[test]
fn subst_translate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"heLLo\" \"XYcXYc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tbl (make-char-table 'translation-table)))
  (aset tbl ?a ?X) (aset tbl ?b ?Y)
  (list (subst-char-in-string ?l ?L "hello")
        (with-temp-buffer (insert "abcabc") (translate-region (point-min) (point-max) tbl) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn text_quoting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"itXs\" \"<a><b><c>\" \"a_b_c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-replace "'" "X" "it's")
        (replace-regexp-in-string "\\([a-z]\\)\\1" "<\\1>" "aabbcc")
        (subst-char-in-string ?\s ?_ "a b c"))"##,
        expect,
    );
}
