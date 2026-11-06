//! GNU 31.1 oracle for the five global-lock release points.
//!
//! GNU threads run one at a time under `global_lock`; every operation that
//! lets another Lisp thread run releases it first. The simulated backend has
//! one OS thread, so those release points are the only places a created
//! worker or a woken waiter can run. The pinned GNU 31.1 release points are
//! `thread_select` (src/thread.c:613-643, reached by `sleep-for`,
//! `accept-process-output` and the `read-event` wait), `thread-yield`
//! (:725-740), a contended `mutex-lock` (:212-246), `condition-wait`
//! (:446-501) and `thread-join` (:1095-1144).
//!
//! Every expected value below is the result of running the same form on the
//! pinned GNU build (Emacs 31.1, rev a360712c9d) in `--batch`; each was
//! repeated and is deterministic. The forms use global variables because a
//! GNU worker does not see the main thread's dynamic `let` bindings
//! (`Fmake_thread` gives the new thread an empty specpdl, :913-919).

use neovm_value::value::{Value, list_to_vec};

fn sim_eval(form: &str) -> Value {
    let mut eval = crate::test_utils::new_context();
    eval.eval_str(form)
        .unwrap_or_else(|flow| panic!("Neomacs rejected the GNU oracle form: {flow:?}"))
}

fn green_eval(form: &str) -> Value {
    let mut eval = crate::test_utils::new_context();
    eval.set_green_threading_mode_at_construction_for_test();
    eval.eval_str(form)
        .unwrap_or_else(|flow| panic!("Neomacs Green rejected the GNU oracle form: {flow:?}"))
}

fn items(value: &Value) -> Vec<Value> {
    list_to_vec(value).expect("oracle result is a proper list")
}

fn t() -> Value {
    Value::T
}

fn nil() -> Value {
    Value::NIL
}

// ---------------------------------------------------------------------------
// Forms (GNU batch probes, verbatim)
// ---------------------------------------------------------------------------

/// A `sleep-for` after `make-thread`: the worker has not run before the
/// select release point and has run after it. GNU probe: `(nil t t)`.
const SLEEP_FOR_RELEASE_FORM: &str = r#"
(progn
  (setq gnu-rp-x nil)
  (make-thread (lambda () (setq gnu-rp-x t)))
  (list gnu-rp-x (progn (sleep-for 0.02) gnu-rp-x) (null (sleep-for 0))))
"#;

/// `accept-process-output` reaches the same `thread_select`. GNU: `(nil t)`.
const ACCEPT_OUTPUT_RELEASE_FORM: &str = r#"
(progn
  (setq gnu-rp-x nil)
  (make-thread (lambda () (setq gnu-rp-x t)))
  (list gnu-rp-x (progn (accept-process-output nil 0.02) gnu-rp-x)))
"#;

