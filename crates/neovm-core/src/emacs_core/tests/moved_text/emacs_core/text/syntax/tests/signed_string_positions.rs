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
use neovm_value::emacs_char::EmacsChar;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::symbol::CanonicalAccess;
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use strum::{EnumString, IntoStaticStr};

use neovm_edit::buffer::CharRange;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;
use neovm_value::value::positions::CharLen;
use neovm_value::value::positions::CharPos0;

use neovm_text::emacs_core::syntax::*;

use neovm_value::heap_types::{GnuDeclaredCharacterCount, GnuStringCharPos};

#[test]
fn signed_property_coordinate_is_retained_as_a_typed_diagnostic() {
    let context = crate::test_utils::new_context();
    let mut string = LispString::from_emacs_bytes_with_declared_chars(
        b"abcde".to_vec(),
        GnuDeclaredCharacterCount::try_from(1).expect("bounded metadata"),
    );
    with_intervals(&mut string, 1);
    let props = SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers);
    let run = StringSyntaxPropByteRun::new(props, &string, string.intervals().as_table());
    assert_eq!(run.position_error(), None);
    assert_eq!(run.syntax_table_prop_at_string_byte(3), None);
    assert_eq!(
        run.position_error(),
        Some(StringSyntaxPositionError::OutOfRange {
            position: GnuStringCharPos::new(-1)
        })
    );
    // A deferred diagnostic is sticky and must be checked by the frontend
    // before ANY match data/success is published. It is not a no-property result.
    run.syntax_table_prop_at_string_byte(4);
    assert_eq!(
        run.position_error()
            .expect("retained diagnostic")
            .position()
            .expect("coordinate diagnostic")
            .get(),
        -1
    );
}

#[test]
fn positive_property_coordinate_beyond_declared_bounds_is_also_rejected() {
    let context = crate::test_utils::new_context();
    let mut string = LispString::from_emacs_bytes_with_declared_chars(
        b"abcdefg".to_vec(),
        GnuDeclaredCharacterCount::try_from(1).expect("bounded metadata"),
    );
    with_intervals(&mut string, 1);
    let run = StringSyntaxPropByteRun::new(
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        &string,
        string.intervals().as_table(),
    );
    run.syntax_table_prop_at_string_byte(2);
    assert_eq!(
        run.position_error()
            .expect("out-of-range positive coordinate")
            .position()
            .expect("coordinate diagnostic")
            .get(),
        2
    );
}

#[test]
fn ordinary_property_and_ignore_controls_have_no_coordinate_diagnostic() {
    let context = crate::test_utils::new_context();
    let mut string = LispString::from_utf8("abécd");
    with_intervals(&mut string, 5);
    let honoring = StringSyntaxPropByteRun::new(
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        &string,
        string.intervals().as_table(),
    );
    for byte in [0, 1, 2, 4, 5] {
        honoring.property_free_until(byte);
        assert_eq!(honoring.position_error(), None);
    }
    let ignoring = StringSyntaxPropByteRun::new(
        SyntaxProperties::Ignore,
        &string,
        string.intervals().as_table(),
    );
    assert_eq!(ignoring.property_free_until(usize::MAX - 1), usize::MAX);
    assert_eq!(ignoring.position_error(), None);
}

fn with_intervals(string: &mut LispString, count: usize) {
    string.intervals_mut().put_property_for_object_char_len(
        CharRange::new(CharPos0::ZERO, CharPos0::new(count)),
        CharLen::new(count),
        Value::symbol("face"),
        Value::symbol("bold"),
    );
}
