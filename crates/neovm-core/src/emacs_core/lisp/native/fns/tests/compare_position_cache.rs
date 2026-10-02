//! Structural regressions for GNU's cached compare-strings START conversions.

use super::super::builtin_compare_strings;
use crate::emacs_core::{emacs_char, string_pos_cache, value::Value};
use crate::heap_types::LispString;

fn init_cache_test() {
    // Nextest launches each test in its own process. Set the process knob
    // before tracing or the Lisp runtime can initialize or start workers.
    unsafe { std::env::set_var("NEOVM_COMPARE_STRINGS_POS_CACHE", "on") };
    crate::test_utils::init_test_tracing();
    string_pos_cache::reset_string_pos_cache();
}

fn compare_token(needle: Value, source: Value, start: usize, len: usize) -> Value {
    builtin_compare_strings(vec![
        needle,
        Value::fixnum(0),
        Value::fixnum(len as i64),
        source,
        Value::fixnum(start as i64),
        Value::fixnum((start + len) as i64),
    ])
    .expect("valid token comparison")
}

#[test]
fn compare_position_cache_reuses_far_start_for_ascii_token() {
    init_cache_test();
    let source = Value::string(format!("{}xy{}", "ж".repeat(1000), "あ".repeat(1000)));
    let needle = Value::string("xy");
    assert_eq!(compare_token(needle, source, 1000, 2), Value::T);

    emacs_char::reset_position_conversion_scan_steps_for_test();
    for _ in 0..20 {
        assert_eq!(compare_token(needle, source, 1000, 2), Value::T);
    }
    assert_eq!(
        emacs_char::position_conversion_scan_steps_for_test(),
        0,
        "a short ASCII operand must preserve the cached multibyte START"
    );
}

#[test]
fn compare_position_cache_keeps_same_string_operand_identity() {
    init_cache_test();
    let source = Value::string(format!("{}xy{}", "ж".repeat(1000), "😀".repeat(1000)));
    let args = vec![
        source,
        Value::fixnum(1000),
        Value::fixnum(1002),
        source,
        Value::fixnum(1000),
        Value::fixnum(1002),
    ];
    assert_eq!(builtin_compare_strings(args.clone()).unwrap(), Value::T);

    emacs_char::reset_position_conversion_scan_steps_for_test();
    for _ in 0..20 {
        assert_eq!(builtin_compare_strings(args.clone()).unwrap(), Value::T);
    }
    assert_eq!(
        emacs_char::position_conversion_scan_steps_for_test(),
        0,
        "both operands must identify the same cache entry"
    );
}

#[test]
fn compare_position_cache_invalidates_changed_character_boundaries() {
    init_cache_test();
    let source = Value::heap_string(LispString::from_emacs_bytes(
        "a😀axy😀a".as_bytes().to_vec(),
    ));
    let needle = Value::string("xy");
    assert_eq!(compare_token(needle, source, 3, 2), Value::T);

    // This internal payload mutation keeps the same byte length and identity
    // while shifting character boundaries. Lisp-level GNU mutation coverage
    // uses ASCII aset and same-width fillarray; GNU's own fillarray cache bug
    // makes its layout-changing version unsuitable as a parity expectation.
    source.with_lisp_string_mut(|string| {
        string.mutate_bytes(|bytes| {
            bytes.clear();
            bytes.extend_from_slice("жжaxy😀ж".as_bytes());
        });
    });
    assert_eq!(compare_token(needle, source, 3, 2), Value::T);
}
