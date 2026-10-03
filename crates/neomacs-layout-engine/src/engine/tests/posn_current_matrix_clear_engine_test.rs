//! Real accepted TTY rows, Lisp buffer setters, and cold current-matrix queries.
//! Every fixture has one exclusive Context/Frame mutator. Startup selects the
//! extent policy; no test changes the environment or a process OnceLock.
use super::*;
use neovm_core::window::{FrameId, SplitDirection, SplitPlacement, WindowId};

fn accepted_ascii_frame(enabled: bool) -> (Context, FrameId, BufferId, WindowId, LayoutEngine) {
    assert_eq!(
        neovm_core::window::posn_object_extent_mode().enabled(),
        enabled,
        "isolate with the inventory's NEOMACS_POSN_OBJECT_EXTENT startup value"
    );
    let text = format!("{}\n", "a".repeat(80)).repeat(100);
    let (mut eval, frame_id, buffer_id, window) = incr_editing_frame(&text, 120, 40);
    eval.set_variable("noninteractive", Value::NIL);
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
            buffer.set_buffer_local(name, Value::NIL);
        }
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(9));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.initial = false;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        // Frame::new used GUI metrics; realize the live GNU probe's one-cell
        // TTY minibuffer before computing the desired matrix allocation.
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut bounds = *mini.bounds();
            bounds.height = 1.0;
            mini.set_bounds(bounds);
        }
        frame.set_window_layout_text_size(120, 40);
    }
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    assert!(
        frame.redisplay_snapshot(window).is_some(),
        "actual accepted renderer rows"
    );
    assert_eq!(frame.tty_posn_pool().is_some(), enabled);
    (eval, frame_id, buffer_id, window, engine)
}

fn split_cold_child(
    eval: &mut Context,
    frame_id: FrameId,
    buffer_id: BufferId,
    window: WindowId,
) -> WindowId {
    let child = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            window,
            SplitDirection::Vertical,
            buffer_id,
            Some(18),
            SplitPlacement::AfterTarget,
        )
        .expect("live vertical split");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            frame_id,
            child,
            Some(18),
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
    child
}

fn current_extents(eval: &mut Context, window: WindowId) -> Vec<String> {
    // This is the actual Task3 unavailable-adapter seam, not a mocked query.
    eval.set_variable("noninteractive", Value::T);
    eval.set_variable("matrix-clear-window", Value::make_window(window.0));
    (0..8)
        .map(|i| {
            let x = 2 + i;
            let y = i % 4;
            let posn = eval
                .eval_str(&format!("(posn-at-x-y {x} {y} matrix-clear-window)"))
                .expect("real Lisp posn-at-x-y");
            let fields =
                neovm_core::emacs_core::value::list_to_vec(&posn).expect("position fields");
            assert_eq!(fields.len(), 10);
            assert_eq!(fields[0].as_window_id(), Some(window.0));
            neovm_core::emacs_core::print::print_value(&fields[9])
        })
        .collect()
}

fn same_buffer_setter(eval: &mut Context, window: WindowId, keep: &str) {
    eval.set_variable("matrix-clear-window", Value::make_window(window.0));
    eval.eval_str(&format!(
        "(set-window-buffer matrix-clear-window (window-buffer matrix-clear-window){keep})"
    ))
    .expect("actual Lisp buffer setter");
    eval.eval_str("(set-window-start matrix-clear-window 1)")
        .expect("live start");
    eval.eval_str("(set-window-point matrix-clear-window 10)")
        .expect("live point");
}

#[test]
fn tty_posn_default_same_buffer_setter_clears_cold_child_current_rows() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    let inherited = current_extents(&mut eval, child);
    assert!(
        inherited.iter().any(|v| v != "(0 . 0)"),
        "real inherited numeric glyphs exercise the missing clear"
    );
    same_buffer_setter(&mut eval, child, "");
    // All eight zeros are captured by the unchanged strict live GNU Task3
    // fixture, not invented expected ASCII geometry or a hand-built matrix.
    assert_eq!(current_extents(&mut eval, child), vec!["(0 . 0)"; 8]);
    let frame = eval.frame_manager().get(frame).expect("frame");
    assert!(
        frame.redisplay_snapshot(child).is_none(),
        "query cannot accept output"
    );
    assert!(
        frame.tty_posn_pool().is_some(),
        "local clear preserves the physical frame pool"
    );
}

