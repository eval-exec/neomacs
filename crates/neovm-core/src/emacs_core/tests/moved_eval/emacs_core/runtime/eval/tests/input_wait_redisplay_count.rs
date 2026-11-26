//! A maintenance wake does not start another logical command-input read.
//!
//! GNU keyboard.c:2693-2721 does preliminary redisplay before its blocking
//! read. Internal maintenance deadlines must resume that read rather than
//! re-enter the Lisp-bearing preliminary redisplay path.

use super::*;
#[cfg(unix)]
use crate::emacs_core::eval::input_hooks::CommandInputWaitOutcome;
use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
use crate::emacs_core::eval::{DisplayMaintenanceOutcome, DisplayMaintenancePublication};
use crate::test_utils::input_wait::{
    InputGateOwner, Readiness, WaitWitness, install_after_redisplay, select_visible_fixture,
};
use std::cell::Cell;

fn counted_context() -> Context {
    let mut eval = crate::test_utils::new_context();
    select_visible_fixture(&mut eval, "*idle-wake-pre-redisplay*");
    // This fixture observes the real Lisp callback boundary. It does not
    // impersonate an accepted frontend presentation or count paint callbacks.
    eval.redisplay_fn = Some(Box::new(|_| {}));
    eval.eval_str(
        "(setq timer-list nil
               timer-idle-list nil
               inhibit-redisplay nil
               idle-wake-pre-count 0
               pre-redisplay-function
               (lambda (_windows)
                 (setq idle-wake-pre-count (1+ idle-wake-pre-count))))",
    )
    .expect("install the Lisp pre-redisplay counter");
    eval
}

/// Expired frontend maintenance deadlines exercise the same outer-reader
/// resume as positive idle maintenance waits, without sleeping or depending
/// on the scheduler. The final None reaches an observed positive native wait.
fn install_no_progress_deadlines(eval: &mut Context, deadlines: usize) -> Rc<Cell<usize>> {
    install_native_deadlines(eval, deadlines, DisplayMaintenancePublication::Idle)
}

fn install_native_deadlines(
    eval: &mut Context,
    deadlines: usize,
    publication: DisplayMaintenancePublication,
) -> Rc<Cell<usize>> {
    let calls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&calls);
    eval.display_idle_maintenance_fn = Some(Box::new(move |_| {
        let call = observed.get();
        observed.set(call + 1);
        DisplayMaintenanceOutcome {
            next_wake: (call < deadlines).then_some(Duration::ZERO),
            publication,
        }
    }));
    calls
}

fn assert_pre_count(eval: &mut Context, expected: i64) {
    assert_eq!(
        eval.eval_symbol("idle-wake-pre-count")
            .expect("Lisp callback count"),
        Value::fixnum(expected),
        "internal maintenance must not replay preliminary Lisp redisplay"
    );
}

#[test]
fn command_input_idle_maintenance_deadlines_do_not_replay_pre_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    for deadlines in [0, 1, 8] {
        let mut eval = counted_context();
        let calls = install_no_progress_deadlines(&mut eval, deadlines);
        let owner = InputGateOwner::install(&mut eval);
        let (gate, publication) = owner.gate(vec![InputEvent::key_press(KeyEvent::char('x'))]);
        gate.arm(&mut eval, WaitWitness::InputWaitEntered);

        // read_key_sequence uses the command-input policy; plain read_char
        // intentionally does not install GUI idle-maintenance deadlines.
        let read = eval.read_key_sequence();
        let observed = publication.finish().expect("required positive input wait");
        assert_eq!(observed.witness, WaitWitness::InputWaitEntered);
        assert!(observed.entry.requested_timeout() > Duration::ZERO);
        let (keys, _) = read.expect("read the key published after maintenance");
        assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
        assert_eq!(calls.get(), deadlines + 1, "all maintenance turns ran");
        assert_pre_count(&mut eval, 1);
    }
}

