use super::*;

use neovm_edit::buffer::buffer::BUFFER_SLOT_CATEGORY_TABLE;
use neovm_text::emacs_core::text_host::TextHost;
use neovm_value::boolvec::{self, BoolVectorView};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::tagged::SymbolMode;
use std::cell::{Cell, RefCell};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::category::*;
use neovm_value::value::Value;

use neovm_value::error::FlowKind;

use neovm_value::heap_types::LispString;

#[test]
fn copy_category_table_deep_copies_docstrings_and_sets() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    builtin_define_category(
        &mut eval,
        vec![Value::char('!'), Value::string("bang"), table],
    )
    .unwrap();
    builtin_modify_category_entry(&mut eval, vec![Value::char('A'), Value::char('!'), table])
        .unwrap();

    let copy = builtin_copy_category_table(vec![table]).unwrap();
    builtin_define_category(
        &mut eval,
        vec![Value::char('"'), Value::string("quote"), copy],
    )
    .unwrap();
    builtin_modify_category_entry(&mut eval, vec![Value::char('B'), Value::char('!'), copy])
        .unwrap();

    assert!(
        builtin_category_docstring(&mut eval, vec![Value::char('"'), table])
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        builtin_category_set_mnemonics(vec![
            super::super::chartable::builtin_char_table_range(vec![table, Value::char('B')], None)
                .unwrap(),
        ])
        .unwrap(),
        Value::string("")
    );
    assert_eq!(
        builtin_category_set_mnemonics(vec![
            super::super::chartable::builtin_char_table_range(vec![copy, Value::char('B')], None)
                .unwrap(),
        ])
        .unwrap(),
        Value::string("!")
    );

    let table_docs =
        super::super::chartable::builtin_char_table_extra_slot(vec![table, Value::fixnum(0)])
            .unwrap();
    let copy_docs =
        super::super::chartable::builtin_char_table_extra_slot(vec![copy, Value::fixnum(0)])
            .unwrap();
    assert!(table_docs.is_vector(), "expected category docstring vector");
    assert!(copy_docs.is_vector(), "expected category docstring vector");
    assert_ne!(table_docs, copy_docs);
}

#[test]
fn define_category_redefinition_matches_gnu_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    builtin_define_category(
        &mut eval,
        vec![Value::char('a'), Value::string("one"), table],
    )
    .unwrap();
    let err = builtin_define_category(
        &mut eval,
        vec![Value::char('a'), Value::string("two"), table],
    )
    .unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Category ‘a’ is already defined")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn get_unused_category_scans_ascii_graphics() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    assert_eq!(
        builtin_get_unused_category(&mut eval, vec![table]).unwrap(),
        Value::char(' ')
    );
    builtin_define_category(
        &mut eval,
        vec![Value::char(' '), Value::string("space"), table],
    )
    .unwrap();
    assert_eq!(
        builtin_get_unused_category(&mut eval, vec![table]).unwrap(),
        Value::char('!')
    );
}

#[test]
fn set_category_table_nil_returns_current_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let current = builtin_category_table(&mut eval, vec![]).unwrap();
    let out = builtin_set_category_table(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(current, out);
}

#[test]
fn modify_category_entry_honors_optional_table_argument() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    builtin_define_category(
        &mut eval,
        vec![Value::char('!'), Value::string("bang"), table],
    )
    .unwrap();
    builtin_modify_category_entry(
        &mut eval,
        vec![
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('C' as i64)),
            Value::char('!'),
            table,
        ],
    )
    .unwrap();

    for ch in ['A', 'B', 'C'] {
        let set =
            super::super::chartable::builtin_char_table_range(vec![table, Value::char(ch)], None)
                .unwrap();
        assert_eq!(
            builtin_category_set_mnemonics(vec![set]).unwrap(),
            Value::string("!")
        );
    }
    let current = builtin_category_table(&mut eval, vec![]).unwrap();
    let current_set =
        super::super::chartable::builtin_char_table_range(vec![current, Value::char('A')], None)
            .unwrap();
    assert_eq!(
        builtin_category_set_mnemonics(vec![current_set]).unwrap(),
        Value::string("")
    );
}

