//! Exercise real configuration capture/restore and the real GNU hook pass.
//! Fixture state belongs to one exclusive Context; numeric policy guards are
//! test-only and cannot migrate between mutator threads.
use super::super::*;
use crate::emacs_core::eval::{Context, RedisplayHookPolicyGuard};

fn fixture() -> (Context, crate::window::FrameId, crate::window::WindowId) {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let root = eval.buffers.current_buffer_id().expect("root buffer");
    let frame_id = eval.frames.create_frame("config-mini", 80, 25, root);
    let frame = eval.frames.get_mut(frame_id).expect("frame");
    frame.char_width = 1.0;
    frame.char_height = 1.0;
    frame.shrink_mini_window();
    let mini = frame.minibuffer_window.expect("minibuffer");
    for name in [
        "window-buffer-change-functions",
        "window-size-change-functions",
        "window-selection-change-functions",
        "window-state-change-functions",
        "window-configuration-change-hook",
        "window-state-change-hook",
        "inhibit-redisplay",
        "inhibit-quit",
    ] {
        eval.obarray.set_symbol_value(name, Value::NIL);
    }
    // A real callback requests window_change_record; dimensions alone do not.
    eval.eval_str("(setq window-state-change-functions (list (lambda (_frame) nil)))")
        .expect("recording hook");
    eval.gnu_mark_frame_window_change(frame_id);
    run_redisplay_window_change_hooks(&mut eval).expect("initial real record");
    (eval, frame_id, mini)
}

fn capture(eval: &mut Context) {
    eval.eval_str("(setq d5-config (current-window-configuration))")
        .expect("capture real configuration");
}

fn buffer_callbacks(eval: &mut Context, buffer: crate::buffer::BufferId) {
    eval.buffers.set_current(buffer);
    eval.eval_str(
        "(progn (setq d5-buffer-calls nil)
         (set (make-local-variable 'window-buffer-change-functions)
          (list (lambda (_window)
           (setq d5-buffer-calls (cons (buffer-name) d5-buffer-calls))))))",
    )
    .expect("buffer-local callback");
}

#[test]
fn gnu_dont_set_miniwindow_restores_geometry_after_sizing_transfer() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame_id, mini_id) = fixture();
    capture(&mut eval);
    let live = eval.buffers.create_buffer(" *live-mini-source*");
    eval.buffers
        .get_mut(live)
        .expect("live buffer")
        .insert("abcde");
    {
        let window = eval
            .frames
            .get_mut(frame_id)
            .expect("frame")
            .find_window_mut(mini_id)
            .expect("mini");
        window.set_buffer(live);
        crate::window::window_markers::attach_window_position_markers(&mut eval.buffers, window);
        crate::window::window_markers::set_window_start_with_marker(
            &mut eval.buffers,
            window,
            LispCharPos1::new(2),
        );
        crate::window::window_markers::set_window_point_with_marker(
            &mut eval.buffers,
            window,
            LispCharPos1::new(4),
        );
    }
    let frame = eval.frames.get_mut(frame_id).expect("frame");
    let plan = frame.plan_mini_window_resize(3.0, 3.0).expect("grow plan");
    frame.apply_mini_window_resize(plan);
    // The sizing callback transfers before committing the requested shrink.
    // Restore is the actual DONT-SET-MINIWINDOW native unwind producer.
    let transferred = eval
        .eval_str(
            "(catch 'mini-sizing (unwind-protect (throw 'mini-sizing 'interrupted) (set-window-configuration d5-config nil t)))",
        )
        .expect("configuration cleanup during the sizing transfer");
    assert_eq!(transferred, Value::symbol("interrupted"));
    let frame = eval.frames.get(frame_id).expect("frame");
    let mini = frame.find_window(mini_id).expect("mini");
    assert_eq!(mini.bounds().height, 1.0);
    assert_eq!(mini.buffer_id(), Some(live));
    assert_eq!(mini.window_start(), Some(LispCharPos1::new(2)));
    if let crate::window::Window::Leaf { point, .. } = mini {
        assert_eq!(*point, LispCharPos1::new(4));
    }
    assert_eq!(
        frame.root_window().bounds().y + frame.root_window().bounds().height,
        mini.bounds().y
    );
}