#[test]
fn native_publication_and_deferral_deadlines_do_not_replay_pre_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    for publication in [
        DisplayMaintenancePublication::PublishedNatively,
        DisplayMaintenancePublication::Deferred,
    ] {
        let mut eval = counted_context();
        let calls = install_native_deadlines(&mut eval, 8, publication);
        let owner = InputGateOwner::install(&mut eval);
        let (gate, publication) = owner.gate(vec![InputEvent::key_press(KeyEvent::char('x'))]);
        gate.arm(&mut eval, WaitWitness::InputWaitEntered);

        let read = eval.read_key_sequence();
        let observed = publication.finish().expect("required positive input wait");
        assert!(observed.entry.requested_timeout() > Duration::ZERO);
        let (keys, _) = read.expect("command key after native maintenance");
        assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
        assert_eq!(calls.get(), 9, "all native maintenance turns ran");
        assert_pre_count(&mut eval, 1);
    }
}

#[test]
fn command_input_queued_unread_key_bypasses_pre_redisplay_and_maintenance() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    eval.eval_str("(setq unread-command-events '(120))")
        .expect("one already queued command event");
    eval.display_idle_maintenance_fn = Some(Box::new(|_| {
        panic!("queued command input must bypass idle maintenance")
    }));
    let _owner = InputGateOwner::install(&mut eval);

    let (keys, _) = eval.read_key_sequence().expect("queued command key");
    assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
    assert_pre_count(&mut eval, 0);
}

#[test]
fn command_input_idle_maintenance_preserves_real_resize_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    let calls = install_no_progress_deadlines(&mut eval, 8);
    let owner = InputGateOwner::install(&mut eval);
    let (resize_gate, resize_publication) = owner.gate(vec![InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    }]);
    let (key_gate, key_publication) = owner.gate(vec![InputEvent::key_press(KeyEvent::char('x'))]);
    // Do not release the key until the resize has actually been applied and
    // its redisplay producer reached. A skipped resize cannot pass this gate.
    install_after_redisplay(&mut eval, key_gate, |eval| {
        match eval.frame_manager().selected_frame() {
            Some(frame) if (frame.width, frame.height) == (700, 800) => {
                Readiness::Ready(WaitWitness::InputWaitEntered)
            }
            _ => Readiness::Pending,
        }
    });
    resize_gate.arm(&mut eval, WaitWitness::InputWaitEntered);

    let read = eval.read_key_sequence();
    let resize = resize_publication.finish().expect("observed resize wait");
    let key = key_publication.finish().expect("observed post-resize wait");
    assert!(resize.entry.requested_timeout() > Duration::ZERO);
    assert!(key.entry.requested_timeout() > Duration::ZERO);
    let (keys, _) = read.expect("command key after the real resize");
    assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
    assert!(
        calls.get() >= 9,
        "no-progress maintenance ran before resize"
    );
    assert_pre_count(&mut eval, 2);
}

#[test]
fn command_input_idle_maintenance_preserves_monitor_hook_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    let calls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&calls);
    eval.display_idle_maintenance_fn = Some(Box::new(move |_| {
        let call = observed.get();
        observed.set(call + 1);
        // The positive native wait admits the monitor event through the real
        // wait service. Its deadline also guarantees an outer-reader resume
        // when that service fails to preserve the hook's redisplay debt.
        let delay = if call < 8 {
            Duration::ZERO
        } else {
            Duration::from_millis(50)
        };
        DisplayMaintenanceOutcome {
            next_wake: Some(delay),
            publication: DisplayMaintenancePublication::Idle,
        }
    }));
    eval.eval_str(
        "(setq idle-wake-monitor-seen nil
               display-monitors-changed-functions
               (list (lambda (_terminal)
                       (insert \"monitor\\n\")
                       (setq idle-wake-monitor-seen t))))",
    )
    .expect("install the real monitor event hook");
    let owner = InputGateOwner::install(&mut eval);
    let (monitor_gate, monitor_publication) =
        owner.gate(vec![InputEvent::MonitorsChanged { monitors: vec![] }]);
    let (key_gate, key_publication) = owner.gate(vec![InputEvent::key_press(KeyEvent::char('x'))]);
    install_after_redisplay(&mut eval, key_gate, |eval| {
        if eval
            .builtin_var_value_by_name("idle-wake-monitor-seen")
            .expect("monitor hook state")
            .is_truthy()
        {
            Readiness::Ready(WaitWitness::InputWaitEntered)
        } else {
            Readiness::Pending
        }
    });
    monitor_gate.arm(&mut eval, WaitWitness::InputWaitEntered);

    let read = eval.read_key_sequence();
    let monitor = monitor_publication.finish().expect("observed monitor wait");
    let key = key_publication
        .finish()
        .expect("redisplay after monitor hook");
    assert!(monitor.entry.requested_timeout() > Duration::ZERO);
    assert!(key.entry.requested_timeout() > Duration::ZERO);
    let (keys, _) = read.expect("command key after the monitor hook");
    assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
    assert!(calls.get() >= 9, "maintenance ran before the monitor hook");
    assert_pre_count(&mut eval, 2);
}

