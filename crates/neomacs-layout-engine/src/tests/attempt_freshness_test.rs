use super::*;
use neovm_core::emacs_core::Context;
use neovm_core::window::DisplayFrameManagerExt;
use std::borrow::Cow;

#[test]
fn freshness_spans_default_on_and_preserve_explicit_capture_controls() {
    assert_eq!(FreshnessSpanMode::parse(None), FreshnessSpanMode::Reuse);
    for value in ["on", "1", "true", "yes", " ON "] {
        assert_eq!(
            FreshnessSpanMode::parse(Some(value.as_ref())),
            FreshnessSpanMode::Reuse
        );
    }
    for value in ["off", "0", "false", "", "invalid"] {
        assert_eq!(
            FreshnessSpanMode::parse(Some(value.as_ref())),
            FreshnessSpanMode::Capture
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            FreshnessSpanMode::parse(Some(std::ffi::OsStr::from_bytes(b"on\xff"))),
            FreshnessSpanMode::Capture
        );
    }
}

#[test]
fn native_preparation_passes_complete_inputs_while_control_recaptures() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("native-span", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    let conditions =
        crate::display_when::DisplayWhenConditions::evaluated_forms(Default::default());
    let admission = preparation_admission(
        neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome::Unchanged,
        &conditions,
    );
    assert_eq!(admission, LayoutLispAdmission::NativeOnly);
    let reused = after_span(&eval, &before, admission, FreshnessSpanMode::Reuse).unwrap();
    let captured = after_span(&eval, &before, admission, FreshnessSpanMode::Capture).unwrap();
    assert!(matches!(reused, Cow::Borrowed(_)));
    assert!(matches!(captured, Cow::Owned(_)));
    assert_eq!(reused, captured);
}

#[test]
fn native_window_end_publication_preserves_the_same_complete_token() {
    use neovm_core::buffer::{EmacsBytePos, LispCharPos1};
    use neovm_core::window::{MatrixRow0, WindowEndRecord};
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("x");
    let frame = eval
        .frame_manager_mut()
        .create_frame("native-publication", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.publish_redisplay_window_end(
        frame,
        window,
        WindowEndRecord::from_positions(
            LispCharPos1::new(2),
            EmacsBytePos::new(2),
            LispCharPos1::new(2),
            EmacsBytePos::new(2),
            MatrixRow0::new(0),
        ),
    );
    let reused = after_span(
        &eval,
        &before,
        LayoutLispAdmission::NativeOnly,
        FreshnessSpanMode::Reuse,
    )
    .unwrap();
    let captured = after_span(
        &eval,
        &before,
        LayoutLispAdmission::NativeOnly,
        FreshnessSpanMode::Capture,
    )
    .unwrap();
    assert!(matches!(reused, Cow::Borrowed(_)));
    assert_eq!(reused, captured);
}
