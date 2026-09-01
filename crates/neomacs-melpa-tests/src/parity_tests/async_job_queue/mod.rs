use std::time::Duration;

use crate::{ASYNC_JOB_QUEUE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod dispatch;
mod lifecycle;
mod registry;
mod structures;
mod timers;
mod workflows;

const ASYNC_JOB_QUEUE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ASYNC_JOB_QUEUE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun async-job-queue-parity-wait-until
    (predicate)
  (let ((deadline
         (+ (float-time) 30)))
    (while
        (and
         (not
          (funcall predicate))
         (< (float-time)
            deadline))
      (accept-process-output
       nil 0.02))
    (unless
        (funcall predicate)
      (error
       "Timed out waiting for async-job-queue fixture"))
    t))

(defun async-job-queue-parity-table-state
    (table)
  (list
   :id
   (async-job-queue--table-id table)
   :active
   (async-job-queue--table-active table)
   :in-use
   (async-job-queue--table-in-use table)
   :free
   (async-job-queue--table-free table)
   :used-slots
   (async-job-queue--slots-in-use-list table)
   :free-slots
   (async-job-queue--slots-free-list table)
   :queued
   (queue-length
    (async-job-queue--table-queue table))
   :timer
   (and
    (async-job-queue--table-timer table)
    t)))

(defun async-job-queue-parity-job-state
    (job)
  (list
   :id
   (async-job-queue--job-id job)
   :table
   (and
    (async-job-queue--job-table job)
    (async-job-queue--table-id
     (async-job-queue--job-table job)))
   :run-slot
   (async-job-queue--job-run-slot job)
   :started
   (and
    (async-job-queue--job-started job)
    t)
   :future
   (async-job-queue--job-future job)
   :ended
   (and
    (async-job-queue--job-ended job)
    t)
   :returned
   (async-job-queue--job-returned job)
   :result
   (async-job-queue--job-result job)))

(defun async-job-queue-parity-normalized-table-state
    (table)
  (let ((state
         (async-job-queue-parity-table-state
          table)))
    (plist-put
     state
     :used-slots
     (sort
      (copy-sequence
       (plist-get state :used-slots))
      #'<))
    (plist-put
     state
     :free-slots
     (sort
      (copy-sequence
       (plist-get state :free-slots))
      #'<))
    state))
"##;

fn async_job_queue_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASYNC_JOB_QUEUE_MELPA_PIN, source_file)
        .expect("prepare pinned async-job-queue source below ./tmp")
        .with_prelude(ASYNC_JOB_QUEUE_TEST_PRELUDE)
        .with_timeout(ASYNC_JOB_QUEUE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed async-job-queue parity test")
        .into()
}

/// Multi-probe batch for `assert_async_job_queue_autoload_parity` cases (2a).
pub(crate) fn assert_async_job_queue_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_job_queue_oracle("async-job-queue-autoloads.el"),
        &name,
        "async_job_queue_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_async_job_queue_parity` cases (2a).
pub(crate) fn assert_async_job_queue_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_job_queue_oracle("async-job-queue.el"),
        &name,
        "async_job_queue_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn async_job_queue_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_async_job_queue_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_async_job_queue_autoload_batch(&cases);
}

#[test]
fn async_job_queue_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        dispatch::dispatch_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        registry::registry_async_job_queue_batch_cases(),
        structures::structures_public_surface_batch_cases(),
        timers::timers_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_async_job_queue_batch(&cases);
}

// END generated package batch tests
