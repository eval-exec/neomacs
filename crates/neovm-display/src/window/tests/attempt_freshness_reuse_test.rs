//! Request-local buffer capture reuse preserves owned freshness inputs.
use crate::window::DisplayFrameManagerExt;
use crate::window::display::WindowDisplayContextExt;
use neovm_eval::emacs_core::eval::Context;

#[test]
fn same_buffer_attempt_captures_fontifier_identity_once() {
    let (eval, live, frame, window) =
        crate::test_utils::upper_layer_helpers::attempt_freshness_projections(Context::new(
            crate::test_utils::unit_runtime(),
        ));
    // Initialize the existing symbol table and all first-read metadata before
    // isolating this request's capture budget. This is not an Ir measurement.
    eval.window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    neovm_value::intern::reset_intern_calls();
    let captured = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    let names = neovm_value::intern::intern_call_names();
    let reads = names
        .iter()
        .filter(|name| name.as_str() == "fontification-functions")
        .count();
    // The capture projects the hook's effective value through its
    // pre-resolved identity, so the budget is no by-name resolution at all.
    assert_eq!(reads, 0, "capture resolved the hook by name");
    assert_eq!(captured.live_buffer, captured.source_buffer);
    assert_eq!(
        captured.live_buffer,
        eval.buffer_layout_inputs(live).unwrap()
    );
}

#[test]
fn native_font_open_and_matrix_growth_refresh_attempt_inputs() {
    use crate::window::DisplayFrameExt;
    use crate::window::WindowLayoutLispAdmission;
    use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};

    let (mut eval, live, frame, window) =
        crate::test_utils::upper_layer_helpers::attempt_freshness_projections(Context::new(
            crate::test_utils::unit_runtime(),
        ));
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(neovm_value::value::Value::symbol("neo")));
    let identity = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11);
    identity.observe_opened_font_metrics(12, 4);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_display_identity(crate::window::FrameDisplayIdentity::Graphical(
            identity.clone(),
        ));
    let captured = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    assert!(matches!(
        eval.window_layout_attempt_freshness_after_span(
            &captured,
            WindowLayoutLispAdmission::NativeOnly
        )
        .unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));

    identity.observe_opened_font_metrics(9, 3);
    let refreshed = eval
        .window_layout_attempt_freshness_after_span(
            &captured,
            WindowLayoutLispAdmission::NativeOnly,
        )
        .unwrap();
    assert!(matches!(&refreshed, std::borrow::Cow::Owned(_)));
    assert_eq!(refreshed.frame.smallest_opened_font_height, 12);
    drop(refreshed);

    let captured = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
        .display_mut()
        .unwrap()
        .glyph_matrix_row_scale
        .grow_if(std::num::NonZeroU32::MIN)
        .unwrap();
    let refreshed = eval
        .window_layout_attempt_freshness_after_span(
            &captured,
            WindowLayoutLispAdmission::NativeOnly,
        )
        .unwrap();
    assert!(matches!(&refreshed, std::borrow::Cow::Owned(_)));
    assert_eq!(refreshed.window.glyph_matrix_row_scale.row_scale().get(), 2);
}
