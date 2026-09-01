use std::time::Duration;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, VERTICO_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const VERTICO_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const VERTICO_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'vertico)

(defvar neomacs-vertico-test-observations nil)
(defvar neomacs-vertico-test-minibuffer-messages nil)
(defvar-local neomacs-vertico-test-deferred-events nil)

(defconst neomacs-vertico-test-service-candidates
  '("api-gateway" "api-worker" "app-console"))

(defun neomacs-vertico-test-service-annotation (candidate)
  "Return a realistic environment annotation for CANDIDATE."
  (if (string-prefix-p "api-" candidate)
      "  [production API]"
    "  [operator UI]"))

(defun neomacs-vertico-test-service-group (candidate transform)
  "Group service CANDIDATE, preserving it when TRANSFORM is non-nil."
  (if transform
      candidate
    (if (string-prefix-p "api-" candidate)
        "API services"
      "Applications")))

(defun neomacs-vertico-test-service-table (string predicate action)
  "Completion table with real annotations and service groups."
  (if (eq action 'metadata)
      '(metadata
        (annotation-function . neomacs-vertico-test-service-annotation)
        (group-function . neomacs-vertico-test-service-group))
    (complete-with-action
     action neomacs-vertico-test-service-candidates string predicate)))

(defun neomacs-vertico-test-face-segments (string face)
  "Return exact segments of STRING carrying FACE."
  (let ((position 0)
        segment-start
        segments)
    (while (< position (length string))
      (let* ((next (next-single-property-change
                    position 'face string (length string)))
             (faces (ensure-list (get-text-property position 'face string))))
        (cond
         ((and (memq face faces) (not segment-start))
          (setq segment-start position))
         ((and segment-start (not (memq face faces)))
          (push (list segment-start position
                      (substring-no-properties string segment-start position))
                segments)
          (setq segment-start nil)))
        (setq position next)))
    (when segment-start
      (push (list segment-start (length string)
                  (substring-no-properties string segment-start))
            segments))
    (nreverse segments)))

(defun neomacs-vertico-test-observe ()
  "Pause queued input so the normal Vertico post-command display can settle."
  (interactive)
  (setq neomacs-vertico-test-deferred-events unread-command-events
        unread-command-events nil))

(defun neomacs-vertico-test-observe-minibuffer-message (original &rest args)
  "Call ORIGINAL with ARGS and record its real minibuffer overlay text."
  (prog1 (apply original args)
    (when (and (minibufferp (current-buffer) t)
               (overlayp minibuffer--message-overlay))
      (push (substring-no-properties
             (or (overlay-get minibuffer--message-overlay 'after-string) ""))
            neomacs-vertico-test-minibuffer-messages))))

(defun neomacs-vertico-test-record-display ()
  "Record Vertico's settled UI after the observation command."
  (when (eq this-command 'neomacs-vertico-test-observe)
    (unwind-protect
  (let* ((display (or (overlay-get vertico--candidates-ov 'before-string) ""))
         (count (or (overlay-get vertico--count-ov 'before-string) "")))
    (push
     (list :prompt (buffer-substring-no-properties
                    (point-min) (minibuffer-prompt-end))
           :input (minibuffer-contents-no-properties)
           :point (- (point) (minibuffer-prompt-end))
           :index vertico--index
           :total vertico--total
           :count (substring-no-properties count)
           :display (substring-no-properties display)
           :current (neomacs-vertico-test-face-segments display 'vertico-current)
           :semantic-faces
           (delq nil
                 (mapcar
                  (lambda (face)
                    (when-let* ((segments
                                 (neomacs-vertico-test-face-segments
                                  display face)))
                      (cons face segments)))
                  '(completions-annotations
                    vertico-group-title
                    vertico-group-separator)))
           :return-command (key-binding (kbd "RET"))
           :tab-command (key-binding (kbd "TAB"))
           :next-command (key-binding (kbd "C-n"))
           :message (and (current-message)
                         (substring-no-properties (current-message))))
     neomacs-vertico-test-observations))
      (setq unread-command-events
            (append neomacs-vertico-test-deferred-events unread-command-events)
            neomacs-vertico-test-deferred-events nil))))

(defun neomacs-vertico-test-install-observer ()
  "Install the post-command observer in the active minibuffer."
  (add-hook 'post-command-hook #'neomacs-vertico-test-record-display t t))

(defmacro neomacs-vertico-test-with-mode (&rest body)
  "Run BODY with a real, isolated Vertico completion UI."
  (declare (indent 0) (debug t))
  `(let ((vertico-was-enabled vertico-mode)
         (vertico-map (copy-keymap vertico-map))
         (vertico-count 4)
         (vertico-scroll-margin 1)
         (vertico-preselect 'first)
         (vertico-cycle t)
         (vertico-sort-override-function #'identity)
         (completion-styles '(basic))
         (completion-category-defaults nil)
         (completion-category-overrides nil)
         (executing-kbd-macro t)
         (unread-command-events nil)
         (minibuffer-history nil)
         (file-name-history nil)
         (neomacs-vertico-test-observations nil)
         (neomacs-vertico-test-minibuffer-messages nil))
     (define-key vertico-map (kbd "<f8>") #'neomacs-vertico-test-observe)
     (unwind-protect
         (progn
           (advice-add 'minibuffer-message :around
                       #'neomacs-vertico-test-observe-minibuffer-message)
           (unwind-protect
               (progn
                 (unless vertico-was-enabled
                   (vertico-mode 1))
                 ,@body)
             (advice-remove
              'minibuffer-message
              #'neomacs-vertico-test-observe-minibuffer-message)))
       (unless vertico-was-enabled
         (vertico-mode -1)))))
"####;

fn vertico_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(VERTICO_MELPA_PIN, "vertico.el")
        .expect("prepare exact shallow Vertico source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact shallow compat dependency below ./tmp")
        .with_prelude(VERTICO_TEST_PRELUDE)
        .with_timeout(VERTICO_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed vertico parity test")
        .into()
}

fn assert_vertico_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        vertico_oracle(),
        &current_test_name(),
        "vertico_parity",
        cases,
    );
}

#[test]
fn vertico_package_batch() {
    assert_vertico_batch(&workflows::workflow_batch_cases());
}
