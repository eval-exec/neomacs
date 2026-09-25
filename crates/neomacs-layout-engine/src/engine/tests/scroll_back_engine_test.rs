//! P3.5 G3 (`NEOMACS_LAYOUT_SCROLL_BACK=on`): a window whose start moved
//! BACK walks only the newly exposed rows and reuses its old rows below
//! them, shifted down (GNU `try_window_reusing_current_matrix`). Each frame
//! is compared with a fresh full layout of the same state.

use super::*;
use crate::incremental_layout::edit_sync::set_scroll_back_for_test;

struct ScrollBackGuard;

impl ScrollBackGuard {
    fn on() -> Self {
        set_scroll_back_for_test(Some(true));
        ScrollBackGuard
    }
}

impl Drop for ScrollBackGuard {
    fn drop(&mut self) {
        set_scroll_back_for_test(None);
    }
}

const LINE: &str = "(defun f (a b) (+ a b))\n";

/// Scroll the selected window to `start_line` with point `point_line`
/// lines down, lay out incrementally, and compare with a fresh full layout.
fn scroll_and_compare(
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame_id: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
    buf_id: BufferId,
    start_line: usize,
    point_line: usize,
) -> LayoutStats {
    scroll_window_to(
        eval,
        frame_id,
        window,
        buf_id,
        (start_line * LINE.len()) as i64 + 1,
        point_line * LINE.len(),
    );
    engine.layout_frame_rust(eval, frame_id);
    let stats = engine.last_layout_stats().clone();
    let incremental = selected_window_layout_trace(eval, engine, frame_id);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(eval, frame_id);
    let reference = selected_window_layout_trace(eval, &fresh, frame_id);
    assert_eq!(
        incremental, reference,
        "start line {start_line}: a backward scroll must lay out like a full layout ({stats:?})"
    );
    stats
}

#[test]
fn scrolling_back_one_line_walks_one_row_and_shifts_the_rest() {
    let _back = ScrollBackGuard::on();
    let (mut eval, frame_id, buf_id, window) = incr_editing_frame(&LINE.repeat(80), 800, 600);
    let mut engine = LayoutEngine::new();
    scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 10, 20);
    let stats = scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 9, 20);
    assert_eq!(stats.scroll_windows, 1, "{stats:?}");
    let damage = enabled_body_row_damage(&engine, window);
    let relaid = damage
        .iter()
        .filter(|(_, damage)| damage.is_relaid())
        .count();
    assert_eq!(relaid, 1, "only the exposed row is walked: {damage:?}");
    assert!(
        damage.iter().any(|(_, damage)| matches!(
            damage,
            RowDamage::ReusedShifted { dvpos } if dvpos.get() > 0.0
        )),
        "the old rows moved down: {damage:?}"
    );
}

#[test]
fn scrolling_back_several_lines_and_back_and_forth_stays_exact() {
    let _back = ScrollBackGuard::on();
    let (mut eval, frame_id, buf_id, window) = incr_editing_frame(&LINE.repeat(80), 800, 600);
    let mut engine = LayoutEngine::new();
    scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 30, 40);
    for (start, point) in [(25, 40), (28, 40), (20, 30), (21, 30), (0, 10), (3, 10)] {
        scroll_and_compare(
            &mut eval,
            &mut engine,
            frame_id,
            window,
            buf_id,
            start,
            point,
        );
    }
}

#[test]
fn scrolling_back_by_more_than_the_window_lays_out_in_full() {
    let _back = ScrollBackGuard::on();
    let (mut eval, frame_id, buf_id, window) = incr_editing_frame(&LINE.repeat(200), 800, 600);
    let mut engine = LayoutEngine::new();
    scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 150, 160);
    let stats = scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 50, 60);
    assert_eq!(stats.scroll_windows, 0, "{stats:?}");
}

#[test]
fn without_the_knob_a_backward_scroll_lays_out_in_full() {
    let (mut eval, frame_id, buf_id, window) = incr_editing_frame(&LINE.repeat(80), 800, 600);
    let mut engine = LayoutEngine::new();
    scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 10, 20);
    let stats = scroll_and_compare(&mut eval, &mut engine, frame_id, window, buf_id, 9, 20);
    assert_eq!(stats.scroll_windows, 0, "{stats:?}");
}