#[test]
fn modify_category_entry_range_preserves_existing_subranges() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    for (category, doc) in [('!', "bang"), ('#', "hash"), ('?', "question")] {
        builtin_define_category(
            &mut eval,
            vec![Value::char(category), Value::string(doc), table],
        )
        .unwrap();
    }

    builtin_modify_category_entry(&mut eval, vec![Value::char('A'), Value::char('!'), table])
        .unwrap();
    builtin_modify_category_entry(&mut eval, vec![Value::char('B'), Value::char('?'), table])
        .unwrap();
    builtin_modify_category_entry(
        &mut eval,
        vec![
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('B' as i64)),
            Value::char('#'),
            table,
        ],
    )
    .unwrap();

    let a_set =
        super::super::chartable::builtin_char_table_range(vec![table, Value::char('A')], None)
            .unwrap();
    let b_set =
        super::super::chartable::builtin_char_table_range(vec![table, Value::char('B')], None)
            .unwrap();
    assert_eq!(
        builtin_category_set_mnemonics(vec![a_set]).unwrap(),
        Value::string("!#")
    );
    assert_eq!(
        builtin_category_set_mnemonics(vec![b_set]).unwrap(),
        Value::string("#?")
    );
}

#[test]
fn modify_category_entry_interns_equal_category_sets() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    builtin_define_category(&mut eval, vec![Value::char('x'), Value::string("x"), table]).unwrap();

    builtin_modify_category_entry(&mut eval, vec![Value::char('a'), Value::char('x'), table])
        .unwrap();
    builtin_modify_category_entry(&mut eval, vec![Value::char('b'), Value::char('x'), table])
        .unwrap();

    let a_set =
        super::super::chartable::builtin_char_table_range(vec![table, Value::char('a')], None)
            .unwrap();
    let b_set =
        super::super::chartable::builtin_char_table_range(vec![table, Value::char('b')], None)
            .unwrap();
    assert_eq!(a_set, b_set);
}

#[test]
fn define_category_preserves_raw_unibyte_docstring() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    builtin_define_category(&mut eval, vec![Value::char('x'), raw, table]).unwrap();
    let result = builtin_category_docstring(&mut eval, vec![Value::char('x'), table]).unwrap();
    let text = result.as_lisp_string().expect("string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn define_category_symbol_signals_categoryp() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    // GNU's `Fdefine_category` runs `CHECK_CATEGORY`, so a symbol (not a
    // character) signals `(wrong-type-argument categoryp x)`.
    let err = builtin_define_category(
        &mut eval,
        vec![Value::symbol("x"), Value::string("doc"), table],
    )
    .unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("categoryp"), Value::symbol("x")]
            );
        }
        other => panic!("expected wrong-type-argument categoryp, got {other:?}"),
    }
}

#[test]
fn define_category_control_char_signals_categoryp() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    let err = builtin_define_category(
        &mut eval,
        vec![Value::fixnum(1), Value::string("doc"), table],
    )
    .unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("categoryp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument categoryp, got {other:?}"),
    }
}

#[test]
fn define_category_valid_char_still_defines() {
    crate::test_utils::init_test_tracing();
    let mut eval = fresh_eval();
    let table = builtin_make_category_table(vec![]).unwrap();
    builtin_define_category(
        &mut eval,
        vec![Value::char('!'), Value::string("bang"), table],
    )
    .unwrap();
    assert_eq!(
        builtin_category_docstring(&mut eval, vec![Value::char('!'), table]).unwrap(),
        Value::string("bang")
    );
}

fn fresh_eval() -> crate::emacs_core::eval::Context {
    reset_category_thread_locals();
    crate::test_utils::new_context()
}
