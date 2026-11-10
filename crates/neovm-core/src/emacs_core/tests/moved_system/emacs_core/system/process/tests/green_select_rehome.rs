//! Green select tests that need the system wait loop and process backend.
//!
//! They used to live in neovm-eval, which cannot name this crate. The
//! fine-timeout assertion calls `WaitContextExt::wait_until`, and the connect
//! test calls the system process setters. The assertions are the ones the
//! eval tests had.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::print::print_value;
use neovm_system::emacs_core::process::SystemProcessExt;
use neovm_system::emacs_core::wait::WaitContextExt;
use neovm_value::error::{EvalResult, LispCondition, signal};
use neovm_value::value::Value;
use std::io::Write;
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn eval_print(context: &mut Context, source: &str) -> String {
    match context.eval_str(source) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green_context() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    neovm_eval::emacs_core::jit::force_jit_off_for_test(true);
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    context
}

fn write_ready_after_park(
    listener: TcpListener,
    parked: mpsc::Receiver<()>,
    ack: mpsc::Sender<()>,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    let poller = polling::Poller::new()?;
    // SAFETY: listener remains owned here until after the local poller is
    // deleted; no descriptor can be closed or replaced concurrently.
    unsafe {
        poller.add(&listener, polling::Event::readable(1))?;
    }
    let mut events = polling::Events::new();
    poller.wait(&mut events, Some(Duration::from_secs(5)))?;
    if !events.iter().any(|event| event.key == 1 && event.readable) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Green peer never connected",
        ));
    }
    let (mut stream, _) = listener.accept()?;
    poller.delete(&listener)?;
    parked.recv_timeout(Duration::from_secs(5)).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "actual Main Select witness missing",
        )
    })?;
    stream.write_all(b"ready")?;
    ack.send(()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "park owner did not retain its write ACK",
        )
    })?;
    Ok(())
}

/// Writable :nowait completion must be decoded after an idle scheduler
/// wake. A readable-only broad poll skips a process still in `connect`.
#[test]
fn writable_connect_readiness_survives_a_green_select_with_a_yielding_peer() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut ev = green_context();
    let setup = format!(
        r#"(progn
      (setq g3f-connect-stop nil g3f-connect-open nil g3f-connect-output nil
            g3f-connect-open-count 0)
      (setq g3f-connect-process
        (make-network-process :name "g3f-connect" :host "127.0.0.1"
          :service {port} :family 'ipv4 :nowait t :noquery t
          :sentinel (lambda (process event)
                      (if (string-equal event "open\n")
                        (setq g3f-connect-open-count (1+ g3f-connect-open-count)
                              g3f-connect-open (process-status process))))
          :filter (lambda (_process text) (setq g3f-connect-output text))))
      (setq g3f-connect-initial (process-status g3f-connect-process)))"#
    );
    ev.eval_str(&setup)
        .expect("create the original real pending TCP connect");
    let process = ev
        .eval_symbol("g3f-connect-process")
        .expect("pending process")
        .as_process_id()
        .expect("actual ProcessId");
    let (parked, ack, completion, wake) =
        neovm_eval::test_support::authority::arm_main_connect_select_park_for_test(
            &mut ev, process,
        );
    let (writable_tx, writable_rx) = mpsc::channel();
    ev.processes_mut()
        .after_next_native_connect_writable_for_test(process, move |event| {
            let _ = writable_tx.send(event);
        });
    let (release, until_finished) = mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let result = (|| -> std::io::Result<()> {
            write_ready_after_park(listener, parked, ack)?;
            match until_finished.recv_timeout(Duration::from_secs(5)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "evaluator did not release its Green peer",
                )),
            }
        })();
        if result.is_err() {
            wake();
        }
        result
    });
    let source = r#"(let* ((p g3f-connect-process)
             (initial g3f-connect-initial)
             (th (make-thread (lambda ()
                   (while (null g3f-connect-stop) (thread-yield))))))
        (unwind-protect
            (progn
              (let ((tries 0))
                (while (and (null g3f-connect-output) (< tries 20))
                  (setq tries (1+ tries))
                  (accept-process-output p 0.05)))
              (list initial g3f-connect-open g3f-connect-output))
          (setq g3f-connect-stop t)
          (thread-join th)
          (delete-process p)))"#;
    let result = eval_print(&mut ev, source);
    drop(release);
    let server_result = server.join().expect("Green connect peer owner joined");
    let parked_result = completion.recv_timeout(Duration::from_secs(5));
    let native_writable = writable_rx.recv_timeout(Duration::from_secs(5));
    assert_eq!(result, "(connect open \"ready\")");
    server_result.expect("actual parked-connect output release");
    assert_eq!(
        parked_result.expect("actual pending-connect park/write ACK"),
        Ok(())
    );
    let native_writable =
        native_writable.expect("real backend writable event for the pending connect");
    assert_eq!(native_writable.process(), process);
    assert_eq!(native_writable.owner(), std::thread::current().id());
    assert_eq!(eval_print(&mut ev, "g3f-connect-open-count"), "1");
}

#[cfg(feature = "jit")]
fn require_native() -> Result<(), neovm_value::error::Flow> {
    if neovm_eval::test_support::authority::native_depth_for_test() == 0 {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "F5 witness was reached without a compiled native frame",
            )],
        ));
    }
    Ok(())
}

#[cfg(feature = "jit")]
fn native_witness(context: &mut Context) -> EvalResult {
    require_native()?;
    let count = context
        .visible_variable_value_or_nil("f5-jit-witness-count")
        .as_int()
        .unwrap_or_default();
    context.set_variable("f5-jit-witness-count", Value::fixnum(count + 1));
    Ok(Value::T)
}

