//! Accepted-output unit boundary, not a row producer or renderer execution.
use crate::window::display::WindowDisplayContextExt;
use crate::window::geometry::PresentationId;
use crate::window::{
    DisplayFrameExt, DisplayFrameManagerExt, FrameId, MatrixRow0, WindowDisplaySnapshot,
    WindowEndRecord, WindowId, WindowPresentationSnapshot,
};
use neovm_edit::buffer::{BufferId, EmacsBytePos, LispCharPos1};
use neovm_eval::emacs_core::eval::data::window_layout_query::{
    WindowLayoutQuery, WindowLayoutQueryOutcome,
};
use neovm_eval::emacs_core::eval::{Context, RedisplayHookPolicyGuard};

// The test supplies numeric completed geometry, then uses the real Frame
// presentation/accepted-output APIs. Original whole-layout RED rows separately
// verify that a canonical body producer reaches this boundary.
fn accepted_late_overlay() -> (Context, FrameId, WindowId, BufferId) {
    let (mut eval, buffer, frame, window) =
        crate::test_utils::upper_layer_helpers::attempt_freshness_projections(Context::new(
            crate::test_utils::unit_runtime(),
        ));
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("abc");
    let produced = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(setq accuracy-test-overlay (make-overlay 1 2))")
        .unwrap();
    eval.eval_str("(overlay-put accuracy-test-overlay 'face 'bold)")
        .unwrap();
    let accuracy = eval.gnu_window_accuracy(frame, window, buffer).unwrap();
    assert!(accuracy.overlay_modified_tick > produced.buffer.overlay_modified_tick);
    let snapshot = WindowDisplaySnapshot {
        window_id: window,
        buffer_modiff: Some(produced.buffer.modified_tick),
        layout_freshness: Some(produced),
        window_end_record: Some(WindowEndRecord::from_positions(
            LispCharPos1::new(4),
            EmacsBytePos::new(3),
            LispCharPos1::new(4),
            EmacsBytePos::new(3),
            MatrixRow0::ZERO,
        )),
        ..WindowDisplaySnapshot::default()
    };
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .prepare_display_presentation(
            PresentationId::new(1),
            vec![WindowPresentationSnapshot::live(snapshot)],
        )
        .unwrap();
    assert!(
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
            .set_gnu_redisplay_accuracy(PresentationId::new(1), accuracy)
    );
    eval.buffer_manager()
        .get(buffer)
        .unwrap()
        .acknowledge_mode_line_clip();
    (eval, frame, window, buffer)
}

#[test]
fn post_chrome_accuracy_is_current_without_renewing_original_rows() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, buffer) = accepted_late_overlay();
    let original = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .layout_freshness
        .clone()
        .unwrap();
    assert!(
        eval.gnu_current_matrix_snapshot(frame, window, buffer)
            .is_some()
    );
    assert!(
        eval.fresh_window_display_snapshot(frame, window, buffer)
            .is_none()
    );
    eval.set_window_point_for_redisplay(frame, window, LispCharPos1::new(2));
    assert!(
        eval.gnu_current_matrix_snapshot(frame, window, buffer)
            .is_some()
    );
    assert!(
        eval.fresh_window_display_snapshot(frame, window, buffer)
            .is_none()
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .layout_freshness
            .as_ref(),
        Some(&original)
    );

    eval.eval_str("(overlay-put accuracy-test-overlay 'face 'italic)")
        .unwrap();
    assert!(
        eval.gnu_current_matrix_snapshot(frame, window, buffer)
            .is_none()
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .gnu_text_hover_is_current(eval.buffer_manager()),
        Some(false)
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .layout_freshness
            .as_ref(),
        Some(&original)
    );
}

#[test]
fn text_hover_uses_source_accuracy_not_prevent_gate() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, buffer) = accepted_late_overlay();
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .gnu_text_hover_is_current(eval.buffer_manager()),
        Some(true)
    );
    eval.gnu_mark_buffer_prevent_redisplay_optimizations(buffer);
    assert!(
        eval.gnu_current_matrix_snapshot(frame, window, buffer)
            .is_none()
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .gnu_text_hover_is_current(eval.buffer_manager()),
        Some(true)
    );
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
        .invalidate_window_end();
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .gnu_text_hover_is_current(eval.buffer_manager()),
        Some(false)
    );
}

