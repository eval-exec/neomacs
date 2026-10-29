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

#[test]
fn read_char_wakes_for_input_published_at_poll_entry_after_idle_timer() {
    input_after_idle_timer_wakes_wait(InputRead::Character);
}

#[test]
fn read_key_sequence_wakes_for_input_published_at_poll_entry_after_idle_timer() {
    input_after_idle_timer_wakes_wait(InputRead::KeySequence);
}
