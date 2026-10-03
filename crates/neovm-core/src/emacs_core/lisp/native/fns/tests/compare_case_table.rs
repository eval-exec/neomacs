//! GNU's character upcase rules in buffer-aware string comparison.

use super::super::{builtin_compare_strings, compare_strings_in_state_with_parity};
use crate::emacs_core::{
    casetab, chartable, emacs_char, eval::Context, string_pos_cache, value::Value,
};
use crate::heap_types::LispString;

fn context_with_mappings(mappings: &[(u32, u32)]) -> Context {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let standard = casetab::builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    let table = chartable::copy_char_table(standard).unwrap();
    let up = chartable::copy_char_table(table.as_char_table_obj().unwrap().extras.as_slice()[0])
        .unwrap();
    for &(code, mapped) in mappings {
        chartable::ct_set_single(&up, code as i64, Value::fixnum(mapped as i64));
    }
    table.with_char_table_mut(|obj| obj.extras.ensure_owned()[0] = up);
    casetab::builtin_set_case_table(&mut ctx, vec![table]).unwrap();
    ctx
}

fn args(left: Value, right: Value) -> Vec<Value> {
    vec![
        left,
        Value::NIL,
        Value::NIL,
        right,
        Value::NIL,
        Value::NIL,
        Value::T,
    ]
}

fn compare(ctx: &mut Context, left: &str, right: &str) -> Value {
    compare_strings_in_state_with_parity(ctx, args(Value::string(left), Value::string(right)), true)
        .unwrap()
}

#[test]
fn compare_case_table_custom_ascii_upcase() {
    let mut ctx = context_with_mappings(&[(b'A' as u32, b'z' as u32), (b'z' as u32, b'z' as u32)]);
    assert_eq!(compare(&mut ctx, "Az", "zz"), Value::T);
}

#[test]
fn compare_case_table_turkish_dotless_i() {
    let mut ctx = context_with_mappings(&[(0x131, b'I' as u32), (b'i' as u32, 0x130)]);
    assert_eq!(compare(&mut ctx, "ı", "I"), Value::T);
    assert_eq!(compare(&mut ctx, "i", "I"), Value::fixnum(1));
}

#[test]
fn compare_case_table_unibyte_buffer_preserves_latin1_characters() {
    let mut ctx = context_with_mappings(&[(0xe9, 0xc9)]);
    assert_eq!(compare(&mut ctx, "é", "É"), Value::T);
    let id = ctx.buffers.current_buffer_id().unwrap();
    ctx.buffers.set_buffer_multibyte_flag(id, false).unwrap();
    assert_eq!(compare(&mut ctx, "é", "É"), Value::fixnum(1));
}

#[test]
fn compare_case_table_unibyte_buffer_truncates_changed_ascii_mapping() {
    let mut ctx = context_with_mappings(&[(b'i' as u32, 0x130)]);
    let id = ctx.buffers.current_buffer_id().unwrap();
    ctx.buffers.set_buffer_multibyte_flag(id, false).unwrap();
    assert_eq!(compare(&mut ctx, "i", "0"), Value::T);
}

#[test]
fn compare_case_table_mixed_unibyte_and_multibyte_raw_byte_mapping() {
    let raw = emacs_char::byte8_to_char(0xe9);
    let mut ctx = context_with_mappings(&[(raw, b'E' as u32)]);
    let left = Value::heap_string(LispString::from_unibyte(vec![0xe9]));
    let right = Value::string("E");
    assert_eq!(
        compare_strings_in_state_with_parity(&mut ctx, args(left, right), true).unwrap(),
        Value::T
    );
}

#[test]
fn compare_case_table_missing_entry_does_not_fall_back_to_unicode() {
    let mut ctx = context_with_mappings(&[]);
    assert_eq!(compare(&mut ctx, "é", "É"), Value::fixnum(1));
}

#[test]
fn compare_case_table_standard_object_mutations_are_observed() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let table = casetab::builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    let up = table.as_char_table_obj().unwrap().extras.as_slice()[0];
    chartable::ct_set_single(&up, b'a' as i64, Value::fixnum(b'X' as i64));
    assert_eq!(compare(&mut ctx, "a", "X"), Value::T);
}

#[test]
fn compare_case_table_ascii_parent_fallback_is_observed() {
    let mut ctx = context_with_mappings(&[]);
    let table = casetab::builtin_current_case_table(&mut ctx, vec![]).unwrap();
    let up = table.as_char_table_obj().unwrap().extras.as_slice()[0];
    let parent = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    chartable::ct_set_single(&parent, b'a' as i64, Value::fixnum(b'X' as i64));
    chartable::ct_set_single(&up, b'a' as i64, Value::NIL);
    up.with_char_table_mut(|obj| obj.parent = parent);
    assert_eq!(compare(&mut ctx, "a", "X"), Value::T);
}

#[test]
fn compare_case_table_ranges_keep_relative_position_and_sign() {
    let mut ctx = context_with_mappings(&[(b'A' as u32, b'z' as u32), (b'z' as u32, b'z' as u32)]);
    let left = Value::string("xxAqyy");
    let right = Value::string("zzzz");
    let ranged = vec![
        left,
        Value::fixnum(2),
        Value::fixnum(4),
        right,
        Value::fixnum(0),
        Value::fixnum(2),
        Value::T,
    ];
    assert_eq!(
        compare_strings_in_state_with_parity(&mut ctx, ranged, true).unwrap(),
        Value::fixnum(-2)
    );
    assert_eq!(compare(&mut ctx, "zz", "Aq"), Value::fixnum(2));
    assert_eq!(compare(&mut ctx, "A", "zz"), Value::fixnum(-2));
}

#[test]
fn compare_case_table_preserves_start_position_cache() {
    let mut ctx = context_with_mappings(&[(b'A' as u32, b'z' as u32), (b'z' as u32, b'z' as u32)]);
    string_pos_cache::reset_string_pos_cache();
    let source = Value::string(format!("{}A{}", "ж".repeat(1000), "あ".repeat(1000)));
    let needle = Value::string("z");
    let ranged = vec![
        needle,
        Value::NIL,
        Value::NIL,
        source,
        Value::fixnum(1000),
        Value::fixnum(1001),
        Value::T,
    ];
    assert_eq!(
        compare_strings_in_state_with_parity(&mut ctx, ranged.clone(), true).unwrap(),
        Value::T
    );
    emacs_char::reset_position_conversion_scan_steps_for_test();
    assert_eq!(
        compare_strings_in_state_with_parity(&mut ctx, ranged, true).unwrap(),
        Value::T
    );
    assert_eq!(emacs_char::position_conversion_scan_steps_for_test(), 0);
}

#[test]
fn compare_case_table_off_and_case_sensitive_paths_keep_legacy_result() {
    let mut ctx = context_with_mappings(&[(b'A' as u32, b'z' as u32), (b'z' as u32, b'z' as u32)]);
    let mut input = args(Value::string("A"), Value::string("z"));
    assert_eq!(
        compare_strings_in_state_with_parity(&mut ctx, input.clone(), false).unwrap(),
        builtin_compare_strings(input.clone()).unwrap()
    );
    input[6] = Value::NIL;
    assert_eq!(
        compare_strings_in_state_with_parity(&mut ctx, input.clone(), true).unwrap(),
        builtin_compare_strings(input).unwrap()
    );
}
