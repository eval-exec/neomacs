use super::*;
use crate::window::display::WindowDisplayContextExt;
use neovm_eval::emacs_core::eval::Context;
use std::borrow::Cow;

fn fixture() -> (Context, FrameId, WindowId, BufferId) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("native-freshness", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    (eval, frame, window, buffer)
}

#[test]
fn possible_lisp_always_recaptures_the_complete_token() {
    let (mut eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("changed source");
    let after = eval
        .window_layout_attempt_freshness_after_span(
            &before,
            WindowLayoutLispAdmission::MayHaveEntered,
        )
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn native_character_table_initialization_refuses_borrowed_inputs() {
    let (eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    crate::window::note_char_table_layout_mutation();
    let after = eval
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly)
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn native_dispatch_publication_refuses_borrowed_inputs() {
    let (mut eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.obarray_mut()
        .set_symbol_function("native-freshness-dispatch", Value::T);
    let after = eval
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly)
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn previously_captured_fontset_generation_refuses_native_borrow() {
    let (eval, frame, window, buffer) = fixture();
    let mut before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    // Represent a token from immediately before an independent registry write,
    // without mutating process-global font definitions used by other fixtures.
    before.fontset_generation = before.fontset_generation.wrapping_sub(1);
    let after = eval
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly)
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn native_span_cannot_borrow_another_contexts_input_token() {
    let (eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    let (other, _, _, _) = fixture();
    let after = other
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly);
    assert!(!matches!(after, Some(Cow::Borrowed(_))));
}
