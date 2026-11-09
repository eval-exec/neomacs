//! motion_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use crate::window::DisplayFrameManagerExt;
use crate::window::display::WindowDisplayContextExt;
use neovm_display::test_utils::motion_test_upper_layer_helpers::{eob_overlay_snapshot, row};
use neovm_edit::buffer::LispCharPos1;
use neovm_value::value::Value;

#[test]
fn integer_motion_expands_a_query_that_covers_source_eob_but_not_its_overlay_tail() {
    use crate::window::{EobOverlayMotionCoverage, WindowLayoutQuery, WindowLayoutQueryOutcome};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let mut eval = crate::test_utils::new_context();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("abc");
    let frame = eval
        .frame_manager_mut()
        .create_frame("overlay-motion", 400, 240, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let calls = Arc::new(AtomicUsize::new(0));
    let query_calls = Arc::clone(&calls);
    eval.install_window_layout_query(move |_, _, _, scope| {
        let crate::window::WindowLayoutQueryScope::Rows { count, .. } = scope else {
            panic!("motion requests rows");
        };
        let mut snapshot = if query_calls.fetch_add(1, Ordering::Relaxed) == 0 {
            assert_eq!(count.get(), 4);
            let mut partial = eob_overlay_snapshot(false, 3);
            partial.rows.pop();
            partial.eob_overlay_motion = EobOverlayMotionCoverage::Incomplete {
                anchor: LispCharPos1::new(4),
            };
            partial
        } else {
            assert_eq!(count.get(), 8);
            eob_overlay_snapshot(false, 5)
        };
        snapshot.window_id = window;
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(
            LispCharPos1::new(4),
            Some(snapshot),
        ))
    })
    .expect("install the layout-query handler");
    let result = eval
        .eval_str("(let ((noninteractive nil)) (goto-char 1) (list (vertical-motion 1) (point)))")
        .expect("source EOB does not prematurely terminate an unfinished display walk");
    assert_eq!(neovm_eval::emacs_core::print::print_value(&result), "(6 4)");
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}
