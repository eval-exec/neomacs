//! Exercise the real reader session, native configuration unwind, and GNU
//! window hook epochs. One exclusively owned Context remains live throughout.
//! The counting frontend accepts real frame transactions but measures no glyphs.
//!
//! `HostInputPublisher` and `ensure_selected_frame_id` are upper-crate routes.
//! The reader setup hook stays private; the cfg(test) delegate in this crate
//! is the route. Assertions are unchanged.

use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
use crate::emacs_core::eval::{ConditionFrame, Context, RedisplayHookPolicyGuard, ResumeTarget};
use crate::emacs_core::host_input::HostInputPublisher;
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::FrameId;
use neovm_value::error::{Flow, signal};
use neovm_value::value::{Value, list_to_vec};

fn fixture() -> (Context, FrameId, WindowId, neovm_edit::buffer::BufferId) {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let mini = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .minibuffer_window
        .expect("mini");
    let inactive = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .find_window(mini)
        .and_then(|window| window.buffer_id())
        .expect("inactive source");
    {
        let state = eval.frame_manager_mut().get_mut(frame).expect("frame");
        state.char_width = 1.0;
        state.char_height = 1.0;
        state.shrink_mini_window();
    }
    for name in [
        "window-buffer-change-functions",
        "window-size-change-functions",
        "window-selection-change-functions",
        "window-state-change-functions",
        "window-configuration-change-hook",
        "window-state-change-hook",
        "pre-redisplay-function",
        "inhibit-redisplay",
        "inhibit-quit",
        "minibuffer-setup-hook",
        "minibuffer-exit-hook",
        "resize-mini-windows",
    ] {
        eval.obarray_mut().set_symbol_value(name, Value::NIL);
    }
    eval.eval_str(
        r#"(progn
          (setq d5-mini-buffer-calls nil)
          (setq window-state-change-functions (list (lambda (_frame) nil)))
          (setq window-buffer-change-functions
           (list (lambda (frame)
            (setq d5-mini-buffer-calls (cons frame d5-mini-buffer-calls)))))
          (setq d5-mini-map (make-sparse-keymap))
          (define-key d5-mini-map " " (lambda () (interactive) (exit-minibuffer))))"#,
    )
    .expect("record hooks and exit keymap");
    eval.gnu_mark_frame_window_change(frame);
    crate::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(&mut eval)
        .expect("record inactive minibuffer");
    eval.redisplay_fn = Some(Box::new(
        crate::test_utils::mock_redisplay::accept_all_frames,
    ));
    (eval, frame, mini, inactive)
}

fn exercise_reader(restore_windows: bool, throw_inactive: bool, record_active: bool) {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, inactive) = fixture();
    let caller = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("caller buffer");
    let selected = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .selected_window;
    let saved_stamp = eval.frame_manager().get(frame).expect("frame").change_stamp;
    eval.obarray_mut().set_symbol_value(
        "read-minibuffer-restore-windows",
        Value::bool(restore_windows),
    );
    if throw_inactive {
        eval.eval_str(
            "(fset 'minibuffer-inactive-mode (lambda () (throw 'd5-mini-inactive 'interrupted)))",
        )
        .expect("inactive-mode transfer");
    }
    let tx = HostInputPublisher::install(&mut eval);
    tx.send(InputEvent::key_press(KeyEvent::char(' ')))
        .expect("queue reader exit");
    let map = eval
        .obarray()
        .symbol_value_copied("d5-mini-map")
        .expect("map");
    let mut expired = None;
    let handlers = eval.condition_stack_for_test().len();
    if throw_inactive {
        // Match sf_catch_value: GNU throw requires an active catch handler.
        // Without it this fixture would signal no-catch before observing the
        // final configuration/minibuffer-buffer unwind ordering.
        eval.push_condition_frame(ConditionFrame::Catch {
            tag: Value::symbol("d5-mini-inactive"),
            resume: ResumeTarget::InterpreterCatch,
        });
    }
    let result = crate::emacs_core::minibuffer::read_entries::finish_read_from_minibuffer_in_vm_runtime_with_setup_for_test(
        &mut eval,
        &[Value::string("Prompt: "), Value::string("answer"), map],
        |eval| {
            expired = eval.buffer_manager().current_buffer_id();
            assert_ne!(expired, Some(inactive));
            if record_active {
                eval.gnu_mark_frame_window_change(frame);
                crate::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(eval)?;
                assert_ne!(
                    eval.frame_manager().get(frame).expect("frame").change_stamp,
                    saved_stamp
                );
                Ok(Value::NIL)
            } else {
                // Leave during setup before a hook cycle can record the active
                // buffer. This is the real same-epoch window excursion control.
                Err(signal(
                    "error",
                    vec![Value::string("stop before active record")],
                ))
            }
        },
    );
    if throw_inactive {
        eval.pop_condition_frame();
        assert_eq!(eval.condition_stack_for_test().len(), handlers);
        assert!(
            result
                .as_ref()
                .err()
                .and_then(Flow::as_throw)
                .is_some_and(|thrown| thrown.tag.is_bare_symbol_named("d5-mini-inactive")
                    && thrown.value.is_bare_symbol_named("interrupted"))
        );
    } else if record_active {
        assert_eq!(
            result.expect("normal reader result").as_utf8_str(),
            Some("answer")
        );
    } else {
        assert!(
            result
                .as_ref()
                .err()
                .and_then(Flow::as_signal)
                .is_some_and(|error| error.symbol_name() == "error")
        );
    }
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(caller));
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .selected_window,
        selected
    );
    assert_eq!(eval.minibuffers().depth(), 0);
    assert_eq!(*eval.active_minibuffer_window(), None);
    let leaf = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .find_window(mini)
        .expect("mini");
    assert_eq!(leaf.buffer_id(), Some(inactive));
    assert_eq!(
        leaf.old_buffer(),
        if record_active {
            expired
        } else {
            Some(inactive)
        },
        "the final mini source restoration follows DONT-SET-MINIWINDOW configuration restoration"
    );
    eval.obarray_mut()
        .set_symbol_value("d5-mini-buffer-calls", Value::NIL);
    // The live shrink or configuration restore marks this in the TUI oracle.
    // Keep this epoch-focused native seam independent of glyph sizing.
    eval.gnu_mark_frame_window_change(frame);
    crate::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(&mut eval)
        .expect("first post-reader hook pass");
    let calls = eval
        .obarray()
        .symbol_value_copied("d5-mini-buffer-calls")
        .expect("calls");
    let calls = list_to_vec(&calls).expect("call list");
    assert_eq!(
        calls,
        if record_active {
            vec![Value::make_frame(frame.0)]
        } else {
            vec![]
        }
    );
    crate::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(&mut eval)
        .expect("idle post-reader pass");
    let idle = eval
        .obarray()
        .symbol_value_copied("d5-mini-buffer-calls")
        .expect("idle calls");
    assert_eq!(
        list_to_vec(&idle).expect("idle list"),
        calls,
        "the buffer transition is recorded once"
    );
}

#[test]
fn gnu_reader_final_mini_buffer_restore_follows_configuration_after_active_record() {
    exercise_reader(true, false, true);
}

#[test]
fn gnu_reader_inactive_throw_keeps_final_mini_buffer_transition_after_configuration() {
    exercise_reader(true, true, true);
}

#[test]
fn gnu_reader_without_configuration_preserves_final_mini_buffer_transition() {
    exercise_reader(false, false, true);
}

#[test]
fn gnu_reader_same_epoch_setup_abort_keeps_mini_buffer_callbacks_silent() {
    exercise_reader(true, false, false);
}
