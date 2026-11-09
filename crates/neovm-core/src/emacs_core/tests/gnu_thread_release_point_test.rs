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

// ---------------------------------------------------------------------------
// thread-signal delivery (GNU 31.1 src/thread.c).
//
// C2: a signal stored before the worker runs is raised by Fsignal inside the
// worker at the next post_acquire_global_lock after handlerlist exists
// (thread.c:141-154, first acquire at :805 is before the sentinel at :810).
// In these forms that reacquire is sleep-for's select return. The worker's
// condition-case therefore runs, and thread-join still reports the error
// because it snapshotted error_symbol before waiting (C3, :1118-1140).
// C4: thread-last-error changes only when the error escapes the thread
// function (record_thread_error, :754-758).
// C1: sleep-for is not woken by thread-signal; only a condvar wait is.
//
// Globals, not a dynamic `let`: a worker does not see the parent's dynamic
// bindings (Fmake_thread, :913-919). The log is updated with `setq`/`cons`,
// not `push`: a bare test Context has no subr.el, so `push` is void-function.
// `cons` onto nil prints the same list GNU's `push` produced. Pinned GNU 31.1
// (a360712c9d27, batch) printed the expected strings below.
// ---------------------------------------------------------------------------

/// Signal before the worker's first run. GNU:
/// `(nil (join-err arith-error ("bad")) nil nil ((caught arith-error ("bad"))))`.
const SIGNAL_BEFORE_RUN_CAUGHT_FORM: &str = r#"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (sleep-for 0.05) 'done)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (list (thread-signal gnu-ts-w 'arith-error '("bad"))
        (condition-case err
            (thread-join gnu-ts-w)
          (error (list 'join-err (car err) (cdr err))))
        (thread-live-p gnu-ts-w)
        (thread-last-error)
        gnu-ts-log))
"#;

/// Signal after the worker is inside `sleep-for`. GNU does not wake select,
/// so the join waits out the rest of the sleep (lower bound only) and the
/// handler still runs at reacquire. GNU:
/// `(t nil (join-err arith-error ("bad")) nil nil ((caught arith-error ("bad"))) t)`.
const SIGNAL_DURING_SLEEP_CAUGHT_FORM: &str = r#"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (sleep-for 0.35) 'done)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (sleep-for 0.05)
  (setq gnu-ts-live (thread-live-p gnu-ts-w))
  (setq gnu-ts-sig (thread-signal gnu-ts-w 'arith-error '("bad")))
  (let ((start (float-time)))
    (setq gnu-ts-join
          (condition-case err
              (thread-join gnu-ts-w)
            (error (list 'join-err (car err) (cdr err)))))
    (setq gnu-ts-elapsed (- (float-time) start)))
  (list gnu-ts-live gnu-ts-sig gnu-ts-join (thread-live-p gnu-ts-w)
        (thread-last-error) gnu-ts-log (>= gnu-ts-elapsed 0.20)))
"#;

/// No handler in the worker. The top-level catcher records the error and
/// join still signals its pre-wait snapshot. GNU:
/// `(nil (join-err arith-error ("bad")) nil (arith-error "bad"))`.
const SIGNAL_NO_HANDLER_FORM: &str = r#"
(progn
  (setq gnu-ts-w (make-thread (lambda () (sleep-for 0.05) 'done)))
  (list (thread-signal gnu-ts-w 'arith-error '("bad"))
        (condition-case err
            (thread-join gnu-ts-w)
          (error (list 'join-err (car err) (cdr err))))
        (thread-live-p gnu-ts-w)
        (thread-last-error)))
"#;

/// Worker blocked in `mutex-lock`. `thread-signal` broadcasts that condvar,
/// the handler catches the error, and an immediate join still reports the
/// snapshot. GNU:
/// `(t nil (join-err arith-error ("bad")) nil nil ((caught arith-error ("bad"))))`.
const SIGNAL_MUTEX_CAUGHT_FORM: &str = r#"
(progn
  (setq gnu-ts-log nil gnu-ts-m (make-mutex))
  (mutex-lock gnu-ts-m)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (mutex-lock gnu-ts-m) 'done)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (sleep-for 0.05)
  (setq gnu-ts-blocked (eq (thread--blocker gnu-ts-w) gnu-ts-m))
  (setq gnu-ts-sig (thread-signal gnu-ts-w 'arith-error '("bad")))
  (setq gnu-ts-join
        (condition-case err
            (thread-join gnu-ts-w)
          (error (list 'join-err (car err) (cdr err)))))
  (mutex-unlock gnu-ts-m)
  (list gnu-ts-blocked gnu-ts-sig gnu-ts-join (thread-live-p gnu-ts-w)
        (thread-last-error) gnu-ts-log))
"#;

const SIGNAL_BEFORE_RUN_EXPECT: &str =
    "(nil (join-err arith-error (\"bad\")) nil nil ((caught arith-error (\"bad\"))))";
const SIGNAL_DURING_SLEEP_EXPECT: &str =
    "(t nil (join-err arith-error (\"bad\")) nil nil ((caught arith-error (\"bad\"))) t)";
const SIGNAL_NO_HANDLER_EXPECT: &str =
    "(nil (join-err arith-error (\"bad\")) nil (arith-error \"bad\"))";
const SIGNAL_MUTEX_CAUGHT_EXPECT: &str =
    "(t nil (join-err arith-error (\"bad\")) nil nil ((caught arith-error (\"bad\"))))";

#[test]
fn gnu_thread_signal_before_run_runs_condition_handler() {
    assert_eq!(
        sim_printed(SIGNAL_BEFORE_RUN_CAUGHT_FORM),
        SIGNAL_BEFORE_RUN_EXPECT
    );
}

#[test]
fn green_thread_signal_before_run_runs_condition_handler() {
    assert_eq!(
        green_printed(SIGNAL_BEFORE_RUN_CAUGHT_FORM),
        SIGNAL_BEFORE_RUN_EXPECT
    );
}

#[test]
fn gnu_thread_signal_during_sleep_for_is_caught_at_reacquire() {
    assert_eq!(
        sim_printed(SIGNAL_DURING_SLEEP_CAUGHT_FORM),
        SIGNAL_DURING_SLEEP_EXPECT
    );
}

#[test]
fn green_thread_signal_during_sleep_for_is_caught_at_reacquire() {
    assert_eq!(
        green_printed(SIGNAL_DURING_SLEEP_CAUGHT_FORM),
        SIGNAL_DURING_SLEEP_EXPECT
    );
}

#[test]
fn gnu_thread_signal_without_handler_records_last_error() {
    assert_eq!(
        sim_printed(SIGNAL_NO_HANDLER_FORM),
        SIGNAL_NO_HANDLER_EXPECT
    );
}

#[test]
fn green_thread_signal_without_handler_records_last_error() {
    assert_eq!(
        green_printed(SIGNAL_NO_HANDLER_FORM),
        SIGNAL_NO_HANDLER_EXPECT
    );
}

#[test]
fn gnu_thread_signal_wakes_mutex_wait_into_handler() {
    assert_eq!(
        sim_printed(SIGNAL_MUTEX_CAUGHT_FORM),
        SIGNAL_MUTEX_CAUGHT_EXPECT
    );
}

#[test]
fn green_thread_signal_wakes_mutex_wait_into_handler() {
    assert_eq!(
        green_printed(SIGNAL_MUTEX_CAUGHT_FORM),
        SIGNAL_MUTEX_CAUGHT_EXPECT
    );
}

/// R5. Two signals before delivery: `thread_set_error` overwrites, so only
/// the last pair is delivered and snapshotted. GNU 31.1 batch:
/// `((join-err error ("last")) nil nil ((caught error ("last"))))`.
const SIGNAL_LAST_OF_TWO_FORM: &str = r#"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (thread-yield) 'ran)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'arith)
             (error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'other)))))
  (thread-signal gnu-ts-w 'arith-error '("first"))
  (thread-signal gnu-ts-w 'error '("last"))
  (list
   (condition-case err
       (thread-join gnu-ts-w)
     (error (list 'join-err (car err) (cdr err))))
   (thread-live-p gnu-ts-w)
   (thread-last-error)
   gnu-ts-log))
"#;

const SIGNAL_LAST_OF_TWO_EXPECT: &str =
    "((join-err error (\"last\")) nil nil ((caught error (\"last\"))))";

/// R6. Signal a thread that has already exited (`thread.c:76`, `m_specpdl`
/// is NULL). Join still signals the stored pair; liveness and
/// `thread-last-error` stay unchanged. The wait is bounded so a sim
/// `thread-live-p` that stays true cannot hang. GNU 31.1 batch, after the
/// worker had exited:
/// `(nil nil nil nil (join-err arith-error ("bad")) nil nil)`.
const SIGNAL_EXITED_THREAD_FORM: &str = r#"
(progn
  (setq gnu-ts-w (make-thread (lambda () 42)))
  (setq gnu-r6-deadline (+ (float-time) 0.5))
  (while (and (thread-live-p gnu-ts-w) (< (float-time) gnu-r6-deadline))
    (thread-yield))
  (setq gnu-r6-live1 (thread-live-p gnu-ts-w))
  (setq gnu-r6-le1 (thread-last-error))
  (thread-signal gnu-ts-w 'arith-error '("bad"))
  (list gnu-r6-live1
        (thread-live-p gnu-ts-w)
        gnu-r6-le1
        (thread-last-error)
        (condition-case err
            (list :ok (thread-join gnu-ts-w))
          (error (list 'join-err (car err) (cdr err))))
        (thread-last-error)
        (thread-live-p gnu-ts-w)))
"#;

const SIGNAL_EXITED_THREAD_EXPECT: &str =
    "(nil nil nil nil (join-err arith-error (\"bad\")) nil nil)";

/// R7. The first join re-raises the pre-wait snapshot. Delivery clears the
/// pair, so a second join returns the handler's result. GNU 31.1 batch:
/// `((join-err arith-error ("bad")) (:ok caught) nil nil ((caught arith-error ("bad"))))`.
const SIGNAL_SECOND_JOIN_FORM: &str = r#"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (thread-yield) 'ran)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (thread-signal gnu-ts-w 'arith-error '("bad"))
  (setq gnu-r7-j1
        (condition-case err
            (list :ok (thread-join gnu-ts-w))
          (error (list 'join-err (car err) (cdr err)))))
  (setq gnu-r7-j2
        (condition-case err
            (list :ok (thread-join gnu-ts-w))
          (error (list 'join-err (car err) (cdr err)))))
  (list gnu-r7-j1 gnu-r7-j2 (thread-live-p gnu-ts-w) (thread-last-error) gnu-ts-log))
"#;

const SIGNAL_SECOND_JOIN_EXPECT: &str =
    "((join-err arith-error (\"bad\")) (:ok caught) nil nil ((caught arith-error (\"bad\"))))";

/// R8 pins K1. The top-level handler exists before the user's
/// `condition-case`, so a yield that reacquires first lets the signal
/// escape. GNU 31.1 batch:
/// `((join-err arith-error ("bad")) nil (arith-error "bad") nil)`.
const SIGNAL_YIELD_BEFORE_HANDLER_FORM: &str = r#"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (thread-yield)
           (condition-case err
               'body
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (thread-signal gnu-ts-w 'arith-error '("bad"))
  (list
   (condition-case err
       (thread-join gnu-ts-w)
     (error (list 'join-err (car err) (cdr err))))
   (thread-live-p gnu-ts-w)
   (thread-last-error)
   gnu-ts-log))
"#;

const SIGNAL_YIELD_BEFORE_HANDLER_EXPECT: &str =
    "((join-err arith-error (\"bad\")) nil (arith-error \"bad\") nil)";

/// R9. Cleanup runs after the handler, while the dynamic binding is still
/// in force, and the binding is restored afterwards. GNU 31.1 batch
/// (lexical-binding t, `defvar` makes the let dynamic):
/// `(t (join-err arith-error ("bad")) nil nil ((caught arith-error ("bad"))) (inner t) outer)`.
const SIGNAL_UNWIND_LET_FORM: &str = r#"
(progn
  (defvar gnu-r9-x)
  (setq gnu-r9-x 'outer gnu-r9-caught nil gnu-r9-cleanup nil gnu-r9-log nil)
  (setq gnu-r9-m (make-mutex))
  (mutex-lock gnu-r9-m)
  (setq gnu-r9-w
        (make-thread
         (lambda ()
           (let ((gnu-r9-x 'inner))
             (unwind-protect
                 (condition-case err
                     (mutex-lock gnu-r9-m)
                   (arith-error
                    (setq gnu-r9-caught t)
                    (setq gnu-r9-log
                          (cons (list 'caught (car err) (cdr err)) gnu-r9-log))
                    'caught))
               (setq gnu-r9-cleanup (list gnu-r9-x gnu-r9-caught)))))))
  (sleep-for 0.05)
  (setq gnu-r9-blocked (eq (thread--blocker gnu-r9-w) gnu-r9-m))
  (thread-signal gnu-r9-w 'arith-error '("bad"))
  (setq gnu-r9-join
        (condition-case err
            (list :ok (thread-join gnu-r9-w))
          (error (list 'join-err (car err) (cdr err)))))
  (mutex-unlock gnu-r9-m)
  (list gnu-r9-blocked gnu-r9-join (thread-live-p gnu-r9-w) (thread-last-error)
        gnu-r9-log gnu-r9-cleanup gnu-r9-x))
"#;

const SIGNAL_UNWIND_LET_EXPECT: &str =
    "(t (join-err arith-error (\"bad\")) nil nil ((caught arith-error (\"bad\"))) (inner t) outer)";

#[test]
fn gnu_thread_signal_keeps_only_the_last() {
    assert_eq!(
        sim_printed(SIGNAL_LAST_OF_TWO_FORM),
        SIGNAL_LAST_OF_TWO_EXPECT
    );
}

#[test]
fn green_thread_signal_keeps_only_the_last() {
    assert_eq!(
        green_printed(SIGNAL_LAST_OF_TWO_FORM),
        SIGNAL_LAST_OF_TWO_EXPECT
    );
}

#[test]
fn gnu_thread_signal_of_an_exited_thread_stays_dead() {
    assert_eq!(
        sim_printed(SIGNAL_EXITED_THREAD_FORM),
        SIGNAL_EXITED_THREAD_EXPECT
    );
}

#[test]
fn green_thread_signal_of_an_exited_thread_stays_dead() {
    assert_eq!(
        green_printed(SIGNAL_EXITED_THREAD_FORM),
        SIGNAL_EXITED_THREAD_EXPECT
    );
}

#[test]
fn gnu_thread_signal_second_join_returns_the_result() {
    assert_eq!(
        sim_printed(SIGNAL_SECOND_JOIN_FORM),
        SIGNAL_SECOND_JOIN_EXPECT
    );
}

#[test]
fn green_thread_signal_second_join_returns_the_result() {
    assert_eq!(
        green_printed(SIGNAL_SECOND_JOIN_FORM),
        SIGNAL_SECOND_JOIN_EXPECT
    );
}

#[test]
fn gnu_thread_signal_before_handler_escapes() {
    assert_eq!(
        sim_printed(SIGNAL_YIELD_BEFORE_HANDLER_FORM),
        SIGNAL_YIELD_BEFORE_HANDLER_EXPECT
    );
}

#[test]
fn green_thread_signal_before_handler_escapes() {
    assert_eq!(
        green_printed(SIGNAL_YIELD_BEFORE_HANDLER_FORM),
        SIGNAL_YIELD_BEFORE_HANDLER_EXPECT
    );
}

#[test]
fn gnu_thread_signal_unwind_protect_and_let_survive_the_wait() {
    assert_eq!(
        sim_printed(SIGNAL_UNWIND_LET_FORM),
        SIGNAL_UNWIND_LET_EXPECT
    );
}

#[test]
fn green_thread_signal_unwind_protect_and_let_survive_the_wait() {
    assert_eq!(
        green_printed(SIGNAL_UNWIND_LET_FORM),
        SIGNAL_UNWIND_LET_EXPECT
    );
}

/// GNU `thread_live_p` is `(STATE)->m_specpdl != NULL` (src/thread.c:76).
/// `run_thread` clears that pointer at :822-823, before join, and unlinks
/// the thread at :853-859. A worker that has returned is dead and absent
/// from `all-threads` while its result is still joinable.
/// Pinned GNU 31.1 (`a360712c9d27`) batch print: `((t t) nil nil 42 nil)`.
const RETURNED_UNJOINED_IS_DEAD_FORM: &str = r#"
(let* ((w (make-thread (lambda () 42)))
       (before (list (thread-live-p w) (and (memq w (all-threads)) t))))
  (sleep-for 0.2)
  (list before
        (thread-live-p w)
        (and (memq w (all-threads)) t)
        (thread-join w)
        (thread-live-p w)))
"#;

const RETURNED_UNJOINED_IS_DEAD_EXPECT: &str = "((t t) nil nil 42 nil)";

#[test]
fn gnu_returned_unjoined_worker_is_dead_and_unlisted() {
    assert_eq!(
        sim_printed(RETURNED_UNJOINED_IS_DEAD_FORM),
        RETURNED_UNJOINED_IS_DEAD_EXPECT
    );
}

#[test]
fn green_returned_unjoined_worker_is_dead_and_unlisted() {
    assert_eq!(
        green_printed(RETURNED_UNJOINED_IS_DEAD_FORM),
        RETURNED_UNJOINED_IS_DEAD_EXPECT
    );
}

/// `finish_thread` leaves `Completing(Returned)` with join still unobserved.
/// `mark_thread_exited` has not run, so the phase is not `Terminal` yet.
/// GNU `thread_live_p` is already false in that window (`src/thread.c:76`,
/// cleared at `:822-823`, before join).
///
/// Lisp cannot see the window. The sim runner calls `finish_make_thread_result`
/// and `finish_sim_thread_registration` with no release between them, and a
/// green worker holds the baton from `finish_make_thread_result` through
/// `prepare_thread_completion`. No exit hook or process sentinel runs there.
#[test]
fn completing_returned_unobserved_is_dead_before_mark_exited() {
    crate::test_utils::init_test_tracing();
    let mut mgr = neovm_eval::emacs_core::threads::ThreadManager::new();
    let native = mgr
        .try_create_thread(
            Value::NIL,
            neovm_eval::emacs_core::threads::ThreadName::UNNAMED,
        )
        .expect("thread identity")
        .native_key();
    mgr.finish_thread(native, Value::fixnum(42));
    let state = mgr.get_thread(native).expect("still registered").native();
    assert_eq!(
        state.phase(),
        neovm_value::heap_types::threading::ThreadPhase::Completing(
            neovm_value::heap_types::threading::ThreadOutcome::Returned
        )
    );
    assert_eq!(
        state.join_observation(),
        neovm_value::heap_types::threading::JoinObservation::Unobserved
    );
    assert!(!mgr.thread_alive_p(native));
}
