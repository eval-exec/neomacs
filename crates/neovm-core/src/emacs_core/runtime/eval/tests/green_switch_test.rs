//! Phase 0 step 2, round 3d: Lisp threads switching on the Baton.

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(crate::emacs_core::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            crate::emacs_core::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    Context::new_with_threading(ThreadingMode::Green)
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

thread_local! {
    static STOREFAST_CERTIFICATE_THREAD: std::cell::Cell<bool> = const {
        std::cell::Cell::new(false)
    };
}

fn storefast_check_worker_thread(_context: &mut Context) -> crate::emacs_core::error::EvalResult {
    assert!(
        !STOREFAST_CERTIFICATE_THREAD.with(std::cell::Cell::get),
        "the mutating Green owner must run on a different OS thread"
    );
    Ok(Value::T)
}

#[test]
fn storefast_world_certificate_follows_green_stores_on_another_os_thread() {
    use crate::emacs_core::subr::SubrSpec;
    use crate::tagged::collection_reads::capture;

    STOREFAST_CERTIFICATE_THREAD.with(|main| main.set(true));
    let mut ev = green();
    ev.register_subr(SubrSpec::fixed0(
        "storefast-check-worker-thread",
        storefast_check_worker_thread,
    ));
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq storefast-source (cons nil nil)
                     storefast-stable (cons nil nil))
               (thread-join (make-thread (lambda () nil))))"
        ),
        "nil"
    );
    let source = ev.eval_str("storefast-source").unwrap();
    let stable = ev.eval_str("storefast-stable").unwrap();
    let (_, source_certificate) = capture(|| source.cons_car());
    let source_certificate = source_certificate.unwrap();
    let (_, stable_certificate) = capture(|| stable.cons_car());
    let stable_certificate = stable_certificate.unwrap();
    assert_eq!(
        eval(
            &mut ev,
            "(thread-join
               (make-thread
                 (lambda ()
                   (storefast-check-worker-thread)
                   (setcar storefast-source t))))"
        ),
        "t"
    );
    assert!(!source_certificate.unchanged());
    assert!(stable_certificate.unchanged());
    assert!(source.cons_car().is_t());

    // A capture which is still open must also see the worker's mutation;
    // moving only completed-certificate history would miss this case.
    let (_, incoherent) = capture(|| {
        source.cons_cdr();
        assert_eq!(
            eval(
                &mut ev,
                "(thread-join
                   (make-thread
                     (lambda ()
                       (storefast-check-worker-thread)
                       (setcdr storefast-source t))))"
            ),
            "t"
        );
    });
    assert!(incoherent.is_none());
    ev.green_teardown();
    STOREFAST_CERTIFICATE_THREAD.with(|main| main.set(false));
}

#[test]
fn a_local_rebound_during_backtrace_exchange_reattaches_its_restore() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            r#"(progn
      (thread-join (make-thread (lambda () nil)))
      (defvar g3e-local 'default)
      (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
              (backtrace-eval
                '(let ((g3e-local 'exchange-bound))
                   (list g3e-local
                     (progn (kill-local-variable 'g3e-local) g3e-local)
                     (progn (make-local-variable 'g3e-local)
                            (setq g3e-local 'remade) g3e-local)))
                0 'g3e-local-probe)
              g3e-local))))
      (set-buffer (get-buffer-create "g3e-exchanged-local"))
      (set (make-local-variable 'g3e-local) 'local)
      (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local)))"#
        ),
        "((bound (exchange-bound default remade) bound) local t)"
    );
}

#[test]
fn a_thread_runs_and_join_returns_its_value() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((th (make-thread (lambda () (+ 1 2))))) (thread-join th))"
        ),
        "3"
    );
}

#[test]
fn yields_interleave_threads_in_turn() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgs-log nil)
               (let ((a (make-thread (lambda ()
                                       (setq tgs-log (cons 'a1 tgs-log))
                                       (thread-yield)
                                       (setq tgs-log (cons 'a2 tgs-log)))))
                     (b (make-thread (lambda ()
                                       (setq tgs-log (cons 'b1 tgs-log))
                                       (thread-yield)
                                       (setq tgs-log (cons 'b2 tgs-log))))))
                 (thread-join a)
                 (thread-join b)
                 (nreverse tgs-log)))"
        ),
        "(a1 b1 a2 b2)"
    );
}

#[test]
fn a_new_thread_sees_global_values_not_its_creators_bindings() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (defvar tgs-v 0)
               (let ((tgs-v 1))
                 (list tgs-v
                       (thread-join (make-thread (lambda () tgs-v)))
                       (thread-join (make-thread (lambda () (let ((tgs-v 2)) (thread-yield) tgs-v))))
                       tgs-v)))"
        ),
        "(1 0 2 1)"
    );
    assert_eq!(eval(&mut ev, "tgs-v"), "0");
}

#[test]
fn a_parked_threads_values_survive_another_threads_collection() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((kept (make-list 1000 'kept)))
               (thread-join
                (make-thread (lambda ()
                               (let ((i 0))
                                 (while (< i 2000)
                                   (make-list 50 i)
                                   (setq i (1+ i))))
                               (garbage-collect))))
               (list (length kept) (car kept) (nth 999 kept)))"
        ),
        "(1000 kept kept)"
    );
}

#[test]
fn joining_the_current_thread_signals() {
    let mut ev = green();
    eval(&mut ev, "(thread-join (make-thread (lambda () nil)))");
    assert_eq!(
        eval(
            &mut ev,
            "(condition-case e (thread-join (current-thread)) (error (car (cdr e))))"
        ),
        "\"Cannot join current thread\""
    );
}

#[test]
fn a_mutex_serializes_threads_across_a_yield() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgm-m (make-mutex \"tgm\") tgm-log nil)
               (let ((a (make-thread (lambda ()
                                       (mutex-lock tgm-m)
                                       (setq tgm-log (cons 'a1 tgm-log))
                                       (thread-yield)
                                       (setq tgm-log (cons (eq (thread--blocker tgm-b) tgm-m) tgm-log))
                                       (mutex-unlock tgm-m)))))
                 (setq tgm-b (make-thread (lambda ()
                                            (mutex-lock tgm-m)
                                            (setq tgm-log (cons 'b1 tgm-log))
                                            (mutex-unlock tgm-m))))
                 (thread-join a)
                 (thread-join tgm-b)
                 (list (nreverse tgm-log) (thread--blocker tgm-b))))"
        ),
        "((a1 t b1) nil)"
    );
}

#[test]
fn a_mutex_is_recursive_and_only_its_owner_unlocks_it() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgr-m (make-mutex))
               (mutex-lock tgr-m)
               (mutex-lock tgr-m)
               (list (thread-join
                      (make-thread
                       (lambda ()
                         (condition-case e (mutex-unlock tgr-m) (error (car (cdr e)))))))
                     (mutex-unlock tgr-m)
                     (mutex-unlock tgr-m)
                     (condition-case e (mutex-unlock tgr-m) (error 'not-held))))"
        ),
        "(\"Cannot unlock mutex owned by another thread\" nil nil not-held)"
    );
}

#[test]
fn condition_wait_releases_the_mutex_until_notified() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgc-m (make-mutex) tgc-c (make-condition-variable tgc-m)
                     tgc-q nil tgc-got nil)
               (let ((consumer (make-thread
                                (lambda ()
                                  (mutex-lock tgc-m)
                                  (while (null tgc-q) (condition-wait tgc-c))
                                  (setq tgc-got (list (car tgc-q)
                                                      (thread--blocker (current-thread))))
                                  (mutex-unlock tgc-m)))))
                 (thread-yield)
                 (list (eq (thread--blocker consumer) tgc-c)
                       (progn (mutex-lock tgc-m)
                              (setq tgc-q (list 42))
                              (condition-notify tgc-c)
                              (mutex-unlock tgc-m)
                              (thread-join consumer)
                              tgc-got))))"
        ),
        "(t (42 nil))"
    );
}

#[test]
fn condition_notify_wakes_one_waiter_or_all() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgn-m (make-mutex) tgn-c (make-condition-variable tgn-m) tgn-woke 0)
               (let ((ths (mapcar (lambda (_)
                                    (make-thread
                                     (lambda ()
                                       (mutex-lock tgn-m)
                                       (condition-wait tgn-c)
                                       (setq tgn-woke (1+ tgn-woke))
                                       (mutex-unlock tgn-m))))
                                  '(1 2 3))))
                 (thread-yield)
                 (mutex-lock tgn-m) (condition-notify tgn-c) (mutex-unlock tgn-m)
                 (thread-yield) (thread-yield)
                 (let ((one tgn-woke))
                   (mutex-lock tgn-m) (condition-notify tgn-c t) (mutex-unlock tgn-m)
                   (mapc #'thread-join ths)
                   (list one tgn-woke))))"
        ),
        "(1 3)"
    );
}

