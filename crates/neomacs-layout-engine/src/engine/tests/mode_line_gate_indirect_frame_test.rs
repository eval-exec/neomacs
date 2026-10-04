//! GNU's overlay redisplay signal belongs to the shared text group even when
//! the overlay owner itself is undisplayed and only its base/indirect peer is
//! visible on another frame.
use super::*;

fn indirect_overlay_evaluates(overlay_on_base: bool) {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, selected, current, _, mut engine) = settled_frame("ML", LINE.len() * 5 + 5);
    let base = eval
        .buffer_manager_mut()
        .create_buffer("mode-line-overlay-base");
    let indirect = eval
        .buffer_manager_mut()
        .create_indirect_buffer(base, "mode-line-overlay-indirect", false)
        .expect("shared text peer");
    let shown = if overlay_on_base { indirect } else { base };
    let peer = eval
        .frame_manager_mut()
        .create_frame("mode-line-indirect-peer", 800, 600, shown);
    bind_minibuffer_buffer(&mut eval, peer);
    eval.frame_manager_mut().select_frame(selected);
    redisplay(&mut eval, &mut engine, peer);
    redisplay(&mut eval, &mut engine, selected);
    let owner = if overlay_on_base {
        "mode-line-overlay-base"
    } else {
        "mode-line-overlay-indirect"
    };
    eval.eval_str(&format!(
        "(save-current-buffer (set-buffer \"{owner}\") \
         (overlay-put (make-overlay 1 1) 'before-string \"overlay\"))"
    ))
    .expect("overlay-only edit in undisplayed group owner");
    assert!(
        eval.buffer_manager()
            .get(shown)
            .expect("displayed peer")
            .changed_char_range()
            .is_none(),
        "the symptom must not depend on a character/property dirty span"
    );
    assert_eq!(eval.buffer_manager().current_buffer_id(), Some(current));
    eval.eval_str("(insert \"x\")").expect("selected typing");
    assert!(
        eval.frame_manager()
            .other_window_buffer_changed(eval.buffer_manager()),
        "GNU bset_redisplay counts the displayed shared-text peer of an undisplayed overlay owner"
    );
    assert!(
        redisplay(&mut eval, &mut engine, selected) > 0,
        "a shared-text overlay change on another frame refuses optimization 1"
    );
}

#[test]
fn an_overlay_on_an_undisplayed_base_with_an_indirect_peer_evaluates_the_selected_mode_line() {
    indirect_overlay_evaluates(true);
}

#[test]
fn an_overlay_on_an_undisplayed_indirect_with_a_base_peer_evaluates_the_selected_mode_line() {
    indirect_overlay_evaluates(false);
}
