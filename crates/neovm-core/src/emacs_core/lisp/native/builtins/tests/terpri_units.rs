//! Typed TERPRI destination state, separate from GNU-refreshed output oracles.
//! GNU src/print.c:142-162 validates/selects destinations before bolp at 647-650.
use super::{TerpriLineState, terpri_line_state};
use crate::buffer::{EmacsByteRange, LispCharPos1};
use crate::emacs_core::error::{FlowKind, IntoConditionSym, LispCondition};
use crate::emacs_core::eval::Context;
use crate::emacs_core::marker::{make_marker_value, make_registered_buffer_marker};
use crate::emacs_core::value::Value;

#[test]
fn terpri_buffer_line_state_tracks_destination_point_and_narrowed_minimum() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let destination = context.buffers.create_buffer("terpri-state-destination");
    context.buffers.get_mut(destination).unwrap().insert("é\nb");
    let target = Value::make_buffer(destination);
    // The current buffer remains the caller; the explicit stream owns point.
    for (position, expected) in [
        (1, TerpriLineState::BeginningOfLine),
        (2, TerpriLineState::NeedsNewline),
        (3, TerpriLineState::BeginningOfLine),
        (4, TerpriLineState::NeedsNewline),
    ] {
        let buffer = context.buffers.get_mut(destination).unwrap();
        let point = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(position));
        buffer.goto_emacs_byte_pos(point);
        assert_eq!(terpri_line_state(&context, target).unwrap(), expected);
    }
    let buffer = context.buffers.get_mut(destination).unwrap();
    let start = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(2));
    let end = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(4));
    buffer.narrow_to_emacs_byte_range(EmacsByteRange::new(start, end));
    buffer.goto_emacs_byte_pos(start);
    assert_eq!(
        terpri_line_state(&context, target).unwrap(),
        TerpriLineState::BeginningOfLine
    );
}

#[test]
fn terpri_marker_line_state_uses_marker_without_moving_buffer_point() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let destination = context.buffers.create_buffer("terpri-state-marker");
    context.buffers.get_mut(destination).unwrap().insert("é\nb");
    let marker = make_registered_buffer_marker(
        &mut context.buffers,
        destination,
        LispCharPos1::new(2),
        false,
    );
    let buffer = context.buffers.get_mut(destination).unwrap();
    let point = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(3));
    buffer.goto_emacs_byte_pos(point);
    assert_eq!(
        terpri_line_state(&context, marker).unwrap(),
        TerpriLineState::NeedsNewline
    );
    assert_eq!(
        context
            .buffers
            .get(destination)
            .unwrap()
            .point_emacs_byte_pos(),
        point
    );
    let beginning = make_registered_buffer_marker(
        &mut context.buffers,
        destination,
        LispCharPos1::new(3),
        false,
    );
    assert_eq!(
        terpri_line_state(&context, beginning).unwrap(),
        TerpriLineState::BeginningOfLine
    );
}

#[test]
fn terpri_marker_validation_precedes_beginning_of_line_shortcut() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let detached = make_marker_value(None, None, false);
    let destination = context.buffers.create_buffer("terpri-state-invalid-marker");
    context.buffers.get_mut(destination).unwrap().insert("é\nb");
    // Position one is BOL in the full buffer but inaccessible after narrowing.
    let inaccessible = make_registered_buffer_marker(
        &mut context.buffers,
        destination,
        LispCharPos1::new(1),
        false,
    );
    let buffer = context.buffers.get_mut(destination).unwrap();
    let start = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(2));
    let end = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(4));
    buffer.narrow_to_emacs_byte_range(EmacsByteRange::new(start, end));
    for target in [detached, inaccessible] {
        let error = terpri_line_state(&context, target).unwrap_err();
        let FlowKind::Signal(payload) = error.into_kind() else {
            panic!("marker validation must return a Lisp condition");
        };
        assert_eq!(payload.symbol, LispCondition::Error.condition_sym());
    }
}
