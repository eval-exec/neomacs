use super::*;
use crate::keyboard::{InputEvent, KeyEvent, ReadKeySequenceOptions};
use crate::test_utils::input_wait::InputGateOwner;

#[derive(Clone, Copy, Debug)]
enum InputRead {
    Character,
    KeySequence,
}

#[derive(Debug, PartialEq, Eq)]
enum WakeOutcome {
    Published,
    RescueRequired,
    GateNotReached,
}

/// Force publication after the last idle timer has run and after the wait loop
/// checked the input queue, but before entering the kernel poll. GNU's readable
/// keyboard descriptor wakes this wait; our channel producer must notify it.
/// A separate rescue wake makes a missing notification fail without hanging.
fn input_after_idle_timer_wakes_wait(read: InputRead) {
    crate::test_utils::init_test_tracing();
    let _policy = legacy_redisplay_fixture_policy();
    let mut ev = runtime_startup_context();
    let scratch = ev.buffers.create_buffer("*input-wakeup*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(ev.frames.select_frame(frame));
    // Change the displayed buffer so redisplay cannot skip the gate callback
    // merely because its visible-state signature is unchanged. Clear timers
    // again after insertion: GNU undo amalgamation can schedule its own timer.
    ev.eval_str(
        r#"(progn
             (setq timer-list nil timer-idle-list nil)
             (setq input-wake-idle-fired nil)
             (run-with-idle-timer
              0.01 nil
              (lambda ()
                (insert "idle timer fired")
                (setq timer-list nil timer-idle-list nil
                      input-wake-idle-fired 'done))))"#,
    )
    .expect("schedule GNU idle timer");

    let input_owner = InputGateOwner::install(&mut ev);
    let publisher = input_owner.publisher();
    let _keepalive = publisher.clone();
    let rescue = ev.wait_notifier().expect("input notification backend");
    let (gate_tx, gate_rx) = crossbeam_channel::bounded(1);
    let (published_tx, published_rx) = crossbeam_channel::bounded(1);
    let (completed_tx, completed_rx) = crossbeam_channel::bounded(1);
    let mut gate = Some((gate_tx, published_rx));
    ev.redisplay_fn = Some(Box::new(move |context| {
        if context.eval_symbol("input-wake-idle-fired").ok() == Some(Value::symbol("done")) {
            if let Some((gate_tx, published_rx)) = gate.take() {
                context
                    .processes
                    .before_next_backend_wait_for_test(move |entry| {
                        let timeout = entry.requested_timeout();
                        assert!(
                            timeout >= Duration::from_secs(60),
                            "last idle timer must leave an unbounded input wait, got {timeout:?}"
                        );
                        gate_tx.send(()).expect("signal pre-poll gate");
                        published_rx
                            .recv_timeout(Duration::from_secs(5))
                            .expect("publisher acknowledges input");
                    });
            }
        }
    }));

    let worker = thread::spawn(move || {
        match gate_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(()) => {}
            Err(
                crossbeam_channel::RecvTimeoutError::Timeout
                | crossbeam_channel::RecvTimeoutError::Disconnected,
            ) => {
                input_owner
                    .cancel_missing_witness()
                    .expect("cancel missing wait witness");
                return WakeOutcome::GateNotReached;
            }
        }
        publisher
            .send(InputEvent::key_press(KeyEvent::char('a')))
            .expect("publish input");
        published_tx.send(()).expect("acknowledge publication");
        if completed_rx.recv_timeout(Duration::from_secs(2)).is_ok() {
            WakeOutcome::Published
        } else {
            rescue.notify().expect("rescue missing input notification");
            WakeOutcome::RescueRequired
        }
    });

    let result = match read {
        InputRead::Character => ev.read_char().map(|key| vec![key]),
        InputRead::KeySequence => ev
            .read_key_sequence_with_options(ReadKeySequenceOptions::default())
            .map(|(keys, _binding)| keys),
    };
    let _ = completed_tx.send(());
    let outcome = worker.join().expect("input publisher thread");
    assert_eq!(
        outcome,
        WakeOutcome::Published,
        "{read:?} needed a rescue wake"
    );
    let keys = result.expect("read published input");
    assert_eq!(keys, vec![Value::fixnum('a' as i64)]);
    assert_eq!(
        ev.eval_symbol("input-wake-idle-fired").expect("idle flag"),
        Value::symbol("done")
    );
}

/// Callback-driven display changes must affect the very first command key.
#[test]
fn command_loop_uses_selected_buffers_map_after_async_switch() {
    let _policy = legacy_redisplay_fixture_policy();
    let mut ev = runtime_startup_context();
    let scratch = ev.buffers.create_buffer("*async-old*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(ev.frames.select_frame(frame));
    let _input_owner = InputGateOwner::install(&mut ev);
    fn stop(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-async-map-test",
        NativeFn::ContextVec(stop),
        SubrArity::new(0, Some(0)),
    ));
    ev.eval_str(
        r#"(progn
             (setq timer-list nil timer-idle-list nil neo-old-map-count 0)
             (switch-to-buffer "*async-old*")
             (use-local-map (make-sparse-keymap))
             (local-set-key "a" (lambda () (interactive)
                                  (setq neo-old-map-count (1+ neo-old-map-count))))
             (with-current-buffer (get-buffer-create "*async-new*")
               (fundamental-mode))
             (global-set-key "b" (lambda () (interactive) (neo-stop-async-map-test)))
             (run-at-time 0.01 nil
               (lambda ()
                 (save-current-buffer (switch-to-buffer "*async-new*"))
                 (setq unread-command-events (list ?a ?a ?b)))))"#,
    )
    .unwrap();
    ev.command_loop.running = true;
    ev.recursive_edit_inner().unwrap();
    assert_eq!(
        ev.eval_symbol("neo-old-map-count").unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        ev.eval_str("(with-current-buffer \"*async-new*\" (buffer-string))")
            .unwrap(),
        Value::string("aa")
    );
}

/// Lisp key readers retain their caller's buffer despite asynchronous display.
#[test]
fn lisp_key_reader_preserves_current_buffer_after_async_display() {
    let _policy = legacy_redisplay_fixture_policy();
    let mut ev = runtime_startup_context();
    let old = ev.buffers.create_buffer("*reader-old*");
    ev.buffers.set_current(old);
    let frame = ev.frames.create_frame("F1", 80, 24, old);
    assert!(ev.frames.select_frame(frame));
    let _input_owner = InputGateOwner::install(&mut ev);
    ev.eval_str(
        r#"(progn
             (setq timer-list nil timer-idle-list nil)
             (use-local-map (make-sparse-keymap))
             (fset 'neo-old-key (lambda () (interactive)))
             (local-set-key "a" 'neo-old-key)
             (with-current-buffer (get-buffer-create "*reader-new*")
               (fundamental-mode))
             (run-at-time 0.01 nil
               (lambda ()
                 (save-current-buffer (switch-to-buffer "*reader-new*"))
                 (setq unread-command-events (list ?a)))))"#,
    )
    .unwrap();
    let (keys, binding) = ev
        .read_key_sequence_with_options(ReadKeySequenceOptions::default())
        .unwrap();
    assert_eq!(keys, vec![Value::fixnum('a' as i64)]);
    assert_eq!(binding.original(), Value::symbol("neo-old-key"));
    assert_eq!(ev.buffers.current_buffer_id(), Some(old));
}

#[test]
fn read_char_wakes_for_input_published_at_poll_entry_after_idle_timer() {
    input_after_idle_timer_wakes_wait(InputRead::Character);
}

#[test]
fn read_key_sequence_wakes_for_input_published_at_poll_entry_after_idle_timer() {
    input_after_idle_timer_wakes_wait(InputRead::KeySequence);
}