/// The command-input wait (`read-event` with a timeout) is the idle wait
/// class. GNU: `(nil t)`.
const READ_EVENT_RELEASE_FORM: &str = r#"
(progn
  (setq gnu-rp-x nil)
  (make-thread (lambda () (setq gnu-rp-x t)))
  (list gnu-rp-x
        (condition-case e
            (progn (read-event nil nil 0.02) gnu-rp-x)
          (error (list 'err e)))))
"#;

/// A worker blocking on a mutex the main thread owns: GNU parks it on the
/// mutex's condition (`lisp_mutex_lock_for_thread`) and shows it through
/// `thread--blocker`; unlocking and releasing the global lock lets it finish.
/// GNU: `(t nil t nil worker nil nil)`.
const MUTEX_CONTENTION_FORM: &str = r#"
(progn
  (setq gnu-rp-x nil gnu-rp-m (make-mutex))
  (mutex-lock gnu-rp-m)
  (setq gnu-rp-w (make-thread (lambda ()
                                (mutex-lock gnu-rp-m)
                                (setq gnu-rp-x 'worker)
                                (mutex-unlock gnu-rp-m))))
  (sleep-for 0.02)
  (let ((before (list (thread-live-p gnu-rp-w)
                      gnu-rp-x
                      (eq (thread--blocker gnu-rp-w) gnu-rp-m)
                      (thread-last-error))))
    (mutex-unlock gnu-rp-m)
    (sleep-for 0.02)
    (append before (list gnu-rp-x (thread-live-p gnu-rp-w) (thread-join gnu-rp-w)))))
"#;

/// The main thread contends for a mutex a worker holds: GNU blocks main in
/// `mutex_lock_callback` until the worker releases it. GNU:
/// `(t t nil inner-done nil inner-done)`.
const MAIN_MUTEX_CONTENTION_FORM: &str = r#"
(progn
  (setq gnu-rp-x nil gnu-rp-m (make-mutex) gnu-rp-m2 (make-mutex))
  (mutex-lock gnu-rp-m2)
  (setq gnu-rp-w (make-thread (lambda ()
                                (mutex-lock gnu-rp-m)
                                (mutex-lock gnu-rp-m2)
                                (mutex-unlock gnu-rp-m2)
                                (setq gnu-rp-x 'inner-done)
                                (mutex-unlock gnu-rp-m))))
  (sleep-for 0.02)
  (let ((blocked (list (thread-live-p gnu-rp-w)
                       (eq (thread--blocker gnu-rp-w) gnu-rp-m2)
                       (thread-last-error))))
    (mutex-unlock gnu-rp-m2)
    (mutex-lock gnu-rp-m)
    (let ((after gnu-rp-x))
      (mutex-unlock gnu-rp-m)
      (append blocked (list after (thread-join gnu-rp-w) gnu-rp-x)))))
"#;

/// A worker waits on a condition variable; GNU shows it blocked on COND and
/// the notification resumes it, which reacquires the mutex. GNU:
/// `((t t nil) woken nil nil)`.
const WORKER_CONDITION_WAIT_FORM: &str = r#"
(progn
  (setq gnu-rp-state nil gnu-rp-m (make-mutex)
        gnu-rp-cv (make-condition-variable gnu-rp-m))
  (setq gnu-rp-w (make-thread (lambda ()
                                (mutex-lock gnu-rp-m)
                                (condition-wait gnu-rp-cv)
                                (setq gnu-rp-state 'woken)
                                (mutex-unlock gnu-rp-m))))
  (sleep-for 0.02)
  (let ((blocked (list (thread-live-p gnu-rp-w)
                       (eq (thread--blocker gnu-rp-w) gnu-rp-cv)
                       (thread-last-error))))
    (mutex-lock gnu-rp-m)
    (condition-notify gnu-rp-cv)
    (mutex-unlock gnu-rp-m)
    (sleep-for 0.02)
    (list blocked gnu-rp-state (thread-live-p gnu-rp-w) (thread-join gnu-rp-w))))
"#;

/// The main thread waits on a condition variable and a worker notifies it:
/// the worker runs while main's `condition-wait` has released the mutex, and
/// main observes its side effect after the wait returns. GNU:
/// `(yes (main worker) nil yes)`.
const MAIN_CONDITION_WAIT_FORM: &str = r#"
(progn
  (setq gnu-rp-worker-ran nil gnu-rp-log nil gnu-rp-m (make-mutex)
        gnu-rp-cv (make-condition-variable gnu-rp-m))
  (setq gnu-rp-w (make-thread (lambda ()
                                (mutex-lock gnu-rp-m)
                                (setq gnu-rp-worker-ran 'yes)
                                (condition-notify gnu-rp-cv)
                                (setq gnu-rp-log (cons 'worker gnu-rp-log))
                                (mutex-unlock gnu-rp-m))))
  (mutex-lock gnu-rp-m)
  (condition-wait gnu-rp-cv)
  (setq gnu-rp-log (cons 'main gnu-rp-log))
  (setq gnu-rp-after gnu-rp-worker-ran)
  (mutex-unlock gnu-rp-m)
  (list gnu-rp-after gnu-rp-log (thread-join gnu-rp-w) gnu-rp-worker-ran))
"#;

/// `thread-signal` interrupts a worker parked on a contended `mutex-lock`:
/// GNU's `thread_signal_callback` broadcasts the wait, the worker raises the
/// error without taking the mutex, and `thread-last-error` records it. The
/// later join has no injected error left to re-raise. GNU:
/// `(t t nil nil :no-error (error "blocked-oops"))`.
const SIGNAL_BLOCKED_MUTEX_FORM: &str = r#"
(progn
  (setq gnu-rp-x nil gnu-rp-m (make-mutex))
  (mutex-lock gnu-rp-m)
  (setq gnu-rp-w (make-thread (lambda ()
                                (mutex-lock gnu-rp-m)
                                (setq gnu-rp-x t))))
  (sleep-for 0.02)
  (let ((blocked (list (thread-live-p gnu-rp-w)
                       (eq (thread--blocker gnu-rp-w) gnu-rp-m))))
    (thread-signal gnu-rp-w 'error '("blocked-oops"))
    (sleep-for 0.02)
    (let ((joined (condition-case e
                      (progn (thread-join gnu-rp-w) :no-error)
                    (error (cons (car e) (cdr e))))))
      (mutex-unlock gnu-rp-m)
      (append blocked
              (list gnu-rp-x (thread-live-p gnu-rp-w) joined (thread-last-error))))))
"#;

/// The same interruption for `condition-wait`: GNU reacquires the mutex
/// before raising, and the worker's body after the wait does not run. GNU:
/// `(t t nil nil :no-error (error "cv-oops"))`.
const SIGNAL_BLOCKED_CONDITION_FORM: &str = r#"
(progn
  (setq gnu-rp-state nil gnu-rp-m (make-mutex)
        gnu-rp-cv (make-condition-variable gnu-rp-m))
  (setq gnu-rp-w (make-thread (lambda ()
                                (mutex-lock gnu-rp-m)
                                (condition-wait gnu-rp-cv)
                                (setq gnu-rp-state 'woken)
                                (mutex-unlock gnu-rp-m))))
  (sleep-for 0.02)
  (let ((blocked (list (thread-live-p gnu-rp-w)
                       (eq (thread--blocker gnu-rp-w) gnu-rp-cv))))
    (thread-signal gnu-rp-w 'error '("cv-oops"))
    (sleep-for 0.02)
    (let ((joined (condition-case e
                      (progn (thread-join gnu-rp-w) :no-error)
                    (error (cons (car e) (cdr e))))))
      (append blocked
              (list gnu-rp-state (thread-live-p gnu-rp-w) joined (thread-last-error))))))
"#;

fn sim_printed(form: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    let value = eval
        .eval_str(form)
        .unwrap_or_else(|flow| panic!("Neomacs rejected the GNU oracle form: {flow:?}"));
    crate::emacs_core::print::print_value(&value)
}

fn green_printed(form: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_green_threading_mode_at_construction_for_test();
    let value = eval
        .eval_str(form)
        .unwrap_or_else(|flow| panic!("Neomacs Green rejected the GNU oracle form: {flow:?}"));
    crate::emacs_core::print::print_value(&value)
}

// ---------------------------------------------------------------------------
// Sim backend (production default)
// ---------------------------------------------------------------------------

#[test]
fn gnu_sleep_for_is_a_thread_release_point() {
    assert_eq!(
        items(&sim_eval(SLEEP_FOR_RELEASE_FORM)),
        vec![nil(), t(), t()]
    );
}

#[test]
fn gnu_accept_process_output_is_a_thread_release_point() {
    assert_eq!(
        items(&sim_eval(ACCEPT_OUTPUT_RELEASE_FORM)),
        vec![nil(), t()]
    );
}

#[test]
fn gnu_read_event_wait_is_a_thread_release_point() {
    assert_eq!(items(&sim_eval(READ_EVENT_RELEASE_FORM)), vec![nil(), t()]);
}

#[test]
fn gnu_contended_mutex_lock_parks_the_worker_and_wakes_it() {
    assert_eq!(
        items(&sim_eval(MUTEX_CONTENTION_FORM)),
        vec![
            t(),
            nil(),
            t(),
            nil(),
            Value::symbol("worker"),
            nil(),
            nil()
        ]
    );
}

#[test]
fn gnu_main_mutex_lock_waits_for_the_owner() {
    assert_eq!(
        items(&sim_eval(MAIN_MUTEX_CONTENTION_FORM)),
        vec![
            t(),
            t(),
            nil(),
            Value::symbol("inner-done"),
            nil(),
            Value::symbol("inner-done")
        ]
    );
}

#[test]
fn gnu_condition_wait_handoff_resumes_the_worker() {
    let result = sim_eval(WORKER_CONDITION_WAIT_FORM);
    let outer = items(&result);
    assert_eq!(outer.len(), 4);
    assert_eq!(items(&outer[0]), vec![t(), t(), nil()]);
    assert_eq!(outer[1], Value::symbol("woken"));
    assert_eq!(outer[2], nil());
    assert_eq!(outer[3], nil());
}

#[test]
fn gnu_main_condition_wait_is_notified_by_a_worker() {
    let result = sim_eval(MAIN_CONDITION_WAIT_FORM);
    let outer = items(&result);
    assert_eq!(outer.len(), 4);
    assert_eq!(outer[0], Value::symbol("yes"));
    assert_eq!(
        items(&outer[1]),
        vec![Value::symbol("main"), Value::symbol("worker")]
    );
    assert_eq!(outer[2], nil());
    assert_eq!(outer[3], Value::symbol("yes"));
}

#[test]
fn gnu_signal_interrupts_a_worker_parked_on_a_mutex() {
    assert_eq!(
        sim_printed(SIGNAL_BLOCKED_MUTEX_FORM),
        "(t t nil nil :no-error (error \"blocked-oops\"))"
    );
}

#[test]
fn gnu_signal_interrupts_a_worker_parked_on_a_condition() {
    assert_eq!(
        sim_printed(SIGNAL_BLOCKED_CONDITION_FORM),
        "(t t nil nil :no-error (error \"cv-oops\"))"
    );
}

// ---------------------------------------------------------------------------
// Green backend: real carriers, so these release points are the runtime's
// own native waits; the GNU observable results must be identical.
// ---------------------------------------------------------------------------

#[test]
fn green_sleep_for_is_a_thread_release_point() {
    assert_eq!(
        items(&green_eval(SLEEP_FOR_RELEASE_FORM)),
        vec![nil(), t(), t()]
    );
}

#[test]
fn green_accept_process_output_is_a_thread_release_point() {
    assert_eq!(
        items(&green_eval(ACCEPT_OUTPUT_RELEASE_FORM)),
        vec![nil(), t()]
    );
}

#[test]
fn green_read_event_wait_is_a_thread_release_point() {
    assert_eq!(
        items(&green_eval(READ_EVENT_RELEASE_FORM)),
        vec![nil(), t()]
    );
}

#[test]
fn green_contended_mutex_lock_parks_the_worker_and_wakes_it() {
    assert_eq!(
        items(&green_eval(MUTEX_CONTENTION_FORM)),
        vec![
            t(),
            nil(),
            t(),
            nil(),
            Value::symbol("worker"),
            nil(),
            nil()
        ]
    );
}

#[test]
fn green_main_mutex_lock_waits_for_the_owner() {
    assert_eq!(
        items(&green_eval(MAIN_MUTEX_CONTENTION_FORM)),
        vec![
            t(),
            t(),
            nil(),
            Value::symbol("inner-done"),
            nil(),
            Value::symbol("inner-done")
        ]
    );
}

#[test]
fn green_condition_wait_handoff_resumes_the_worker() {
    let result = green_eval(WORKER_CONDITION_WAIT_FORM);
    let outer = items(&result);
    assert_eq!(outer.len(), 4);
    assert_eq!(items(&outer[0]), vec![t(), t(), nil()]);
    assert_eq!(outer[1], Value::symbol("woken"));
    assert_eq!(outer[2], nil());
    assert_eq!(outer[3], nil());
}

#[test]
fn green_signal_interrupts_a_worker_parked_on_a_mutex() {
    assert_eq!(
        green_printed(SIGNAL_BLOCKED_MUTEX_FORM),
        "(t t nil nil :no-error (error \"blocked-oops\"))"
    );
}

#[test]
fn green_signal_interrupts_a_worker_parked_on_a_condition() {
    assert_eq!(
        green_printed(SIGNAL_BLOCKED_CONDITION_FORM),
        "(t t nil nil :no-error (error \"cv-oops\"))"
    );
}

#[test]
fn green_main_condition_wait_is_notified_by_a_worker() {
    let result = green_eval(MAIN_CONDITION_WAIT_FORM);
    let outer = items(&result);
    assert_eq!(outer.len(), 4);
    assert_eq!(outer[0], Value::symbol("yes"));
    assert_eq!(
        items(&outer[1]),
        vec![Value::symbol("main"), Value::symbol("worker")]
    );
    assert_eq!(outer[2], nil());
    assert_eq!(outer[3], Value::symbol("yes"));
}
