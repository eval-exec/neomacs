//! GNU's character upcase rules in buffer-aware string comparison.

use super::super::{builtin_compare_strings, compare_strings_in_state_with_parity};
use crate::emacs_core::eval::Context;
use neovm_text::emacs_core::casetab::{
    current_case_table_for_buffer_in_state_with_symbols,
    ensure_case_table_derived_slots_with_symbols,
    ensure_standard_case_table_object_in_state_with_symbols, is_case_table_with_symbols,
    set_current_case_table_for_buffer_in_state,
};
use neovm_text::emacs_core::chartable::{copy_char_table, ct_set_single};
use neovm_value::error::{EvalResult, LispCondition, signal};
use neovm_value::heap_types::LispString;
use neovm_value::{emacs_char, string_pos_cache, value::Value};

/// `(standard-case-table)`: the core entry is this text helper under the
/// audited legacy value write.
fn standard_case_table(ctx: &mut Context) -> EvalResult {
    let mode = ctx.symbol_mode();
    // SAFETY: lazy standard-table creation only reads and stores a variable
    // value. It does not call Lisp or publish canonical dispatch metadata.
    unsafe {
        ctx.obarray_view_mut().with_legacy_value_write(|obarray| {
            ensure_standard_case_table_object_in_state_with_symbols(obarray, mode)
        })
    }
}

/// `(set-case-table TABLE)`: the same check, standard-table ensure, derived
/// slots, and buffer-slot write the core entry performs.
fn set_case_table(ctx: &mut Context, table: Value) -> EvalResult {
    let mode = ctx.symbol_mode();
    if !is_case_table_with_symbols(&table, mode) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), table],
        ));
    }
    let _ = standard_case_table(ctx)?;
    ensure_case_table_derived_slots_with_symbols(table, mode)?;
    set_current_case_table_for_buffer_in_state(&mut ctx.edit.buffers, table)?;
    Ok(table)
}

/// `(current-case-table)`: the core entry's split borrow of the buffer
/// manager and the obarray view.
fn current_case_table(ctx: &mut Context) -> EvalResult {
    let mode = ctx.symbol_mode();
    let (buffers, mut obarray_view) = ctx.buffers_and_obarray_view_mut();
    // SAFETY: this selects and seeds case-table values and the buffer slot.
    // It does not call Lisp or change function metadata.
    unsafe {
        obarray_view.with_legacy_value_write(|obarray| {
            current_case_table_for_buffer_in_state_with_symbols(obarray, buffers, mode)
        })
    }
}

fn context_with_mappings(mappings: &[(u32, u32)]) -> Context {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let standard = standard_case_table(&mut ctx).unwrap();
    let table = copy_char_table(standard).unwrap();
    let up = copy_char_table(table.as_char_table_obj().unwrap().extras.as_slice()[0]).unwrap();
    for &(code, mapped) in mappings {
        ct_set_single(&up, code as i64, Value::fixnum(mapped as i64));
    }
    table.with_char_table_mut(|obj| obj.set_extra(0, up));
    set_case_table(&mut ctx, table).unwrap();
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
    let id = ctx.edit.buffers.current_buffer_id().unwrap();
    ctx.edit
        .buffers
        .set_buffer_multibyte_flag(id, false)
        .unwrap();
    assert_eq!(compare(&mut ctx, "é", "É"), Value::fixnum(1));
}

#[test]
fn compare_case_table_unibyte_buffer_truncates_changed_ascii_mapping() {
    let mut ctx = context_with_mappings(&[(b'i' as u32, 0x130)]);
    let id = ctx.edit.buffers.current_buffer_id().unwrap();
    ctx.edit
        .buffers
        .set_buffer_multibyte_flag(id, false)
        .unwrap();
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
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let table = standard_case_table(&mut ctx).unwrap();
    let up = table.as_char_table_obj().unwrap().extras.as_slice()[0];
    ct_set_single(&up, b'a' as i64, Value::fixnum(b'X' as i64));
    assert_eq!(compare(&mut ctx, "a", "X"), Value::T);
}

#[test]
fn compare_case_table_ascii_parent_fallback_is_observed() {
    let mut ctx = context_with_mappings(&[]);
    let table = current_case_table(&mut ctx).unwrap();
    let up = table.as_char_table_obj().unwrap().extras.as_slice()[0];
    let parent = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    ct_set_single(&parent, b'a' as i64, Value::fixnum(b'X' as i64));
    ct_set_single(&up, b'a' as i64, Value::NIL);
    up.with_char_table_mut(|obj| obj.set_parent(parent));
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
    neovm_value::test_support::authority::reset_position_conversion_scan_steps_for_test();
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
