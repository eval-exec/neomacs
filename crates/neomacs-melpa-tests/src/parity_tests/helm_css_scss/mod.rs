use std::time::Duration;

use crate::{CachedMelpaOracle, HELM_CSS_SCSS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_CSS_SCSS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HELM_CSS_SCSS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'css-mode)
(require 'less-css-mode)

(defconst hcss-test-nested-fixture
  "/* .disabled {
  color: gray;
} */

.dashboard,
.dashboard--compact {
  color: red;

  .card {
    padding: 1rem;

    &__title,
    &__subtitle {
      color: blue;
    }

    .toolbar[data-state=\"ready\"] {
      &:hover {
        opacity: .8;
      }
    }
  }
}

.footer {
  color: black;
}

@media (min-width: 80rem) {
  .wide-panel {
    display: grid;
  }
}
")

(defvar hcss-test-owned-buffers nil)

(defun hcss-test-buffer (name mode contents)
  "Create and own one deterministic editing buffer."
  (when (get-buffer name)
    (error "Helm CSS SCSS test buffer already exists: %s" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer hcss-test-owned-buffers)
    (with-current-buffer buffer
      (insert contents)
      (funcall mode))
    buffer))

(defun hcss-test-capture (function)
  "Return FUNCTION's exact value or nonlocal condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition) :data (cdr condition)))))

(defun hcss-test-face-runs ()
  "Return every applied face run in the current public output buffer."
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list position next face
                      (buffer-substring-no-properties position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun hcss-test-generated-comment-count ()
  "Count exact generated close-comment markers in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (let ((count 0))
      (while (search-forward "/*__" nil t)
        (setq count (1+ count)))
      count)))

(defun hcss-test-movement-state (command)
  "Call public movement COMMAND interactively and record exact state."
  (let ((value (call-interactively command)))
    (list :return value :message (current-message)
          :point (point) :line (line-number-at-pos)
          :column (current-column) :char-after (char-after))))

(defun hcss-test-session-advice-state ()
  "Return package temporary-session advice enablement."
  (mapcar
   (lambda (entry)
     (and (ad-advice-enabled
           (ad-find-advice (nth 0 entry) 'around (nth 1 entry))) t))
   '((helm-next-line helm-css-scss--next-line)
     (helm-previous-line helm-css-scss--previous-line)
     (helm-next-line helm-css-scss-multi--next-line)
     (helm-previous-line helm-css-scss-multi--previous-line)
     (helm-move--next-line-fn helm-css-scss--next-line-cycle)
     (helm-move--previous-line-fn helm-css-scss--previous-line-cycle))))

(defun hcss-test-clean-state (root)
  "Return final owned and package session state for ROOT."
  (list
   :owned-live (and (seq-some #'buffer-live-p hcss-test-owned-buffers) t)
   :root-exists (file-exists-p root)
   :overlay-buffer
   (and (overlayp helm-css-scss-overlay)
        (overlay-buffer helm-css-scss-overlay))
   :invisible-targets helm-css-scss-invisible-targets
   :session-advices (hcss-test-session-advice-state)
   :session-hook
   (and (memq #'helm-css-scss--keep-nearest-position
              helm-after-update-hook) t)
   :helm-alive (and helm-alive-p t)
   :cache-hook-count
   (cl-count #'helm-css-scss--clear-cache after-save-hook :test #'eq)))

(defun hcss-test-run (name function)
  "Run FUNCTION with fail-closed package-local ownership and cleanup."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root)
                 (not (string-empty-p sandbox-root))
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (let* ((root (file-name-as-directory (expand-file-name name sandbox-root)))
           (hcss-test-owned-buffers nil)
           (helm-css-scss-overlay nil)
           (helm-css-scss-invisible-targets nil)
           (helm-css-scss-last-point nil)
           (helm-css-scss-last-line-info nil)
           (helm-css-scss-target-buffer nil)
           (helm-css-scss-synchronizing-window nil)
           (helm-css-scss-move-line-action-last-buffer nil)
           result cleanup first-error)
      (when (file-exists-p root)
        (delete-directory root t))
      (make-directory root t)
      (condition-case condition
          (save-window-excursion
            (save-current-buffer
              (let ((default-directory root))
                (setq result (funcall function root)))))
        (error (setq first-error condition)))
      (condition-case condition
          (when (overlayp helm-css-scss-overlay)
            (delete-overlay helm-css-scss-overlay))
        (error (unless first-error (setq first-error condition))))
      (condition-case condition
          (helm-css-scss--restore-unveiled-overlay)
        (error (unless first-error (setq first-error condition))))
      (dolist (buffer hcss-test-owned-buffers)
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))
          (error (unless first-error (setq first-error condition)))))
      (condition-case condition
          (when (file-exists-p root)
            (delete-directory root t))
        (error (unless first-error (setq first-error condition))))
      (condition-case condition
          (setq cleanup (hcss-test-clean-state root))
        (error (unless first-error (setq first-error condition))))
      (setq hcss-test-owned-buffers nil)
      (when first-error
        (signal (car first-error) (cdr first-error)))
      (list :result result :cleanup cleanup))))
"####;

fn helm_css_scss_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact helm-css-scss source below ./tmp")
        .with_prelude(HELM_CSS_SCSS_TEST_PRELUDE)
        .with_timeout(HELM_CSS_SCSS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed helm-css-scss parity test")
        .into()
}

fn assert_helm_css_scss_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_css_scss_oracle(),
        &current_test_name(),
        "helm_css_scss_parity",
        cases,
    );
}

#[test]
fn helm_css_scss_package_batch() {
    assert_helm_css_scss_batch(&workflows::public_workflow_cases());
}
