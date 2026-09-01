use std::time::Duration;

use crate::{ANYBAR_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANYBAR_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ANYBAR_TEST_PRELUDE: &str = r##"
(setq
 neomacs-anybar-test-events nil
 neomacs-anybar-test-instances nil
 neomacs-anybar-test-connections nil)

(defun neomacs-anybar-test-reset ()
  (setq
   neomacs-anybar-test-events nil
   neomacs-anybar-test-instances nil
   neomacs-anybar-test-connections nil))

(defun neomacs-anybar-test-instance (port)
  (cdr (assq port neomacs-anybar-test-instances)))

(defun neomacs-anybar-test-shell-command
    (command &optional output-buffer error-buffer)
  (unless
      (string-match
       "\\`ANYBAR_PORT=\\([0-9]+\\) open -n \\(.+\\)\\'"
       command)
    (error "Invalid AnyBar launch command: %s" command))
  (let ((port (string-to-number (match-string 1 command)))
        (application (match-string 2 command)))
    (setq neomacs-anybar-test-instances
          (assq-delete-all port neomacs-anybar-test-instances))
    (push
     (cons
      port
      (list
       :application application
       :style "white"))
     neomacs-anybar-test-instances)
    (push
     (list
      'launch
      :port port
      :application application
      :output-buffer output-buffer
      :error-buffer error-buffer)
     neomacs-anybar-test-events)
    0))

(defun neomacs-anybar-test-make-network-process (&rest arguments)
  (unless
      (equal
       arguments
       (list
        :name "anybar"
        :type 'datagram
        :host 'local
        :service (plist-get arguments :service)))
    (error "Invalid AnyBar datagram connection: %S" arguments))
  (let ((connection
         (list
          :name (plist-get arguments :name)
          :type (plist-get arguments :type)
          :host (plist-get arguments :host)
          :port (plist-get arguments :service)
          :deleted nil)))
    (push connection neomacs-anybar-test-connections)
    (push
     (list
      'connect
      :name (plist-get connection :name)
      :type (plist-get connection :type)
      :host (plist-get connection :host)
      :port (plist-get connection :port))
     neomacs-anybar-test-events)
    connection))

(defun neomacs-anybar-test-process-send-string (connection command)
  (unless
      (memq connection neomacs-anybar-test-connections)
    (error "Unknown AnyBar connection: %S" connection))
  (when
      (plist-get connection :deleted)
    (error "AnyBar connection is already closed"))
  (let* ((port (plist-get connection :port))
         (instance (neomacs-anybar-test-instance port)))
    (push
     (list
      'send
      :port port
      :command command)
     neomacs-anybar-test-events)
    (cond
     ((equal command "quit")
      (setq neomacs-anybar-test-instances
            (assq-delete-all port neomacs-anybar-test-instances)))
     (instance
      (plist-put instance :style command))))
  nil)

(defun neomacs-anybar-test-delete-process (connection)
  (unless
      (memq connection neomacs-anybar-test-connections)
    (error "Unknown AnyBar connection: %S" connection))
  (plist-put connection :deleted t)
  (push
   (list
    'close
    :port (plist-get connection :port))
   neomacs-anybar-test-events)
  nil)

(defun neomacs-anybar-test-call-with-boundary (function)
  (cl-letf
      (((symbol-function 'shell-command)
        #'neomacs-anybar-test-shell-command)
       ((symbol-function 'make-network-process)
        #'neomacs-anybar-test-make-network-process)
       ((symbol-function 'process-send-string)
        #'neomacs-anybar-test-process-send-string)
       ((symbol-function 'delete-process)
        #'neomacs-anybar-test-delete-process))
    (funcall function)))

(defun neomacs-anybar-test-state ()
  (sort
   (mapcar
    (lambda (entry)
      (let ((port (car entry))
            (instance (cdr entry)))
        (list
         :port port
         :application (plist-get instance :application)
         :style (plist-get instance :style))))
    neomacs-anybar-test-instances)
   (lambda (left right)
     (< (plist-get left :port)
        (plist-get right :port)))))

(defun neomacs-anybar-test-events ()
  (reverse neomacs-anybar-test-events))
"##;

fn anybar_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANYBAR_MELPA_PIN, "anybar.el")
        .expect("prepare pinned anybar source below ./tmp")
        .with_prelude(ANYBAR_TEST_PRELUDE)
        .with_timeout(ANYBAR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed anybar parity test").into()
}

/// Multi-probe batch for `assert_anybar_parity` cases (2a).
pub(crate) fn assert_anybar_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anybar_oracle(), &name, "anybar_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anybar_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anybar_batch(&cases);
}

// END generated package batch tests