#[test]
fn command_input_idle_maintenance_preserves_pending_safe_funcall_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    eval.eval_str(
        "(progn
           (setq idle-wake-safe-callback-seen nil)
           (fset 'idle-wake-safe-callback
                 (lambda ()
                   (insert \"safe\\n\")
                   (setq idle-wake-safe-callback-seen t))))",
    )
    .expect("define the deferred Lisp callback");
    let calls = install_no_progress_deadlines(&mut eval, 8);
    let mut callback_queued = false;
    eval.redisplay_fn = Some(Box::new(move |eval| {
        if !callback_queued {
            callback_queued = true;
            // The real producer owns mutable Context after preliminary Lisp
            // redisplay, before the first maintenance wait. Timer service
            // flushes this deferred work even when no timer fires.
            eval.queue_pending_safe_funcall(Value::symbol("idle-wake-safe-callback"), vec![]);
        }
    }));
    let owner = InputGateOwner::install(&mut eval);
    let (key_gate, key_publication) = owner.gate(vec![InputEvent::key_press(KeyEvent::char('x'))]);
    install_after_redisplay(&mut eval, key_gate, |eval| {
        if eval
            .builtin_var_value_by_name("idle-wake-safe-callback-seen")
            .expect("deferred callback state")
            .is_truthy()
        {
            Readiness::Ready(WaitWitness::InputWaitEntered)
        } else {
            Readiness::Pending
        }
    });

    let read = eval.read_key_sequence();
    let key = key_publication
        .finish()
        .expect("redisplay after the actual deferred callback");
    assert!(key.entry.requested_timeout() > Duration::ZERO);
    let (keys, _) = read.expect("command key after the deferred callback");
    assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
    assert_eq!(calls.get(), 9, "all maintenance turns ran");
    assert_pre_count(&mut eval, 2);
}

#[test]
fn command_input_idle_maintenance_preserves_bound_window_close_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    install_minimal_special_event_command_runtime(&mut eval);
    eval.eval_str(
        "(progn
           (setq idle-wake-close-event nil)
           (fset 'handle-delete-frame
                 (lambda (event)
                   (insert \"close\\n\")
                   (setq idle-wake-close-event event))))",
    )
    .expect("install a bound window-close event handler");
    let calls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&calls);
    eval.display_idle_maintenance_fn = Some(Box::new(move |_| {
        let call = observed.get();
        observed.set(call + 1);
        let delay = if call < 8 {
            Duration::ZERO
        } else {
            Duration::from_millis(50)
        };
        DisplayMaintenanceOutcome {
            next_wake: Some(delay),
            publication: DisplayMaintenancePublication::Idle,
        }
    }));
    let owner = InputGateOwner::install(&mut eval);
    let (close_gate, close_publication) =
        owner.gate(vec![InputEvent::WindowClose { emacs_frame_id: 0 }]);
    let (key_gate, key_publication) = owner.gate(vec![InputEvent::key_press(KeyEvent::char('x'))]);
    // The handler may choose to keep the frame alive. Its Lisp side effects
    // still require redisplay before another positive command-input wait.
    install_after_redisplay(&mut eval, key_gate, |eval| {
        if eval
            .builtin_var_value_by_name("idle-wake-close-event")
            .expect("bound window-close handler state")
            .is_truthy()
        {
            Readiness::Ready(WaitWitness::InputWaitEntered)
        } else {
            Readiness::Pending
        }
    });
    close_gate.arm(&mut eval, WaitWitness::InputWaitEntered);

    let read = eval.read_key_sequence();
    let close = close_publication
        .finish()
        .expect("observed window-close wait");
    let key = key_publication
        .finish()
        .expect("redisplay after window-close handler");
    assert!(close.entry.requested_timeout() > Duration::ZERO);
    assert!(key.entry.requested_timeout() > Duration::ZERO);
    let (keys, _) = read.expect("command key after the bound window-close handler");
    assert_eq!(keys, vec![Value::fixnum('x' as i64)]);
    assert!(
        calls.get() >= 9,
        "maintenance ran before the window-close handler"
    );
    assert_eq!(
        eval.eval_str("(car idle-wake-close-event)")
            .expect("window-close event head"),
        Value::symbol("delete-frame")
    );
    assert_pre_count(&mut eval, 2);
}

