//! Signal and method-call dispatch — GNU `xd_read_message_1`'s
//! `(:signal ...)` / `(:method ...)` arms.
//!
//! `unregistered_signal_stores_no_event` and
//! `registered_signal_fills_the_handler_slot` need no bus: they exercise
//! `dbus-registered-objects-table` and the input queue, both of which exist at
//! startup.  The end-to-end tests need the session bus and are skipped without
//! one, like `call.rs`.

use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{Value, eq_value};

use super::super::event;

fn session() -> Value {
    Value::keyword_by_name(":session")
}

fn signal_message(path: &str, interface: &str, member: &str) -> dbus::Message {
    dbus::Message::new_signal(path, interface, member).expect("signal message")
}

/// The event's HANDLER slot: nine fields precede it
/// (`BUS TYPE SERIAL SERVICE DESTINATION PATH INTERFACE MEMBER`).
fn handler_slot(queued: Value) -> Value {
    let mut rest = queued;
    for _ in 0..9 {
        rest = rest.cons_cdr();
    }
    rest.cons_car()
}

fn load_dbus(eval: &mut Context) -> bool {
    match eval.eval_str("(progn (require 'dbus) (dbus-ignore-errors (dbus-get-unique-name :session)))")
    {
        Ok(name) if name.is_string() => true,
        Ok(_) => false,
        Err(err) => panic!("(require 'dbus) failed: {err:?}"),
    }
}

/// A message nobody registered for stores nothing.
///
/// GNU never queues a `dbus-event` with a nil handler, and `dbus-check-event`
/// rejects that shape with "Not a valid D-Bus event".  One unhandled signal —
/// Avahi's `CacheExhausted` on the service-type browser `zeroconf-init` opens —
/// therefore aborted whatever the wait loop was pumping, which is how
/// byte-compiling `lisp/net/tramp-archive.el` (its toplevel
/// `(require 'tramp-gvfs)`) failed a whole `fresh-build`.
#[test]
fn unregistered_signal_stores_no_event() {
    crate::test_utils::init_test_tracing();
    super::super::reset_thread_locals();
    let mut eval = crate::test_utils::runtime_startup_context();

    event::queue_events(
        &mut eval,
        session(),
        signal_message("/neomacs/test", "org.neomacs.Test", "Boom"),
    )
    .expect("dispatch should not fail");

    assert!(
        eval.command_loop.keyboard.kboard.unread_events.is_empty(),
        "a signal with no registration must not queue a dbus-event"
    );
    super::super::reset_thread_locals();
}

/// A registered handler lands in the handler slot, and the path filter still
/// applies.
#[test]
fn registered_signal_fills_the_handler_slot() {
    crate::test_utils::init_test_tracing();
    super::super::reset_thread_locals();
    let mut eval = crate::test_utils::runtime_startup_context();

    eval.eval_str(
        r#"(progn
             (defun neomacs-signal-test-handler (&rest _) t)
             (puthash '(:signal :session "org.neomacs.Test" "Boom")
                      (list (list nil nil "/neomacs/test" 'neomacs-signal-test-handler nil))
                      dbus-registered-objects-table)
             t)"#,
    )
    .expect("registering the signal should work");

    event::queue_events(
        &mut eval,
        session(),
        signal_message("/neomacs/test", "org.neomacs.Test", "Boom"),
    )
    .expect("dispatch should not fail");

    let registered = eval
        .eval_str("'neomacs-signal-test-handler")
        .expect("handler symbol");
    let queued = eval
        .command_loop
        .keyboard
        .kboard
        .unread_events
        .pop_front()
        .expect("a registered signal queues one dbus-event");
    let slot = handler_slot(queued);
    assert!(
        eq_value(&slot, &registered),
        "handler slot should hold the registered handler, got {slot:?}"
    );
    assert!(
        eval.command_loop.keyboard.kboard.unread_events.is_empty(),
        "one message queues one event per handler"
    );

    // The same signal at another object path matches nothing.
    event::queue_events(
        &mut eval,
        session(),
        signal_message("/neomacs/other", "org.neomacs.Test", "Boom"),
    )
    .expect("dispatch should not fail");
    assert!(
        eval.command_loop.keyboard.kboard.unread_events.is_empty(),
        "a path the registration does not name must not match"
    );
    super::super::reset_thread_locals();
}

/// A directed signal reaches the handler `dbus-register-signal` installed.
#[test]
fn directed_signal_reaches_its_handler() {
    crate::test_utils::init_test_tracing();
    super::super::reset_thread_locals();
    let mut eval = crate::test_utils::runtime_startup_context();
    if !load_dbus(&mut eval) {
        super::super::reset_thread_locals();
        return;
    }

    let calls = eval
        .eval_str(
            r#"(progn
                 (defvar neomacs-signal-test-calls nil)
                 (defun neomacs-signal-test-handler (&rest args)
                   (setq neomacs-signal-test-calls (cons args neomacs-signal-test-calls)))
                 (dbus-register-signal
                  :session nil "/neomacs/test" "org.neomacs.Test" "Boom"
                  #'neomacs-signal-test-handler)
                 (dbus-send-signal
                  :session (dbus-get-unique-name :session)
                  "/neomacs/test" "org.neomacs.Test" "Boom" :string "payload")
                 ;; Pump the wait loop so the queued dbus-event is dispatched.
                 (ignore-errors (read-event nil nil 0.5))
                 neomacs-signal-test-calls)"#,
        )
        .unwrap_or_else(|err| panic!("signal dispatch failed: {err:?}"));

    assert!(
        !calls.is_nil(),
        "the handler registered for the signal should have been called"
    );
    super::super::reset_thread_locals();
}

/// A directed method call reaches the handler `dbus-register-method` installed.
#[test]
fn directed_method_call_reaches_its_handler() {
    crate::test_utils::init_test_tracing();
    super::super::reset_thread_locals();
    let mut eval = crate::test_utils::runtime_startup_context();
    if !load_dbus(&mut eval) {
        super::super::reset_thread_locals();
        return;
    }

    let report = eval
        .eval_str(
            r#"(condition-case err
                   (progn
                     (defvar neomacs-method-test-calls nil)
                     (defvar neomacs-method-test-error nil)
                     (defun neomacs-method-test-handler (&rest args)
                       (setq neomacs-method-test-calls (cons args neomacs-method-test-calls))
                       :ignore)
                     (dbus-register-method
                      :session nil "/neomacs/test" "org.neomacs.Test" "Call"
                      #'neomacs-method-test-handler
                      ;; DONT-REGISTER-SERVICE: SERVICE is nil here, and
                      ;; registering it would ask the bus to RequestName nil.
                      t)
                     ;; Sent without a reply handler, so this does not wait.
                     (dbus-message-internal
                      1 :session (dbus-get-unique-name :session)
                      "/neomacs/test" "org.neomacs.Test" "Call" nil)
                     (condition-case e (read-event nil nil 0.5)
                       (error (setq neomacs-method-test-error (format "%S" e))))
                     (format "calls=%S error=%S"
                             neomacs-method-test-calls neomacs-method-test-error))
                 (error (format "OUTER=%S" err)))"#,
        )
        .expect("the form itself should evaluate");
    let text = report.as_utf8_str().unwrap_or("<not a string>");
    assert!(
        text.starts_with("calls=(nil)"),
        "the handler registered for the method should have been called: {text}"
    );
    super::super::reset_thread_locals();
}
