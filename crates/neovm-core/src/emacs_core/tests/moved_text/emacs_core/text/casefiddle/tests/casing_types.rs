use super::*;

use neovm_edit::emacs_core::case_encoding::CaseEncoding;
use neovm_value::error::LispCondition;
use neovm_value::error::expect_args;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::value::ValueKind;

use crate::emacs_core::casetab_entries::builtin_set_case_table;
use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::casefiddle::*;
use neovm_text::emacs_core::casetab::CaseMap;
use neovm_text::emacs_core::casetab::CaseTableOverride;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

#[test]
fn missing_installed_case_entries_are_identity() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    super::super::casetab::builtin_set_case_table(&mut ev, vec![table]).unwrap();
    let cases = CaseTableOverride::for_buffer_readonly_with_symbols(
        ev.edit.buffers.current_buffer().unwrap(),
        ev.symbol_mode(),
    );
    for which in [CaseMap::Up, CaseMap::Down, CaseMap::Canon] {
        assert_eq!(cases.map(which, 'a' as i64), Some('a' as i64));
        assert_eq!(cases.map(which, 'É' as i64), Some('É' as i64));
    }
}

#[test]
fn target_policy_distinguishes_unibyte_strings_and_buffers() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let table = super::super::casetab::make_case_table_with_pair(304, 105);
    super::super::casetab::builtin_set_case_table(&mut ev, vec![table]).unwrap();
    let cases = CaseTableOverride::for_buffer_readonly_with_symbols(
        ev.edit.buffers.current_buffer().unwrap(),
        ev.symbol_mode(),
    );
    let input = LispString::from_unibyte(vec![b'i']);
    let string = casify_text_with_extents(
        &input,
        CaseAction::Up,
        CaseTarget::String,
        standard_word_predicate,
        &cases,
        |_| {},
    );
    let buffer = casify_text_with_extents(
        &input,
        CaseAction::Up,
        CaseTarget::Buffer,
        standard_word_predicate,
        &cases,
        |_| {},
    );
    assert_eq!(string.as_bytes(), b"I");
    assert_eq!(buffer.as_bytes(), b"0");
}
