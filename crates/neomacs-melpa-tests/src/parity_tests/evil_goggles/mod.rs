use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_GOGGLES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_GOGGLES_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const EVIL_GOGGLES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'evil-goggles)

(evil-mode 1)

(defvar neomacs-evil-goggles-test-events nil)
(defvar neomacs-evil-goggles-test--original-called-interactively-p
  (symbol-function 'called-interactively-p))
(defvar neomacs-evil-goggles-test--original-run-at-time
  (symbol-function 'run-at-time))

(defconst neomacs-evil-goggles-test--faces
  '(evil-goggles-delete-face
    evil-goggles-yank-face
    evil-goggles-change-face
    evil-goggles-indent-face
    evil-goggles-join-face
    evil-goggles-shift-face
    evil-goggles-paste-face
    evil-goggles-set-marker-face)
  "Package faces whose foreground the shared workflows customize.")

(defun neomacs-evil-goggles-test--attribute-plist (attributes)
  "Convert face ATTRIBUTES from an alist to a property list."
  (apply #'append
         (mapcar (lambda (attribute)
                   (list (car attribute) (cdr attribute)))
                 attributes)))

(defun neomacs-evil-goggles-test--save-faces ()
  "Capture customized foregrounds on every live frame."
  (mapcar
   (lambda (face)
     (list face
           (mapcar (lambda (frame)
                     (cons frame
                           (list
                            (cons :foreground
                                  (face-attribute
                                   face :foreground frame nil)))))
                   (frame-list))))
   neomacs-evil-goggles-test--faces))

(defun neomacs-evil-goggles-test--restore-faces (saved-faces)
  "Restore SAVED-FACES exactly on defaults and surviving frames."
  (dolist (saved-face saved-faces)
    (let ((face (nth 0 saved-face))
          (frame-attributes (nth 1 saved-face)))
      (dolist (saved-frame frame-attributes)
        (when (frame-live-p (car saved-frame))
          (apply #'set-face-attribute face (car saved-frame)
                 (neomacs-evil-goggles-test--attribute-plist
                  (cdr saved-frame))))))))

(defun neomacs-evil-goggles-test--overlays ()
  "Return live Evil Goggles overlays in the current buffer in range order."
  (sort
   (cl-remove-if-not
    (lambda (overlay)
      (equal (overlay-get overlay 'priority) 9999))
    (overlays-in (point-min) (point-max)))
   (lambda (left right)
     (or (< (overlay-start left) (overlay-start right))
         (and (= (overlay-start left) (overlay-start right))
              (< (overlay-end left) (overlay-end right)))))))

(defun neomacs-evil-goggles-test--overlay-summary (overlay)
  "Return stable observable properties of OVERLAY."
  (list
   :range (list (overlay-start overlay) (overlay-end overlay))
   :text (buffer-substring-no-properties
          (overlay-start overlay) (overlay-end overlay))
   :face (overlay-get overlay 'face)
   :priority (overlay-get overlay 'priority)
   :selected-window (eq (overlay-get overlay 'window) (selected-window))
   :insert-behind
   (and (memq 'evil-goggles--overlay-insert-behind-hook
              (overlay-get overlay 'insert-behind-hooks))
        t)))

(defun neomacs-evil-goggles-test--live-summary ()
  "Return the live asynchronous hint, timer, and cleanup-hook state."
  (list
   :overlays
   (mapcar #'neomacs-evil-goggles-test--overlay-summary
           (neomacs-evil-goggles-test--overlays))
   :timer (and (timerp evil-goggles--timer) t)
   :pre-command-cleanup
   (and (memq 'evil-goggles--vanish pre-command-hook) t)))

(defun neomacs-evil-goggles-test-configure-visible-faces ()
  "Give each operation a distinct practical foreground customization."
  (dolist (face-color
           '((evil-goggles-delete-face . "#dc322f")
             (evil-goggles-yank-face . "#268bd2")
             (evil-goggles-change-face . "#cb4b16")
             (evil-goggles-indent-face . "#6c71c4")
             (evil-goggles-join-face . "#2aa198")
             (evil-goggles-shift-face . "#d33682")
             (evil-goggles-paste-face . "#859900")
             (evil-goggles-set-marker-face . "#b58900")))
    (set-face-attribute (car face-color) (selected-frame)
                        :foreground (cdr face-color))))

(defun neomacs-evil-goggles-test--record (kind fields)
  "Append an event of KIND with FIELDS and the current overlays."
  (setq neomacs-evil-goggles-test-events
        (append
         neomacs-evil-goggles-test-events
         (list
          (append
           (list kind)
           fields
           (list :this-command this-command
                 :real-this-command real-this-command)
           (list
            :overlays
            (mapcar #'neomacs-evil-goggles-test--overlay-summary
                    (neomacs-evil-goggles-test--overlays))))))))

(defun neomacs-evil-goggles-test--run-at-time
    (duration repeat function &rest arguments)
  "Record asynchronous hint scheduling at the external time boundary."
  (neomacs-evil-goggles-test--record
   :timer
   (list :duration duration
         :repeat repeat
         :function function
         :arguments arguments
         :cleanup-hook
         (and (or (memq 'evil-goggles--vanish pre-command-hook)
                  (memq 'evil-goggles--vanish
                        (default-value 'pre-command-hook)))
              t)))
  ;; Return a genuine, inactive timer so package cleanup still exercises
  ;; `timerp' and `cancel-timer' rather than a test double.
  (timer-create))

(defun neomacs-evil-goggles-test--sit-for (duration &rest _arguments)
  "Record a blocking hint at the external time boundary."
  (neomacs-evil-goggles-test--record
   :blocking (list :duration duration))
  t)

(defun neomacs-evil-goggles-test--pulse (overlay face)
  "Record pulse animation at its external presentation boundary."
  (neomacs-evil-goggles-test--record
   :pulse
   (list :face face
         :background (face-background face nil t)
         :target (neomacs-evil-goggles-test--overlay-summary overlay)))
  nil)

(defun neomacs-evil-goggles-test--physical-advice-p ()
  "Return non-nil only in advice for the physical Evil command.

The same advice function can serve several commands, and an Evil operator can
call another advised command internally.  Walk GNU's documented
`backtrace-frame' interface to identify the command immediately wrapped by the
active package advice, then compare it with `real-this-command'."
  (let ((index 0)
        (inside-package-advice nil)
        frame function target)
    (while (and (< index 64)
                (not target)
                (setq frame
                      (backtrace-frame
                       index 'neomacs-evil-goggles-test--physical-advice-p)))
      (setq function (nth 1 frame))
      (cond
       ((memq function
              '(evil-goggles--generic-blocking-advice
                evil-goggles--generic-async-advice
                evil-goggles--generic-async-advice-1
                evil-goggles--delete-line-advice
                evil-goggles--join-advice
                evil-goggles--set-marker-advice
                evil-goggles--record-macro-advice
                evil-goggles--paste-advice))
        (setq inside-package-advice t))
       ((and inside-package-advice
             (assq function evil-goggles--commands))
        (setq target function)))
      (setq index (1+ index)))
    (eq target real-this-command)))

(defun neomacs-evil-goggles-test-keys (keys)
  "Execute real Evil KEYS while modeling physical command-loop dispatch.

Batch keyboard macros are not considered physical user input by
`called-interactively-p' with kind `interactive'.  Evil Goggles deliberately
uses that distinction.  This adapter supplies only the unattended UI fact;
Evil still resolves and runs every key binding, motion, operator, register,
marker, edit, advice, overlay, and cleanup hook."
  (cl-letf (((symbol-function 'called-interactively-p)
             (lambda (&optional kind)
               (if (eq kind 'interactive)
                   (neomacs-evil-goggles-test--physical-advice-p)
                 (funcall
                  neomacs-evil-goggles-test--original-called-interactively-p
                  kind))))
            ((symbol-function 'run-at-time)
             (lambda (duration repeat function &rest arguments)
               (if (eq function 'evil-goggles--vanish)
                   (apply #'neomacs-evil-goggles-test--run-at-time
                          duration repeat function arguments)
                 (apply neomacs-evil-goggles-test--original-run-at-time
                        duration repeat function arguments))))
            ((symbol-function 'sit-for)
             #'neomacs-evil-goggles-test--sit-for)
            ((symbol-function 'pulse-momentary-highlight-overlay)
             #'neomacs-evil-goggles-test--pulse))
    (execute-kbd-macro (kbd keys))))

(defun neomacs-evil-goggles-test-reset ()
  "Restore global package state before or after a workflow."
  (evil-goggles--vanish)
  (evil-goggles-mode -1)
  (setq neomacs-evil-goggles-test-events nil
        kill-ring nil
        kill-ring-yank-pointer nil))

(defun neomacs-evil-goggles-test-with-state (workflow)
  "Call WORKFLOW without leaking faces, kill data, or package state."
  (let ((saved-faces (neomacs-evil-goggles-test--save-faces))
        (saved-kill-ring kill-ring)
        (saved-kill-ring-yank-pointer kill-ring-yank-pointer))
    (prog1
        (let (;; Dynamic bindings preserve the caller's exact ring and yank
              ;; pointer; the workflow receives a deterministic private ring.
              (kill-ring nil)
              (kill-ring-yank-pointer nil)
              (neomacs-evil-goggles-test-events nil))
          (unwind-protect
              (progn
                (neomacs-evil-goggles-test-reset)
                (funcall workflow))
            (neomacs-evil-goggles-test-reset)
            (neomacs-evil-goggles-test--restore-faces saved-faces)))
      (let ((restored-faces (neomacs-evil-goggles-test--save-faces)))
        (unless (and (equal saved-faces restored-faces)
                     (eq saved-kill-ring kill-ring)
                     (eq saved-kill-ring-yank-pointer
                         kill-ring-yank-pointer))
          (error
           "Evil Goggles workflow leaked state: faces=%S ring=%S pointer=%S"
           (and (not (equal saved-faces restored-faces))
                (list saved-faces restored-faces))
           (not (eq saved-kill-ring kill-ring))
           (not (eq saved-kill-ring-yank-pointer
                    kill-ring-yank-pointer))))))))
"##;

fn evil_goggles_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_GOGGLES_MELPA_PIN, "evil-goggles.el")
        .expect("prepare exact Evil Goggles source and Evil below ./tmp")
        .with_prelude(EVIL_GOGGLES_TEST_PRELUDE)
        .with_timeout(EVIL_GOGGLES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Evil Goggles parity test")
        .into()
}

fn assert_evil_goggles_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        evil_goggles_oracle(),
        &current_test_name(),
        "evil_goggles_parity",
        cases,
    );
}

#[test]
fn evil_goggles_package_batch() {
    assert_evil_goggles_batch(&workflows::workflow_batch_cases());
}
