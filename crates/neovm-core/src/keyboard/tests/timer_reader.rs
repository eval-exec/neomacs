use super::*;

#[test]
fn timer_callback_observes_published_pending_prefix() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn
           (setq timer-visible-keys nil timer-visible-raw nil
                 unread-command-events '(24))
           (run-at-time 0 nil
             (lambda ()
               (setq timer-visible-keys (this-command-keys-vector)
                     timer-visible-raw (this-single-command-raw-keys)
                     unread-command-events '(102 122)))))",
    )
    .expect("arm observing timer");
    assert_eq!(
        eval.read_key_sequence().unwrap().0,
        vec![Value::fixnum(24), Value::fixnum(102)]
    );
    assert_eq!(
        eval.eval_str("(equal (list timer-visible-keys timer-visible-raw) '([24] [24]))")
            .unwrap(),
        Value::T
    );
    assert_eq!(
        eval.read_key_sequence().unwrap().0,
        vec![Value::fixnum(122)]
    );
}

#[test]
fn timer_callback_finishes_ambiguous_read_key_escape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval
        .eval_str(
            "(progn
               (define-key input-decode-map \"\\e[A\" [up])
               (setq unread-command-events '(27))
               (with-timeout (0.15 'timed-out) (read-key)))",
        )
        .expect("read-key ambiguity probe");
    assert_eq!(result, Value::fixnum(27));
}

#[test]
fn saved_key_reader_roots_heap_state_when_callback_replaces_publication() {
    crate::test_utils::init_test_tracing();
    for exit in ["nil", "(error \"reader callback failed\")"] {
        let mut eval = crate::emacs_core::Context::new();
        let raw = Value::string("saved raw event");
        let translated = Value::string("saved translated event");
        let published = Value::string("saved published event");
        let published_raw = Value::string("saved published raw event");
        let board = &mut eval.command_loop.keyboard.kboard;
        board.current_key_sequence.push_input_event(raw);
        board
            .current_key_sequence
            .replace_translated_events(vec![translated]);
        board.command_keys = vec![published];
        board.raw_command_keys = vec![published_raw];
        board.key_echo_state = KeyEchoState::Immediate {
            prompt: Value::string("Saved prompt: ").as_lisp_string().cloned(),
        };
        let result = eval.with_saved_key_reader(|inner| {
            let board = &mut inner.command_loop.keyboard.kboard;
            assert!(board.current_key_sequence.raw_events().is_empty());
            assert_eq!(board.command_keys, vec![published]);
            assert_eq!(board.raw_command_keys, vec![published_raw]);
            assert!(matches!(board.key_echo_state, KeyEchoState::Inactive));
            board
                .current_key_sequence
                .push_input_event(Value::fixnum(7));
            board.command_keys = vec![Value::fixnum(7)];
            board.raw_command_keys = vec![Value::fixnum(7)];
            inner.eval_str("(garbage-collect)").unwrap();
            if exit == "nil" {
                Ok(Value::NIL)
            } else {
                Err(crate::emacs_core::error::signal(
                    crate::emacs_core::error::LispCondition::Error,
                    vec![Value::string("reader callback failed")],
                ))
            }
        });
        assert_eq!(result.is_err(), exit != "nil");
        let board = &eval.command_loop.keyboard.kboard;
        assert_eq!(board.current_key_sequence.raw_events(), &[raw]);
        assert_eq!(
            board.current_key_sequence.translated_events(),
            &[translated]
        );
        assert_eq!(board.command_keys, vec![published]);
        assert_eq!(board.raw_command_keys, vec![published_raw]);
        assert_eq!(
            raw.as_lisp_string().unwrap().as_utf8_str(),
            Some("saved raw event")
        );
        assert_eq!(
            translated.as_lisp_string().unwrap().as_utf8_str(),
            Some("saved translated event")
        );
        assert_eq!(
            published.as_lisp_string().unwrap().as_utf8_str(),
            Some("saved published event")
        );
        assert_eq!(
            published_raw.as_lisp_string().unwrap().as_utf8_str(),
            Some("saved published raw event")
        );
        assert!(matches!(&board.key_echo_state,
            KeyEchoState::Immediate { prompt: Some(prompt) } if prompt.as_utf8_str() == Some("Saved prompt: ")));
    }
}

#[test]
fn input_method_still_clears_publication_and_restores_outer_reader() {
    crate::test_utils::init_test_tracing();
    for exit in ["event", "(error \"input method failed\")"] {
        let mut eval = crate::emacs_core::Context::new();
        eval.command_loop
            .keyboard
            .kboard
            .current_key_sequence
            .push_input_event(Value::fixnum(24));
        eval.command_loop.keyboard.kboard.command_keys = vec![Value::fixnum(24)];
        eval.command_loop.keyboard.kboard.raw_command_keys = vec![Value::fixnum(24)];
        let function = eval
            .eval_str(&format!(
                "(function (lambda (event)
               (setq input-method-saw-empty
                     (and (= (length (this-command-keys-vector)) 0)
                          (= (length (this-single-command-raw-keys)) 0)))
               (garbage-collect)
               {exit}))"
            ))
            .unwrap();
        let result = eval.apply_input_method_with_saved_reader(function, Value::fixnum(97));
        assert_eq!(result.is_err(), exit != "event");
        assert_eq!(eval.eval_str("input-method-saw-empty").unwrap(), Value::T);
        let board = &eval.command_loop.keyboard.kboard;
        assert!(!board.in_input_method_function);
        assert_eq!(
            board.current_key_sequence.raw_events(),
            &[Value::fixnum(24)]
        );
        assert_eq!(board.command_keys, vec![Value::fixnum(24)]);
        assert_eq!(board.raw_command_keys, vec![Value::fixnum(24)]);
    }
}

#[test]
fn timer_nested_reader_escaping_throw_restores_publication_and_future_input() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval
        .eval_str(
            "(catch 'timer-reader-escape
               (setq unread-command-events '(24))
               (run-at-time 0 nil
                 (lambda ()
                   (setq timer-before-nested (this-command-keys-vector)
                         unread-command-events '(120))
                   (setq timer-nested-keys (read-key-sequence-vector \"\"))
                   (setq unread-command-events '(122 120))
                   (garbage-collect)
                   (throw 'timer-reader-escape 'escaped)))
               (read-key-sequence-vector \"\")
               'did-not-escape)",
        )
        .expect("timer throw reaches actual outer catch");
    assert_eq!(result, Value::symbol("escaped"));
    assert_eq!(
        eval.eval_str("(equal (list timer-before-nested timer-nested-keys) '([24] [120]))")
            .unwrap(),
        Value::T
    );
    let board = &eval.command_loop.keyboard.kboard;
    assert_eq!(
        board.current_key_sequence.raw_events(),
        &[Value::fixnum(24)]
    );
    assert_eq!(board.command_keys, vec![Value::fixnum(24)]);
    assert_eq!(board.raw_command_keys, vec![Value::fixnum(24)]);
    assert_eq!(
        eval.read_key_sequence().unwrap().0,
        vec![Value::fixnum(122)]
    );
    assert_eq!(
        eval.read_key_sequence().unwrap().0,
        vec![Value::fixnum(120)]
    );
}

/// A timer's recursive minibuffer read must not replace the suspended outer
/// key sequence. In GNU that accumulator lives on each read's C stack.
#[test]
fn timer_minibuffer_quit_preserves_outer_key_reader() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let scratch = eval.buffers.create_buffer("*timer-reader*");
    eval.buffers.set_current(scratch);
    let frame = eval.frames.create_frame("F1", 80, 24, scratch);
    assert!(eval.frames.select_frame(frame));
    eval.set_variable("noninteractive", Value::NIL);
    let (_tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.eval_str(
        "(progn
           (setq timer-reader-result nil)
           (run-at-time 0 nil
             (lambda ()
               (setq unread-command-events '(7))
               (condition-case nil
                   (read-string \"Timer: \")
                 (quit (setq timer-reader-result 'quit)))
               (setq unread-command-events '(122 120)))))",
    )
    .expect("arm recursive reader timer");
    let (keys, binding) = eval.read_key_sequence().expect("outer z read");
    assert_eq!(
        eval.eval_str("timer-reader-result").unwrap(),
        Value::symbol("quit")
    );
    assert_eq!(eval.minibuffers.depth(), 0);
    assert!(eval.quit_flag_value().is_nil());
    assert!(!eval.quit_requested.is_requested());
    assert_eq!(
        keys,
        vec![Value::fixnum(122)],
        "inner C-g must not prefix outer z"
    );
    assert_eq!(binding, Value::symbol("self-insert-command"));
    assert_eq!(
        eval.read_key_sequence().unwrap().0,
        vec![Value::fixnum(120)]
    );
}

#[test]
fn timer_minibuffer_acceptance_preserves_outer_prefix_and_input_on_error() {
    crate::test_utils::init_test_tracing();
    for exit in [
        "nil",
        "(error \"callback failed\")",
        "(throw 'timer-reader-tag t)",
    ] {
        let mut eval = crate::test_utils::runtime_startup_context();
        let scratch = eval.buffers.create_buffer("*timer-prefix*");
        eval.buffers.set_current(scratch);
        let frame = eval.frames.create_frame("F1", 80, 24, scratch);
        assert!(eval.frames.select_frame(frame));
        eval.set_variable("noninteractive", Value::NIL);
        let (_tx, rx) = crossbeam_channel::unbounded();
        eval.input_rx = Some(rx);
        eval.eval_str(&format!(
            "(progn
               (setq timer-reader-result nil unread-command-events '(24))
               (run-at-time 0 nil
                 (lambda ()
                   (setq unread-command-events '(13))
                   (setq timer-reader-result (read-string \"Timer: \"))
                   (setq unread-command-events '(102 120))
                   (garbage-collect)
                   {exit})))"
        ))
        .expect("arm acceptance/error timer");
        let (keys, binding) = eval.read_key_sequence().expect("outer C-x f read");
        assert_eq!(
            eval.eval_str("(equal timer-reader-result \"\")").unwrap(),
            Value::T
        );
        assert_eq!(
            keys,
            vec![Value::fixnum(24), Value::fixnum(102)],
            "callback must preserve prefix on {exit}"
        );
        assert_eq!(binding, Value::symbol("set-fill-column"));
        assert_eq!(eval.minibuffers.depth(), 0);
        assert_eq!(
            eval.read_key_sequence().unwrap().0,
            vec![Value::fixnum(120)]
        );
    }
}
