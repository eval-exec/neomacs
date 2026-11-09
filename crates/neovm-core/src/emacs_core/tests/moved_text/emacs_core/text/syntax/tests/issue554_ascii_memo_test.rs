use super::*;

use LispSyntaxObjectReuse::{CanonicalBare, Fresh};
use neovm_edit::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_edit::emacs_core::textprop::CharPropertyResolver;
use neovm_text::emacs_core::regex_emacs::BoundarySyntaxMode;
use neovm_text::emacs_core::regex_emacs::SyntaxAssertion;
use neovm_text::emacs_core::syntax::parse_loop::{
    Entry, Plain, SafePositionRecorder, ScanEnd, ScanFinish, run_parse_loop,
};
use neovm_text::test_support::authority::{reset_syntax_table_decodes_for_test, syntax_table};
use neovm_value::emacs_char::EmacsChar;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::heap_types::{GnuStringCharPos, LispString, StringCharacterCountKind};
use neovm_value::symbol::CanonicalAccess;
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use strum::{EnumString, IntoStaticStr};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::syntax::*;
use neovm_value::heap_types::byte_positions::EmacsBytePos;
use neovm_value::value::Value;

use neovm_value::tagged::SymbolMode;

#[test]
fn issue554_ascii_memo_shares_entries_across_symbol_policies() {
    let mut context = crate::test_utils::new_context();
    context
        .eval_str("(progn (erase-buffer) (insert (make-string 128 ?a)))")
        .unwrap();
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    let purpose = unsafe { context.tagged_heap_mut() }
        .alloc_symbol_with_pos(Value::symbol("char-code-property-table"), Value::fixnum(0));
    let whitespace = syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Whitespace));
    let word = syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Word));
    let open = syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Open));
    let parent = super::super::chartable::make_char_table_with_extra_slots(purpose, whitespace, 5);
    let child = super::super::chartable::make_char_table_with_extra_slots(purpose, Value::NIL, 5);
    super::super::chartable::builtin_set_char_table_range(
        vec![parent, Value::fixnum(i64::from(b'a')), word],
        None,
    )
    .unwrap();
    super::super::chartable::builtin_set_char_table_range(
        vec![child, Value::fixnum(i64::from(b'(')), open],
        None,
    )
    .unwrap();
    super::super::chartable::builtin_set_char_table_parent(vec![child, parent]).unwrap();
    let table = syntax_table(child);

    // GNU lisp.h:2133-2146 / chartab.c:230-246: ASCII, default and
    // parent reads never inspect the purpose symbol. A positioned uniprop
    // purpose changes non-ASCII compressed lookup, but not these slots.
    for code in 0..128 {
        assert_eq!(
            syntax_entry_at_char_code_with_symbols(&child, code, SymbolMode::Bare),
            syntax_entry_at_char_code_with_symbols(&child, code, SymbolMode::Positioned),
        );
    }
    assert_eq!(
        syntax_entry_from_table(&table, 'a').class,
        SyntaxClass::Word
    );
    assert_eq!(
        syntax_entry_from_table(&table, '(').class,
        SyntaxClass::Open
    );
    assert_eq!(
        syntax_entry_from_table(&table, 'z').class,
        SyntaxClass::Whitespace
    );
    let expected = std::array::from_fn::<_, 128, _>(|code| {
        syntax_entry_from_table(&table, code as u8 as char)
    });
    let _ = flat_ascii_entries_for_table(&table);
    let _ = flat_ascii_syntax_entry(&table, b'a');
    reset_syntax_table_decodes_for_test();
    let buffer = context.edit.buffers.current_buffer().unwrap();
    for _ in 0..5 {
        for properties in [SyntaxProperties::Ignore, SyntaxProperties::PositionedIgnore] {
            let bytes = SyntaxPropByteRun::new(properties);
            let chars = SyntaxPropRange::new(properties);
            for (code, expected) in expected.iter().enumerate() {
                let ch = code as u8 as char;
                assert_eq!(
                    effective_syntax_entry_for_char_at_byte(
                        buffer,
                        &table,
                        ch,
                        EmacsBytePos::new(code),
                        &bytes,
                    ),
                    *expected,
                );
                assert_eq!(
                    effective_syntax_entry_for_abs_char(buffer, &table, ch, code, &chars),
                    *expected,
                );
                assert!(bytes.ascii()[code].is_some());
                assert!(chars.ascii()[code].is_some());
            }
        }
    }
    assert_eq!(syntax_table_decodes_for_test(), 0);
}

#[test]
fn issue554_ascii_syntax_properties_override_a_warm_table_memo() {
    let mut context = crate::test_utils::new_context();
    context
        .eval_str(
            "(progn (erase-buffer) (insert \"aa\")
                    (put-text-property 1 2 'syntax-table '(1)))",
        )
        .unwrap();
    let table = SyntaxTable::new_standard();
    let word = flat_ascii_syntax_entry(&table, b'a');
    assert_eq!(word.class, SyntaxClass::Word);
    for mode in [SymbolMode::Bare, SymbolMode::Positioned] {
        let properties = SyntaxProperties::for_scan_with_symbols(
            true,
            context.variables(),
            &context.edit.buffers,
            mode,
        );
        let bytes = SyntaxPropByteRun::new(properties);
        let chars = SyntaxPropRange::new(properties);
        neovm_text::test_support::authority::set_syntax_prop_byte_run_ascii_for_test(
            &bytes,
            b'a' as usize,
            Some(word),
        );
        neovm_text::test_support::authority::set_syntax_prop_range_ascii_for_test(
            &chars,
            b'a' as usize,
            Some(word),
        );
        let buffer = context.edit.buffers.current_buffer().unwrap();
        assert_eq!(
            effective_syntax_entry_for_char_at_byte(
                buffer,
                &table,
                'a',
                EmacsBytePos::new(0),
                &bytes,
            )
            .class,
            SyntaxClass::Punctuation,
        );
        assert_eq!(
            effective_syntax_entry_for_abs_char(buffer, &table, 'a', 0, &chars).class,
            SyntaxClass::Punctuation,
        );
        assert_eq!(
            effective_syntax_entry_for_char_at_byte(
                buffer,
                &table,
                'a',
                EmacsBytePos::new(1),
                &bytes,
            )
            .class,
            SyntaxClass::Word,
        );
        assert_eq!(
            effective_syntax_entry_for_abs_char(buffer, &table, 'a', 1, &chars).class,
            SyntaxClass::Word,
        );
    }
}
