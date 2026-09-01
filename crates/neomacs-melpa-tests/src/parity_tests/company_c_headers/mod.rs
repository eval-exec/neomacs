use std::time::Duration;

use crate::{COMPANY_C_HEADERS_MELPA_PIN, COMPANY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const COMPANY_C_HEADERS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'cc-mode)
(require 'company)
(require 'company-c-headers)

(defvar cch354-test-path-calls nil)
(defvar cch354-test-user-path-value nil)
(defvar cch354-test-system-path-value nil)
(defvar cch354-test-provider-root nil)

(defun cch354-test-record-path-provider (provider value)
  "Record PROVIDER's exact public call context, then return VALUE."
  (unless (and (stringp cch354-test-provider-root)
               (file-name-absolute-p cch354-test-provider-root))
    (error "COMPANY-C-HEADERS provider lacks its owned absolute root"))
  (push (list :provider provider
              :mode major-mode
              :directory
              (cch354-test-relative default-directory
                                    cch354-test-provider-root)
              :line
              (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))
              :point (point))
        cch354-test-path-calls)
  value)

(defun cch354-test-user-path-provider ()
  "Return the configured user paths and record the public provider call."
  (cch354-test-record-path-provider
   'user cch354-test-user-path-value))

(defun cch354-test-system-path-provider ()
  "Return the configured system paths and record the public provider call."
  (cch354-test-record-path-provider
   'system cch354-test-system-path-value))

(defun cch354-test-failing-path-provider ()
  "Represent a user's failing project include-path provider."
  (cch354-test-record-path-provider 'failing nil)
  (error "owned project include provider unavailable"))

(defun cch354-test-write-file (root relative contents)
  "Write CONTENTS to owned RELATIVE path below ROOT."
  (let* ((file (expand-file-name relative root))
         (relative-file (file-relative-name file root)))
    (unless (and (not (file-name-absolute-p relative))
                 (not (equal relative-file ".."))
                 (not (string-prefix-p "../" relative-file)))
      (error "COMPANY-C-HEADERS fixture escaped owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (unless (file-in-directory-p (file-truename file) (file-truename root))
      (error "COMPANY-C-HEADERS fixture resolved outside owned root: %s" file))
    (write-region contents nil file nil 'silent)
    file))

(defun cch354-test-relative (path root)
  "Return PATH relative to ROOT, retaining a directory suffix."
  (when path
    (let ((relative (file-relative-name path root)))
      (if (and (string-suffix-p "/" path)
               (not (string-suffix-p "/" relative)))
          (concat relative "/")
        relative))))

(defun cch354-test-plain-candidates ()
  "Return active Company candidates without display properties."
  (mapcar #'substring-no-properties company-candidates))

(defun cch354-test-candidate (candidate root)
  "Describe CANDIDATE through the active public Company backend."
  (let* ((directory (company-call-backend 'meta candidate))
         (location (company-call-backend 'location candidate)))
    (list :text (substring-no-properties candidate)
          :directory (cch354-test-relative directory root)
          :location
          (list (cch354-test-relative (car location) root)
                (cdr location)))))

(defun cch354-test-prepare-company-buffer (mode contents)
  "Prepare current buffer for a real Company C Headers session."
  (switch-to-buffer (current-buffer))
  (funcall mode)
  (setq-local company-backends '(company-c-headers)
              company-frontends '(company-pseudo-tooltip-frontend
                                  company-echo-metadata-frontend)
              company-idle-delay nil
              company-minimum-prefix-length 0)
  (company-mode 1)
  (insert contents))

(defun cch354-test-capture (function)
  "Return FUNCTION's value or its exact signaled condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition)
             :data (cdr condition)
             :message (error-message-string condition)))))

(defun cch354-test-wait-for-company (predicate description)
  "Wait boundedly for PREDICATE, failing with Company state for DESCRIPTION."
  (let ((deadline (+ (float-time) 3.0)))
    (while (and (not (funcall predicate))
                (< (float-time) deadline))
      (accept-process-output nil 0.01))
    (unless (funcall predicate)
      (error "COMPANY-C-HEADERS timed out waiting for %s: prefix=%S candidates=%S timer=%S"
             description company-prefix
             (cch354-test-plain-candidates) company-timer))))

(defun cch354-test-run (name function)
  "Run FUNCTION in an owned filesystem and editor world named NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root)
                 (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" name)
      (error "COMPANY-C-HEADERS invalid owned case name: %S" name))
    (let* ((root (file-name-as-directory (expand-file-name name sandbox-root)))
           (root-owned nil)
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (current-buffer-baseline (current-buffer))
           (window-buffer-baseline (window-buffer))
           (this-command-baseline this-command)
           (company-timer-baseline company-timer)
           (emulation-maps-baseline (copy-sequence emulation-mode-map-alists))
           (electric-window-baseline company--electric-saved-window-configuration)
           (enable-local-variables nil)
           (enable-dir-local-variables nil)
           (enable-local-eval nil)
           (default-directory root)
           result body-error cleanup cleanup-errors)
      (when (file-exists-p root)
        (error "COMPANY-C-HEADERS owned case root already exists: %s" root))
      (cl-labels
          ((attempt
            (phase callback)
            (condition-case condition
                (funcall callback)
              (t (push (list phase condition) cleanup-errors) nil)))
           (kill-new-buffers
            (phase)
            (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
              (attempt
               phase
               (lambda ()
                 (when (buffer-live-p buffer)
                   (with-current-buffer buffer
                     (when (bound-and-true-p company-mode)
                       (when company-candidates (company-abort))
                       (company-mode -1))
                     (set-buffer-modified-p nil))
                   (kill-buffer buffer))))))
           (stop-new-processes
            (phase)
            (dolist (process (seq-difference (process-list) process-baseline #'eq))
              (attempt
               phase
               (lambda ()
                 (set-process-query-on-exit-flag process nil)
                 (when (process-live-p process) (delete-process process))))))
           (cancel-new-timers
            (phase)
            (dolist (timer (seq-difference timer-idle-list idle-timer-baseline #'eq))
              (attempt phase (lambda () (cancel-timer timer))))
            (dolist (timer (seq-difference timer-list timer-baseline #'eq))
              (attempt phase (lambda () (cancel-timer timer))))))
        (unwind-protect
            (condition-case condition
                (progn
                  (make-directory root)
                  (setq root-owned t)
                  (let ((this-command this-command-baseline)
                        (company-timer company-timer-baseline)
                        (emulation-mode-map-alists
                         (copy-sequence emulation-maps-baseline))
                        (company--electric-saved-window-configuration
                         electric-window-baseline))
                    (save-window-excursion
                      (save-current-buffer
                        (setq result (funcall function root))))))
              (t (setq body-error condition)))
          (kill-new-buffers 'buffers-first-sweep)
          (stop-new-processes 'processes-first-sweep)
          (cancel-new-timers 'timers-first-sweep)
          (kill-new-buffers 'buffers-second-sweep)
          (stop-new-processes 'processes-second-sweep)
          (cancel-new-timers 'timers-second-sweep)
          (kill-new-buffers 'buffers-final-sweep)
          (stop-new-processes 'processes-final-sweep)
          (cancel-new-timers 'timers-final-sweep)
          (attempt
           'root
           (lambda ()
             (when root-owned
               (when (file-exists-p root) (delete-directory root t))
               (unless (file-exists-p root) (setq root-owned nil)))))
          (attempt
           'state
           (lambda ()
             (setq cleanup
                   (list
                    :new-buffers
                    (delq nil
                          (mapcar (lambda (buffer)
                                    (and (buffer-live-p buffer)
                                         (buffer-name buffer)))
                                  (seq-difference (buffer-list)
                                                  buffer-baseline #'eq)))
                    :new-processes
                    (mapcar #'process-name
                            (seq-difference (process-list)
                                            process-baseline #'eq))
                    :new-timers
                    (+ (length (seq-difference timer-list timer-baseline #'eq))
                       (length (seq-difference timer-idle-list
                                               idle-timer-baseline #'eq)))
                    :root-exists (file-exists-p root)
                    :root-owned root-owned
                    :current-buffer-restored
                    (eq (current-buffer) current-buffer-baseline)
                    :window-restored
                    (eq (window-buffer) window-buffer-baseline)
                    :this-command-restored
                    (eq this-command this-command-baseline)
                    :company-timer-restored
                    (eq company-timer company-timer-baseline)
                    :emulation-maps-restored
                    (equal emulation-mode-map-alists emulation-maps-baseline)
                    :electric-window-restored
                    (equal company--electric-saved-window-configuration
                           electric-window-baseline)
                    :body-error body-error
                    :cleanup-errors (nreverse cleanup-errors))))))
        (let ((dirty
               (or body-error cleanup-errors
                   (plist-get cleanup :new-buffers)
                   (plist-get cleanup :new-processes)
                   (not (= (plist-get cleanup :new-timers) 0))
                   (plist-get cleanup :root-exists)
                   (plist-get cleanup :root-owned)
                   (not (plist-get cleanup :current-buffer-restored))
                   (not (plist-get cleanup :window-restored))
                   (not (plist-get cleanup :this-command-restored))
                   (not (plist-get cleanup :company-timer-restored))
                   (not (plist-get cleanup :emulation-maps-restored))
                   (not (plist-get cleanup :electric-window-restored)))))
          (when dirty
            (error "COMPANY-C-HEADERS world failed: body=%S cleanup=%S phase-errors=%S"
                   body-error cleanup cleanup-errors))
          (list :result result :cleanup cleanup))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_C_HEADERS_MELPA_PIN, "company-c-headers.el")
        .expect("prepare exact shallow Company C Headers source below ./tmp")
        .with_melpa_dependency(COMPANY_MELPA_PIN)
        .expect("prepare exact shallow Company dependency below ./tmp")
        .with_prelude(COMPANY_C_HEADERS_TEST_PRELUDE)
        .with_timeout(Duration::from_secs(240))
}

#[test]
fn company_c_headers_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "company-c-headers-package-batch",
        "Company C Headers",
        &workflows::workflow_batch_cases(),
    );
}
