use std::time::Duration;

use crate::{ASYNC1_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod construction;
mod pipelines;
mod plist;
mod registry;
mod timers;

const ASYNC1_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ASYNC1_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

;; A deterministic virtual timer queue exercises the package's real callback
;; topology without wall-clock races.  Dedicated timer tests below still use
;; GNU Emacs/Neomacs `run-at-time' and their actual event loops.
(defvar async1-test-now 0)
(defvar async1-test-next-id 0)
(defvar async1-test-timer-queue nil)

(defun async1-test-reset-scheduler
    ()
  (setq async1-test-now 0
        async1-test-next-id 0
        async1-test-timer-queue nil))

(defun async1-test-schedule
    (delay repeat function &rest arguments)
  (let* ((id
          (setq async1-test-next-id
                (1+ async1-test-next-id)))
         (due
          (+ async1-test-now delay))
         (event
          (list due id repeat function arguments)))
    (push event async1-test-timer-queue)
    (list :async1-test-timer id)))

(defun async1-test-event-before-p
    (left right)
  (or
   (< (nth 0 left)
      (nth 0 right))
   (and
    (= (nth 0 left)
       (nth 0 right))
    (< (nth 1 left)
       (nth 1 right)))))

(defun async1-test-drain
    ()
  (let (trace)
    (while async1-test-timer-queue
      (setq async1-test-timer-queue
            (sort async1-test-timer-queue
                  #'async1-test-event-before-p))
      (let* ((event
              (pop async1-test-timer-queue))
             (due
              (nth 0 event))
             (id
              (nth 1 event))
             (repeat
              (nth 2 event))
             (function
              (nth 3 event))
             (arguments
              (nth 4 event)))
        (setq async1-test-now due)
        (push
         (list
          :at due
          :id id
          :repeat repeat
          :function
          (if
              (symbolp function)
              function
            :closure)
          :arguments arguments)
         trace)
        (apply function arguments)))
    (nreverse trace)))

(defun async1-test-await
    (predicate timeout)
  (let ((deadline
         (+ (float-time)
            timeout)))
    (while
        (and
         (not
          (funcall predicate))
         (< (float-time)
            deadline))
      (sit-for 0.005))
    (funcall predicate)))

(defun async1-test-error
    (thunk)
  (condition-case error
      (list :ok
            (funcall thunk))
    (error
     (list
      :error
      (car error)
      (cdr error)))))
"##;

fn async1_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASYNC1_MELPA_PIN, source_file)
        .expect("prepare revision-pinned async1 source below ./tmp")
        .with_prelude(ASYNC1_TEST_PRELUDE)
        .with_timeout(ASYNC1_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed async1 parity test").into()
}

/// Multi-probe batch for `assert_async1_autoload_parity` cases (2a).
pub(crate) fn assert_async1_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async1_oracle("async1-autoloads.el"),
        &name,
        "async1_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_async1_parity` cases (2a).
pub(crate) fn assert_async1_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(async1_oracle("async1.el"), &name, "async1_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn async1_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_async1_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_async1_autoload_batch(&cases);
}

#[test]
fn async1_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        construction::construction_public_surface_batch_cases(),
        pipelines::pipelines_public_surface_batch_cases(),
        plist::plist_public_surface_batch_cases(),
        registry::registry_async1_batch_cases(),
        timers::timers_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_async1_batch(&cases);
}

// END generated package batch tests
