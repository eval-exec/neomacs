//! Existing accepted renderer -> live allocation/margin change -> split/query
//! controls. One Context owns each complete fixture. Isolated startup pins the
//! extent policy ON; helpers never mutate environment, TLS or a pool object.
//! GNU31.1 owned-PTY probes in tmp/d5-r1-posn-pool-gnu-swap-r2 confirm
//! Undrawn for both changed-allocation cases; source fields are compared to
//! this same producer under the corresponding unchanged topology control.
use super::*;
use neovm_core::window::{
    FrameId, SplitDirection, SplitPlacement, WindowId, WindowLayoutQueryOutcome,
};

fn accepted_empty_frame() -> (Context, FrameId, BufferId, WindowId, LayoutEngine) {
    assert!(
        neovm_core::window::posn_object_extent_mode().enabled(),
        "isolate with NEOMACS_POSN_OBJECT_EXTENT=on before process startup"
    );
    let (mut eval, frame_id, buffer_id, window) = incr_editing_frame("", 120, 40);
    eval.set_variable("noninteractive", Value::NIL);
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
            buffer.set_buffer_local(name, Value::NIL);
        }
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.initial = false;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(120, 40);
    }
    eval.set_variable("posn-control-frame", Value::make_frame(frame_id.0));
    eval.set_variable("posn-control-root", Value::make_window(window.0));
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .redisplay_snapshot(window)
            .is_some()
    );
    (eval, frame_id, buffer_id, window, engine)
}

fn split_and_probe(
    eval: &mut Context,
    frame_id: FrameId,
    buffer_id: BufferId,
    window: WindowId,
    engine: &LayoutEngine,
) -> Vec<(String, Vec<String>)> {
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("abcdefghijkl\nmnopq\n");
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let child = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            window,
            SplitDirection::Vertical,
            buffer_id,
            Some(10),
            SplitPlacement::AfterTarget,
        )
        .expect("real live split");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            frame_id,
            child,
            Some(10),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("real split geometry");
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .redisplay_snapshot(child)
            .is_none()
    );
    // The queried child must remain margin-free: otherwise a local-margin
    // guard would mask the independently changed sibling/mini allocation.
    let queried_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .find_window(child)
        .expect("child");
    let neovm_core::window::Window::Leaf { margins, .. } = queried_window else {
        panic!("queried window is a leaf");
    };
    assert_eq!(*margins, neovm_core::window::WindowMargins::ZERO);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    query.synchronize(engine.window_layout_query_seed());
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    eval.set_variable("posn-control-child", Value::make_window(child.0));
    let mut captures = Vec::new();
    for (x, y) in [(0, 0), (1, 0), (3, 0), (0, 2), (1, 2), (3, 2)] {
        let position = eval
            .eval_str(&format!("(posn-at-x-y {x} {y} posn-control-child)"))
            .expect("real Lisp posn query");
        let fields =
            neovm_core::emacs_core::value::list_to_vec(&position).expect("full position tuple");
        assert_eq!(fields.len(), 10);
        assert_eq!(fields[0].as_window_id(), Some(child.0));
        // Convert every observation to owned text before Context lifetime ends.
        // This retains source/geometry separately from the object extent.
        let print = neovm_core::emacs_core::print::print_value;
        captures.push((print(&fields[9]), fields[1..9].iter().map(print).collect()));
    }
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .redisplay_snapshot(child)
            .is_none(),
        "a query must not publish the cold child's current matrix"
    );
    captures
}

#[derive(Clone, Copy)]
enum Change {
    None,
    ResizeRoundTrip,
    MinibufferMargin,
    UnchangedSibling,
    SiblingMargin,
}

fn capture(change: Change) -> Vec<(String, Vec<String>)> {
    let (mut eval, frame, buffer, window, mut engine) = accepted_empty_frame();
    if matches!(change, Change::UnchangedSibling | Change::SiblingMargin) {
        // GNU's sibling-margin probe accepts two margin-free leaves before
        // changing one and splitting the other. The old root remains selected.
        let sibling = eval
            .frame_manager_mut()
            .split_window(
                frame,
                window,
                SplitDirection::Vertical,
                buffer,
                Some(19),
                SplitPlacement::AfterTarget,
            )
            .expect("real accepted sibling topology");
        eval.frame_manager_mut()
            .apply_staged_split_sizes(
                frame,
                sibling,
                Some(19),
                Value::NIL,
                SplitDirection::Vertical,
            )
            .expect("accepted sibling geometry");
        engine.layout_frame_rust(&mut eval, frame);
        activate_last_engine_presentation(&mut eval, &engine, frame);
        if matches!(change, Change::SiblingMargin) {
            eval.set_variable("posn-control-sibling", Value::make_window(sibling.0));
            eval.eval_str("(set-window-margins posn-control-sibling 2 0)")
                .expect("real live sibling margin change");
        }
    }
    match change {
        Change::ResizeRoundTrip => {
            eval.eval_str("(set-frame-size posn-control-frame 121 41)")
                .expect("real requested layout size change");
            eval.eval_str("(set-frame-size posn-control-frame 120 40)")
                .expect("restore original layout size before acceptance");
        }
        Change::MinibufferMargin => {
            let mini = eval
                .frame_manager()
                .get(frame)
                .expect("frame")
                .minibuffer_window
                .expect("real minibuffer leaf");
            eval.set_variable("posn-control-mini", Value::make_window(mini.0));
            eval.eval_str("(set-window-margins posn-control-mini 2 0)")
                .expect("real live minibuffer margin change");
        }
        Change::None | Change::UnchangedSibling | Change::SiblingMargin => {}
    }
    split_and_probe(&mut eval, frame, buffer, window, &engine)
}

fn assert_undrawn_without_source_change(
    changed: &[(String, Vec<String>)],
    unchanged: &[(String, Vec<String>)],
    scenario: &str,
) {
    assert_eq!(changed.len(), 6);
    assert_eq!(unchanged.len(), 6);
    for ((extent, source), (_, baseline_source)) in changed.iter().zip(unchanged) {
        assert_eq!(
            source, baseline_source,
            "{scenario}: allocation eligibility cannot change the live query's source/geometry fields"
        );
        assert_eq!(
            extent, "(0 . 0)",
            "{scenario}: GNU31.1 cold-child captures report Undrawn after allocation invalidation; full source fields: {source:?}"
        );
    }
}

#[test]
fn tty_posn_resize_away_and_back_does_not_revive_old_frame_pool() {
    // Every Context is dropped before the next probe constructs its owner.
    let unchanged = capture(Change::None);
    let resized = capture(Change::ResizeRoundTrip);
    assert_undrawn_without_source_change(&resized, &unchanged, "resize-away-back");
}

#[test]
fn tty_posn_live_other_window_margins_block_cold_leaf_pool_repartition() {
    let unchanged = capture(Change::None);
    let mini_margin = capture(Change::MinibufferMargin);
    assert_undrawn_without_source_change(&mini_margin, &unchanged, "mini-margin");
    let unchanged_sibling = capture(Change::UnchangedSibling);
    let sibling_margin = capture(Change::SiblingMargin);
    assert_undrawn_without_source_change(&sibling_margin, &unchanged_sibling, "sibling-margin");
}
