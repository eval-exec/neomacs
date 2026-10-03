use crate::emacs_core::value::Value;

fn arm_watched_timer(eval: &mut crate::emacs_core::Context, operation: &str, exit: &str) {
    eval.eval_str(&format!(
        "(progn
           (setq boundary-operation '{operation} boundary-count 0
                 boundary-before nil boundary-raw nil boundary-nested nil
                 boundary-body-ran nil unread-command-events '(24))
           (defun boundary-watcher (_symbol new-value operation _where)
             (when (and (eq operation boundary-operation)
                        (or (eq operation 'let) (null new-value))
                        (equal (this-command-keys-vector) [24]))
               (remove-variable-watcher 'inhibit-quit #'boundary-watcher)
               (setq boundary-count (1+ boundary-count)
                     boundary-before (this-command-keys-vector)
                     boundary-raw (this-single-command-raw-keys)
                     unread-command-events '(120))
               (setq boundary-nested (read-key-sequence-vector \"\"))
               (setq unread-command-events '(102 122 120))
               (garbage-collect)
               {exit}))
           (when (eq boundary-operation 'let)
             (add-variable-watcher 'inhibit-quit #'boundary-watcher))
           (run-at-time 0 nil
             (lambda ()
               (setq boundary-body-ran t)
               (when (eq boundary-operation 'unlet)
                 (add-variable-watcher 'inhibit-quit #'boundary-watcher)))))"
    ))
    .expect("arm timer binding watcher");
}

fn assert_observations(eval: &mut crate::emacs_core::Context) {
    assert_eq!(
        eval.eval_str(
            "(equal (list boundary-count boundary-before boundary-raw boundary-nested)
                '(1 [24] [24] [120]))"
        )
        .unwrap(),
        Value::T
    );
    assert!(eval.active_variable_watchers.is_empty());
    assert_eq!(
        eval.eval_str("(get-variable-watchers 'inhibit-quit)")
            .unwrap(),
        Value::NIL
    );
}

fn assert_future(eval: &mut crate::emacs_core::Context, events: &[i64]) {
    for &event in events {
        assert_eq!(
            eval.read_key_sequence().unwrap().0,
            vec![Value::fixnum(event)]
        );
    }
}

fn pending_prefix_case(operation: &str, exit: &str) {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    arm_watched_timer(&mut eval, operation, exit);
    let specpdl = eval.specpdl.len();
    let roots = eval.save_vm_roots();
    let frame_roots = eval.save_vm_frame_roots();
    assert_eq!(
        eval.read_key_sequence().unwrap().0,
        vec![Value::fixnum(24), Value::fixnum(102)]
    );
    assert_eq!(eval.specpdl.len(), specpdl);
    assert_eq!(eval.save_vm_frame_roots(), frame_roots);
    eval.restore_vm_roots(roots);
    assert_observations(&mut eval);
    assert_eq!(
        eval.eval_str("boundary-body-ran").unwrap(),
        if operation == "let" && exit != "nil" {
            Value::NIL
        } else {
            Value::T
        }
    );
    assert_future(&mut eval, &[122, 120]);
}

#[test]
fn timer_let_watcher_preserves_pending_prefix() {
    pending_prefix_case("let", "nil");
}

#[test]
fn timer_unlet_watcher_preserves_pending_prefix() {
    pending_prefix_case("unlet", "nil");
}

#[test]
fn timer_let_watcher_signal_preserves_pending_prefix() {
    pending_prefix_case("let", "(error \"let watcher failed\")");
}

#[test]
fn timer_unlet_watcher_signal_preserves_pending_prefix() {
    pending_prefix_case("unlet", "(error \"unlet watcher failed\")");
}

fn escaping_throw_case(operation: &str) {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    arm_watched_timer(&mut eval, operation, "(throw 'boundary-escape 'escaped)");
    let specpdl = eval.specpdl.len();
    let roots = eval.save_vm_roots();
    let frame_roots = eval.save_vm_frame_roots();
    assert_eq!(
        eval.eval_str("(catch 'boundary-escape (read-key-sequence-vector \"\") 'not-escaped)")
            .unwrap(),
        Value::symbol("escaped")
    );
    assert_eq!(eval.specpdl.len(), specpdl);
    assert_eq!(eval.save_vm_frame_roots(), frame_roots);
    eval.restore_vm_roots(roots);
    let board = &eval.command_loop.keyboard.kboard;
    assert_eq!(
        board.current_key_sequence.raw_events(),
        &[Value::fixnum(24)]
    );
    assert_eq!(board.command_keys, vec![Value::fixnum(24)]);
    assert_eq!(board.raw_command_keys, vec![Value::fixnum(24)]);
    assert_observations(&mut eval);
    assert_future(&mut eval, &[102, 122, 120]);
}

#[test]
fn timer_let_watcher_escaping_throw_restores_reader() {
    escaping_throw_case("let");
}

#[test]
fn timer_unlet_watcher_escaping_throw_restores_reader() {
    escaping_throw_case("unlet");
}

#[test]
fn timer_watcher_gc_roots_heap_reader_on_entry_and_exit() {
    crate::test_utils::init_test_tracing();
    for operation in ["let", "unlet"] {
        for exit in ["nil", "(error \"heap watcher failed\")"] {
            let mut eval = crate::test_utils::runtime_startup_context();
            let callback = eval
                .eval_str(&format!(
                    "(progn
                   (setq heap-operation '{operation} heap-count 0)
                   (defun heap-watcher (_symbol new-value operation _where)
                     (when (and (eq operation heap-operation)
                                (or (eq operation 'let) (null new-value)))
                       (remove-variable-watcher 'inhibit-quit #'heap-watcher)
                       (setq heap-count (1+ heap-count)
                             unread-command-events '(120))
                       (read-key-sequence-vector \"\")
                       (setq unread-command-events '(122 120))
                       (garbage-collect)
                       {exit}))
                   (when (eq heap-operation 'let)
                     (add-variable-watcher 'inhibit-quit #'heap-watcher))
                   (function (lambda ()
                     (setq heap-body-inhibited inhibit-quit)
                     (when (eq heap-operation 'unlet)
                       (add-variable-watcher 'inhibit-quit #'heap-watcher)))))"
                ))
                .unwrap();
            // These heap events are held only by the suspended reader; the
            // watcher replaces publication and collects before returning.
            let raw = Value::string("saved raw");
            let translated = Value::string("saved translated");
            let published = Value::string("saved published");
            let published_raw = Value::string("saved published raw");
            let board = &mut eval.command_loop.keyboard.kboard;
            board.current_key_sequence.push_input_event(raw);
            board
                .current_key_sequence
                .replace_translated_events(vec![translated]);
            board.command_keys = vec![published];
            board.raw_command_keys = vec![published_raw];
            let specpdl = eval.specpdl.len();
            let roots = eval.save_vm_roots();
            let frame_roots = eval.save_vm_frame_roots();
            eval.run_timer_callback_preserving_state(callback, vec![])
                .unwrap();
            assert_eq!(eval.specpdl.len(), specpdl);
            assert_eq!(eval.save_vm_frame_roots(), frame_roots);
            eval.restore_vm_roots(roots);
            let board = &eval.command_loop.keyboard.kboard;
            assert_eq!(board.current_key_sequence.raw_events(), &[raw]);
            assert_eq!(
                board.current_key_sequence.translated_events(),
                &[translated]
            );
            assert_eq!(board.command_keys, vec![published]);
            assert_eq!(board.raw_command_keys, vec![published_raw]);
            for (value, expected) in [
                (raw, "saved raw"),
                (translated, "saved translated"),
                (published, "saved published"),
                (published_raw, "saved published raw"),
            ] {
                assert_eq!(
                    value.as_lisp_string().unwrap().as_utf8_str(),
                    Some(expected)
                );
            }
            assert_eq!(eval.eval_str("heap-count").unwrap(), Value::fixnum(1));
            if operation == "unlet" || exit == "nil" {
                assert_eq!(eval.eval_str("heap-body-inhibited").unwrap(), Value::T);
            }
            if exit == "nil" {
                assert_eq!(eval.eval_str("inhibit-quit").unwrap(), Value::NIL);
            }
            assert!(eval.active_variable_watchers.is_empty());
            assert_future(&mut eval, &[122, 120]);
        }
    }
}