#[test]
fn matrix_accuracy_refuses_a_later_restriction_without_changing_source_ticks() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, buffer) = accepted_late_overlay();
    let accepted = eval.gnu_window_accuracy(frame, window, buffer).unwrap();
    assert!(
        eval.gnu_current_matrix_snapshot(frame, window, buffer)
            .is_some()
    );
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::new(
            EmacsBytePos::new(0),
            EmacsBytePos::new(1),
        ));

    let current = eval.gnu_window_accuracy(frame, window, buffer).unwrap();
    assert_ne!(accepted.accessible_chars, current.accessible_chars);
    assert_eq!(
        accepted.redisplay_optimizations,
        current.redisplay_optimizations
    );
    assert!(accepted.source_is_current_for(
        current.buffer_id,
        current.modified_tick,
        current.overlay_modified_tick,
    ));
    assert!(!accepted.is_current_for(current));
    assert!(
        eval.gnu_current_matrix_snapshot(frame, window, buffer)
            .is_none()
    );
}

#[test]
fn a_query_or_wrong_generation_cannot_stamp_accepted_window_output() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, buffer) = accepted_late_overlay();
    let accepted = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap()
        .display()
        .unwrap()
        .redisplay_output()
        .unwrap()
        .clone();
    let query_geometry = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .clone();
    eval.install_window_layout_query(move |_, _, _, _| {
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(
            LispCharPos1::new(4),
            Some(query_geometry.clone()),
        ))
    })
    .unwrap();
    assert!(matches!(
        eval.query_window_layout(frame, window),
        WindowLayoutQueryOutcome::Ready(_)
    ));
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .display()
            .unwrap()
            .redisplay_output(),
        Some(&accepted)
    );
    let accuracy = eval.gnu_window_accuracy(frame, window, buffer).unwrap();
    assert!(
        !eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
            .set_gnu_redisplay_accuracy(PresentationId::new(2), accuracy)
    );
    let other_buffer = eval.buffer_manager_mut().create_buffer("accuracy-other");
    let mut wrong_owner = accuracy;
    wrong_owner.buffer_id = other_buffer;
    assert!(
        !eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
            .set_gnu_redisplay_accuracy(PresentationId::new(1), wrong_owner)
    );
    let other = eval
        .frame_manager_mut()
        .create_frame("no-accepted-output", 800, 600, buffer);
    let other_window = eval.frame_manager().get(other).unwrap().selected_window;
    assert!(
        !eval
            .frame_manager_mut()
            .get_mut(other)
            .unwrap()
            .find_window_mut(other_window)
            .unwrap()
            .set_gnu_redisplay_accuracy(PresentationId::new(1), accuracy)
    );
}

#[test]
fn another_window_uses_accepted_ticks_then_detects_a_subsequent_overlay_write() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, buffer) = accepted_late_overlay();
    assert!(
        eval.buffer_manager()
            .get(buffer)
            .unwrap()
            .changed_char_range()
            .is_some(),
        "retained production dirtiness is deliberately not the GNU accuracy stamp"
    );
    let other_buffer = eval.buffer_manager_mut().create_buffer("accuracy-selected");
    let other = eval
        .frame_manager_mut()
        .create_frame("accuracy-selected", 800, 600, other_buffer);
    eval.frame_manager_mut().select_frame(other);
    assert!(
        !eval
            .frame_manager()
            .other_window_buffer_changed(eval.buffer_manager())
    );
    eval.eval_str("(overlay-put accuracy-test-overlay 'face 'italic)")
        .unwrap();
    assert!(
        eval.frame_manager()
            .other_window_buffer_changed(eval.buffer_manager())
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .gnu_text_hover_is_current(eval.buffer_manager()),
        Some(false)
    );
}
