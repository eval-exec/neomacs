//! Focused state tests: actual GNU oracle covers the public60-case behavior.

use super::*;

use LispSyntaxObjectReuse::{CanonicalBare, Fresh};
use neovm_edit::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
use neovm_edit::buffer::{
    Buffer, BufferManager, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, LispCharPos1,
    TextPropertyTable,
};
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_edit::emacs_core::textprop::CharPropertyResolver;
use neovm_text::emacs_core::regex_emacs::BoundarySyntaxMode;
use neovm_text::emacs_core::regex_emacs::SyntaxAssertion;
use neovm_text::emacs_core::syntax::parse_loop::{
    Entry, Plain, SafePositionRecorder, ScanEnd, ScanFinish, run_parse_loop,
};
use neovm_value::emacs_char::EmacsChar;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::symbol::CanonicalAccess;
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use strum::{EnumString, IntoStaticStr};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::syntax::*;
use neovm_value::heap_types::GnuStringCharPos;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

use neovm_value::heap_types::GnuDeclaredCharacterCount;

#[test]
fn supplied_no_interval_state_reports_gnu_to_end_logic_after_initial_run() {
    let context = crate::test_utils::new_context();
    let source = supplied_ascii(0);
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let run = StringSyntaxPropByteRun::for_source(
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        StringSyntaxPositionSource::try_from(&source).expect("validated test source"),
        None,
    );
    assert_eq!(run.syntax_table_prop_at_string_byte(0), None);
    assert_eq!(run.position_error(), None);
    assert_eq!(run.syntax_table_prop_at_string_byte(1), None);
    assert_eq!(
        run.position_error(),
        Some(StringSyntaxPositionError::ToEndIntervals)
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

#[test]
fn supplied_first_negative_lookup_signals_its_actual_signed_coordinate() {
    let context = crate::test_utils::new_context();
    let source = supplied_ascii(0);
    let string = source.as_lisp_string().expect("test string");
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let run = StringSyntaxPropByteRun::for_source(
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        StringSyntaxPositionSource::try_from(&source).expect("validated test source"),
        None,
    );
    assert_eq!(run.syntax_table_prop_at_string_byte(4), None);
    assert_eq!(
        run.position_error(),
        Some(StringSyntaxPositionError::OutOfRange {
            position: GnuStringCharPos::new(-1)
        })
    );
    // Already-recorded errors suppress later callback cache side effects.
    // From anchor(char=-1,byte=4), byte2 is a backward tie and becomes -3.
    // An accidental callback conversion at byte0 would instead return2.
    run.syntax_table_prop_at_string_byte(0);
    assert_eq!(
        neovm_value::string_pos_cache::string_byte_to_gnu_char(source, string, 2).get(),
        -3
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

#[test]
fn supplied_capability_query_never_skips_or_performs_gnu_syntax_reads() {
    let context = crate::test_utils::new_context();
    let source = supplied_ascii(0);
    let run = StringSyntaxPropByteRun::for_source(
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        StringSyntaxPositionSource::try_from(&source).expect("validated test source"),
        None,
    );
    assert_eq!(run.property_free_until(4), 4);
    assert_eq!(run.position_error(), None);
    // The first actual read must still perform interval_of and report -1.
    assert_eq!(run.syntax_table_prop_at_string_byte(4), None);
    assert_eq!(
        run.position_error(),
        Some(StringSyntaxPositionError::OutOfRange {
            position: GnuStringCharPos::new(-1)
        })
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

#[test]
fn ignoring_supplied_counts_keeps_the_position_independent_gnu_path() {
    let context = crate::test_utils::new_context();
    let source = supplied_ascii(0);
    let ignoring = StringSyntaxPropByteRun::for_source(
        SyntaxProperties::Ignore,
        StringSyntaxPositionSource::try_from(&source).expect("validated test source"),
        None,
    );
    assert_eq!(ignoring.property_free_until(4), usize::MAX);
    assert_eq!(ignoring.syntax_table_prop_at_string_byte(4), None);
    assert_eq!(ignoring.position_error(), None);
    let ordinary = LispString::from_utf8("abécd");
    let computed = StringSyntaxPropByteRun::new(
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        &ordinary,
        None,
    );
    assert_eq!(computed.property_free_until(4), usize::MAX);
    assert_eq!(computed.position_error(), None);
}

#[test]
fn syntax_heap_source_validates_object_payload_and_rejects_scalars() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        StringSyntaxPositionSource::try_from(&Value::NIL).unwrap_err(),
        StringSyntaxPositionSourceError::NotString
    );
    let source = supplied_ascii(0);
    let binding = StringSyntaxPositionSource::try_from(&source).expect("test string");
    assert!(std::ptr::eq(
        binding.string(),
        source.as_lisp_string().expect("test payload")
    ));
    // The evaluator publishes the byte-mutation hook at its one-time seams;
    // this test drives the cache without a Context, so install the
    // production hook first (assertions unchanged).
    neovm_value::string_pos_cache::install_notification_hook_for_test();
    neovm_value::string_pos_cache::reset_string_pos_cache();
    assert_eq!(binding.byte_to_gnu_char_pos(4).get(), -1);
    assert_eq!(
        neovm_value::string_pos_cache::string_byte_to_gnu_char(source, binding.string(), 2).get(),
        -3
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

fn supplied_ascii(chars: i128) -> Value {
    Value::heap_string(LispString::from_emacs_bytes_with_declared_chars(
        b"abcde".to_vec(),
        GnuDeclaredCharacterCount::try_from(chars).expect("bounded metadata"),
    ))
}