#[test]
fn tty_posn_keep_margins_same_buffer_setter_preserves_cold_child_current_rows() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    let inherited = current_extents(&mut eval, child);
    assert!(inherited.iter().any(|v| v != "(0 . 0)"));
    same_buffer_setter(&mut eval, child, " t");
    assert_eq!(current_extents(&mut eval, child), inherited);
}

#[test]
fn tty_posn_default_same_buffer_setter_clears_accepted_local_current_rows() {
    let (mut eval, frame, _, root, _) = accepted_ascii_frame(true);
    let accepted = current_extents(&mut eval, root);
    assert!(accepted.iter().any(|v| v != "(0 . 0)"));
    assert!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .redisplay_snapshot(root)
            .expect("accepted local rows")
            .posn_matrix
            .is_some()
    );
    same_buffer_setter(&mut eval, root, " nil");
    assert_eq!(current_extents(&mut eval, root), vec!["(0 . 0)"; 8]);
}

#[test]
fn tty_posn_fresh_acceptance_restores_same_buffer_setter_current_rows() {
    let (mut eval, frame, buffer, root, mut engine) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    let inherited = current_extents(&mut eval, child);
    same_buffer_setter(&mut eval, child, "");
    eval.set_variable("noninteractive", Value::NIL);
    engine.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &engine, frame);
    assert_eq!(
        current_extents(&mut eval, child),
        inherited,
        "real fresh current-matrix publication replaces a previous clear"
    );
}

#[test]
fn tty_posn_later_split_repartitions_preserved_frame_pool_after_same_buffer_clear() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    let inherited = current_extents(&mut eval, child);
    same_buffer_setter(&mut eval, child, "");
    let sibling = eval
        .frame_manager_mut()
        .split_window(
            frame,
            child,
            SplitDirection::Vertical,
            buffer,
            Some(10),
            SplitPlacement::AfterTarget,
        )
        .expect("another live vertical split");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            frame,
            sibling,
            Some(10),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("actual changed matrix allocation");
    assert_eq!(
        current_extents(&mut eval, child),
        inherited,
        "GNU fake_current_matrices repopulates from the unchanged physical frame pool"
    );
    assert!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .redisplay_snapshot(child)
            .is_none()
    );
}

#[test]
fn tty_posn_buffer_setter_extent_off_preserves_legacy_cold_fallback() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(false);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    let legacy = current_extents(&mut eval, child);
    assert_eq!(legacy, vec!["(0 . 0)"; 8]);
    same_buffer_setter(&mut eval, child, "");
    assert_eq!(current_extents(&mut eval, child), legacy);
    assert!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .tty_posn_pool()
            .is_none()
    );
}

#[test]
fn tty_posn_local_only_accepted_publication_is_cleared_without_allocating_a_frame_pool() {
    let (mut eval, frame_id, _, root, engine) = accepted_ascii_frame(true);
    let snapshot = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .redisplay_snapshot(root)
        .expect("actual producer snapshot")
        .clone();
    assert!(snapshot.posn_matrix.is_some());
    let presentation = neovm_core::window::geometry::PresentationId::new(
        engine
            .last_frame_display_state
            .as_ref()
            .expect("accepted state")
            .presentation_id
            .get()
            + 1,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        // Existing public Frame lifecycle: the real producer's immutable local
        // matrix can be published without a physical pool after reallocation.
        // No row, glyph, source rectangle, or empty pool is hand constructed.
        frame.resize_pixelwise(121, 41);
        frame.resize_pixelwise(120, 40);
        assert!(frame.tty_posn_pool().is_none());
        frame
            .prepare_live_window_presentation(presentation, vec![snapshot])
            .expect("validated existing-API local-only publication");
        frame
            .activate_display_presentation(presentation)
            .expect("activate local-only presentation");
    }
    assert!(
        current_extents(&mut eval, root)
            .iter()
            .any(|v| v != "(0 . 0)")
    );
    same_buffer_setter(&mut eval, root, "");
    assert_eq!(current_extents(&mut eval, root), vec!["(0 . 0)"; 8]);
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .tty_posn_pool()
            .is_none(),
        "clearing a local matrix must not synthesize a pool"
    );
}

