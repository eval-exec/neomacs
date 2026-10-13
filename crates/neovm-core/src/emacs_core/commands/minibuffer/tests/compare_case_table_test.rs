//! Completion primitives call GNU compare-strings with current buffer casing.

use super::super::*;
use crate::emacs_core::{casetab, chartable, eval::Context};
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
    table.with_char_table_mut(|obj| obj.set_extra(0, up));
    casetab::builtin_set_case_table(&mut ctx, vec![table]).unwrap();
    ctx
}

#[test]
fn completion_compare_case_table_custom_prefix() {
    let mut ctx = context_with_mappings(&[(0x131, b'I' as u32)]);
    let string = LispString::from_utf8("I");
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let completion = CompletionText::LispObject(Value::string("ıx"));
    assert!(
        completion_prefix_matches_with_parity(&mut ctx, &string, &prefix, &completion, true, true)
            .unwrap()
    );
}

#[test]
fn completion_compare_case_table_custom_equality() {
    let mut ctx = context_with_mappings(&[(0x131, b'I' as u32)]);
    let string = LispString::from_utf8("IQ");
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let completion = CompletionText::LispObject(Value::string("ıQ"));
    assert!(
        completion_equals_with_parity(&mut ctx, &string, &prefix, &completion, true, true).unwrap()
    );
}

#[test]
fn completion_compare_case_table_unibyte_buffer() {
    let mut ctx = context_with_mappings(&[(0xe9, 0xc9)]);
    let id = ctx.buffers.current_buffer_id().unwrap();
    ctx.buffers.set_buffer_multibyte_flag(id, false).unwrap();
    let string = LispString::from_utf8("é");
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let completion = CompletionText::LispObject(Value::string("É"));
    assert!(
        !completion_equals_with_parity(&mut ctx, &string, &prefix, &completion, true, true)
            .unwrap()
    );
}

#[test]
fn completion_compare_case_table_unibyte_operand_becomes_raw_byte() {
    let mut ctx = context_with_mappings(&[]);
    let string = LispString::from_unibyte(vec![0xe9]);
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let completion = CompletionText::LispObject(Value::string("é"));
    assert!(
        !completion_prefix_matches_with_parity(&mut ctx, &string, &prefix, &completion, true, true)
            .unwrap()
    );
}

#[test]
fn completion_compare_case_table_state_is_resolved_for_each_comparison() {
    let mut ctx = context_with_mappings(&[(b'a' as u32, b'X' as u32)]);
    let string = LispString::from_utf8("a");
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let completion = CompletionText::LispObject(Value::string("X"));
    assert!(
        completion_equals_with_parity(&mut ctx, &string, &prefix, &completion, true, true).unwrap()
    );
    let current = casetab::builtin_current_case_table(&mut ctx, vec![]).unwrap();
    let up = current.as_char_table_obj().unwrap().extras.as_slice()[0];
    chartable::ct_set_single(&up, b'a' as i64, Value::fixnum(b'A' as i64));
    assert!(
        !completion_equals_with_parity(&mut ctx, &string, &prefix, &completion, true, true)
            .unwrap()
    );
}

#[test]
fn completion_compare_case_table_off_and_case_sensitive_stay_legacy() {
    let mut ctx = context_with_mappings(&[(0x131, b'I' as u32)]);
    let string = LispString::from_utf8("I");
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let completion = CompletionText::LispObject(Value::string("ı"));
    for (ignore, parity) in [(true, false), (false, true)] {
        assert_eq!(
            completion_prefix_matches_with_parity(
                &mut ctx,
                &string,
                &prefix,
                &completion,
                ignore,
                parity
            )
            .unwrap(),
            prefix.matches(&completion, ignore)
        );
        assert_eq!(
            completion_equals_with_parity(&mut ctx, &string, &prefix, &completion, ignore, parity)
                .unwrap(),
            completion_text_equals_string(&completion, prefix.characters(), ignore)
        );
    }
}
