//! Re-homed from display::window::tests::window_test (P1 B1, oracleb).
//!
//! The test is blocked in display's cfg(test) unit runtime by the core-owned
//! `set-fontset-font` subr, which the absent-layer runtime cannot supply.
//! Core's boot facade has every layer real. The body is the display original;
//! the only adaptations are the signed API drift (`crate::test_utils::new_context()`
//! for `Context::new(unit_runtime())`, the `edit.buffers` buffer accessor, and
//! the explicit freshness-trait imports) plus the explicit
//! `neovm_display::window::WindowLayoutLispBoundary` path.

use crate::window::DisplayFrameManagerExt;
use crate::window::display::WindowDisplayContextExt;

#[test]
fn fontset_changes_invalidate_window_query_and_attempt_freshness() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.current_buffer_id().unwrap();
    let frame = eval
        .frame_manager_mut()
        .create_frame("fontset-freshness", 800, 600, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let before = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    let attempt = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(set-fontset-font t #x25cb '(nil . \"iso10646-1\"))")
        .unwrap();
    let after = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    let after_attempt = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    assert_ne!(
        before, after,
        "geometry queries must not return old font measurements"
    );
    assert!(!before.same_scroll_content(&after));
    assert!(before.query_vscroll_delta(&after).is_none());
    for boundary in [
        neovm_display::window::WindowLayoutLispBoundary::BufferBody,
        neovm_display::window::WindowLayoutLispBoundary::WindowChrome,
    ] {
        assert!(!attempt.remains_valid_across(&after_attempt, boundary));
    }
}
