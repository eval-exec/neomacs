//! GNU bset_redisplay observes ordinary windows across the whole frame set.
use super::*;

fn add_frame(eval: &mut Context, buffer: BufferId) -> neovm_core::window::FrameId {
    let frame = eval
        .frame_manager_mut()
        .create_frame("mode-line-peer", 800, 600, buffer);
    bind_minibuffer_buffer(eval, frame);
    frame
}

#[test]
fn an_edit_to_shared_text_shown_on_another_frame_evaluates_the_selected_mode_line() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame, buffer, _, mut engine) = settled_frame("S%I", LINE.len() * 5 + 5);
    let peer = add_frame(&mut eval, buffer);
    eval.frame_manager_mut().select_frame(frame);
    redisplay(&mut eval, &mut engine, peer);
    redisplay(&mut eval, &mut engine, frame);
    eval.eval_str("(insert \"x\")").expect("selected edit");
    assert!(
        redisplay(&mut eval, &mut engine, frame) > 0,
        "GNU bset_redisplay refuses optimization 1 when the text is displayed twice across frames"
    );
    let incremental = rendered_mode_line_text(&engine);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert_eq!(incremental, rendered_mode_line_text(&fresh));
}

#[test]
fn an_edit_to_text_only_shown_on_another_frame_evaluates_the_selected_mode_line() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame, _, _, mut engine) = settled_frame("ML", LINE.len() * 5 + 5);
    let other = eval
        .buffer_manager_mut()
        .create_buffer("mode-line-other-frame");
    let peer = add_frame(&mut eval, other);
    eval.frame_manager_mut().select_frame(frame);
    redisplay(&mut eval, &mut engine, peer);
    redisplay(&mut eval, &mut engine, frame);
    eval.eval_str("(save-current-buffer (set-buffer \"mode-line-other-frame\") (insert \"z\"))")
        .expect("process-like edit outside this frame");
    eval.eval_str("(insert \"x\")").expect("selected edit");
    assert!(
        redisplay(&mut eval, &mut engine, frame) > 0,
        "GNU considers every frame when an ordinary non-selected window's buffer changes"
    );
}

#[test]
fn an_overlay_change_in_a_buffer_shown_on_another_frame_evaluates_the_selected_mode_line() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame, _, _, mut engine) = settled_frame("ML", LINE.len() * 5 + 5);
    let other = eval
        .buffer_manager_mut()
        .create_buffer("mode-line-other-frame");
    let peer = add_frame(&mut eval, other);
    eval.frame_manager_mut().select_frame(frame);
    redisplay(&mut eval, &mut engine, peer);
    redisplay(&mut eval, &mut engine, frame);
    eval.eval_str("(save-current-buffer (set-buffer \"mode-line-other-frame\") (overlay-put (make-overlay 1 1) 'before-string \"overlay\"))")
        .expect("overlay-only edit outside this frame");
    eval.eval_str("(insert \"x\")").expect("selected edit");
    assert!(
        redisplay(&mut eval, &mut engine, frame) > 0,
        "overlay-only changes also raise GNU consider_all_windows_p"
    );
}