#[cfg(feature = "jit")]
fn native_select_fine_timeout(context: &mut Context) -> EvalResult {
    native_witness(context)?;
    // Exclude compilation and the first witness from the floor measurement.
    // Switched is this continuation's proof that a real Select park occurred,
    // even if a notification legitimately made it Runnable before main ran.
    let duration = Duration::from_millis(50);
    let started = Instant::now();
    let deadline = started + duration;
    if !matches!(
        context.green_select_switch(duration)?,
        neovm_eval::emacs_core::eval::SelectTurn::Switched
    ) {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "compiled worker did not traverse the Select park",
            )],
        ));
    }
    // Readiness may return Select early. Retry the original fine deadline;
    // never restart its duration or mistake activity for timeout completion.
    context.wait_until(deadline)?;
    let elapsed = started.elapsed();
    native_witness(context)?;
    if elapsed < Duration::from_millis(50) {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "compiled Green select returned before its fine timeout",
            )],
        ));
    }
    Ok(Value::fixnum(
        elapsed.as_nanos().min(i64::MAX as u128) as i64
    ))
}

#[cfg(feature = "jit")]
fn worker_native_select_timeout(context: &mut Context) -> EvalResult {
    struct RestoreProfitGate;
    impl Drop for RestoreProfitGate {
        fn drop(&mut self) {
            neovm_eval::test_support::authority::force_profit_gate_for_test(true);
        }
    }
    neovm_eval::test_support::authority::force_profit_gate_for_test(false);
    let _restore = RestoreProfitGate;
    context.apply0(Value::symbol("f5-native-select-leaf"))
}

#[cfg(feature = "jit")]
fn install_native_select_leaf(context: &mut Context) {
    use neovm_eval::emacs_core::subr::SubrSpec;
    use neovm_eval::test_support::authority::{
        jit_mark_hot_for_test, seal_hand_assembled_ops_for_test,
    };
    use neovm_value::value::LambdaParams;

    context.register_subr(SubrSpec::fixed0(
        "f5-native-select-wait",
        native_select_fine_timeout,
    ));
    context.register_subr(SubrSpec::fixed0(
        "f5-native-select-worker",
        worker_native_select_timeout,
    ));
    context.register_subr(SubrSpec::fixed0("f5-jit-witness", native_witness));
    let mut function =
        crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    function.lexical = true;
    function.ops = vec![
        crate::emacs_core::bytecode::Op::Constant(0),
        crate::emacs_core::bytecode::Op::Call(0),
        crate::emacs_core::bytecode::Op::Return,
    ];
    function.constants = vec![Value::symbol("f5-native-select-wait")].into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    seal_hand_assembled_ops_for_test(&mut function);
    jit_mark_hot_for_test(&function);
    let symbol = neovm_value::intern::intern("f5-native-select-leaf");
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function_id(symbol, Value::make_bytecode(function))
    });
}

/// Compiled Green select keeps its fine deadline while the peer is idle.
#[test]
#[cfg(feature = "jit")]
fn green_jit_switch_native_select_keeps_its_fine_timeout_while_idle() {
    struct RestoreProfitGate;
    impl Drop for RestoreProfitGate {
        fn drop(&mut self) {
            neovm_eval::test_support::authority::force_profit_gate_for_test(true);
        }
    }
    crate::test_utils::init_test_tracing();
    neovm_eval::emacs_core::jit::force_jit_off_for_test(false);
    assert!(neovm_eval::emacs_core::jit::jit_runtime_enabled());
    neovm_eval::test_support::authority::force_profit_gate_for_test(false);
    let _restore = RestoreProfitGate;
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    context.set_lexical_binding(true);
    // This test targets the normal owned poller, not its degraded fallback.
    assert!(context.processes().green_select_handle().is_some());
    context
        .eval_str("(setq f5-jit-witness-count 0)")
        .expect("witness counter");
    install_native_select_leaf(&mut context);
    context
        .eval_str("(setq noninteractive t timer-list nil timer-idle-list nil)")
        .unwrap();
    context
        .eval_str("(setq f5-native-select-thread (make-thread #'f5-native-select-worker))")
        .unwrap();
    // Main remains Runnable during this handoff, so the worker must
    // traverse Select. In the quiet case main's subsequent join leaves
    // only its deadline ready, and pick_next uses the owned idle poller.
    // Notifications may legitimately make the worker Runnable sooner;
    // its own Switched result is the authoritative traversal proof.
    let first_turn = context.eval_str("(thread-yield)");
    let health_before_join = context
        .eval_str(
            "(list (thread-live-p f5-native-select-thread)
                   (thread-last-error f5-native-select-thread) f5-jit-witness-count)",
        )
        .map(|value| print_value(&value));
    let joined = context.eval_str("(thread-join f5-native-select-thread)");
    let health_after_join = context
        .eval_str(
            "(list (thread-live-p f5-native-select-thread)
                   (thread-last-error f5-native-select-thread) f5-jit-witness-count)",
        )
        .map(|value| print_value(&value));
    // Assert only after joining: a self-signalled worker must fail via its
    // last-error, not look like a successful nil-valued sleep. No upper
    // timing bound is imposed; scheduling delays may make the wait late.
    assert!(first_turn.unwrap().is_nil());
    assert_eq!(health_before_join.unwrap(), "(t nil 1)");
    assert!(
        joined
            .unwrap()
            .as_int()
            .is_some_and(|nanoseconds| nanoseconds >= 50_000_000)
    );
    assert_eq!(health_after_join.unwrap(), "(nil nil 2)");
    assert_eq!(
        neovm_eval::test_support::authority::native_depth_for_test(),
        0
    );
}
