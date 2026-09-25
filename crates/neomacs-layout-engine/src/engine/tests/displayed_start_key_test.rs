//! A layout that moves the window start itself retains a key naming the start
//! it displayed, so the next frame can still reuse its rows.
//!
//! Point moved past the window's end: the layout scrolls within one attempt
//! (GNU's `try_scrolling`/recentering inside `redisplay_window`). The key used
//! to keep the start from before the layout, so the first keystroke after
//! `M->` from a distant point was a full layout -- and, under
//! `NEOMACS_MODE_LINE_GATE=gnu`, evaluated the mode line where GNU's
//! optimization 1 keeps it.

use super::*;
use crate::incremental_layout::mode_line_gate::{ModeLineGate, set_mode_line_gate_for_test};

const LINE: &str = "(defun f (a b) (+ a b))\n";

struct GateGuard;

impl Drop for GateGuard {
    fn drop(&mut self) {
        set_mode_line_gate_for_test(None);
    }
}

fn redisplay(
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> u32 {
    engine.layout_frame_rust(eval, frame_id);
    activate_last_engine_presentation(eval, engine, frame_id);
    crate::display_status_line::mode_line_eval_count()
}

#[test]
fn typing_after_the_layout_scrolled_to_point_reuses_the_window() {
    set_mode_line_gate_for_test(Some(ModeLineGate::Gnu));
    let _gate = GateGuard;
    let (mut eval, frame_id, buf_id, window) = incr_editing_frame(&LINE.repeat(200), 800, 600);
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.set_buffer_local("mode-line-format", Value::string("ML"));
        buf.set_buffer_local("bidi-paragraph-direction", Value::symbol("left-to-right"));
    }
    let mut engine = LayoutEngine::new();
    redisplay(&mut eval, &mut engine, frame_id);
    let start_before = window_start_of(&eval, frame_id, window);

    // Point far past the window's end: this layout moves the start.
    eval.eval_str("(goto-char (point-max))").expect("goto");
    redisplay(&mut eval, &mut engine, frame_id);
    let start_after = window_start_of(&eval, frame_id, window);
    assert_ne!(start_before, start_after, "the layout scrolled to point");

    eval.eval_str("(insert \"x\")").expect("insert");
    let evaluations = redisplay(&mut eval, &mut engine, frame_id);
    let stats = engine.last_layout_stats();
    // The text window replays the edit; the one full window is the
    // mini-window, laid out from scratch every frame unless
    // NEOMACS_LAYOUT_MINI_STILL is on.
    assert_eq!(
        stats.edit_windows, 1,
        "the keystroke after the scroll replays the edit: {stats:?}"
    );
    assert_eq!(stats.full_windows, 1, "only the mini-window: {stats:?}");
    assert_eq!(evaluations, 0, "GNU's optimization 1 keeps the mode line");
    assert_eq!(
        window_start_of(&eval, frame_id, window),
        start_after,
        "the start did not move again"
    );
}

fn window_start_of(
    eval: &Context,
    frame_id: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
) -> Option<i64> {
    eval.frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window))
        .and_then(neovm_core::window::Window::window_start)
        .map(|start| start.as_i64())
}
