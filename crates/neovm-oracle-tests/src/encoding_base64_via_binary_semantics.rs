//! Oracle parity for encoding, base64, and related ops via binary.
//! GNU src/fns.c, src/coding.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- base64 roundtrip ---

#[test]
fn oracle_base64_encode_decode_roundtrip_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (setq encoded (base64-encode-string "hello"))
  (base64-decode-string encoded))"#,
        expect,
    );
    assert_ok_eq("\"hello\"", &o, &n);
}

#[test]
fn oracle_base64_encode_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"aGVsbG8=\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(base64-encode-string "hello")"#, expect);
    assert_ok_eq("\"aGVsbG8=\"", &o, &n);
}

// --- decode-coding-string ---

#[test]
fn oracle_decode_coding_string_utf8_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(decode-coding-string "hello" 'utf-8)"#,
        expect,
    );
    assert_ok_eq("\"hello\"", &o, &n);
}

// --- encode-coding-string ---

#[test]
fn oracle_encode_coding_string_utf8_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(encode-coding-string "hello" 'utf-8)"#,
        expect,
    );
    assert_ok_eq("\"hello\"", &o, &n);
}

// --- string-bytes ---

#[test]
fn oracle_string_bytes_ascii_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-bytes "hello")"#, expect);
    assert_ok_eq("5", &o, &n);
}

// --- string-make-multibyte / unibyte ---

#[test]
fn oracle_string_make_multibyte_unibyte_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (setq s (string-make-unibyte "abc"))
  (string-make-multibyte s))"#,
        expect,
    );
    assert_ok_eq("\"abc\"", &o, &n);
}
