use super::*;
use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
use crate::test_utils::input_wait::InputGateOwner;
use neovm_value::system_vocab::input::ReadKeySequenceOptions;

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
    let scratch = ev.edit.buffers.create_buffer("*input-wakeup*");
    ev.edit.buffers.set_current(scratch);
    let frame = ev.frame_manager_mut().create_frame("F1", 80, 24, scratch);
    assert!(ev.frame_manager_mut().select_frame(frame));
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
                    .processes_mut()
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
    let scratch = ev.edit.buffers.create_buffer("*async-old*");
    ev.edit.buffers.set_current(scratch);
    let frame = ev.frame_manager_mut().create_frame("F1", 80, 24, scratch);
    assert!(ev.frame_manager_mut().select_frame(frame));
    let _input_owner = InputGateOwner::install(&mut ev);
    fn stop(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        ctx.command_loop_mut().running = false;
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
    ev.command_loop_mut().running = true;
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

/// E7-P.2 end-to-end interactive `command_loop_1` integration test:
/// verifies `run_exit_wrapped_command_loop` establishes the D1
/// `CommandLoopBaseline` witness, defers `PendingUpgrade` inside nested
/// `recursive-edit`, activates ONCE at the top-level `command_loop_1`
/// `BetweenCommands` boundary, withdraws legacy serial JIT, and tears down cleanly.
#[test]
fn e7p2_interactive_command_loop_1_between_commands_and_nested_recursive_edit() {
    crate::test_utils::init_test_tracing();
    let _policy = legacy_redisplay_fixture_policy();
    let mut ev = crate::test_utils::new_green_context();
    // SAFETY: serial fixture setup on fresh Context before any concurrent work.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            ev.tagged_heap_mut(),
            true,
        );
    }
    ev.setup_thread_locals();
    ev.set_variable("noninteractive", Value::NIL);
    let scratch = ev.edit.buffers.create_buffer("*e7p2-cmd-loop*");
    ev.edit.buffers.set_current(scratch);
    let frame = ev.frame_manager_mut().create_frame("F1", 80, 24, scratch);
    assert!(ev.frame_manager_mut().select_frame(frame));

    fn arm_and_probe(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        assert!(
            ctx.is_command_loop_baseline_active_for_test(),
            "D1: interactive command_loop_1 must carry an active CommandLoopBaseline witness"
        );
        ctx.arm_pending_upgrade_for_test();
        Ok(Value::bool_val(
            ctx.is_parallel_activation_active_for_test(),
        ))
    }
    fn probe_active(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        Ok(Value::list(vec![
            Value::bool_val(ctx.is_parallel_activation_active_for_test()),
            Value::bool_val(ctx.legacy_jit_withdrawn_for_test()),
        ]))
    }
    fn stop_loop(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        ctx.command_loop_mut().running = false;
        Ok(Value::NIL)
    }

    ev.register_subr(SubrSpec::new(
        "e7p2-test-arm",
        NativeFn::ContextVec(arm_and_probe),
        SubrArity::new(0, Some(0)),
    ));
    ev.register_subr(SubrSpec::new(
        "e7p2-test-probe",
        NativeFn::ContextVec(probe_active),
        SubrArity::new(0, Some(0)),
    ));
    ev.register_subr(SubrSpec::new(
        "e7p2-test-stop",
        NativeFn::ContextVec(stop_loop),
        SubrArity::new(0, Some(0)),
    ));

    // Start Green on Main after registering subrs and before attaching the
    // thread-affine input host (E4 hosted Green startup order).
    neovm_eval::test_support::authority::start_green_threads_for_test(&mut ev)
        .expect("start Green runtime before attaching input host");
    let _input_owner = InputGateOwner::install(&mut ev);

    // Sequence of interactive commands:
    //   ?r -> enters nested `recursive-edit`, runs ?a (arms PendingUpgrade while
    //         nested), runs ?p (must STILL be inactive because nested recursive-edit
    //         refuses BetweenCommands activation!), runs ?x (`exit-recursive-edit`)
    //   ?m -> runs `(execute-kbd-macro "k")` mid-command after returning from
    //         nested recursive-edit? Wait: run ?m BEFORE ?r exits? No, ?r already
    //         exits back to top-level command_loop_1! So run ?m FIRST at top-level:
    //         ?m arms PendingUpgrade AND immediately runs `(execute-kbd-macro "k")`
    //         which re-enters `command_loop_1` -> `parallel_between_commands_boundary`
    //         while `recursive_depth == 1 && minibuffers().depth() == 0`!
    //         HIGH #3 requires `k` inside `execute-kbd-macro` to observe `(nil nil)`!
    //   ?r -> enters nested `recursive-edit`, runs ?p (still `(nil nil)`), runs ?x
    //   ?q -> top-level command after unwinding back through top-level `command_loop_1`
    //         `BetweenCommands` boundary: MUST now observe Active + legacy JIT withdrawn!
    //   ?s -> stops outermost command loop cleanly.
    ev.eval_str(
        r#"(progn
             (fset 'command-execute (lambda (cmd &rest _) (funcall cmd)))
             (setq e7p2-arm-res nil e7p2-kmacro-probe nil e7p2-nested-probe nil e7p2-top-probe nil)
             (use-global-map (make-sparse-keymap))
             (let ((map (current-global-map)))
               (define-key map "m" (lambda () (interactive)
                                     (setq e7p2-arm-res (e7p2-test-arm))
                                     (execute-kbd-macro "k")
                                     (setq unread-command-events (list ?q ?s))))
               (define-key map "k" (lambda () (interactive) (setq e7p2-kmacro-probe (e7p2-test-probe))))
               (define-key map "r" (lambda () (interactive) (recursive-edit)))
               (define-key map "p" (lambda () (interactive) (setq e7p2-nested-probe (e7p2-test-probe))))
               (define-key map "x" (lambda () (interactive) (throw 'exit nil)))
               (define-key map "q" (lambda () (interactive) (setq e7p2-top-probe (e7p2-test-probe))))
               (define-key map "s" (lambda () (interactive) (e7p2-test-stop))))
             (setq unread-command-events (list ?m)))"#,
    )
    .unwrap();

    ev.command_loop_mut().running = true;
    ev.recursive_edit_inner().unwrap();

    assert_eq!(
        ev.eval_symbol("e7p2-arm-res").unwrap(),
        Value::NIL,
        "D2: arming PendingUpgrade mid-command must not activate synchronously"
    );
    assert_eq!(
        crate::emacs_core::print::print_value(&ev.eval_symbol("e7p2-kmacro-probe").unwrap()),
        "(nil nil)",
        "HIGH #3: execute-kbd-macro re-entry into command_loop_1 must defer PendingUpgrade"
    );
    assert_eq!(
        crate::emacs_core::print::print_value(&ev.eval_symbol("e7p2-top-probe").unwrap()),
        "(t t)",
        "D1/D2/D4: returning to top-level command_loop_1 BetweenCommands must open era and withdraw legacy JIT"
    );
    neovm_eval::test_support::authority::green_teardown_for_test(&mut ev);

    // Also verify nested `recursive-edit` deferral end-to-end in a fresh Context.
    drop(_input_owner);
    drop(ev);
    let mut ev2 = crate::test_utils::new_green_context();
    // SAFETY: serial fixture setup on fresh Context before any concurrent work.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            ev2.tagged_heap_mut(),
            true,
        );
    }
    ev2.setup_thread_locals();
    ev2.set_variable("noninteractive", Value::NIL);
    let scratch2 = ev2.edit.buffers.create_buffer("*e7p2-cmd-loop-2*");
    ev2.edit.buffers.set_current(scratch2);
    let frame2 = ev2.frame_manager_mut().create_frame("F2", 80, 24, scratch2);
    assert!(ev2.frame_manager_mut().select_frame(frame2));
    ev2.register_subr(SubrSpec::new(
        "e7p2-test-arm",
        NativeFn::ContextVec(arm_and_probe),
        SubrArity::new(0, Some(0)),
    ));
    ev2.register_subr(SubrSpec::new(
        "e7p2-test-probe",
        NativeFn::ContextVec(probe_active),
        SubrArity::new(0, Some(0)),
    ));
    ev2.register_subr(SubrSpec::new(
        "e7p2-test-stop",
        NativeFn::ContextVec(stop_loop),
        SubrArity::new(0, Some(0)),
    ));
    neovm_eval::test_support::authority::start_green_threads_for_test(&mut ev2)
        .expect("start Green runtime before attaching input host");
    let _input_owner2 = InputGateOwner::install(&mut ev2);
    ev2.eval_str(
        r#"(progn
             (fset 'command-execute (lambda (cmd &rest _) (funcall cmd)))
             (setq e7p2-arm-res nil e7p2-nested-probe nil e7p2-top-probe nil)
             (use-global-map (make-sparse-keymap))
             (let ((map (current-global-map)))
               (define-key map "r" (lambda () (interactive) (recursive-edit)))
               (define-key map "a" (lambda () (interactive) (setq e7p2-arm-res (e7p2-test-arm))))
               (define-key map "p" (lambda () (interactive) (setq e7p2-nested-probe (e7p2-test-probe))))
               (define-key map "x" (lambda () (interactive) (throw 'exit nil)))
               (define-key map "q" (lambda () (interactive) (setq e7p2-top-probe (e7p2-test-probe))))
               (define-key map "s" (lambda () (interactive) (e7p2-test-stop))))
             (setq unread-command-events (list ?r ?a ?p ?x ?q ?s)))"#,
    )
    .unwrap();
    ev2.command_loop_mut().running = true;
    ev2.recursive_edit_inner().unwrap();
    assert_eq!(
        crate::emacs_core::print::print_value(&ev2.eval_symbol("e7p2-nested-probe").unwrap()),
        "(nil nil)",
        "D1: nested recursive-edit must defer PendingUpgrade without opening era"
    );
    assert_eq!(
        crate::emacs_core::print::print_value(&ev2.eval_symbol("e7p2-top-probe").unwrap()),
        "(t t)",
        "D1/D2/D4: returning to top-level command_loop_1 BetweenCommands must open era and withdraw legacy JIT"
    );
    neovm_eval::test_support::authority::green_teardown_for_test(&mut ev2);
}

/// Lisp key readers retain their caller's buffer despite asynchronous display.
#[test]
fn lisp_key_reader_preserves_current_buffer_after_async_display() {
    let _policy = legacy_redisplay_fixture_policy();
    let mut ev = runtime_startup_context();
    let old = ev.edit.buffers.create_buffer("*reader-old*");
    ev.edit.buffers.set_current(old);
    let frame = ev.frame_manager_mut().create_frame("F1", 80, 24, old);
    assert!(ev.frame_manager_mut().select_frame(frame));
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
    assert_eq!(ev.edit.buffers.current_buffer_id(), Some(old));
}

#[test]
fn read_char_wakes_for_input_published_at_poll_entry_after_idle_timer() {
    input_after_idle_timer_wakes_wait(InputRead::Character);
}

#[test]
fn read_key_sequence_wakes_for_input_published_at_poll_entry_after_idle_timer() {
    input_after_idle_timer_wakes_wait(InputRead::KeySequence);
}
