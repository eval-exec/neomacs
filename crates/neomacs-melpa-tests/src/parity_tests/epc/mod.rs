use std::time::Duration;

use crate::{CONCURRENT_MELPA_PIN, CTABLE_MELPA_PIN, CachedMelpaOracle, EPC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EPC_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const EPC_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'epc)
(require 'epcs)

;; Modern Emacs rejects `:server' together with `:nowait'.  The pinned
;; epcs.el still passes both; override the public server entry so the
;; rest of the package exercises real loopback RPC on current Emacsen.
(defun epcs:server-start (connect-function &optional port)
  "Start TCP Server and return the main process object."
  (lexical-let*
      ((connect-function connect-function)
       (name (format "EPC Server %s" (epc:uid)))
       (buf (epc:make-procbuf (format "*%s*" name)))
       (main-process
        (make-network-process
         :name name
         :buffer buf
         :family 'ipv4
         :server t
         :host "127.0.0.1"
         :service (or port t)
         :sentinel
         (lambda (process message)
           (epcs:sentinel process message connect-function)))))
    (unless port
      (message "%s\n" (process-contact main-process :service)))
    (push (cons main-process
                (make-epcs:server
                 :name name :process main-process
                 :port (process-contact main-process :service)
                 :connect-function connect-function))
          epcs:server-processes)
    main-process))

(defun neomacs-epc-test-with-loopback (connect-function function)
  "Start a local EPCS server, connect a debug client, and call FUNCTION.
FUNCTION receives (client-manager server-process)."
  (let* ((server-process (epcs:server-start connect-function t))
         (port (process-contact server-process :service))
         (client nil)
         (exit-events nil))
    (unwind-protect
        (progn
          (setq client (epc:start-epc-debug port))
          ;; Let the server accept the client and run CONNECT-FUNCTION.
          (let ((limit 200))
            (while (and (> limit 0) (null epcs:client-processes))
              (accept-process-output nil 0.01)
              (setq limit (1- limit)))
            (when (null epcs:client-processes)
              (error "EPC server never accepted the loopback client")))
          (neomacs-epc-test-pump 0.05)
          (funcall function client server-process))
      (when client
        (ignore-errors (epc:stop-epc client)))
      (when (and server-process (process-live-p server-process))
        (ignore-errors (epcs:server-stop server-process)))
      (ignore-errors (epcs:kill-all-processes)))))

(defun neomacs-epc-test-pump (&optional seconds)
  "Run process filters and deferred workers for about SECONDS."
  (let ((limit (max 1 (truncate (* (or seconds 0.05) 100)))))
    (while (> limit 0)
      (accept-process-output nil 0.01)
      (when (and (boundp 'deferred:queue) deferred:queue)
        (deferred:flush-queue!))
      (setq limit (1- limit)))))

;; Batch Emacs has no command loop, so pump every process and flush the
;; deferred queue instead of waiting only on the client socket.
(defun epc:sync (mngr d)
  "Wait for deferred D while driving the loopback connection."
  (let ((result 'epc:nothing))
    (deferred:$ d
      (deferred:nextc it
        (lambda (x) (setq result x)))
      (deferred:error it
        (lambda (er) (setq result (cons 'error er)))))
    (let ((limit 500))
      (while (and (eq result 'epc:nothing) (> limit 0))
        (accept-process-output nil 0.01)
        (when (and (boundp 'deferred:queue) deferred:queue)
          (deferred:flush-queue!))
        (setq limit (1- limit))))
    (when (eq result 'epc:nothing)
      (error "EPC call timed out"))
    (if (and (consp result) (eq 'error (car result)))
        (error "%s" (cdr result))
      result)))

(defun neomacs-epc-test-sync (client deferred)
  "Resolve DEFERRED via the batch-safe `epc:sync'."
  (epc:sync client deferred))

(defun neomacs-epc-test-call (client method args)
  "Synchronous peer call that normalizes errors into a tagged list."
  (condition-case err
      (list :ok (epc:call-sync client method args))
    (error (list :error (error-message-string err)))))
"####;

fn epc_oracle() -> CachedMelpaOracle {
    // Use installed autoloads rather than re-loading epc.el after the prelude:
    // the prelude must override `epcs:server-start` and `epc:sync` for modern
    // Emacs / batch process pumping, and a second source load would undo that.
    CachedMelpaOracle::new(EPC_MELPA_PIN, "epc.el")
        .expect("prepare exact shallow EPC source below ./tmp")
        .with_melpa_dependency(CONCURRENT_MELPA_PIN)
        .expect("prepare exact shallow concurrent dependency below ./tmp")
        .with_melpa_dependency(CTABLE_MELPA_PIN)
        .expect("prepare exact shallow ctable dependency below ./tmp")
        .with_installed_autoloads()
        .with_prelude(EPC_TEST_PRELUDE)
        .with_timeout(EPC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed epc parity test")
        .into()
}

fn assert_epc_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(epc_oracle(), &current_test_name(), "epc_parity", cases);
}

#[test]
fn epc_package_batch() {
    assert_epc_batch(&workflows::workflow_batch_cases());
}
