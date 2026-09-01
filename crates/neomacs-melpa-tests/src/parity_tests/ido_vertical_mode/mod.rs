use std::time::Duration;

use crate::{CachedMelpaOracle, IDO_VERTICAL_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const IDO_VERTICAL_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const IDO_VERTICAL_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'ido-vertical-mode)

(defvar neomacs-ido-vertical-test-observations nil)
(defvar neomacs-ido-vertical-test-deferred-events nil)
(defvar neomacs-ido-vertical-test-observation-p nil)
(defvar neomacs-ido-vertical-test-project-root nil)

(defun neomacs-ido-vertical-test-property-runs (string)
  "Return the exact non-nil face runs in rendered STRING."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (next-single-property-change
                    position 'face string (length string))))
        (when face
          (push (list position next
                      (substring-no-properties string position next)
                      (copy-tree face))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-ido-vertical-test-normalize-project (string)
  "Replace the isolated project root in STRING with a stable marker."
  (if (and neomacs-ido-vertical-test-project-root string)
      (replace-regexp-in-string
       (regexp-quote neomacs-ido-vertical-test-project-root)
       "<PROJECT>/" string t t)
    string))

(defun neomacs-ido-vertical-test-observe ()
  "Pause queued keys until IDO has rendered the current real minibuffer."
  (interactive)
  (when neomacs-ido-vertical-test-deferred-events
    (error "IDO Vertical observer still owns deferred events"))
  (setq neomacs-ido-vertical-test-deferred-events unread-command-events
        unread-command-events nil
        neomacs-ido-vertical-test-observation-p t
        ;; The sentinel is not a user operation.  Preserve command identity so
        ;; IDO's consecutive-RET confirmation protocol remains exact.
        this-command last-command))

(defun neomacs-ido-vertical-test-record-display ()
  "Record IDO's settled public minibuffer display after the sentinel key."
  (when (or neomacs-ido-vertical-test-observation-p
            ;; Confirmation exists only between the first and second RET.
            ;; Observe that real post-command state without injecting a
            ;; command that would break IDO's consecutive-RET protocol.
            (and (boundp 'ido-eoinput)
                 (integer-or-marker-p ido-eoinput)
                 (<= ido-eoinput (point-max))
                 (string-match-p
                  "\\[Confirm\\]"
                  (buffer-substring-no-properties ido-eoinput (point-max)))))
    (unless (and (boundp 'ido-eoinput)
                 (integer-or-marker-p ido-eoinput)
                 (<= (minibuffer-prompt-end) ido-eoinput)
                 (<= ido-eoinput (point-max)))
      (error "IDO did not establish a valid input/display boundary"))
    (unwind-protect
        (let* ((prompt
                (buffer-substring-no-properties
                 (point-min) (minibuffer-prompt-end)))
               (input
                (buffer-substring-no-properties
                 (minibuffer-prompt-end) ido-eoinput))
               (display (buffer-substring ido-eoinput (point-max))))
          (push
           (list
            :item ido-cur-item
            :prompt (neomacs-ido-vertical-test-normalize-project prompt)
            :input input
            :point (- (point) (minibuffer-prompt-end))
            :display (substring-no-properties display)
            :face-runs (neomacs-ido-vertical-test-property-runs display)
            :matches
            (mapcar (lambda (candidate)
                      (substring-no-properties (ido-name candidate)))
                    ido-matches)
            :regexp ido-enable-regexp
            :incomplete-regexp ido-incomplete-regexp
            :directory
            (and (memq ido-cur-item '(file dir))
                 neomacs-ido-vertical-test-project-root
                 (file-relative-name
                  ido-current-directory
                  neomacs-ido-vertical-test-project-root))
            :truncate-lines truncate-lines
            :keys
            (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                    '("C-n" "C-p" "<up>" "<down>"
                      "<left>" "<right>" "C-c C-t"))
            :message (and (current-message)
                          (substring-no-properties (current-message))))
           neomacs-ido-vertical-test-observations))
      (setq unread-command-events
            (append neomacs-ido-vertical-test-deferred-events
                    unread-command-events)
            neomacs-ido-vertical-test-deferred-events nil
            neomacs-ido-vertical-test-observation-p nil))))

(defun neomacs-ido-vertical-test-install-key ()
  "Bind the observation sentinel in IDO's newly-created session map."
  (define-key ido-completion-map (kbd "<f8>")
              #'neomacs-ido-vertical-test-observe))

(defun neomacs-ido-vertical-test-install-observer ()
  "Append the observer after IDO's real post-command renderer."
  (add-hook 'post-command-hook
            #'neomacs-ido-vertical-test-record-display t t))

(add-hook 'ido-setup-hook #'neomacs-ido-vertical-test-install-key)
(add-hook 'ido-minibuffer-setup-hook
          #'neomacs-ido-vertical-test-install-observer)

(defun neomacs-ido-vertical-test-finish (expected-count)
  "Return ordered observations, rejecting incomplete scripted sessions."
  (when unread-command-events
    (error "IDO Vertical workflow left unread events: %S"
           unread-command-events))
  (when neomacs-ido-vertical-test-deferred-events
    (error "IDO Vertical workflow left deferred events: %S"
           neomacs-ido-vertical-test-deferred-events))
  (when neomacs-ido-vertical-test-observation-p
    (error "IDO Vertical workflow left a pending observation"))
  (when (active-minibuffer-window)
    (error "IDO Vertical workflow left an active minibuffer"))
  (let ((observations (nreverse neomacs-ido-vertical-test-observations)))
    (unless (= (length observations) expected-count)
      (error "IDO Vertical expected %d observations, recorded %d"
             expected-count (length observations)))
    observations))

(defun neomacs-ido-vertical-test-call (function)
  "Call FUNCTION with public modes enabled and prove complete restoration."
  (when (or ido-mode ido-vertical-mode)
    (error "IDO modes leaked into a new workflow"))
  (let ((original-completions (symbol-function 'ido-completions))
        (original-decorations (copy-tree ido-decorations))
        (original-setup-hook (copy-sequence ido-setup-hook))
        (original-minibuffer-hook
         (copy-sequence ido-minibuffer-setup-hook))
        (ido-use-faces t)
        (ido-case-fold nil)
        (ido-enable-flex-matching nil)
        (ido-enable-prefix nil)
        (ido-max-prospects 4)
        (ido-vertical-indicator "界")
        (ido-vertical-padding "·")
        (ido-vertical-show-count t)
        (ido-vertical-pad-list t)
        (ido-vertical-disable-if-short nil)
        (ido-vertical-define-keys 'C-n-C-p-up-down-left-right)
        (ido-vertical-old-decorations nil)
        (ido-vertical-old-completions nil)
        (ido-vertical-decorations nil)
        (ido-vertical-count-active nil)
        (executing-kbd-macro t)
        (unread-command-events nil)
        (minibuffer-history nil)
        (ido-buffer-history nil)
        (ido-file-history nil)
        (file-name-history nil)
        (ido-completion-map nil)
        (ido-work-directory-list nil)
        (ido-last-directory-list nil)
        (ido-work-file-list nil)
        (ido-dir-file-cache nil)
        (ido-record-commands nil)
        (neomacs-ido-vertical-test-observations nil)
        (neomacs-ido-vertical-test-deferred-events nil)
        (neomacs-ido-vertical-test-observation-p nil)
        (neomacs-ido-vertical-test-project-root nil)
        result)
    (unwind-protect
        (progn
          (ido-mode 1)
          (ido-vertical-mode 1)
          (setq result
                (save-window-excursion
                  (save-current-buffer
                    (funcall function)))))
      (when ido-vertical-mode
        (ido-vertical-mode -1))
      (when ido-mode
        (ido-mode -1)))
    (unless (and (not ido-mode)
                 (not ido-vertical-mode)
                 (eq (symbol-function 'ido-completions)
                     original-completions)
                 (equal ido-decorations original-decorations)
                 (equal ido-setup-hook original-setup-hook)
                 (equal ido-minibuffer-setup-hook original-minibuffer-hook)
                 (null unread-command-events)
                 (null neomacs-ido-vertical-test-deferred-events)
                 (not neomacs-ido-vertical-test-observation-p)
                 (not (active-minibuffer-window)))
      (error "IDO Vertical workflow did not restore editor state"))
    (list :workflow result
          :cleanup '(:ido-mode nil
                     :ido-vertical-mode nil
                     :renderer restored
                     :decorations restored
                     :hooks restored
                     :events empty
                     :minibuffer inactive))))
"####;

fn ido_vertical_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IDO_VERTICAL_MODE_MELPA_PIN, "ido-vertical-mode.el")
        .expect("prepare exact shallow Ido Vertical Mode source below ./tmp")
        .with_prelude(IDO_VERTICAL_MODE_TEST_PRELUDE)
        .with_timeout(IDO_VERTICAL_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed ido-vertical-mode parity test")
        .into()
}

fn assert_ido_vertical_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        ido_vertical_mode_oracle(),
        &current_test_name(),
        "ido_vertical_mode_parity",
        cases,
    );
}

#[test]
fn ido_vertical_mode_package_batch() {
    assert_ido_vertical_mode_batch(&workflows::workflow_batch_cases());
}