#[test]
fn condition_wait_takes_the_mutex_back_with_its_count() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgk-m (make-mutex) tgk-c (make-condition-variable tgk-m))
               (let ((th (make-thread
                          (lambda ()
                            (mutex-lock tgk-m)
                            (mutex-lock tgk-m)
                            (condition-wait tgk-c)
                            (list (mutex-unlock tgk-m)
                                  (mutex-unlock tgk-m)
                                  (condition-case nil (mutex-unlock tgk-m)
                                    (error 'released)))))))
                 (thread-yield)
                 (mutex-lock tgk-m) (condition-notify tgk-c) (mutex-unlock tgk-m)
                 (thread-join th)))"
        ),
        "(nil nil released)"
    );
}

#[test]
fn thread_signal_ends_a_wait_for_a_mutex_a_condition_or_a_join() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgw-m (make-mutex) tgw-n (make-mutex)
                     tgw-c (make-condition-variable tgw-n))
               (mutex-lock tgw-m)
               (let* ((locker (setq tgw-locker (make-thread
                               (lambda ()
                                 (condition-case e (progn (mutex-lock tgw-m) 'locked)
                                   (error (list 'lock e)))))))
                      (waiter (make-thread
                               (lambda ()
                                 (mutex-lock tgw-n)
                                 (condition-case e (progn (condition-wait tgw-c) 'notified)
                                   (error (list 'wait e (mutex-unlock tgw-n)))))))
                      (joiner (make-thread
                               (lambda ()
                                 (condition-case e (progn (thread-join tgw-locker) 'joined)
                                   (error (list 'join e)))))))
                 (thread-yield)
                 (thread-signal locker 'error '(\"l\"))
                 (thread-signal waiter 'error '(\"w\"))
                 (thread-signal joiner 'error '(\"j\"))
                 (thread-yield)
                 (mutex-unlock tgw-m)
                 (mapcar #'thread-join (list locker waiter joiner))))"
        ),
        "((lock (error \"l\")) (wait (error \"w\") nil) (join (error \"j\")))"
    );
}

#[test]
fn a_signal_into_a_thread_that_has_not_run_waits_for_its_first_switch() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgd-log nil)
               (let ((th (make-thread
                          (lambda ()
                            (setq tgd-log (cons 'start tgd-log))
                            (condition-case nil (setq tgd-log (cons 'body tgd-log))
                              (error (setq tgd-log (cons 'early tgd-log))))
                            (condition-case e (thread-yield)
                              (error (setq tgd-log (cons (list 'at-yield e) tgd-log))))))))
                 (thread-signal th 'error '(\"x\"))
                 (thread-yield)
                 (thread-yield)
                 (thread-join th)
                 (nreverse tgd-log)))"
        ),
        "(start body (at-yield (error \"x\")))"
    );
}

#[test]
fn thread_join_re_raises_a_signal_pending_when_it_began() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgj-m (make-mutex))
               (mutex-lock tgj-m)
               (let ((th (make-thread (lambda ()
                                        (condition-case nil (mutex-lock tgj-m)
                                          (error 'caught))))))
                 (thread-yield)
                 (thread-signal th 'error '(\"y\"))
                 (prog1 (condition-case e (thread-join th) (error (list 'join e)))
                   (mutex-unlock tgj-m))))"
        ),
        "(join (error \"y\"))"
    );
}

#[test]
fn a_signal_to_the_main_thread_raises_nothing_in_batch() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-join (make-thread (lambda ()
                                              (thread-signal main-thread 'error '(\"m\"))
                                              'sent)))
                   (thread-yield)
                   'main-ok)"
        ),
        "(sent nil main-ok)"
    );
}

#[test]
fn a_thread_is_dead_once_its_function_returns_joined_or_not() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((th (make-thread (lambda () 1))))
               (list (thread-live-p th)
                     (length (all-threads))
                     (progn (thread-yield) (thread-live-p th))
                     (length (all-threads))
                     (thread-join th)))"
        ),
        "(t 2 nil 1 1)"
    );
}

#[test]
fn waits_let_other_threads_run() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgz-a 0 tgz-b 0)
               (make-thread (lambda () (setq tgz-a 1)))
               (sleep-for 0.02)
               (make-thread (lambda () (setq tgz-b 1)))
               (accept-process-output nil 0.02)
               (list tgz-a tgz-b))"
        ),
        "(1 1)"
    );
}

#[test]
fn sleeping_threads_sleep_at_the_same_time() {
    let mut ev = green();
    let started = std::time::Instant::now();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((ths (mapcar (lambda (_) (make-thread (lambda () (sleep-for 0.2) 'woke)))
                                '(1 2 3 4))))
               (sleep-for 0.2)
               (mapcar #'thread-join ths))"
        ),
        "(woke woke woke woke)"
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(200),
        "a 0.2 s sleep returned early after {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(600),
        "five 0.2 s sleeps took {elapsed:?}"
    );
}

#[test]
fn killing_a_threads_buffer_moves_it_and_signals_it() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgb-log nil)
               (let* ((b (setq tgb-b (get-buffer-create \"tgb-x\")))
                      (th (save-current-buffer
                            (set-buffer b)
                            (make-thread
                             (lambda ()
                               (condition-case e (progn (thread-yield) (setq tgb-log (cons 'resumed tgb-log)))
                                 (error (setq tgb-log (cons (list e (eq (current-buffer) tgb-b)
                                                                (buffer-live-p (current-buffer)))
                                                          tgb-log)))))))))
                 (thread-yield)
                 (setq tgb-log (cons (list 'kill (kill-buffer b) (buffer-live-p b)) tgb-log))
                 (setq tgb-log (cons (condition-case e (thread-join th) (error (list 'join e))) tgb-log))
                 (nreverse tgb-log)))"
        ),
        "((kill t nil) ((thread-buffer-killed) nil t) (join (thread-buffer-killed)))"
    );
}

#[test]
fn a_thread_buffer_disposition_of_silently_or_t() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgq-log nil)
               (let* ((s (setq tgq-s (get-buffer-create \"tgq-s\")))
                      (k (get-buffer-create \"tgq-k\"))
                      (quiet (save-current-buffer
                               (set-buffer s)
                               (make-thread (lambda ()
                                              (thread-yield)
                                              (setq tgq-log (cons (list 'quiet (eq (current-buffer) tgq-s) (buffer-live-p (current-buffer)))
                                                                tgq-log)))
                                            nil 'silently)))
                      (keeper (save-current-buffer
                                (set-buffer k)
                                (make-thread (lambda ()
                                               (thread-yield)
                                               (setq tgq-log (cons (list 'keeper (buffer-name)) tgq-log)))
                                             nil t))))
                 (thread-yield)
                 (setq tgq-log (cons (list (kill-buffer s) (kill-buffer k) (buffer-live-p k)) tgq-log))
                 (thread-join quiet)
                 (thread-join keeper)
                 (nreverse tgq-log)))"
        ),
        "((t nil t) (quiet nil t) (keeper \"tgq-k\"))"
    );
}

#[test]
fn a_thread_starting_leaves_the_previous_threads_buffer_alone() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgr-a (get-buffer-create \"tgr-a\") tgr-b (get-buffer-create \"tgr-b\"))
               (let* ((first (save-current-buffer
                               (set-buffer tgr-a)
                               (make-thread (lambda () (thread-yield) (buffer-name)) nil t)))
                      (second (save-current-buffer
                                (set-buffer tgr-b)
                                (make-thread (lambda () (thread-yield) (buffer-name))))))
                 (thread-yield)
                 (list (kill-buffer tgr-a) (buffer-live-p tgr-a)
                       (thread-join first) (thread-join second))))"
        ),
        "(nil t \"tgr-a\" \"tgr-b\")"
    );
}

#[test]
fn the_last_thread_to_block_is_told_that_all_threads_are_blocked() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgl-m (make-mutex) tgl-c (make-condition-variable tgl-m))
               (thread-join (make-thread (lambda ()
                                           (mutex-lock tgl-m)
                                           (condition-case e (condition-wait tgl-c)
                                             (error (car (cdr e))))))))"
        ),
        "\"All threads are blocked: this one would wait forever (GNU Emacs hangs here; neomacs signals)\""
    );
}

#[test]
fn kill_emacs_in_a_thread_exits_from_main_without_cleanups() {
    let mut ev = green();
    eval(&mut ev, "(setq tgx-cleanup nil tgx-m (make-mutex))");
    let outcome = eval(
        &mut ev,
        "(progn
           (mutex-lock tgx-m)
           (make-thread (lambda ()
                          (unwind-protect (mutex-lock tgx-m)
                            (setq tgx-cleanup 'blocked-thread))))
           (unwind-protect
               (thread-join (make-thread (lambda ()
                                           (unwind-protect (kill-emacs 7)
                                             (setq tgx-cleanup 'killer)))))
             (setq tgx-cleanup 'main)))",
    );
    assert!(outcome.starts_with("ERR"), "{outcome}");
    assert_eq!(
        ev.shutdown_request().map(|request| request.exit_code),
        Some(7)
    );
    assert!(ev.visible_variable_value_or_nil("tgx-cleanup").is_nil());
    ev.green_teardown();
    assert!(ev.visible_variable_value_or_nil("tgx-cleanup").is_nil());
}

#[test]
fn main_ending_ends_every_thread_without_running_their_cleanups() {
    let mut ev = green();
    let started = std::time::Instant::now();
    eval(
        &mut ev,
        "(progn
           (setq tgt-m (make-mutex) tgt-ran nil)
           (mutex-lock tgt-m)
           (setq tgt-a (make-thread (lambda ()
                                      (unwind-protect (mutex-lock tgt-m)
                                        (setq tgt-ran 'a)))))
           (setq tgt-b (make-thread (lambda ()
                                      (unwind-protect (sleep-for 30)
                                        (setq tgt-ran 'b)))))
           (thread-yield)
           (setq tgt-c (make-thread (lambda () (setq tgt-ran 'c)))))",
    );
    ev.green_teardown();
    assert!(ev.visible_variable_value_or_nil("tgt-ran").is_nil());
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-live-p tgt-a) (thread-live-p tgt-b) (thread-live-p tgt-c))"
        ),
        "(nil nil nil)"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    drop(ev);
}

/// `(lambda () (list (make-list 100 'x) (tgv-next) (make-list 100 'y)))` as
/// byte-code: its first list lives only on the VM stack across the call.
const TGV_BYTECODE: &str = "(fset 'tgv-f
   (make-byte-code nil
                   (unibyte-string 192 193 194 195 34 196 32 193 194 197 34 35 135)
                   [list make-list 100 x tgv-next y]
                   6))";

#[test]
fn a_parked_threads_vm_frames_survive_other_threads_collections() {
    let mut ev = green();
    eval(&mut ev, TGV_BYTECODE);
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgv-depth 10)
               (fset 'tgv-next (lambda ()
                                 (if (> tgv-depth 0)
                                     (progn (setq tgv-depth (1- tgv-depth)) (tgv-f))
                                   (thread-yield)
                                   'bottom)))
               (fset 'tgv-count (lambda (r)
                                  (let ((n 0))
                                    (while (consp r)
                                      (let ((xs (car r)))
                                        (while xs
                                          (if (eq (car xs) 'x) (setq n (1+ n)))
                                          (setq xs (cdr xs))))
                                      (setq r (car (cdr r))))
                                    n)))
               (let ((th (make-thread (lambda () (tgv-count (tgv-f))))))
                 (thread-yield)
                 (let ((i 0))
                   (while (< i 3)
                     (let ((j 0)) (while (< j 3000) (make-list 20 j) (setq j (1+ j))))
                     (garbage-collect)
                     (setq i (1+ i))))
                 (thread-join th)))"
        ),
        "1100"
    );
}

#[test]
fn eight_threads_yield_and_collect_without_losing_values() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (fset 'tg8-run
                     (lambda (n)
                       (let ((mine nil) (k 0))
                         (while (< k 30)
                           (setq mine (cons (make-list 20 (+ (* n 1000) k)) mine))
                           (thread-yield)
                           (if (= (% k 10) 0) (garbage-collect))
                           (setq k (1+ k)))
                         (let ((sum 0))
                           (while mine
                             (let ((xs (car mine)))
                               (while xs (setq sum (+ sum (car xs)) xs (cdr xs))))
                             (setq mine (cdr mine)))
                           sum))))
               (let ((ths nil) (i 0))
                 (while (< i 8)
                   (setq ths (cons (make-thread (list 'lambda nil (list 'tg8-run i))) ths))
                   (setq i (1+ i)))
                 (mapcar #'thread-join (nreverse ths))))"
        ),
        "(8700 608700 1208700 1808700 2408700 3008700 3608700 4208700)"
    );
}

#[test]
fn every_thread_shares_the_standard_case_table() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgct-old (standard-case-table)
                     tgct-new (make-char-table 'case-table))
               (let ((early (make-thread (lambda ()
                                           (thread-yield)
                                           (eq (standard-case-table) tgct-new)))))
                 (thread-yield)
                 (set-standard-case-table tgct-new)
                 (prog1 (list (thread-join early)
                              (thread-join (make-thread
                                            (lambda () (eq (standard-case-table) tgct-new))))
                              (eq (standard-case-table) tgct-new))
                   (set-standard-case-table tgct-old))))"
        ),
        "(t t t)"
    );
}

#[test]
fn parked_root_buffer_reuses_its_allocation_across_peer_collections() {
    let mut ev = green();
    let kept = Value::string("only the parked main owner's VM frame keeps this alive");
    ev.push_vm_root_frame();
    ev.push_vm_frame_root(kept);
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgr-stop nil)
               (setq tgr-peer
                 (make-thread (lambda ()
                   (while (null tgr-stop) (garbage-collect) (thread-yield)))))
               (thread-yield))",
        ),
        "nil"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-live-p tgr-peer) (thread-last-error))"
        ),
        "(t nil)",
        "the peer must remain live after its first collection and yield"
    );
    let buffer = &mut ev.green_seat().expect("seated").parked_roots_buffer;
    buffer.reserve(8192);
    let allocation = (buffer.as_ptr(), buffer.capacity());
    assert!(allocation.1 > 0);
    for turn in 0..32 {
        let collections = ev.tagged_heap.gc_collections();
        ev.green_wait(super::Wait::Runnable)
            .expect("peer collects and yields");
        assert!(
            ev.tagged_heap.gc_collections() > collections,
            "the peer must actually collect while main is parked on turn {turn}"
        );
        assert!(
            ev.green_seat().expect("seated").parked_root_batch.is_some(),
            "the returning owner retains its empty typed batch storage"
        );
        let buffer = &ev.green_seat().expect("seated").parked_roots_buffer;
        assert!(
            buffer.is_empty(),
            "running owners retain capacity, not roots"
        );
        assert_eq!((buffer.as_ptr(), buffer.capacity()), allocation);
        assert_eq!(
            print_value(&kept),
            "\"only the parked main owner's VM frame keeps this alive\""
        );
    }
    assert_eq!(
        eval(
            &mut ev,
            "(progn (setq tgr-stop t)
               (list (thread-join tgr-peer) (thread-live-p tgr-peer) (thread-last-error)))"
        ),
        "(nil nil nil)",
        "the peer must exit normally without a recorded error"
    );
    ev.pop_vm_root_frame();
}

/// Callgrind micro for the 3f baseline: `(Ir(N) - Ir(0)) / N` is one yield
/// round trip, main to a worker and back: two Baton hand-offs (park, root
/// publication, channel transfer, resume) plus one iteration of the
/// worker's interpreted `(while (null stop) (thread-yield))`.
#[test]
#[cfg(feature = "perf-probes")]
fn green_yield_switch_micro() {
    let n: usize = std::env::var("TGY_MICRO_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
           (setq tgy-stop nil)
           (setq tgy-th (make-thread (lambda () (while (null tgy-stop) (thread-yield)))))
           (thread-yield))",
        ),
        "nil",
        "yield micro setup must succeed"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-live-p tgy-th) (thread-last-error tgy-th))"
        ),
        "(t nil)",
        "yield micro needs a live peer"
    );
    for _ in 0..n {
        ev.green_wait(super::Wait::Runnable).expect("yield");
    }
    assert_eq!(
        eval(&mut ev, "(progn (setq tgy-stop t) (thread-join tgy-th))"),
        "nil"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-live-p tgy-th) (thread-last-error tgy-th))"
        ),
        "(nil nil)",
        "joined yield peer must have completed without error"
    );
}

#[test]
fn the_standard_case_table_is_one_table_after_admission() {
    let mut ev = green();
    ev.admit_threads().expect("admit");
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgsa-old (standard-case-table) tgsa-new (make-char-table 'case-table))
               (list (eq (standard-case-table) tgsa-old)
                     (eq (set-standard-case-table tgsa-new) tgsa-new)
                     (eq (standard-case-table) tgsa-new)
                     (progn (set-standard-case-table tgsa-old)
                            (list (upcase \"abc\") (eq (standard-case-table) tgsa-old)))))"
        ),
        "(t t t (\"ABC\" t))"
    );
}

#[test]
fn an_uncaught_throw_ends_a_thread_as_its_last_error() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (thread-last-error t)
               (let ((th (make-thread (lambda () (throw 'tgu-tag 7)))))
                 (list (thread-join th) (thread-live-p th) (thread-last-error t))))"
        ),
        "(nil nil (no-catch tgu-tag 7))"
    );
}

#[test]
fn a_thread_starts_in_its_creators_buffer_and_keeps_its_own() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgo-a (get-buffer-create \"tgo-a\") tgo-b (get-buffer-create \"tgo-b\"))
               (set-buffer tgo-a)
               (let ((th (make-thread (lambda ()
                                        (let ((start (buffer-name)))
                                          (set-buffer tgo-b)
                                          (thread-yield)
                                          (list start (buffer-name)))))))
                 (thread-yield)
                 (list (buffer-name) (thread-join th) (buffer-name))))"
        ),
        "(\"tgo-a\" (\"tgo-a\" \"tgo-b\") \"tgo-a\")"
    );
}

/// GNU switches from unwind-protect cleanups. A join forces the cleanup
/// continuation to park until its worker has completed.
#[test]
fn a_switch_inside_an_unwind_cleanup_runs_the_peer() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(unwind-protect 'body
               (thread-join (make-thread (lambda () (garbage-collect) 'peer))))"
        ),
        "body"
    );
}

/// The pending return/Flow is in drain_unwind_to's VM root scope; the
/// popped cleanup and its restoration environment are in PoppedBindingScope.
/// Each cleanup creates and joins a fresh collector, forcing those roots
/// through Green parking before another owner collects. GNU31.1's outputs
/// were captured under the PID sandbox (round3e).
#[test]
fn cleanup_switches_publish_results_flows_and_lexical_environments() {
    let mut ev = green();
    eval(
        &mut ev,
        "(fset 'g3e-collect-peer
           (lambda ()
             (thread-join
               (make-thread (lambda ()
                              (dotimes (i 300) (make-list 20 i))
                              (garbage-collect)
                              'collected)))))",
    );
    let cases = [
        (
            "(unwind-protect (list 'normal (make-string 40 ?n)) (g3e-collect-peer))",
            "(normal \"nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn\")",
        ),
        (
            "(catch 'tag
               (unwind-protect (throw 'tag (list 'thrown (make-string 40 ?t)))
                 (g3e-collect-peer)))",
            "(thrown \"tttttttttttttttttttttttttttttttttttttttt\")",
        ),
        (
            "(condition-case err
               (unwind-protect (signal 'error (list (make-string 40 ?e)))
                 (g3e-collect-peer))
               (error err))",
            "(error \"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\")",
        ),
        (
            "(progn
               (eval '(let ((only-here (list 'closure (make-string 40 ?c))))
                        (unwind-protect 'body
                          (g3e-collect-peer)
                          (setq g3e-capture only-here))) t)
               g3e-capture)",
            "(closure \"cccccccccccccccccccccccccccccccccccccccc\")",
        ),
        (
            "(catch 'outer
               (unwind-protect
                   (unwind-protect (throw 'outer 'original)
                     (throw 'outer (list 'replacement (make-string 40 ?r))))
                 (g3e-collect-peer)))",
            "(replacement \"rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr\")",
        ),
        // GNU byte-compile generated these byte strings for an unwind-protect
        // body and its cleanup callback. This exercises the VM's carried
        // result and the callable form of SpecBinding::UnwindProtect.
        (
            "(funcall
               (make-byte-code nil
                 (unibyte-string 192 142 193 194 195 196 34 41 68 135)
                 (vector
                   (make-byte-code nil (unibyte-string 192 32 135) [g3e-collect-peer] 1)
                   'vm 'make-string 40 118)
                 4))",
            "(vm \"vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv\")",
        ),
    ];
    for (form, expected) in cases {
        assert_eq!(eval(&mut ev, form), expected, "{form}");
    }
}

#[test]
fn native_gc_inhibit_still_refuses_a_cleanup_switch() {
    let mut ev = green();
    eval(&mut ev, "(thread-join (make-thread (lambda () nil)))");
    let mut inhibited = super::super::GcInhibitGuard::enter(&mut ev);
    assert_eq!(
        eval(
            inhibited.context(),
            "(condition-case e (unwind-protect 'body (thread-yield))
               (error (car (cdr e))))",
        ),
        "\"Cannot switch threads while garbage collection is inhibited\"",
    );
}

#[test]
fn main_ending_also_ends_threads_created_by_threads() {
    let mut ev = green();
    eval(
        &mut ev,
        "(progn
           (setq tgg-m (make-mutex) tgg-child nil)
           (mutex-lock tgg-m)
           (setq tgg-parent
                 (make-thread (lambda ()
                                (setq tgg-child (make-thread (lambda () (mutex-lock tgg-m))))
                                (thread-join tgg-child))))
           (thread-yield)
           (thread-yield)
           (list (thread-live-p tgg-parent) (thread-live-p tgg-child)))",
    );
    ev.green_teardown();
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-live-p tgg-parent) (thread-live-p tgg-child))"
        ),
        "(nil nil)"
    );
}

// The 3d D1 gaps, closed by round 3w: state GNU keeps global is one value
// for every Lisp thread. Each expectation is GNU 31.1's answer (sandboxed
// GNU run, 2026-10-14).

/// The composition is a text property of the string, which every thread
/// reads; the registry behind it being per OS thread (D1) does not show.
#[test]
fn a_thread_sees_a_composition_made_by_another() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgd1-s (copy-sequence \"ab\"))
               (compose-string-internal tgd1-s 0 2)
               (list (find-composition-internal 0 nil tgd1-s t)
                     (thread-join (make-thread
                                   (lambda () (find-composition-internal 0 nil tgd1-s t))))))"
        ),
        "((0 2 [97 98] t nil 1) (0 2 [97 98] t nil 1))"
    );
}

/// The redisplay change marks a buffer change bumps are one World's: the
/// running owner holds them and hands them on with the Baton.
#[test]
fn a_threads_face_and_display_changes_reach_main() {
    let mut ev = green();
    let faces = ev.face_change_count();
    let display = ev.display_var_change_count();
    assert_eq!(
        eval(
            &mut ev,
            "(thread-join (make-thread (lambda ()
               (condition-case err
                   (progn
                     (internal-make-lisp-face 'tg3w-face)
                     (internal-set-lisp-face-attribute 'tg3w-face :foreground \"green\")
                     (set (make-local-variable (quote truncate-lines)) t)
                     'done)
                 (error err)))))"
        ),
        "done"
    );
    assert!(ev.face_change_count() > faces);
    assert!(ev.display_var_change_count() > display);
}

#[test]
fn d1_a_thread_sees_the_time_zone_rule_set_by_another() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (set-time-zone-rule \"XXX-5\")
               (list (current-time-zone)
                     (thread-join (make-thread (lambda () (current-time-zone))))))"
        ),
        "((18000 \"XXX\") (18000 \"XXX\"))"
    );
}

#[test]
fn d1_a_thread_sees_a_process_made_by_another() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgd1-p (make-pipe-process :name \"tgp\"))
               (prog1 (thread-join (make-thread
                                    (lambda ()
                                      (list (format \"%S\" tgd1-p) (process-name tgd1-p)
                                            (and (memq tgd1-p (process-list)) t)))))
                 (delete-process tgd1-p)))"
        ),
        "(\"#<process tgp>\" \"tgp\" t)"
    );
}

#[test]
fn d1_a_thread_uses_a_sqlite_handle_opened_by_another() {
    let mut ev = green();
    // GNU 31.1 here was built without sqlite; any GNU with sqlite answers
    // the row the other thread inserted.
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgd1-db (sqlite-open))
               (sqlite-execute tgd1-db \"create table t (x)\")
               (sqlite-execute tgd1-db \"insert into t values (42)\")
               (thread-join (make-thread
                             (lambda () (sqlite-select tgd1-db \"select x from t\")))))"
        ),
        "((42))"
    );
}

fn select_parks() -> usize {
    super::switch::SELECT_PARKS.load(std::sync::atomic::Ordering::Relaxed)
}

/// A Sim World's waits are today's: no Green runtime, no slice. Only the
/// park count is asserted here; the waitclk tests separately require the
/// full fine-clock timeout in both Sim and Green.
#[test]
fn a_sim_worlds_waits_never_slice() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new_with_threading(ThreadingMode::Sim);
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

/// GNU really_call_select releases even with no ready peer. Each positive
/// wait publishes once; sleeping peers never introduce periodic slices.
#[test]
fn a_green_wait_parks_without_ready_peers_and_never_slices() {
    let mut ev = green();
    eval(&mut ev, "(thread-join (make-thread (lambda () nil)))");
    let before = select_parks();
    assert_eq!(eval(&mut ev, "(progn (sleep-for 0.05) 'alone)"), "alone");
    assert_eq!(select_parks() - before, 1, "a lone wait did not publish");
    let before = select_parks();
    let started = std::time::Instant::now();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((th (make-thread (lambda () (sleep-for 0.2) 'peer))))
               (sleep-for 0.3)
               (thread-join th))"
        ),
        "peer"
    );
    let elapsed = started.elapsed();
    // Main and peer each publish their real wait, with at most the
    // deadline-driven continuation when the peer's wait ends.
    let parks = select_parks() - before;
    assert!(parks <= 4, "{parks} parks while both threads slept");
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "the two waits took {elapsed:?}"
    );
}

/// Observe the committed canonical park, rather than inferring release
/// from elapsed time. The callback is before the physical scheduler wait;
/// this proves publication and a durable control wake before arm.
fn assert_no_ready_peer_select_publication(ev: &mut Context, sleeping_peer: bool) {
    use super::scheduler::{Peers, SelectParkExpectation, ThreadKey, Wait};
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    let control = Arc::clone(&ev.green_seat().expect("seated Green main").control);
    assert!(matches!(
        (
            sleeping_peer,
            control.peers(ThreadKey::Main, Instant::now())
        ),
        (false, Peers::Blocked) | (true, Peers::Sleeping(_))
    ));
    let weak = Arc::downgrade(&control);
    let (published, publication) = mpsc::channel();
    control.after_next_select_park_for_test(
        ThreadKey::Main,
        SelectParkExpectation::Ordinary,
        move |parked| {
            let control = weak.upgrade().expect("live selecting owner");
            assert_eq!(parked.owner(), ThreadKey::Main);
            assert_eq!(
                control.wait_of(ThreadKey::Main),
                Some(Wait::Select {
                    wait_end: parked.wait_end(),
                }),
                "the native waiter must already have published Select"
            );
            assert!(matches!(
                (
                    sleeping_peer,
                    control.peers(ThreadKey::Main, Instant::now())
                ),
                (false, Peers::Blocked) | (true, Peers::Sleeping(_))
            ));
            // Publish durable Runnable before the idle selector arms. Its
            // subsequent recheck must retain this control readiness.
            control.wake(ThreadKey::Main);
            published
                .send(parked.wait_end())
                .expect("publication witness");
        },
    );
    assert!(matches!(
        ev.green_select_switch(Duration::from_secs(30)),
        Ok(super::SelectTurn::Switched)
    ));
    let deadline = publication
        .try_recv()
        .expect("no-ready-peer Select must publish before returning");
    assert!(
        Instant::now() < deadline,
        "control wake did not resume Select"
    );
    assert_eq!(control.wait_of(ThreadKey::Main), Some(Wait::Runnable));
}

#[test]
fn a_lone_green_select_publishes_and_preserves_a_control_wake_before_arm() {
    let mut ev = green();
    eval(&mut ev, "(thread-join (make-thread (lambda () nil)))");
    assert_no_ready_peer_select_publication(&mut ev, false);
}

#[test]
fn a_green_select_with_only_a_sleeping_peer_publishes_its_own_deadline() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq s2r3-select-sleeper
                 (make-thread (lambda ()
                   (condition-case nil (sleep-for 60) (error 'interrupted)))))
               (thread-yield)
               'sleeping)"
        ),
        "sleeping"
    );
    assert_no_ready_peer_select_publication(&mut ev, true);
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (thread-signal s2r3-select-sleeper 'error '(stop))
               (thread-yield)
               (thread-join s2r3-select-sleeper))"
        ),
        "interrupted"
    );
}

#[test]
fn a_lone_green_select_retains_zero_timeout_and_switch_refusal() {
    use std::time::Duration;

    let mut ev = green();
    eval(&mut ev, "(thread-join (make-thread (lambda () nil)))");
    let control = std::sync::Arc::clone(&ev.green_seat().expect("main seat").control);
    let (sender, observed) = std::sync::mpsc::channel();
    control.after_next_select_park_for_test(
        super::ThreadKey::Main,
        super::scheduler::SelectParkExpectation::Ordinary,
        move |_| sender.send(()).expect("live negative-path witness"),
    );
    assert!(matches!(
        ev.green_select_switch(Duration::ZERO),
        Ok(super::SelectTurn::Block(Duration::ZERO))
    ));
    ev.gc_inhibit_depth += 1;
    let refused = ev.green_select_switch(Duration::from_secs(30));
    ev.gc_inhibit_depth -= 1;
    assert!(matches!(
        refused,
        Ok(super::SelectTurn::Block(duration)) if duration == Duration::from_secs(30)
    ));
    assert!(matches!(
        observed.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}

/// A peer stays runnable for the whole timeout. None of its safepoints
/// makes the waiting owner run a periodic 10 ms select slice.
#[test]
fn a_green_select_has_no_periodic_slices() {
    let mut ev = green();
    let before = select_parks();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
           (setq g3f-wait-done nil)
           (let ((th (make-thread (lambda ()
                       (while (null g3f-wait-done) (thread-yield)) 'done))))
             (sleep-for 0.08)
             (setq g3f-wait-done t)
             (thread-join th)))"
        ),
        "done"
    );
    assert_eq!(
        select_parks() - before,
        1,
        "a waiting owner was periodically resumed"
    );
}

/// Joining main is blocked, so the newborn's first yield reacquires its
/// own turn. The deferred signal must still reach its newly installed handler.
/// The first join snapshots and re-raises the pending signal even though the
/// worker catches it; a second join reads the worker's successful result.
#[test]
fn a_newborn_signal_is_delivered_even_when_its_yield_has_no_ready_peer() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
           (setq g3f-newborn-handled nil)
           (let ((th (make-thread (lambda ()
                       (condition-case nil (thread-yield)
                         (error (setq g3f-newborn-handled t) 'caught))))))
             (thread-signal th 'error '(wake))
             (let ((join-error (condition-case e (thread-join th) (error e))))
               (list join-error g3f-newborn-handled
                     (thread-join th) (thread-last-error th)))))"
        ),
        "((error wake) t caught nil)"
    );
}

#[test]
fn a_yield_without_ready_peers_still_refuses_gc_inhibition() {
    let mut ev = green();
    eval(&mut ev, "(thread-join (make-thread (lambda () nil)))");
    ev.gc_inhibit_depth += 1;
    let result = ev.green_thread_yield(&[]);
    ev.gc_inhibit_depth -= 1;
    assert!(result.is_err(), "yield still validates the switch point");
}

/// A scheduler event interrupts a parked select without waiting for its
/// timeout; the resumed owner raises the pending thread signal. Let the
/// worker consume it before join, which otherwise re-raises a pending signal.
#[test]
fn a_signal_wakes_a_thread_waiting_in_select() {
    let mut ev = green();
    let started = std::time::Instant::now();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((th (make-thread (lambda ()
                     (condition-case nil (sleep-for 5)
                       (error 'interrupted))))))
           (thread-yield)
           (thread-signal th 'error '(wake))
           (thread-yield)
           (list (thread-join th) (thread-last-error th)))"
        ),
        "(interrupted nil)"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

/// Readiness reaches an owner parked in accept-process-output while its
/// peer keeps yielding, before the accept timeout or any periodic slice.
#[test]
fn fd_readiness_wakes_a_green_select_while_a_peer_yields() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut ev = green();
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

/// The same descriptor publication reaches main when every other Green
/// owner has exited. The writer requires the actual Select witness before
/// writing; a Baton-held ordinary backend block cannot satisfy that gate.
#[test]
fn fd_readiness_wakes_a_green_select_without_a_ready_peer() {
    use std::time::{Duration, Instant};

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            &format!(
                r#"(progn
                  (thread-join (make-thread (lambda () nil)))
                  (setq s2r3-no-peer-output nil)
                  (setq s2r3-no-peer-process
                    (make-network-process :name "s2r3-no-peer" :host "127.0.0.1"
                      :service {port} :noquery t
                      :filter (lambda (_p text) (setq s2r3-no-peer-output text))))
                  'connected)"#
            )
        ),
        "connected"
    );
    let control = std::sync::Arc::clone(&ev.green_seat().expect("main seat").control);
    assert!(matches!(
        control.peers(super::ThreadKey::Main, Instant::now()),
        super::scheduler::Peers::Blocked
    ));
    let (gate, completion) =
        main_select_park_gate(&mut ev, super::scheduler::SelectParkExpectation::Ordinary);
    let server = std::thread::spawn(move || write_ready_after_main_select_park(listener, gate));
    let selected = ev.green_select_switch(Duration::from_secs(30));
    let stream = server
        .join()
        .expect("descriptor writer joined")
        .expect("descriptor bytes published after canonical park");
    assert_eq!(
        completion
            .recv_timeout(Duration::from_secs(5))
            .expect("actual Select/write acknowledgement"),
        Ok(())
    );
    assert!(matches!(selected, Ok(super::SelectTurn::Switched)));
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (accept-process-output s2r3-no-peer-process 0)
               s2r3-no-peer-output)"
        ),
        "\"ready\""
    );
    assert_eq!(
        eval(&mut ev, "(delete-process s2r3-no-peer-process)"),
        "nil"
    );
    drop(stream);
}

/// Marker for the fresh process `a_worker_panic_aborts_the_process_and_never_hangs_main`
/// spawns; without it the child body returns at once.
const WORKER_PANIC_CHILD_ENV: &str = "NEOVM_TEST_GREEN_WORKER_PANIC_CHILD";

/// The child of `a_worker_panic_aborts_the_process_and_never_hangs_main`.
///
/// The parent re-runs exactly this test in a fresh process with
/// [`WORKER_PANIC_CHILD_ENV`] set; a normal suite run returns here.
#[test]
fn worker_panic_child() {
    if std::env::var_os(WORKER_PANIC_CHILD_ENV).is_none() {
        return;
    }
    let mut ev = green();
    super::switch::PANIC_NEXT_WORKER.store(true, std::sync::atomic::Ordering::Relaxed);
    eval(&mut ev, "(thread-join (make-thread (lambda () 'never)))");
    // Unreachable: the worker's panic aborts the process.
    std::process::exit(0);
}

#[cfg(unix)]
#[test]
fn a_worker_panic_aborts_the_process_and_never_hangs_main() {
    use std::os::unix::process::ExitStatusExt;
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "emacs_core::eval::green::switch_tests::worker_panic_child",
            "--test-threads",
            "1",
        ])
        .env(WORKER_PANIC_CHILD_ENV, "1")
        .env_remove("RUST_LOG")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn the child");
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for the child") {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(60) {
            let _ = child.kill();
            panic!("a worker panic left main hanging");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(status.signal(), Some(libc::SIGABRT), "{status:?}");
}

#[path = "green_two_owner_test.rs"]
mod two_owner_tests;

#[path = "green_alias_test.rs"]
mod alias_tests;
/// The park walk must preserve execution-only values without visiting
/// canonical World cells. Compare every root group too, so adding an owner
/// group to the full walk cannot silently omit it at a thread switch.
#[test]
fn parking_traces_private_roots_without_world_cells() {
    let mut ev = green();
    let world = ev
        .eval_str("(setq g3f-world-root (list 'world-only))")
        .unwrap();
    let private = Value::list(vec![Value::fixnum(37)]);
    ev.eval_temp_roots.push(private);
    let mut full = Vec::new();
    ev.trace_roots(
        ev.tagged_heap.identity(),
        |roots| crate::tagged::transport::collect_shared_root_gc_roots(&ev.tagged_heap, roots),
        &mut |_| {},
        &mut |v| full.push(v),
    );
    assert!(full.contains(&world));
    let mut parked = Vec::new();
    ev.trace_private_roots(&mut parked);
    assert!(parked.contains(&private));
    assert!(!parked.contains(&world));
    let is_world = std::cell::Cell::new(false);
    let mut expected = Vec::new();
    ev.trace_roots(
        ev.tagged_heap.identity(),
        |roots| crate::tagged::transport::collect_shared_root_gc_roots(&ev.tagged_heap, roots),
        &mut |name| {
            is_world.set(matches!(
                name,
                "owned_native"
                    | "owner_bindings"
                    | "treesit"
                    | "shared_roots"
                    | "ccl_registry"
                    | "charset_registry"
                    | "terminal_registry"
                    | "font_registry"
                    | "hash_table_test_registry"
                    | "file_notify_registry"
                    | "dynamic_module_registry"
                    | "obarray"
                    | "autoloads"
                    | "buffers"
                    | "frames"
                    | "coding_systems"
                    | "proc_timer"
                    | "registers"
                    | "interactive"
                    | "xwidgets"
                    | "face_table"
                    | "threads"
                    | "kmacro"
                    | "command_loop"
                    | "modes"
            ))
        },
        &mut |v| {
            if !is_world.get() {
                expected.push(v)
            }
        },
    );
    parked.sort_unstable_by_key(|value| value.bits());
    expected.sort_unstable_by_key(|value| value.bits());
    assert_eq!(parked, expected);
    ev.eval_temp_roots.pop();
}
/// Fixed wall-time wait with a runnable peer: report select parks and peer
/// throughput independently of the peer's machine-dependent turn count.
#[test]
#[cfg(feature = "perf-probes")]
fn green_select_wait_micro() {
    let millis: u64 = std::env::var("TGSELECT_MICRO_MS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(150);
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
           (setq g3f-micro-done nil g3f-micro-turns 0)
           (setq g3f-micro-thread
             (make-thread (lambda ()
               (while (null g3f-micro-done)
                 (setq g3f-micro-turns (1+ g3f-micro-turns))
                 (thread-yield)))))
           (thread-yield))",
        ),
        "nil",
        "wait micro setup must succeed"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(list (thread-live-p g3f-micro-thread) (thread-last-error g3f-micro-thread))"
        ),
        "(t nil)",
        "wait micro needs a live peer"
    );
    let wait = ev
        .eval_str(&format!("'(sleep-for {})", millis as f64 / 1000.0))
        .expect("read wait micro");
    let parks = select_parks();
    let start = std::time::Instant::now();
    ev.eval_form(wait).expect("wait micro");
    let elapsed = start.elapsed();
    let parks = select_parks() - parks;
    let turns = eval(
        &mut ev,
        "(progn (setq g3f-micro-done t)
           (thread-join g3f-micro-thread) g3f-micro-turns)",
    );
    assert!(
        turns.parse::<u64>().expect("numeric peer turns") > 0,
        "the timed wait must run its peer"
    );
    assert_eq!(eval(&mut ev, "(thread-last-error g3f-micro-thread)"), "nil");
    eprintln!(
        "green_select_wait_micro requested_ms={millis} elapsed_ns={} parks={parks} peer_turns={turns}",
        elapsed.as_nanos()
    );
}
/// Writable :nowait completion must be decoded after an idle scheduler
/// wake. A readable-only broad poll skips a process still in `connect`.
#[test]
fn writable_connect_readiness_survives_a_green_select_with_a_yielding_peer() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut ev = green();
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
    let (gate, completion) = main_select_park_gate(
        &mut ev,
        super::scheduler::SelectParkExpectation::MainConnectPending(process),
    );
    let (writable_tx, writable_rx) = std::sync::mpsc::channel();
    ev.processes
        .after_next_native_connect_writable_for_test(process, move |event| {
            let _ = writable_tx.send(event);
        });
    let control = std::sync::Arc::clone(&ev.green_seat().expect("main seat").control);
    let cancel = ev.quit_requested.clone();
    let (release, until_finished) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let result = (|| -> std::io::Result<()> {
            let _stream = write_ready_after_main_select_park(listener, gate)?;
            match until_finished.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(()),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "evaluator did not release its Green peer",
                )),
            }
        })();
        if result.is_err() {
            cancel.request();
            control.wake(super::scheduler::ThreadKey::Main);
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
    let result = eval(&mut ev, source);
    drop(release);
    let server_result = server.join().expect("Green connect peer owner joined");
    let parked_result = completion.recv_timeout(std::time::Duration::from_secs(5));
    let native_writable = writable_rx.recv_timeout(std::time::Duration::from_secs(5));
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
    assert_eq!(eval(&mut ev, "g3f-connect-open-count"), "1");
}

#[test]
fn missing_green_park_witness_cancels_without_output_and_joins_peer() {
    let mut ev = green();
    let (gate, completion) =
        main_select_park_gate(&mut ev, super::scheduler::SelectParkExpectation::Ordinary);
    // Deliberately remove the sole receiver. No socket writer, normal key or
    // replacement output exists on this failure path.
    drop(gate);
    let result = ev.eval_str(
        r#"(progn
      (setq g3f-missing-stop nil g3f-missing-cleanup nil)
      (let ((th (make-thread (lambda ()
                   (while (null g3f-missing-stop) (thread-yield)) 'peer))))
        (unwind-protect
            (progn (sleep-for 2) 'unexpected-success)
          (setq g3f-missing-stop t g3f-missing-cleanup t)
          (thread-join th))))"#,
    );
    let observed = completion.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(
        observed.expect("actual failed park callback"),
        Err(ParkOutputError::WitnessReceiverClosed)
    );
    match result {
        Err(crate::emacs_core::error::EvalError::Signal { symbol, .. }) => {
            assert_eq!(crate::emacs_core::intern::resolve_sym(symbol), "quit");
        }
        other => panic!("missing witness must signal quit, got {other:?}"),
    }
    assert!(
        !ev.quit_requested.is_requested(),
        "failure quit must be drained"
    );
    ev.set_quit_flag_value(Value::NIL);
    assert_eq!(eval(&mut ev, "g3f-missing-cleanup"), "t");
}

fn assert_native_admission_refusal_keeps_ordinary_threads(
    cause: crate::host_effect::wake::NativeReadinessCause,
) {
    let mut context = green();
    context
        .processes
        .refuse_green_native_readiness_for_test(cause);
    assert_eq!(
        context.green_admission_readiness(),
        Err(super::GreenAdmissionRefusal::NativeReadinessUnavailable { cause })
    );
    assert!(!super::green_world(&context));
    assert!(!context.threads_admitted());
    assert_eq!(
        eval(
            &mut context,
            "(let ((worker (make-thread (lambda () 42)))) (list (threadp worker) (thread-join worker)))"
        ),
        "(t 42)"
    );
    assert!(!context.green_threads_running());
    assert!(!context.threads_admitted());
    assert_eq!(
        context.green_admission_readiness(),
        Err(super::GreenAdmissionRefusal::NativeReadinessUnavailable { cause })
    );
}

#[test]
fn absent_native_poller_refuses_green_without_refusing_lisp_threads() {
    assert_native_admission_refusal_keeps_ordinary_threads(
        crate::host_effect::wake::NativeReadinessCause::AbsentPoller,
    );
}

#[test]
fn failed_native_registration_refuses_green_without_refusing_lisp_threads() {
    assert_native_admission_refusal_keeps_ordinary_threads(
        crate::host_effect::wake::NativeReadinessCause::RegistrationFailed(
            std::io::ErrorKind::PermissionDenied,
        ),
    );
}

#[test]
fn native_admission_refusal_trace_records_each_cause_once_and_no_healthy_event() {
    use crate::host_effect::wake::NativeReadinessCause;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::prelude::*;

    #[derive(Clone)]
    struct RefusalEvents(Arc<Mutex<Vec<String>>>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RefusalEvents {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if event.metadata().target() == "neovm::green::admission" {
                struct RefusalField<'a>(&'a mut Vec<String>);
                impl Visit for RefusalField<'_> {
                    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                        if field.name() == "refusal" {
                            self.0.push(format!("{value:?}"));
                        }
                    }
                }
                event.record(&mut RefusalField(&mut self.0.lock().unwrap()));
            }
        }
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(RefusalEvents(Arc::clone(&events)));
    tracing::subscriber::with_default(subscriber, || {
        let healthy = green();
        assert_eq!(healthy.green_admission_readiness(), Ok(()));
        assert!(events.lock().unwrap().is_empty());
        for cause in [
            NativeReadinessCause::AbsentPoller,
            NativeReadinessCause::RegistrationFailed(std::io::ErrorKind::PermissionDenied),
        ] {
            let mut owner = green();
            owner
                .processes
                .refuse_green_native_readiness_for_test(cause);
            for _ in 0..2 {
                assert_eq!(
                    owner.green_admission_readiness(),
                    Err(super::GreenAdmissionRefusal::NativeReadinessUnavailable { cause })
                );
            }
            // Real dispatch keeps working and must not duplicate its recorded
            // admission failure or allocate a Green owner/Baton.
            assert_eq!(
                eval(&mut owner, "(thread-join (make-thread (lambda () 42)))"),
                "42"
            );
            assert!(!owner.green_threads_running());
        }
    });
    let expected = [
        NativeReadinessCause::AbsentPoller,
        NativeReadinessCause::RegistrationFailed(std::io::ErrorKind::PermissionDenied),
    ]
    .map(|cause| {
        format!(
            "{:?}",
            super::GreenAdmissionRefusal::NativeReadinessUnavailable { cause }
        )
    });
    assert_eq!(*events.lock().unwrap(), expected);
}