#[test]
fn wait_service_pending_safe_funcall_redisplays_without_timer_activity() {
    use crate::emacs_core::wait::{WaitContextExt, WaitRequest};

    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    eval.eval_str(
        "(progn
           (setq idle-wake-safe-service-seen nil)
           (fset 'idle-wake-safe-service
                 (lambda ()
                   (insert \"deferred service\\n\")
                   (setq idle-wake-safe-service-seen t))))",
    )
    .expect("define a real deferred callback with visible work");
    let published = Rc::new(Cell::new(false));
    let observed = Rc::clone(&published);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        if eval
            .builtin_var_value_by_name("idle-wake-safe-service-seen")
            .expect("deferred callback state")
            .is_truthy()
        {
            observed.set(true);
        }
    }));
    eval.queue_pending_safe_funcall(Value::symbol("idle-wake-safe-service"), vec![]);

    // This is the actual unified wait service, with no due timers. Deferred
    // deletion hooks can reach this same queue from an earlier wait callback.
    let outcome = eval
        .service_wait_request_once_outcome(&WaitRequest::service_once(true))
        .expect("service the actual deferred Lisp callback");
    assert!(
        eval.builtin_var_value_by_name("idle-wake-safe-service-seen")
            .is_some_and(|value| value.is_truthy()),
        "the deferred Lisp callback actually ran"
    );
    assert!(published.get(), "visible deferred work reached redisplay");
    assert_pre_count(&mut eval, 1);
    assert!(
        !outcome.has_timer_activity(),
        "a deferred safe callback must not impersonate a fired timer"
    );
    assert!(
        !outcome.has_special_input_activity(),
        "redisplay debt must not become special input activity"
    );
}

#[cfg(unix)]
fn gated_wait_socket(
    eval: &mut Context,
    name: &str,
    filter: &'static str,
) -> (
    std::os::unix::net::UnixStream,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use crate::emacs_core::process::{
        NetworkSocket, ProcessCodingSystems, ProcessCodingSystemsExt, ProcessKindWithoutDevice,
        SystemProcessExt,
    };
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let id = eval.processes_mut().create_process_with_kind(
        name.to_owned(),
        Value::NIL,
        "network".to_owned(),
        vec![],
        ProcessKindWithoutDevice::Network,
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    let (socket, peer) = std::os::unix::net::UnixStream::pair().expect("owned socket pair");
    socket
        .set_nonblocking(true)
        .expect("nonblocking process input");
    let process = eval.processes_mut().get_mut(id).expect("socket process");
    process.filter = Value::symbol(filter);
    process
        .live_io_mut()
        .set_network_socket(Some(NetworkSocket::UnixStream(socket)));
    eval.processes()
        .register_socket_fd(id)
        .expect("register process input");
    let mut release = peer.try_clone().expect("clone fixture writer");
    let entered = Arc::new(AtomicBool::new(false));
    let witness = Arc::clone(&entered);
    eval.processes_mut()
        .before_next_backend_wait_for_test(move |entry| {
            assert!(entry.requested_timeout() > Duration::ZERO);
            witness.store(true, Ordering::Release);
            release
                .write_all(b"native wait payload")
                .expect("release process filter");
        });
    // Keep the peer alive through the complete wait: an EOF/sentinel must not
    // supply a second redraw that hides a missing filter/timer redisplay.
    (peer, entered)
}

#[cfg(unix)]
fn record_wait_callback_redisplay(eval: &mut Context, flag: &'static str) -> Rc<Cell<bool>> {
    let published = Rc::new(Cell::new(false));
    let observed = Rc::clone(&published);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        if eval
            .builtin_var_value_by_name(flag)
            .is_some_and(|value| value.is_truthy())
        {
            observed.set(true);
        }
    }));
    published
}

