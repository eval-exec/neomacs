use super::*;

use malachite::integer::Integer;

use neovm_value::error::{expect_args, expect_max_args, expect_min_args};

#[test]
fn expect_args_exact_match() {
    crate::test_utils::init_test_tracing();
    assert!(expect_args("test", &[Value::NIL, Value::NIL], 2).is_ok());
}

#[test]
fn expect_args_wrong_count() {
    crate::test_utils::init_test_tracing();
    let err = expect_args("test", &[Value::NIL], 2);
    assert!(err.is_err());
}

#[test]
fn expect_min_args_at_min() {
    crate::test_utils::init_test_tracing();
    assert!(expect_min_args("test", &[Value::NIL], 1).is_ok());
}

#[test]
fn expect_min_args_below_min() {
    crate::test_utils::init_test_tracing();
    assert!(expect_min_args("test", &[], 1).is_err());
}

#[test]
fn expect_max_args_at_max() {
    crate::test_utils::init_test_tracing();
    assert!(expect_max_args("test", &[Value::NIL, Value::NIL], 2).is_ok());
}

#[test]
fn expect_max_args_above_max() {
    crate::test_utils::init_test_tracing();
    assert!(expect_max_args("test", &[Value::NIL, Value::NIL, Value::NIL], 2).is_err());
}

#[test]
fn collect_insert_text_strings_and_chars() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let s = Value::string("hello");
    let c = Value::char('!');
    let result = collect_insert_text("insert", &[s, c]).unwrap();
    assert_eq!(result, b"hello!");
}

#[test]
fn collect_insert_text_int_as_char() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // ASCII 65 = 'A'
    let result = collect_insert_text("insert", &[Value::fixnum(65)]).unwrap();
    assert_eq!(result, b"A");
}

#[test]
fn collect_insert_text_nonunicode_char_preserves_emacs_code() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let code = 0x3F_FF80i64;
    let result = collect_insert_text("insert", &[Value::fixnum(code)]).unwrap();
    // Raw byte 0x80 encodes as the overlong C0/C1 two-byte sequence
    // [0xC0, 0x80] in Emacs internal encoding.
    let (decoded, len) = neovm_value::emacs_char::string_char(&result);
    assert_eq!(decoded.code(), code as u32);
    assert_eq!(len, result.len());
}

#[test]
fn collect_insert_text_wrong_type() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    assert!(collect_insert_text("insert", &[Value::NIL]).is_err());
}

#[test]
fn logcount_positive() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // 7 = 0b111 → 3 bits
    let result = builtin_logcount(vec![Value::fixnum(7)]).unwrap();
    assert_eq!(result, Value::fixnum(3));
}

#[test]
fn logcount_zero() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let result = builtin_logcount(vec![Value::fixnum(0)]).unwrap();
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn logcount_negative() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // -1 = all 1s → !(-1) = 0 → count_ones = 0
    let result = builtin_logcount(vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(result, Value::fixnum(0));

    // -2 = ...1110 → !(-2) = 1 → count_ones = 1
    let result = builtin_logcount(vec![Value::fixnum(-2)]).unwrap();
    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn logcount_bignum_matches_gnu_integer_path() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let big = Value::make_integer(Integer::from(1u64) << 100u32);
    assert_eq!(builtin_logcount(vec![big]).unwrap(), Value::fixnum(1));

    let mut negative_big = Integer::from(1);
    negative_big <<= 100;
    negative_big = -negative_big;
    let negative = Value::make_integer(negative_big);
    assert_eq!(
        builtin_logcount(vec![negative]).unwrap(),
        Value::fixnum(100)
    );
}

#[test]
fn logcount_wrong_type() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    assert!(builtin_logcount(vec![Value::NIL]).is_err());
}

fn install_test_runtime() {
    // Tagged heap is auto-created in test mode; no manual setup needed.
}
