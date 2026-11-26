//! Original layout-source evidence survives native empty chrome write-back.
//!
//! This exercises the real capture and formatter boundary, not a whole leaf
//! retry: the small whole-frame overlay controls did not reproduce the bug.
use super::*;
use crate::cursor_source_dependencies::{Capture, pause, string_properties};
use crate::string_property_snapshot::StringSourceRootScope;

fn render_empty_native_chrome(
    eval: &mut Context,
    window: neovm_core::window::WindowId,
    buffer: BufferId,
    format: Value,
) -> Value {
    // Production chrome suspends only the containing source capture. Its
    // mutations must still be checked against that ORIGINAL capture afterward.
    let _paused = pause();
    let formatted = neovm_core::emacs_core::xdisp::format_mode_line_for_display_with_sources(
        eval,
        format,
        Value::make_window(window.0),
        Value::make_buffer(buffer),
        80,
    );
    assert!(!eval.has_mode_line_display_flow());
    assert_eq!(formatted.value().as_str_owned().as_deref(), Some(""));
    formatted.value()
}

#[test]
fn empty_native_chrome_preserves_original_empty_source_capture() {
    let (mut eval, frame, buffer, window) = incr_editing_frame("body\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let source = eval
        .eval_str(r#"(setq neo-chrome-original-source (string-as-unibyte ""))"#)
        .expect("root the original empty unibyte source in Lisp");
    let format = eval
        .eval_str(r#"(setq mode-line-format '((:eval nil)))"#)
        .expect("root the actual empty native mode-line format");
    assert_eq!(source.as_str_owned().as_deref(), Some(""));
    assert!(!source.as_lisp_string().unwrap().is_multibyte());

    let capture = Capture::begin(&eval);
    let snapshot = string_properties(
        source,
        &StringSourceRootScope::Rooted(eval.owned_root_retainer()),
    );
    assert!(snapshot.is_none(), "observe original interval absence");
    assert!(capture.unchanged());

    let output = render_empty_native_chrome(&mut eval, window, buffer, format);
    assert!(
        neovm_core::emacs_core::value::eq_value(&source, &output),
        "the real formatter must write back the same canonical empty object"
    );
    assert!(
        capture.unchanged(),
        "native empty chrome changed no original source bytes or properties"
    );
    let original = capture
        .finish()
        .expect("seal the original capture without recapture or renewal");
    assert!(original.unchanged());
    assert!(original.ordinary_cursor_source_proof().is_some());
}

fn actual_source_mutation_rejects_original_capture(
    source_expression: &str,
    mutation: &str,
    changed_state: &str,
    has_properties: bool,
) {
    let (mut eval, frame, buffer, window) = incr_editing_frame("body\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let source = eval
        .eval_str(&format!(
            "(setq neo-chrome-original-source {source_expression})"
        ))
        .expect("root original source");
    let format = eval
        .eval_str(r#"(setq mode-line-format '((:eval nil)))"#)
        .unwrap();
    let capture = Capture::begin(&eval);
    let snapshot = string_properties(
        source,
        &StringSourceRootScope::Rooted(eval.owned_root_retainer()),
    );
    assert_eq!(snapshot.is_some(), has_properties);
    assert!(capture.unchanged());
    render_empty_native_chrome(&mut eval, window, buffer, format);
    assert!(
        capture.unchanged(),
        "unrelated native chrome preserves this source"
    );

    {
        let _paused = pause();
        eval.eval_str(mutation).expect("actual source mutation");
        assert!(eval.eval_str(changed_state).unwrap().is_truthy());
    }

    assert!(
        !capture.unchanged(),
        "real source mutation must revoke evidence"
    );
    assert!(
        capture.finish().is_none(),
        "do not seal or refresh changed evidence"
    );
}

#[test]
fn native_chrome_source_capture_rejects_real_interval_removal() {
    actual_source_mutation_rejects_original_capture(
        r#"(propertize "abc" 'face 'bold)"#,
        "(set-text-properties 0 3 nil neo-chrome-original-source)",
        "(null (text-properties-at 0 neo-chrome-original-source))",
        true,
    );
}

#[test]
fn native_chrome_source_capture_rejects_real_byte_edit() {
    actual_source_mutation_rejects_original_capture(
        r#"(copy-sequence "abc")"#,
        "(aset neo-chrome-original-source 0 ?z)",
        r#"(equal neo-chrome-original-source "zbc")"#,
        false,
    );
}
