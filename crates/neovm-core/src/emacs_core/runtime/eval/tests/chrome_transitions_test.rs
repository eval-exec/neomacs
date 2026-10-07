//! GNU macros.c and keyboard.c raise update_mode_lines at state transitions.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

fn clean_context() -> (Context, [WindowId; 2]) {
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    let first = eval.frames.create_frame("first", 640, 384, buffer);
    let second = eval.frames.create_frame("second", 640, 384, buffer);
    let windows = [
        eval.frames.get(first).unwrap().selected_window,
        eval.frames.get(second).unwrap().selected_window,
    ];
    eval.frames.select_frame(first);
    for window in windows {
        eval.note_chrome_generated(window);
        assert!(!eval.chrome_dirty().is_dirty(window));
    }
    (eval, windows)
}

#[test]
fn starting_and_ending_keyboard_macro_refreshes_all_mode_lines() {
    for hooks in [false, true] {
        let _policy = if hooks {
            RedisplayHookPolicyGuard::gnu()
        } else {
            RedisplayHookPolicyGuard::legacy()
        };
        let (mut eval, windows) = clean_context();
        for (form, defining) in [("(start-kbd-macro nil)", true), ("(end-kbd-macro)", false)] {
            let generation = eval.redisplay_generation();
            eval.eval_str(form).expect("macro transition");
            assert_eq!(
                eval.eval_str("defining-kbd-macro").unwrap().is_truthy(),
                defining
            );
            assert_ne!(
                eval.redisplay_generation(),
                generation,
                "{form}, hooks={hooks}"
            );
            for window in windows {
                assert!(
                    eval.chrome_dirty().is_dirty(window),
                    "{form}, hooks={hooks}"
                );
                eval.note_chrome_generated(window);
            }
        }
    }
}

fn observe_recursive_entry(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    assert!(args.is_empty());
    assert_eq!(eval.recursive_command_loop_depth(), 1);
    for frame in eval.frames.frame_list() {
        let window = eval.frames.get(frame).unwrap().selected_window;
        assert!(
            eval.chrome_dirty().is_dirty(window),
            "recursive-edit entry must invalidate every mode line before its first command"
        );
        eval.note_chrome_generated(window);
    }
    eval.assign("neo-recursive-entry-observed", Value::T);
    Ok(Value::NIL)
}

#[test]
fn recursive_edit_entry_and_unwind_refresh_all_mode_lines() {
    for hooks in [false, true] {
        let _policy = if hooks {
            RedisplayHookPolicyGuard::gnu()
        } else {
            RedisplayHookPolicyGuard::legacy()
        };
        for abort in [false, true] {
            let (mut eval, windows) = clean_context();
            eval.register_subr(SubrSpec::new(
                "neo-observe-recursive-entry",
                NativeFn::ContextVec(observe_recursive_entry),
                SubrArity::new(0, Some(0)),
            ));
            let map = crate::emacs_core::keymap::make_sparse_list_keymap();
            eval.assign("global-map", map);
            eval.select_global_map(map);
            eval.eval_str(&format!(
                "(progn
                   (setq post-command-hook '(neo-observe-recursive-entry))
                   (fset 'neo-leave-recursive-edit
                         (lambda () (interactive) (throw 'exit {})))
                   (fset 'command-execute
                         (lambda (cmd &optional _record _keys _special) (funcall cmd))))",
                if abort { "t" } else { "nil" }
            ))
            .unwrap();
            crate::emacs_core::keymap::list_keymap_define_seq(
                map,
                &[Value::fixnum('q' as i64)],
                Value::symbol("neo-leave-recursive-edit"),
            )
            .unwrap();
            let (sender, receiver) = crossbeam_channel::unbounded();
            drop(sender);
            eval.input_rx = Some(receiver);
            eval.command_loop.running = true;
            eval.command_loop.recursive_depth = 1;
            eval.command_loop
                .keyboard
                .kboard
                .unread_events
                .push_back(Value::fixnum('q' as i64));
            let result = eval.recursive_edit_inner();
            if abort {
                assert!(result.is_err(), "abort must retain GNU quit flow");
            } else {
                assert_eq!(result.unwrap(), Value::NIL);
            }
            assert_eq!(
                eval.eval_str("neo-recursive-entry-observed").unwrap(),
                Value::T
            );
            assert_eq!(eval.recursive_command_loop_depth(), 0);
            for window in windows {
                assert!(
                    eval.chrome_dirty().is_dirty(window),
                    "recursive-edit unwind, abort={abort}, hooks={hooks}"
                );
            }
        }
    }
}
