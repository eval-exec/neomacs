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
