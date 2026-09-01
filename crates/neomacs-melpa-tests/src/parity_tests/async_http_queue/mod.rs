use std::time::Duration;

use crate::{ASYNC_HTTP_QUEUE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod registry;
mod responses;
mod scheduling;
mod state;
mod workflows;

const ASYNC_HTTP_QUEUE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASYNC_HTTP_QUEUE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'json)
(require 'seq)
(require 'url)

(defun async-http-queue-test-error-data (thunk)
  (condition-case error-data
      (list :ok (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun async-http-queue-test-state
    (urls &optional max-concurrent timeout parser
          completion-callback error-callback)
  (async-http-queue--state-create
   :queue
   (mapcar
    (lambda (url)
      `((url . ,url)
        (status . pending)
        (data . nil)))
    urls)
   :active-workers 0
   :max-concurrent (or max-concurrent 5)
   :timeout (or timeout 10)
   :parser
   (if (eq parser :default)
       #'json-parse-buffer
     parser)
   :completion-callback completion-callback
   :error-callback error-callback))

(defun async-http-queue-test-queue-snapshot (state)
  (mapcar
   (lambda (item)
     (list
      (alist-get 'url item)
      (alist-get 'status item)
      (alist-get 'data item)))
   (async-http-queue--state-queue state)))

(defun async-http-queue-test-state-snapshot (state)
  (list
   :queue
   (async-http-queue-test-queue-snapshot state)
   :active
   (async-http-queue--state-active-workers state)
   :limit
   (async-http-queue--state-max-concurrent state)
   :timeout
   (async-http-queue--state-timeout state)
   :parser
   (cond
    ((eq (async-http-queue--state-parser state)
         #'json-parse-buffer)
     'json-parse-buffer)
    ((null
      (async-http-queue--state-parser state))
     nil)
    (t :custom))
   :completion
   (and
    (async-http-queue--state-completion-callback state)
    t)
   :error
   (and
    (async-http-queue--state-error-callback state)
    t)))

(defun async-http-queue-test-http-response
    (status-code body &optional line-ending reason)
  (let ((newline (or line-ending "\r\n")))
    (concat
     (format
      "HTTP/1.1 %d %s"
      status-code
      (or reason "Test"))
     newline
     "Content-Type: application/json"
     newline
     "X-Test: deterministic"
     newline
     newline
     body)))

(defun async-http-queue-test-response-buffer
    (name response)
  (let ((buffer
         (generate-new-buffer
          (concat " *async-http-queue-test-" name "*"))))
    (with-current-buffer buffer
      (insert response))
    buffer))

(defun async-http-queue-test-run-timer-event (event)
  (unless (aref event 6)
    (apply
     (aref event 4)
     (aref event 5))))

(defun async-http-queue-test-timer-summary (events)
  (mapcar
   (lambda (event)
     (list
      (aref event 1)
      (aref event 2)
      (aref event 3)
      (aref event 6)))
   events))

(defun async-http-queue-test-kill-buffer (buffer)
  (when (buffer-live-p buffer)
    (kill-buffer buffer)))
"##;

fn async_http_queue_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASYNC_HTTP_QUEUE_MELPA_PIN, source_file)
        .expect("prepare pinned async-http-queue source below ./tmp")
        .with_prelude(ASYNC_HTTP_QUEUE_TEST_PRELUDE)
        .with_timeout(ASYNC_HTTP_QUEUE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed async-http-queue parity test")
        .into()
}

/// Multi-probe batch for `assert_async_http_queue_autoload_parity` cases (2a).
pub(crate) fn assert_async_http_queue_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_http_queue_oracle("async-http-queue-autoloads.el"),
        &name,
        "async_http_queue_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_async_http_queue_parity` cases (2a).
pub(crate) fn assert_async_http_queue_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_http_queue_oracle("async-http-queue.el"),
        &name,
        "async_http_queue_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn async_http_queue_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_async_http_queue_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_async_http_queue_autoload_batch(&cases);
}

#[test]
fn async_http_queue_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        registry::registry_async_http_queue_batch_cases(),
        responses::responses_public_surface_batch_cases(),
        scheduling::scheduling_public_surface_batch_cases(),
        state::state_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_async_http_queue_batch(&cases);
}

// END generated package batch tests
