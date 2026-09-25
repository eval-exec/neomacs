//! P3.5 C6 (`NEOMACS_PRESENT_HIT=lazy`): a frame's text hit positions are
//! built on the first pointer query, from the frame's shared window
//! snapshots, and are exactly the ones the eager path builds while composing.

use super::*;
use crate::presentation::spatial::{
    PresentedTextPositionsMode, set_presented_text_positions_mode_for_test,
};

struct ModeGuard;

impl Drop for ModeGuard {
    fn drop(&mut self) {
        set_presented_text_positions_mode_for_test(None);
    }
}

fn hit_index_with(
    mode: PresentedTextPositionsMode,
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> neomacs_display_protocol::PresentedHitIndex {
    set_presented_text_positions_mode_for_test(Some(mode));
    engine.layout_frame_rust(eval, frame_id);
    activate_last_engine_presentation(eval, engine, frame_id);
    engine
        .last_frame_display_state
        .as_ref()
        .expect("sealed presentation")
        .presented_hit_index
        .clone()
}

#[test]
fn lazy_text_positions_are_the_eager_ones_and_resolve_alike() {
    let _guard = ModeGuard;
    // Short, long and empty lines: exact positions and row fallbacks both.
    let text = "short\n\nx\n(defun a-longer-line (arg) (list arg arg arg))\n\tindented\n".repeat(8);
    let (mut eval, frame_id, _buf, _window) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let eager = hit_index_with(
        PresentedTextPositionsMode::Eager,
        &mut eval,
        &mut engine,
        frame_id,
    );
    assert!(!eager.text_deferred());
    assert!(!eager.text_positions().is_empty());

    eval.eval_str("(forward-char 1)").expect("move");
    let lazy = hit_index_with(
        PresentedTextPositionsMode::Lazy,
        &mut eval,
        &mut engine,
        frame_id,
    );
    assert!(lazy.text_deferred(), "composing built no text position");

    // The first query builds them, and every point resolves as before.
    let query = |index: &neomacs_display_protocol::PresentedHitIndex, x: f32, y: f32| {
        index
            .resolve(neomacs_display_protocol::PresentedHitQuery::new(
                settled_point(index.presentation(), x, y),
            ))
            .expect("current presentation")
            .and_then(|hit| hit.text_position())
    };
    let mut resolved = 0;
    for y in (0..600).step_by(7) {
        for x in (0..800).step_by(5) {
            let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
            let (eager_hit, lazy_hit) = (query(&eager, x, y), query(&lazy, x, y));
            assert_eq!(lazy_hit, eager_hit, "({x}, {y})");
            resolved += usize::from(eager_hit.is_some());
        }
    }
    assert!(resolved > 100, "the grid reached text: {resolved}");
    assert!(!lazy.text_deferred());
    assert_eq!(lazy.text_positions(), eager.text_positions());
}
