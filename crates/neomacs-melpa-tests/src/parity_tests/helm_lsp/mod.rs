use std::time::Duration;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, HELM_LSP_MELPA_PIN, HELM_MELPA_PIN, LSP_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_LSP_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const HELM_LSP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'helm-lsp)

(defvar neomacs-helm-lsp-test-request-log nil)
(defvar neomacs-helm-lsp-test-display nil)
(defvar neomacs-helm-lsp-test-display-log nil)
(defvar neomacs-helm-lsp-test-pattern nil)
(defvar neomacs-helm-lsp-test-selection-log nil)
(defvar neomacs-helm-lsp-test-pending-symbol-response nil)
(defvar neomacs-helm-lsp-test-symbol-request-state-log nil)

(defun neomacs-helm-lsp-test-capture (function)
  "Return FUNCTION's value or its complete signaled condition."
  (condition-case error-data
      (list :value (funcall function))
    (error
     (list :signal (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-helm-lsp-test-root (name)
  "Create a deterministic sandbox directory for workflow NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "helm-lsp-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-helm-lsp-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-helm-lsp-test-range (line character)
  "Return a zero-width LSP range at LINE and CHARACTER."
  (lsp-make-range
   :start (lsp-make-position :line line :character character)
   :end (lsp-make-position :line line :character character)))

(defun neomacs-helm-lsp-test-symbol (name kind container path line character)
  "Return a real LSP symbol located at PATH."
  (lsp-make-symbol-information
   :name name
   :kind kind
   :container-name? container
   :location
   (lsp-make-location
    :uri (lsp--path-to-uri path)
    :range (neomacs-helm-lsp-test-range line character))))

(defun neomacs-helm-lsp-test-code-action
    (title path line character new-text)
  "Return a real LSP code action inserting NEW-TEXT into PATH."
  (let ((changes (make-hash-table :test 'equal)))
    (puthash
     (lsp--path-to-uri path)
     (vector
      (lsp-make-text-edit
       :range (neomacs-helm-lsp-test-range line character)
       :new-text new-text))
     changes)
    (lsp-make-code-action
     :title title
     :kind? "quickfix.release"
     :edit? (lsp-make-workspace-edit :changes? changes))))

(defun neomacs-helm-lsp-test-position-shape (position)
  "Return POSITION as a stable line and character pair."
  (list (plist-get position :line)
        (plist-get position :character)))

(defun neomacs-helm-lsp-test-code-action-request (root method params)
  "Describe a code-action METHOD and PARAMS relative to ROOT."
  (let* ((document (plist-get params :textDocument))
         (range (plist-get params :range))
         (context (plist-get params :context)))
    (list :method method
          :file
          (file-relative-name
           (lsp--uri-to-path
            (plist-get document :uri))
           root)
          :range
          (list (neomacs-helm-lsp-test-position-shape
                 (plist-get range :start))
                (neomacs-helm-lsp-test-position-shape
                 (plist-get range :end)))
          :diagnostic-count
          (length (plist-get context :diagnostics)))))

(defun neomacs-helm-lsp-test-diagnostic
    (message source severity line character)
  "Return a real LSP diagnostic at LINE and CHARACTER."
  (lsp-make-diagnostic
   :message message
   :source? source
   :severity? severity
   :range (neomacs-helm-lsp-test-range line character)))

(defun neomacs-helm-lsp-test-face-runs (text)
  "Describe every non-nil face interval in TEXT."
  (let ((position 0)
        runs)
    (while (< position (length text))
      (let* ((face (get-text-property position 'face text))
             (next (or (next-single-property-change
                        position 'face text (length text))
                       (length text))))
        (when face
          (push (list :text
                      (substring-no-properties text position next)
                      :face face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-helm-lsp-test-record-display ()
  "Record the complete visible Helm buffer after an update."
  (let ((text (buffer-substring (point-min) (point-max))))
    (setq neomacs-helm-lsp-test-pattern
          (substring-no-properties helm-pattern))
    (setq neomacs-helm-lsp-test-display
          (list :text (substring-no-properties text)
                :faces (neomacs-helm-lsp-test-face-runs text)))
    (unless (equal neomacs-helm-lsp-test-display
                   (car neomacs-helm-lsp-test-display-log))
      (push neomacs-helm-lsp-test-display
            neomacs-helm-lsp-test-display-log))))

(defun neomacs-helm-lsp-test-record-selection ()
  "Record the user-visible candidate selected by Helm."
  (when (overlayp helm-selection-overlay)
    (let ((text (buffer-substring
                 (overlay-start helm-selection-overlay)
                 (overlay-end helm-selection-overlay))))
      (push (list :text (string-trim-right
                         (substring-no-properties text))
                  :faces (neomacs-helm-lsp-test-face-runs text))
            neomacs-helm-lsp-test-selection-log))))

(defun neomacs-helm-lsp-test-process-input ()
  "Run the input transition normally driven by Helm's idle timer."
  (interactive)
  (helm-check-minibuffer-input))

(defun neomacs-helm-lsp-test-deliver-symbol-response ()
  "Deliver the pending deterministic language-server response."
  (interactive)
  (let ((response neomacs-helm-lsp-test-pending-symbol-response))
    (unless response
      (error "No workspace-symbol response is pending"))
    (setq neomacs-helm-lsp-test-pending-symbol-response nil)
    (let ((before-delivery helm-lsp-symbols-request-id))
      (funcall (car response) (cdr response))
      (push (list :before-delivery before-delivery
                  :after-delivery helm-lsp-symbols-request-id)
            neomacs-helm-lsp-test-symbol-request-state-log))))

(defun neomacs-helm-lsp-test-location (root)
  "Describe the selected source location relative to ROOT."
  (list :file (and buffer-file-name
                   (file-relative-name buffer-file-name root))
        :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun neomacs-helm-lsp-test-selected-location (root)
  "Describe the source location displayed in the selected window."
  (let* ((window (selected-window))
         (buffer (window-buffer window)))
    (with-current-buffer buffer
      (save-excursion
        (goto-char (window-point window))
        (neomacs-helm-lsp-test-location root)))))

(defun neomacs-helm-lsp-test-cleanup (root)
  "Kill buffers visiting ROOT, clear observations, and remove ROOT."
  (dolist (buffer (buffer-list))
    (when (and (buffer-live-p buffer)
               (buffer-file-name buffer)
               (string-prefix-p root
                                (expand-file-name
                                 (buffer-file-name buffer))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (when (file-exists-p root)
    (delete-directory root t))
  (setq neomacs-helm-lsp-test-request-log nil
        neomacs-helm-lsp-test-display nil
        neomacs-helm-lsp-test-display-log nil
        neomacs-helm-lsp-test-pattern nil
        neomacs-helm-lsp-test-selection-log nil
        neomacs-helm-lsp-test-pending-symbol-response nil
        neomacs-helm-lsp-test-symbol-request-state-log nil))
"####;

fn helm_lsp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_LSP_MELPA_PIN, "helm-lsp.el")
        .expect("prepare exact shallow Helm LSP source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow Dash dependency below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare exact shallow Helm dependency below ./tmp")
        .with_melpa_dependency(LSP_MODE_MELPA_PIN)
        .expect("prepare exact shallow LSP Mode dependency below ./tmp")
        .with_prelude(HELM_LSP_TEST_PRELUDE)
        .with_timeout(HELM_LSP_TEST_TIMEOUT)
}

fn assert_helm_lsp_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_lsp_oracle(),
        "helm-lsp-package-batch",
        "helm_lsp_parity",
        cases,
    );
}

#[test]
fn helm_lsp_package_batch() {
    assert_helm_lsp_batch(&workflows::workflow_batch_cases());
}
