use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::value::Value;
fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green(make_context: fn() -> Context) -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    make_context()
}

/// Native protocol owned by one test invocation, never evaluator/TLS state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadyWriteCommitted {
    ReadyBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum ParkOutputError {
    #[error("Main Select witness receiver closed")]
    WitnessReceiverClosed,
    #[error("real output write acknowledgement missing")]
    WriteAckMissing,
    #[error("real output writer disconnected")]
    WriteAckDisconnected,
    #[error("Main Select owner closed before failure cancellation")]
    ParkOwnerClosed,
}
static_assertions::assert_impl_all!(ParkOutputError: std::error::Error, Send, Sync);

#[must_use]
#[derive(Debug)]
struct ParkOutputGate {
    parked: std::sync::mpsc::Receiver<super::scheduler::SelectParked>,
    committed: std::sync::mpsc::Sender<ReadyWriteCommitted>,
    expectation: super::scheduler::SelectParkExpectation,
}

static_assertions::assert_impl_all!(ParkOutputGate: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ParkOutputGate: Clone, Copy);

fn main_select_park_gate(
    ev: &mut Context,
    expectation: super::scheduler::SelectParkExpectation,
) -> (
    ParkOutputGate,
    std::sync::mpsc::Receiver<Result<(), ParkOutputError>>,
) {
    // Green startup is lazy. This existing thread path seats its owner before
    // subscribing to that runtime's Control, outside the target timing window.
    ev.eval_str("(thread-join (make-thread (lambda () nil)))")
        .expect("seat the owned Green scheduler");
    let (sender, parked) = std::sync::mpsc::channel();
    let (committed, ack) = std::sync::mpsc::channel();
    let (completed, completion) = std::sync::mpsc::channel();
    let control = std::sync::Arc::clone(&ev.green_seat().expect("Green main seat").control);
    // The one-shot lives in Control; retaining its Arc here would create
    // an owner cycle if preparation refuses before the callback is consumed.
    let cancel_control = std::sync::Arc::downgrade(&control);
    let cancel = ev.quit_requested.clone();
    control.after_next_select_park_for_test(
        super::scheduler::ThreadKey::Main,
        expectation,
        move |witness| {
            let outcome = sender
                .send(witness)
                .map_err(|_| ParkOutputError::WitnessReceiverClosed)
                .and_then(
                    |()| match ack.recv_timeout(std::time::Duration::from_secs(5)) {
                        Ok(ReadyWriteCommitted::ReadyBytes) => Ok(()),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            Err(ParkOutputError::WriteAckMissing)
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            Err(ParkOutputError::WriteAckDisconnected)
                        }
                    },
                );
            // This callback runs outside Control.state's mutex, before
            // pick_next, while the Baton remains local. A matching Main
            // Select cannot become Runnable until the write ACK or failure.
            if outcome.is_err() {
                cancel.request();
                match cancel_control.upgrade() {
                    Some(control) => control.wake(super::scheduler::ThreadKey::Main),
                    None => {
                        let _ = completed.send(Err(ParkOutputError::ParkOwnerClosed));
                        return;
                    }
                }
            }
            let _ = completed.send(outcome);
        },
    );
    (
        ParkOutputGate {
            parked,
            committed,
            expectation,
        },
        completion,
    )
}

fn write_ready_after_main_select_park(
    listener: std::net::TcpListener,
    gate: ParkOutputGate,
) -> std::io::Result<std::net::TcpStream> {
    use std::io::Write;
    use std::time::Duration;
    listener.set_nonblocking(true)?;
    let poller = polling::Poller::new()?;
    // SAFETY: listener remains owned here until after the local poller is
    // deleted/dropped; no descriptor can be closed or replaced concurrently.
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
    let witness = gate
        .parked
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "actual Main Select witness missing",
            )
        })?;
    if witness.owner() != super::scheduler::ThreadKey::Main {
        return Err(std::io::Error::other("unexpected Main Select owner"));
    }
    // The callback holds this committed transition until our write ACK;
    // its nominal deadline is diagnostic, not a substitute for ownership.
    let _nominal_wait_end = witness.wait_end();
    use super::scheduler::{SelectParkExpectation, SelectParkFacts};
    match (gate.expectation, witness.into_facts()) {
        (SelectParkExpectation::Ordinary, SelectParkFacts::Ordinary) => {}
        (
            SelectParkExpectation::MainConnectPending(expected),
            SelectParkFacts::MainConnectPending(pending),
        ) => {
            if pending.process() != expected {
                return Err(std::io::Error::other("wrong pending-connect owner"));
            }
        }
        (SelectParkExpectation::Ordinary, SelectParkFacts::MainConnectPending(_))
        | (SelectParkExpectation::MainConnectPending(_), SelectParkFacts::Ordinary) => {
            return Err(std::io::Error::other("wrong Main Select phase"));
        }
    }
    stream.write_all(b"ready")?;
    gate.committed
        .send(ReadyWriteCommitted::ReadyBytes)
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "park owner did not retain its write ACK",
            )
        })?;
    Ok(stream)
}

fn select_parks() -> usize {
    super::switch::SELECT_PARKS.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn run_a_sim_worlds_waits_never_slice_fixture(
    make_context: fn() -> crate::emacs_core::eval::Context,
) {
    crate::test_utils::init_test_tracing();
    let mut ev = make_context();
    let before = select_parks();
    assert_eq!(
        eval(
            &mut ev,
            "(progn (sleep-for 0.05) (accept-process-output nil 0.05) 'done)"
        ),
        "done"
    );
    assert_eq!(select_parks(), before);
}

pub(crate) fn run_fd_readiness_wakes_a_green_select_while_a_peer_yields_fixture(
    make_context: fn() -> crate::emacs_core::eval::Context,
) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut ev = green(make_context);
    // Establish the connection before arming this ready-peer witness. GNU
    // select now also parks a lone owner during synchronous connect.
    let setup = format!(
        r#"(progn
      (setq g3f-received nil g3f-peer-stop nil)
      (setq g3f-process (make-network-process :name "g3f-ready" :host "127.0.0.1"
                    :service {port} :noquery t
                    :filter (lambda (_p text) (setq g3f-received text)))) t)"#
    );
    assert_eq!(eval(&mut ev, &setup), "t");
    let (gate, completion) =
        main_select_park_gate(&mut ev, super::scheduler::SelectParkExpectation::Ordinary);
    let control = std::sync::Arc::clone(&ev.green_seat().expect("main seat").control);
    let cancel = ev.quit_requested.clone();
    let server = std::thread::spawn(move || {
        let result = write_ready_after_main_select_park(listener, gate);
        if result.is_err() {
            cancel.request();
            control.wake(super::scheduler::ThreadKey::Main);
        }
        result.map(|_stream| ())
    });
    let started = std::time::Instant::now();
    let source = format!(
        r#"(progn
      (let* ((p g3f-process)
             (th (make-thread (lambda ()
                   (while (and (null g3f-received) (null g3f-peer-stop))
                     (thread-yield)) 'peer))))
        (unwind-protect
            (progn (accept-process-output p 2) g3f-received)
          (setq g3f-peer-stop t)
          (thread-join th)
          (delete-process p))))"#
    );
    let result = eval(&mut ev, &source);
    let server_result = server.join().expect("Green peer owner joined");
    let parked_result = completion.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(result, "\"ready\"");
    server_result.expect("actual parked-owner output release");
    assert_eq!(
        parked_result.expect("actual park/write-ACK completion"),
        Ok(())
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}