#[cfg(unix)]
fn bounded_native_maintenance_wait(
    eval: &mut Context,
) -> Result<CommandInputWaitOutcome, neovm_value::error::Flow> {
    use crate::emacs_core::eval::keyboard::IdleTransitionPolicy;
    use crate::emacs_core::wait::WaitContextExt;

    // This is exactly the bounded input wait used for a native maintenance
    // deadline. No next read_char preliminary pass may hide missing redraws.
    eval.wait_for_command_input_with_idle_policy(
        Some(std::time::Instant::now() + Duration::from_secs(1)),
        IdleTransitionPolicy::Manage,
    )
}

#[cfg(unix)]
#[test]
fn native_maintenance_wait_redisplays_timer_fired_inside_the_wait() {
    use std::sync::atomic::Ordering;

    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    eval.eval_str(
        "(progn
           (setq idle-wake-inside-timer-fired nil)
           (fset 'idle-wake-inside-timer-callback
                 (lambda ()
                   (insert \"timer inside native wait\\n\")
                   (setq idle-wake-inside-timer-fired t)))
           (fset 'timer-event-handler
                 (lambda (timer) (funcall (aref timer 5))))
           (fset 'idle-wake-activate-timer-filter
                 (lambda (_process _text)
                   (setq timer-list (list idle-wake-inside-timer)))))",
    )
    .expect("install actual GNU timer callback and activating filter");
    let timer = Value::from(GnuTimerTimestamp::from_duration(Duration::ZERO));
    let _ = timer.set_vector_slot(5, Value::symbol("idle-wake-inside-timer-callback"));
    eval.set_variable("idle-wake-inside-timer", timer);
    let (_peer, entered) = gated_wait_socket(
        &mut eval,
        "idle-wake-timer-activation",
        "idle-wake-activate-timer-filter",
    );
    let published = record_wait_callback_redisplay(&mut eval, "idle-wake-inside-timer-fired");

    let result = bounded_native_maintenance_wait(&mut eval);
    assert_eq!(
        result.expect("bounded timer wait"),
        CommandInputWaitOutcome::DeadlineElapsed
    );
    assert!(
        entered.load(Ordering::Acquire),
        "positive native wait preceded activation"
    );
    assert!(
        eval.builtin_var_value_by_name("idle-wake-inside-timer-fired")
            .unwrap()
            .is_truthy()
    );
    assert!(
        published.get(),
        "timer changes reached the producer inside the bounded wait"
    );
    assert!(
        eval.edit
            .buffers
            .current_buffer()
            .unwrap()
            .buffer_string()
            .contains("timer inside native wait")
    );
}

#[cfg(unix)]
#[test]
fn native_maintenance_wait_redisplays_process_filter_changes() {
    use std::sync::atomic::Ordering;

    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    eval.eval_str(
        "(progn
           (setq idle-wake-inside-filter-fired nil)
           (fset 'idle-wake-inside-filter
                 (lambda (_process text)
                   (insert text)
                   (setq idle-wake-inside-filter-fired t))))",
    )
    .expect("install real process filter");
    let (_peer, entered) =
        gated_wait_socket(&mut eval, "idle-wake-filter", "idle-wake-inside-filter");
    let published = record_wait_callback_redisplay(&mut eval, "idle-wake-inside-filter-fired");

    let result = bounded_native_maintenance_wait(&mut eval);
    assert_eq!(
        result.expect("bounded filter wait"),
        CommandInputWaitOutcome::DeadlineElapsed
    );
    assert!(
        entered.load(Ordering::Acquire),
        "filter input followed positive native entry"
    );
    assert!(
        eval.builtin_var_value_by_name("idle-wake-inside-filter-fired")
            .unwrap()
            .is_truthy()
    );
    assert!(
        published.get(),
        "filter changes reached the producer inside the bounded wait"
    );
    assert!(
        eval.edit
            .buffers
            .current_buffer()
            .unwrap()
            .buffer_string()
            .contains("native wait payload")
    );
}

