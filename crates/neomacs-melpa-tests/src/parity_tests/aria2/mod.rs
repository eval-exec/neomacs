use std::time::Duration;

use crate::{ARIA2_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARIA2_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ARIA2_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-aria2-test-rpc-request
    (url silent)
  (unless
      (and
       (equal
        url
        "http://localhost:6800/jsonrpc")
       silent
       (equal
        url-request-method
        "POST")
       (equal
        url-request-extra-headers
        '(("Content-Type" . "application/json"))))
    (error
     "Unexpected aria2 RPC transport: %S"
     (list
      url
      silent
      url-request-method
      url-request-extra-headers)))
  (let ((request
         (json-read-from-string
          url-request-data)))
    (list
     (alist-get 'id request)
     (alist-get 'method request)
     (append
      (alist-get 'params request)
      nil))))

(defun neomacs-aria2-test-rpc-response
    (request result)
  (let ((buffer
         (generate-new-buffer
          " *neomacs-aria2-rpc-response*")))
    (with-current-buffer buffer
      (insert
       "HTTP/1.1 200 OK\n"
       "Content-Type: application/json\n"
       "\n"
       (json-encode
        `((jsonrpc . "2.0")
          (id . ,(car request))
          (result . ,result)))))
    buffer))

(defun neomacs-aria2-test-cleanup ()
  (when (timerp aria2--master-timer)
    (cancel-timer aria2--master-timer))
  (when (timerp aria2--refresh-timer)
    (cancel-timer aria2--refresh-timer))
  (setq aria2--master-timer nil
        aria2--refresh-timer nil
        aria2--current-buffer-refresh-speed nil
        aria2--url-list-widget nil)
  (dolist (name
           (list
            aria2-list-buffer-name
            aria2-url-list-buffer-name))
    (when-let ((buffer
                (get-buffer name)))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##;

fn aria2_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARIA2_MELPA_PIN, "aria2.el")
        .expect("prepare pinned aria2 source below ./tmp")
        .with_prelude(ARIA2_TEST_PRELUDE)
        .with_timeout(ARIA2_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aria2 parity test").into()
}

/// Multi-probe batch for `assert_aria2_parity` cases (2a).
pub(crate) fn assert_aria2_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aria2_oracle(), &name, "aria2_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aria2_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aria2_batch(&cases);
}

// END generated package batch tests
