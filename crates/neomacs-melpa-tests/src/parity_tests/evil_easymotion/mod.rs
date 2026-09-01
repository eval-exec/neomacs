use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_EASYMOTION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_EASYMOTION_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const EVIL_EASYMOTION_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defvar neomacs-eem-test-hook-trace nil)

(defun neomacs-eem-test-forward-ticket ()
  "Move to the next TICKET marker without selecting the same marker twice."
  (interactive)
  (when (looking-at "TICKET")
    (forward-char 1))
  (search-forward "TICKET")
  (goto-char (match-beginning 0)))

(defun neomacs-eem-test-forward-alert ()
  "Move to the next ALERT marker in the current displayed buffer."
  (interactive)
  (when (looking-at "ALERT")
    (forward-char 1))
  (search-forward "ALERT")
  (goto-char (match-beginning 0)))

(defun neomacs-eem-test-setup ()
  "Install the README configuration and test motions after package activation."
  ;; `CachedPackageOracle::with_prelude' evaluates this function definition
  ;; before it loads the selected source.  Delaying macro evaluation until a
  ;; probe calls this function avoids loading Evil Easymotion twice.
  (evilem-default-keybindings "SPC")
  (unless (fboundp 'neomacs-eem-test-visible-ticket)
    (eval
     '(evilem-make-motion-plain
       neomacs-eem-test-visible-ticket #'neomacs-eem-test-forward-ticket
       :scope 'line
       :initial-point #'line-beginning-position
       :pre-hook (push (list :pre (point)) neomacs-eem-test-hook-trace)
       :post-hook (push (list :post (point)) neomacs-eem-test-hook-trace))))
  (unless (fboundp 'neomacs-eem-test-any-ticket)
    (eval
     '(evilem-make-motion-plain
       neomacs-eem-test-any-ticket #'neomacs-eem-test-forward-ticket
       :scope 'line
       :initial-point #'line-beginning-position
       :include-invisible t)))
  (unless (fboundp 'neomacs-eem-test-alert-all-windows)
    (eval
     '(evilem-define
       (kbd "C-c a") #'neomacs-eem-test-forward-alert
       :name neomacs-eem-test-alert-all-windows
       :all-windows t
       :initial-point #'point-min
       :pre-hook (push (list :pre (buffer-name) (point))
                       neomacs-eem-test-hook-trace)
       :post-hook (push (list :post (buffer-name) (point))
                        neomacs-eem-test-hook-trace))))
  ;; Reinstall the public binding if a workflow restored or replaced it.
  (define-key evil-normal-state-map (kbd "C-c a")
              #'neomacs-eem-test-alert-all-windows))

(defun neomacs-eem-test-with-buffer (name text callback)
  "Display TEXT in a fresh NAME buffer and call CALLBACK in Evil normal state."
  (let ((buffer (generate-new-buffer (format " *evil-easymotion-%s*" name))))
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer buffer)
          (insert text)
          (goto-char (point-min))
          (evil-local-mode 1)
          (evil-normal-state)
          (funcall callback buffer))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun neomacs-eem-test-current-line ()
  "Return the current logical line without text properties."
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position)))

(defun neomacs-eem-test-candidates ()
  "Return stable buffer, position, line, and text data for Avy's candidates."
  (mapcar
   (lambda (candidate)
     (let ((position (if (consp (car candidate))
                         (caar candidate)
                       (car candidate)))
           (window (cdr candidate)))
       (with-current-buffer (window-buffer window)
         (save-excursion
           (goto-char position)
           (list :buffer (buffer-name)
                 :position position
                 :line (line-number-at-pos)
                 :column (current-column)
                 :text (neomacs-eem-test-current-line))))))
   avy-last-candidates))

(defvar neomacs-eem-test-candidates-before-action nil)

(defun neomacs-eem-test-observe-pre-action (result)
  "Record candidates immediately before RESULT is acted on, then delegate."
  (setq neomacs-eem-test-candidates-before-action
        (neomacs-eem-test-candidates))
  (avy-pre-action-default result))

(defvar neomacs-eem-test-label-snapshots nil)

(defun neomacs-eem-test-live-labels ()
  "Describe the real Avy label overlays that are waiting for a key."
  (let (buffers labels)
    (dolist (candidate avy-last-candidates)
      (cl-pushnew (window-buffer (cdr candidate)) buffers))
    (dolist (buffer buffers)
      (with-current-buffer buffer
        (dolist (overlay (overlays-in (point-min) (point-max)))
          (when (eq (overlay-get overlay 'category) 'avy)
            (let ((rendered (or (overlay-get overlay 'display)
                                (overlay-get overlay 'after-string)
                                "")))
              (push (list :buffer (buffer-name buffer)
                          :position (overlay-start overlay)
                          :rendered (if (stringp rendered)
                                        (substring-no-properties rendered)
                                      rendered))
                    labels))))))
    (sort labels
          (lambda (left right)
            (or (string< (plist-get left :buffer)
                         (plist-get right :buffer))
                (and (equal (plist-get left :buffer)
                            (plist-get right :buffer))
                     (< (plist-get left :position)
                        (plist-get right :position))))))))

(defun neomacs-eem-test-observe-input (key)
  "Record Avy's live label map for KEY and delegate the key unchanged."
  (push (list :key key :labels (neomacs-eem-test-live-labels))
        neomacs-eem-test-label-snapshots)
  (identity key))

(defun neomacs-eem-test-ring ()
  "Return stable origins from the live Avy navigation ring."
  (let (result)
    (dotimes (index (ring-length avy-ring) (nreverse result))
      (let* ((entry (ring-ref avy-ring index))
             (window (cdr entry)))
        (push (list :position (car entry)
                    :buffer (buffer-name (window-buffer window))
                    :selected (eq window (selected-window)))
              result)))))

(defun neomacs-eem-test-overlay-count (&rest buffers)
  "Count live Avy overlays in BUFFERS, defaulting to the current buffer."
  (cl-loop for buffer in (or buffers (list (current-buffer)))
           sum (with-current-buffer buffer
                 (cl-count-if
                  (lambda (overlay)
                    (eq (overlay-get overlay 'category) 'avy))
                  (overlays-in (point-min) (point-max))))))

(defun neomacs-eem-test-ticket-workflow (command name)
  "Use COMMAND to select a ticket from a line containing an archived ticket."
  (neomacs-eem-test-with-buffer
   name
   "Archived TICKET-100 | Active TICKET-200 | Next TICKET-300\nOther TICKET-900\n"
   (lambda (_buffer)
     (let* ((hidden-start (progn (search-forward "TICKET-100")
                                 (match-beginning 0)))
            (hidden-end (match-end 0))
            (hidden (make-overlay hidden-start hidden-end))
            (neomacs-eem-test-hook-trace nil)
            (evilem-keys '(?a ?s ?d))
            (avy-single-candidate-jump nil)
            (avy-ring (make-ring 20))
            (neomacs-eem-test-label-snapshots nil)
            (avy-translate-char-function #'neomacs-eem-test-observe-input)
            (avy-last-candidates nil))
       (overlay-put hidden 'invisible t)
       (goto-char (point-min))
       (local-set-key (kbd "C-c t") command)
       (execute-kbd-macro (kbd "C-c t a"))
       (prog1
           (list :point (point)
                 :line (line-number-at-pos)
                 :column (current-column)
                 :ticket (buffer-substring-no-properties
                          (point) (+ (point) 10))
                 :candidates (neomacs-eem-test-candidates)
                 :labels (nreverse neomacs-eem-test-label-snapshots)
                 :hooks (nreverse neomacs-eem-test-hook-trace)
                 :overlays (neomacs-eem-test-overlay-count))
         (delete-overlay hidden))))))
"####;

fn evil_easymotion_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_EASYMOTION_MELPA_PIN, "evil-easymotion.el")
        .expect("prepare revision-pinned Evil Easymotion source below ./tmp")
        .with_prelude(EVIL_EASYMOTION_TEST_PRELUDE)
        .with_timeout(EVIL_EASYMOTION_TEST_TIMEOUT)
}

#[test]
fn evil_easymotion_package_batch() {
    let cases = workflows::practical_workflow_batch_cases();
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed Evil Easymotion parity test");
    assert_oracle_batch_cases(
        evil_easymotion_oracle(),
        test_name,
        "evil-easymotion parity",
        &cases,
    );
}