#[test]
fn tty_posn_setter_clear_is_visible_to_eager_hook_and_survives_hook_error() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    assert!(
        current_extents(&mut eval, child)
            .iter()
            .any(|v| v != "(0 . 0)")
    );
    eval.set_variable("matrix-clear-window", Value::make_window(child.0));
    eval.eval_str("(setq matrix-clear-hook-size nil window-scroll-functions (list (lambda (w start) (setq matrix-clear-hook-size (nth 9 (posn-at-x-y 2 0 w))) (error \"matrix-clear-probe\"))))")
        .expect("actual eager scroll callback");
    let result = eval
        .eval_str("(set-window-buffer matrix-clear-window (window-buffer matrix-clear-window))");
    assert!(result.is_err(), "the actual eager callback must throw");
    let observed = eval
        .eval_str("matrix-clear-hook-size")
        .expect("recorded hook observation");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&observed),
        "(0 . 0)",
        "GNU applies the window adjustment before its eager callback"
    );
    eval.eval_str("(setq window-scroll-functions nil)")
        .expect("clear fixture callback");
    assert_eq!(
        current_extents(&mut eval, child),
        vec!["(0 . 0)"; 8],
        "the error cannot roll back the already applied current-row clear"
    );
}

#[test]
fn tty_posn_keep_margins_after_existing_clear_does_not_reenable_current_rows() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    assert!(
        current_extents(&mut eval, child)
            .iter()
            .any(|v| v != "(0 . 0)")
    );
    same_buffer_setter(&mut eval, child, "");
    same_buffer_setter(&mut eval, child, " t");
    assert_eq!(current_extents(&mut eval, child), vec!["(0 . 0)"; 8]);
}

#[test]
fn tty_posn_unchanged_allocation_does_not_reenable_setter_cleared_current_rows() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    assert!(
        current_extents(&mut eval, child)
            .iter()
            .any(|v| v != "(0 . 0)")
    );
    same_buffer_setter(&mut eval, child, "");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(frame, child, Some(18), Value::NIL, SplitDirection::Vertical)
        .expect("same successful allocation geometry");
    assert_eq!(
        current_extents(&mut eval, child),
        vec!["(0 . 0)"; 8],
        "GNU does not fake rows when allocation flags stayed zero"
    );
}

#[test]
fn tty_posn_other_frame_split_does_not_reenable_setter_cleared_current_rows() {
    let (mut eval, frame, buffer, root, _) = accepted_ascii_frame(true);
    let child = split_cold_child(&mut eval, frame, buffer, root);
    assert!(
        current_extents(&mut eval, child)
            .iter()
            .any(|v| v != "(0 . 0)")
    );
    same_buffer_setter(&mut eval, child, "");
    let other = eval
        .frame_manager_mut()
        .create_frame("unrelated-current-matrix", 120, 40, buffer);
    let other_root = {
        let other = eval
            .frame_manager_mut()
            .get_mut(other)
            .expect("other frame");
        other.char_width = 1.0;
        other.char_height = 1.0;
        other.font_pixel_size = 1.0;
        other.set_window_system(None);
        other.set_window_layout_text_size(120, 40);
        other.selected_window
    };
    let sibling = eval
        .frame_manager_mut()
        .split_window(
            other,
            other_root,
            SplitDirection::Vertical,
            buffer,
            Some(18),
            SplitPlacement::AfterTarget,
        )
        .expect("other frame split");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            other,
            sibling,
            Some(18),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("other frame actual allocation");
    assert_eq!(
        current_extents(&mut eval, child),
        vec!["(0 . 0)"; 8],
        "only this frame's actual matrix adjustment can restore its rows"
    );
}
