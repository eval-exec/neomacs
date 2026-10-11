use crate::emacs_core::{Context, Value, format_eval_result};

fn eval_with_frames(source: &str) -> String {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(buffer);
    eval.frames.create_frame("F1", 800, 600, buffer);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&eval);
    format_eval_result(&eval.eval_str(source))
}

#[test]
fn handle_switch_frame_selects_target_window_and_buffer_with_gnu_command_semantics() {
    // GNU src/frame.c: Fhandle_switch_frame preserves the prefix, runs the
    // leave hook in the old buffer, then do_switch_frame selects the target.
    assert_eq!(
        eval_with_frames(
            r#"(let* ((first (selected-frame))
                      (second (make-terminal-frame '((name . "second"))))
                      (target (get-buffer-create "target"))
                      (window (frame-selected-window second))
                      (current-prefix-arg '(4))
                      (prefix-arg nil)
                      (left nil)
                      (mouse-leave-buffer-hook
                       (list (lambda () (setq left (selected-frame))))))
                 (set-window-buffer window target)
                 (let ((result (handle-switch-frame (list 'switch-frame second))))
                   (list (eq result second)
                         (eq (selected-frame) second)
                         (eq (selected-window) window)
                         (eq (current-buffer) target)
                         (eq left first)
                         prefix-arg)))"#,
        ),
        "OK (t t t t t (4))"
    );
}

#[test]
fn handle_switch_frame_preserves_focus_redirection_and_window_points() {
    assert_eq!(
        eval_with_frames(
            r#"(let* ((first (selected-frame))
                      (old-window (selected-window))
                      (second (make-terminal-frame '((name . "second"))))
                      (new-window (frame-selected-window second))
                      (target (get-buffer-create "target")))
                 (insert "primary")
                 (goto-char 3)
                 (save-current-buffer (set-buffer target) (insert "secondary"))
                 (set-window-buffer new-window target)
                 (set-window-point new-window 5)
                 (redirect-frame-focus first first)
                 (handle-switch-frame (list 'switch-frame second))
                 (list (eq (selected-frame) second)
                       (eq (frame-focus first) first)
                       (window-point old-window)
                       (point)
                       (eq (car (buffer-list second)) target)
                       (eq (handle-switch-frame first) first)
                       (point)))"#,
        ),
        "OK (t t 3 5 t t 3)"
    );
}

#[test]
fn handle_switch_frame_ignores_deleted_frames_but_select_frame_rejects_them() {
    assert_eq!(
        eval_with_frames(
            r#"(let* ((first (selected-frame))
                      (dead (make-terminal-frame '((name . "dead"))))
                      (left 0)
                      (current-prefix-arg 7)
                      (prefix-arg nil)
                      (mouse-leave-buffer-hook (list (lambda () (setq left (1+ left))))))
                 (delete-frame dead)
                 (list (handle-switch-frame (list 'switch-frame dead))
                       (eq (selected-frame) first)
                       left prefix-arg
                       (condition-case err (select-frame dead)
                         (wrong-type-argument (car (cdr err))))
                       (condition-case err (handle-switch-frame '(switch-frame nil))
                         (wrong-type-argument err))
                       left))"#,
        ),
        "OK (nil t 1 7 frame-live-p (wrong-type-argument framep nil) 2)"
    );
}

#[test]
fn handle_switch_frame_rechecks_liveness_after_leave_hook() {
    assert_eq!(
        eval_with_frames(
            r#"(let* ((first (selected-frame))
                      (second (make-terminal-frame '((name . "second"))))
                      (mouse-leave-buffer-hook (list (lambda () (delete-frame second)))))
                 (list (handle-switch-frame (list 'switch-frame second))
                       (frame-live-p second)
                       (eq (selected-frame) first)))"#,
        ),
        "OK (nil nil t)"
    );
}

#[test]
fn select_frame_honors_norecord_and_publishes_consistent_selection_to_hooks() {
    assert_eq!(
        eval_with_frames(
            r#"(let* ((first (selected-frame))
                      (second (make-terminal-frame '((name . "second"))))
                      (target (get-buffer-create "target"))
                      (window (frame-selected-window second))
                      (seen nil))
                 (set-window-buffer window target)
                 (let ((buffer-list-update-hook
                        (list (lambda ()
                                (setq seen
                                      (list (eq (selected-window)
                                                (frame-selected-window (selected-frame)))
                                            (eq (current-buffer)
                                                (window-buffer (selected-window)))))))))
                   (select-frame second t)
                   (let ((suppressed (null seen)))
                     (select-frame first t)
                     (select-frame second)
                     (list suppressed seen (eq (car (buffer-list)) target)))))"#,
        ),
        "OK (t (t t) t)"
    );
}

#[test]
fn cross_frame_select_window_does_not_steal_the_next_physical_key() {
    use crate::keyboard::{InputEvent, KeyEvent};

    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(buffer);
    let first = eval.frames.create_frame("first", 800, 600, buffer);
    let other_buffer = eval.buffers.create_buffer("target");
    let second = eval.frames.create_frame("second", 800, 600, other_buffer);
    for id in [first, second] {
        eval.frames
            .get_mut(id)
            .unwrap()
            .set_window_system(Some(Value::symbol("neo")));
    }
    eval.obarray_mut()
        .set_symbol_value("test-second-frame", Value::make_frame(second.0));
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.init_input_system(rx);
    tx.send(InputEvent::key_press_in_frame(KeyEvent::char('a'), first.0))
        .unwrap();
    assert_eq!(eval.read_char().unwrap(), Value::fixnum('a' as i64));

    eval.eval_str("(select-window (frame-selected-window test-second-frame))")
        .unwrap();
    tx.send(InputEvent::key_press_in_frame(KeyEvent::char('b'), first.0))
        .unwrap();
    // No new native focus notification: the first window still owns the
    // keyboard. GNU do_switch_frame explicitly documents this select-window
    // case as the reason to clear its cached last event frame.
    assert_eq!(
        eval.read_char().unwrap(),
        Value::list(vec![
            Value::symbol("switch-frame"),
            Value::make_frame(first.0),
        ])
    );
    assert_eq!(eval.read_char().unwrap(), Value::fixnum('b' as i64));
}