#[cfg(unix)]
#[test]
fn native_maintenance_wait_redisplays_after_block_exit_sentinel_without_eof() {
    use crate::emacs_core::process::reap::ChildOwnershipIo;
    use crate::emacs_core::process::{
        ProcessCodingSystems, ProcessCodingSystemsExt, SystemProcessExt,
    };
    use neovm_value::heap_types::LispString;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = counted_context();
    eval.eval_str(
        "(progn
           (setq idle-wake-inside-sentinel-fired nil)
           (fset 'idle-wake-inside-sentinel
                 (lambda (process _status)
                   (if (eq (process-status process) 'exit)
                       (progn
                         (insert \"sentinel after native block\\n\")
                         (setq idle-wake-inside-sentinel-fired t))))))",
    )
    .expect("install actual child sentinel");
    let id = eval.processes_mut().create_process_lisp(
        LispString::from_utf8("idle-wake-sentinel"),
        Value::NIL,
        LispString::from_utf8(&find_bin("sh")),
        vec![
            LispString::from_utf8("-c"),
            LispString::from_utf8("IFS= read -r release || exit 99; exit 7"),
        ],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    {
        let process = eval.processes_mut().get_mut(id).expect("owned child");
        // Suspended reads return WouldBlock before any read/EOF processing
        // (process/types.rs:7241). Only child-status notification can redraw.
        process.filter = Value::T;
        process.sentinel = Value::symbol("idle-wake-inside-sentinel");
    }
    eval.processes_mut()
        .spawn_child(id, false)
        .expect("spawn owned gated child");
    let mut release = eval
        .processes()
        .get(id)
        .unwrap()
        .live_io()
        .child()
        .stdin()
        .unwrap()
        .try_clone()
        .expect("owned release descriptor");
    let entered = Arc::new(AtomicBool::new(false));
    let witness = Arc::clone(&entered);
    eval.processes_mut()
        .before_next_backend_wait_for_test(move |entry| {
            assert!(entry.requested_timeout() > Duration::ZERO);
            witness.store(true, Ordering::Release);
            release
                .write_all(b"release\n")
                .expect("release the owned child after initial service");
        });
    let published = record_wait_callback_redisplay(&mut eval, "idle-wake-inside-sentinel-fired");
    let before = crate::emacs_core::process::child_status::status_notify_totals();

    let result = bounded_native_maintenance_wait(&mut eval);
    let after = crate::emacs_core::process::child_status::status_notify_totals();
    let ran = eval
        .builtin_var_value_by_name("idle-wake-inside-sentinel-fired")
        .unwrap()
        .is_truthy();
    let redrawn = published.get();
    let text = eval.edit.buffers.current_buffer().unwrap().buffer_string();
    // Cleanup has authority only over this fixture's own child. Snapshot the
    // assertions first, so cleanup cannot manufacture the callback evidence.
    let _ = eval.apply(
        Value::symbol("delete-process"),
        vec![Value::make_process(id)],
    );

    assert_eq!(
        result.expect("bounded sentinel wait"),
        CommandInputWaitOutcome::DeadlineElapsed
    );
    assert!(
        entered.load(Ordering::Acquire),
        "child stayed alive through initial wait service"
    );
    assert!(
        after.2 > before.2,
        "after-block status_notify visited the exit"
    );
    assert!(ran, "the child exit actually ran its bound sentinel");
    assert!(text.contains("sentinel after native block"));
    assert!(
        redrawn,
        "after-block sentinel changes reached the producer before DeadlineElapsed"
    );
}