#[test]
fn legacy_dont_set_miniwindow_keeps_existing_geometry_policy() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame_id, mini_id) = fixture();
    capture(&mut eval);
    let frame = eval.frames.get_mut(frame_id).expect("frame");
    let plan = frame.plan_mini_window_resize(3.0, 3.0).expect("grow plan");
    frame.apply_mini_window_resize(plan);
    eval.eval_str("(set-window-configuration d5-config nil t)")
        .expect("legacy restoration");
    assert_eq!(
        eval.frames
            .get(frame_id)
            .expect("frame")
            .find_window(mini_id)
            .expect("mini")
            .bounds()
            .height,
        3.0
    );
}

#[test]
fn gnu_configuration_across_record_keeps_unchanged_buffer_callbacks_silent() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame_id, _) = fixture();
    let root = eval.frames.get(frame_id).expect("frame").selected_window;
    let buffer = eval
        .frames
        .get(frame_id)
        .expect("frame")
        .find_window(root)
        .and_then(crate::window::Window::buffer_id)
        .expect("root buffer");
    buffer_callbacks(&mut eval, buffer);
    capture(&mut eval);
    // A selection/size pass while the reader is active advances the epoch.
    eval.eval_str("(setq window-state-change-functions (list (lambda (_frame) nil)))")
        .expect("record hook");
    eval.gnu_mark_frame_window_change(frame_id);
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .window_state_change = true;
    run_redisplay_window_change_hooks(&mut eval).expect("intervening real record");
    let stamp = eval.frames.get(frame_id).expect("frame").change_stamp;
    eval.eval_str("(set-window-configuration d5-config)")
        .expect("restore same buffer");
    assert_eq!(
        eval.frames
            .get(frame_id)
            .expect("frame")
            .find_window(root)
            .and_then(crate::window::Window::change_stamp),
        Some(stamp)
    );
    run_redisplay_window_change_hooks(&mut eval).expect("restored real hook pass");
    assert_eq!(
        eval.obarray.symbol_value("d5-buffer-calls").copied(),
        Some(Value::NIL)
    );
}

#[test]
fn gnu_configuration_across_record_notifies_actual_outgoing_and_restored_buffers() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame_id, _) = fixture();
    let root = eval.frames.get(frame_id).expect("frame").selected_window;
    let original = eval
        .frames
        .get(frame_id)
        .expect("frame")
        .find_window(root)
        .and_then(crate::window::Window::buffer_id)
        .expect("original");
    buffer_callbacks(&mut eval, original);
    capture(&mut eval);
    let outgoing = eval.buffers.create_buffer("outgoing");
    buffer_callbacks(&mut eval, outgoing);
    eval.obarray
        .set_symbol_value("d5-outgoing", Value::make_buffer(outgoing));
    eval.eval_str("(set-window-buffer nil d5-outgoing)")
        .expect("switch window");
    run_redisplay_window_change_hooks(&mut eval).expect("record actual outgoing");
    eval.obarray.set_symbol_value("d5-buffer-calls", Value::NIL);
    eval.eval_str("(set-window-configuration d5-config)")
        .expect("restore original");
    run_redisplay_window_change_hooks(&mut eval).expect("restored hook pass");
    let calls = eval
        .obarray
        .symbol_value("d5-buffer-calls")
        .copied()
        .expect("calls");
    let names = crate::emacs_core::value::list_to_vec(&calls).expect("list");
    assert_eq!(names.len(), 2, "old and new local hooks run once each");
    assert_eq!(
        names[0],
        eval.buffers
            .get(original)
            .expect("original buffer")
            .name_value()
    );
    assert_eq!(names[1].as_utf8_str(), Some("outgoing"));
}

#[test]
fn gnu_configuration_temporary_same_epoch_buffer_excursion_is_silent() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame_id, _) = fixture();
    let root = eval.frames.get(frame_id).expect("frame").selected_window;
    let original = eval
        .frames
        .get(frame_id)
        .expect("frame")
        .find_window(root)
        .and_then(crate::window::Window::buffer_id)
        .expect("original");
    buffer_callbacks(&mut eval, original);
    capture(&mut eval);
    eval.eval_str(r#"(set-window-buffer nil (get-buffer-create "temporary"))"#)
        .expect("temporary buffer");
    eval.eval_str("(set-window-configuration d5-config)")
        .expect("restore in same epoch");
    run_redisplay_window_change_hooks(&mut eval).expect("same-epoch hook pass");
    assert_eq!(
        eval.obarray.symbol_value("d5-buffer-calls").copied(),
        Some(Value::NIL)
    );
}
