use std::time::Duration;

use crate::{ASYNCLOOP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod errors;
mod lifecycle;
mod logging;
mod registry;
mod scheduling;
mod series;
mod timers;

const ASYNCLOOP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASYNCLOOP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defvar asyncloop-test-now 0)
(defvar asyncloop-test-next-id 0)
(defvar asyncloop-test-timer-queue nil)
(defvar asyncloop-test-cancelled nil)

(defun asyncloop-test-reset
    ()
  (asyncloop-reset-all)
  (setq asyncloop-test-now 0
        asyncloop-test-next-id 0
        asyncloop-test-timer-queue nil
        asyncloop-test-cancelled nil
        asyncloop-recursion-ctr 0))

(defun asyncloop-test-schedule
    (delay repeat function &rest arguments)
  (let* ((id
          (setq asyncloop-test-next-id
                (1+ asyncloop-test-next-id)))
         (due
          (+ asyncloop-test-now
             (or delay 0)))
         (timer
          (list :asyncloop-test-timer id))
         (event
          (list due id repeat function arguments timer)))
    (push event asyncloop-test-timer-queue)
    timer))

(defun asyncloop-test-cancel-timer
    (timer)
  (when
      (and
       (consp timer)
       (eq
        (car timer)
        :asyncloop-test-timer))
    (cl-pushnew
     (cadr timer)
     asyncloop-test-cancelled))
  nil)

(defun asyncloop-test-event-due
    (event)
  (nth 0 event))

(defun asyncloop-test-event-id
    (event)
  (nth 1 event))

(defun asyncloop-test-event-repeat
    (event)
  (nth 2 event))

(defun asyncloop-test-event-function
    (event)
  (nth 3 event))

(defun asyncloop-test-event-arguments
    (event)
  (nth 4 event))

(defun asyncloop-test-event-timer
    (event)
  (nth 5 event))

(defun asyncloop-test-event-before-p
    (left right)
  (or
   (<
    (asyncloop-test-event-due left)
    (asyncloop-test-event-due right))
   (and
    (=
     (asyncloop-test-event-due left)
     (asyncloop-test-event-due right))
    (<
     (asyncloop-test-event-id left)
     (asyncloop-test-event-id right)))))

(defun asyncloop-test-drain
    (&optional maximum-events)
  (let ((remaining
         (or maximum-events
             10000))
        trace)
    (while
        (and
         asyncloop-test-timer-queue
         (> remaining 0))
      (setq remaining
            (1- remaining)
            asyncloop-test-timer-queue
            (sort asyncloop-test-timer-queue
                  #'asyncloop-test-event-before-p))
      (let* ((event
              (pop asyncloop-test-timer-queue))
             (due
              (asyncloop-test-event-due event))
             (id
              (asyncloop-test-event-id event))
             (repeat
              (asyncloop-test-event-repeat event))
             (function
              (asyncloop-test-event-function event))
             (arguments
              (asyncloop-test-event-arguments event)))
        (setq asyncloop-test-now due)
        (if
            (memq id asyncloop-test-cancelled)
            (push
             (list :skipped :at due :id id)
             trace)
          (let ((entry
                 (list
                  :ran
                  :at due
                  :id id
                  :repeat repeat
                  :function
                  (if
                      (symbolp function)
                      function
                    :closure))))
            (push entry trace))
          (apply function arguments))))
    (nreverse trace)))

(defmacro asyncloop-test-with-scheduler
    (&rest body)
  `(cl-letf
       (((symbol-function 'run-with-idle-timer)
         #'asyncloop-test-schedule)
        ((symbol-function 'cancel-timer)
         #'asyncloop-test-cancel-timer)
        ((symbol-function 'input-pending-p)
         (lambda () nil)))
     ,@body))

(defun asyncloop-test-error
    (thunk)
  (condition-case error
      (list :ok
            (funcall thunk))
    ((error quit)
     (list
      :signal
      (car error)
      (cdr error)))))

(defun asyncloop-test-log-text
    (buffer)
  (when
      (buffer-live-p buffer)
    (with-current-buffer buffer
      (replace-regexp-in-string
       "[[:digit:]][[:digit:]]:[[:digit:]][[:digit:]]:[[:digit:]][[:digit:]]: "
       "<TIME>: "
       (buffer-substring-no-properties
        (point-min)
        (point-max))))))
"##;

fn asyncloop_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASYNCLOOP_MELPA_PIN, source_file)
        .expect("prepare revision-pinned asyncloop source below ./tmp")
        .with_prelude(ASYNCLOOP_TEST_PRELUDE)
        .with_timeout(ASYNCLOOP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed asyncloop parity test")
        .into()
}

/// Multi-probe batch for `assert_asyncloop_autoload_parity` cases (2a).
pub(crate) fn assert_asyncloop_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asyncloop_oracle("asyncloop-autoloads.el"),
        &name,
        "asyncloop_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_asyncloop_parity` cases (2a).
pub(crate) fn assert_asyncloop_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asyncloop_oracle("asyncloop.el"),
        &name,
        "asyncloop_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn asyncloop_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_asyncloop_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_asyncloop_autoload_batch(&cases);
}

#[test]
fn asyncloop_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        errors::errors_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        logging::logging_public_surface_batch_cases(),
        registry::registry_asyncloop_batch_cases(),
        scheduling::scheduling_public_surface_batch_cases(),
        series::series_public_surface_batch_cases(),
        timers::timers_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_asyncloop_batch(&cases);
}

// END generated package batch tests
