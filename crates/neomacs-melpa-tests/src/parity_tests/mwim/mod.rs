use std::time::Duration;

use crate::{CachedMelpaOracle, MWIM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MWIM_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const MWIM_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'timer)

;; Major-mode activation is irreversible process state.  Load the real
;; compiled built-ins before the shared case baselines, while leaving the
;; exact MELPA subject on the oracle's source-only policy.
(let ((load-suffixes '(".elc" ".el")))
  (require 'cc-mode)
  (require 'org)
  (require 'message))
(defconst mwim358-test-cc-compiled
  (let ((source (symbol-file 'c-mode 'defun)))
    (and source (string-suffix-p ".elc" source))))
(defconst mwim358-test-org-compiled
  (let ((source (symbol-file 'org-mode 'defun)))
    (and source (string-suffix-p ".elc" source))))
(defconst mwim358-test-message-compiled
  (let ((source (symbol-file 'message-mode 'defun)))
    (and source (string-suffix-p ".elc" source))))
(unless (and mwim358-test-cc-compiled
             mwim358-test-org-compiled
             mwim358-test-message-compiled
             (equal load-suffixes '(".el")))
  (error "MWIM built-in load boundary failed: c=%S org=%S message=%S suffixes=%S"
         (symbol-file 'c-mode 'defun)
         (symbol-file 'org-mode 'defun)
         (symbol-file 'message-mode 'defun)
         load-suffixes))

;; GNU reserves its batch frame's menu-bar row on the first configuration
;; restore.  Establish that ordinary geometry once before any case baseline.
(set-window-configuration (current-window-configuration))

(defvar mwim358-test-owned-buffers nil)

(defconst mwim358-test-command-state-symbols
  '(executing-kbd-macro unread-command-events
    this-command real-this-command last-command real-last-command
    last-command-event last-input-event current-prefix-arg prefix-arg
    deactivate-mark)
  "Command-loop globals owned and restored around every shared workflow.")

(defun mwim358-test-copy (value)
  "Copy VALUE recursively, including strings and vectors."
  (cond ((consp value)
         (cons (mwim358-test-copy (car value))
               (mwim358-test-copy (cdr value))))
        ((vectorp value)
         (apply #'vector (mapcar #'mwim358-test-copy (append value nil))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun mwim358-test-variable-state (symbol)
  "Return SYMBOL's exact boundness and copied value."
  (if (boundp symbol)
      (list :bound t :value (mwim358-test-copy (symbol-value symbol)))
    '(:bound nil)))

(defun mwim358-test-restore-variable (symbol state)
  "Restore SYMBOL to STATE returned by `mwim358-test-variable-state'."
  (if (plist-get state :bound)
      (set symbol (mwim358-test-copy (plist-get state :value)))
    (makunbound symbol)))

(defun mwim358-test-window-structure ()
  "Return stable ownership-relevant state for every ordinary window."
  (mapcar
   (lambda (window)
     (list :window window
           :buffer (window-buffer window)
           :edges (window-edges window)
           :point (window-point window)
           :start (window-start window)
           :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :dedicated (window-dedicated-p window)
           :parameters
           (copy-tree (seq-filter #'cdr (window-parameters window)))
           :prev-buffers (copy-tree (window-prev-buffers window))
           :next-buffers (copy-tree (window-next-buffers window))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil 'no-minibuf)))

(defun mwim358-test-restore-window-state (configuration structure)
  "Restore CONFIGURATION and exact per-window STRUCTURE."
  (set-window-configuration configuration)
  (dolist (entry structure)
    (let* ((window (plist-get entry :window))
           (parameters (plist-get entry :parameters)))
      (unless (window-live-p window)
        (error "MWIM baseline window died: %S" window))
      (dolist (parameter (window-parameters window))
        (set-window-parameter window (car parameter) nil))
      (dolist (parameter parameters)
        (set-window-parameter window (car parameter) (cdr parameter)))
      (set-window-prev-buffers
       window (copy-tree (plist-get entry :prev-buffers)))
      (set-window-next-buffers
       window (copy-tree (plist-get entry :next-buffers)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun mwim358-test-own-buffer (suffix mode text)
  "Create, select, and initialize one owned buffer for SUFFIX, MODE, and TEXT."
  (let ((name (format " *mwim358-%s*" suffix)))
    (when (get-buffer name)
      (error "MWIM refuses preexisting owned buffer: %S" name))
    (let ((buffer (generate-new-buffer name)))
      (push buffer mwim358-test-owned-buffers)
      (switch-to-buffer buffer)
      (funcall mode)
      (insert text)
      (set-buffer-modified-p nil)
      (setq buffer-undo-list nil)
      buffer)))

(defun mwim358-test-bind-keys (bindings)
  "Install buffer-local BINDINGS over the real major-mode map."
  (let ((map (make-sparse-keymap)))
    (set-keymap-parent map (current-local-map))
    (dolist (binding bindings)
      (define-key map (kbd (car binding)) (cdr binding)))
    (use-local-map map)
    (mapcar (lambda (binding)
              (cons (car binding) (key-binding (kbd (car binding)))))
            bindings)))

(defun mwim358-test-command-loop (thunk)
  "Run THUNK through a bounded real command-loop world."
  (when unread-command-events
    (error "MWIM command loop began with unread events: %S"
           unread-command-events))
  (prog1 (funcall thunk)
    (when unread-command-events
      (error "MWIM left unread command events: %S" unread-command-events))
    (when (active-minibuffer-window)
      (error "MWIM unexpectedly left an active minibuffer"))))

(defun mwim358-test-position (tag)
  "Return exact public point/mark state tagged TAG."
  (list tag
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :before (and (char-before) (char-to-string (char-before)))
        :after (and (char-after) (char-to-string (char-after)))
        :mark (mark t)
        :active (and mark-active t)))

(defun mwim358-test-condition (thunk)
  "Call THUNK and return its exact plain condition."
  (condition-case condition
      (list :value (funcall thunk))
    (error
     (list :signal (car condition)
           :data (mwim358-test-copy (cdr condition))
           :message (substring-no-properties
                     (error-message-string condition))))))

(defun mwim358-test-second-column ()
  "Return the second accessible column of the current logical line."
  (min (line-end-position) (1+ (line-beginning-position))))

(defun mwim358-test-run (case-name thunk)
  "Run THUNK in the owned shared-batch world CASE-NAME."
  (unless (string-match-p "\\`[a-z0-9-]+\\'" case-name)
    (error "MWIM invalid case name: %S" case-name))
  (let ((source (symbol-file 'mwim 'defun)))
    (unless (and (featurep 'mwim)
                 (package-built-in-p 'seq '(2 24))
                 source
                 (string-suffix-p "/mwim.el" source)
                 (equal load-suffixes '(".el")))
      (error "MWIM activation/dependency boundary failed: mwim=%S seq=%S source=%S suffixes=%S"
             (featurep 'mwim) (package-built-in-p 'seq '(2 24))
             source load-suffixes)))
  (let* ((buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-timers-before (copy-sequence timer-idle-list))
         (current-buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (window-configuration-before (current-window-configuration))
         (window-structure-before (mwim358-test-window-structure))
         (beginning-function-before
          (mwim358-test-variable-state 'mwim-beginning-of-line-function))
         (end-function-before
          (mwim358-test-variable-state 'mwim-end-of-line-function))
         (next-function-before
          (mwim358-test-variable-state 'mwim-next-position-function))
         (beginning-positions-before
          (mwim358-test-variable-state 'mwim-beginning-position-functions))
         (end-positions-before
          (mwim358-test-variable-state 'mwim-end-position-functions))
         (positions-before
          (mwim358-test-variable-state 'mwim-position-functions))
         ;; Preserve these two objects as one identity-bearing structure.
         ;; Copying them independently corrupts the yank pointer's tail
         ;; relationship to `kill-ring'.
         (kill-ring-bound-before (boundp 'kill-ring))
         (kill-ring-before (and kill-ring-bound-before kill-ring))
         (kill-yank-bound-before (boundp 'kill-ring-yank-pointer))
         (kill-yank-before
          (and kill-yank-bound-before kill-ring-yank-pointer))
         (cut-before (mwim358-test-variable-state 'interprogram-cut-function))
         (paste-before
          (mwim358-test-variable-state 'interprogram-paste-function))
         (save-paste-before
          (mwim358-test-variable-state
           'save-interprogram-paste-before-kill))
         (transform-before
          (mwim358-test-variable-state 'kill-transform-function))
         (duplicates-before
          (mwim358-test-variable-state 'kill-do-not-save-duplicates))
         (transient-mark-before
          (mwim358-test-variable-state 'transient-mark-mode))
         (command-state-before
          (mapcar (lambda (symbol)
                    (cons symbol (mwim358-test-variable-state symbol)))
                  mwim358-test-command-state-symbols))
         (mwim358-test-owned-buffers nil)
         result body-error cleanup-errors cleanup-state)
    (cl-labels
        ((attempt
          (phase function)
          (condition-case condition
              (funcall function)
            (t (push (list phase condition) cleanup-errors) nil)))
         (restore-windows
          (phase)
          (attempt
           phase
           (lambda ()
             (mwim358-test-restore-window-state
              window-configuration-before window-structure-before)
             (when (buffer-live-p current-buffer-before)
               (set-buffer current-buffer-before))
             (when (window-live-p selected-window-before)
               (select-window selected-window-before))
             (unless (and
                      (equal (mwim358-test-window-structure)
                             window-structure-before)
                      (compare-window-configurations
                       (current-window-configuration)
                       window-configuration-before))
               (error "MWIM window state did not restore: structure=%S comparator=%S current=%S baseline=%S"
                      (equal (mwim358-test-window-structure)
                             window-structure-before)
                      (compare-window-configurations
                       (current-window-configuration)
                       window-configuration-before)
                      (mwim358-test-window-structure)
                      window-structure-before)))))
         (sweep
          (number)
          (dolist (process
                   (seq-difference (process-list) processes-before #'eq))
            (attempt
             (list 'process number)
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist (timer
                   (delete-dups
                    (append
                     (seq-difference timer-list timers-before #'eq)
                     (seq-difference timer-idle-list
                                     idle-timers-before #'eq))))
            (attempt (list 'timer number) (lambda () (cancel-timer timer))))
          (dolist (buffer
                   (seq-difference (buffer-list) buffers-before #'eq))
            (attempt
             (list 'buffer number)
             (lambda ()
               (when (buffer-live-p buffer)
                 (set-buffer-modified-p nil)
                 (kill-buffer buffer)))))))
      (unwind-protect
          (condition-case condition
              (let ((emacs-lisp-mode-hook nil)
                    (c-mode-hook nil)
                    (org-mode-hook nil)
                    (message-mode-hook nil)
                    (enable-local-variables nil)
                    (enable-dir-local-variables nil)
                    (enable-local-eval nil))
                (setq result (funcall thunk)))
            (t (setq body-error condition)))
        (restore-windows 'window-first)
        (dotimes (number 2) (sweep number))
        (dolist
            (entry
             (append
              (list
               (cons 'mwim-beginning-of-line-function
                     beginning-function-before)
               (cons 'mwim-end-of-line-function end-function-before)
               (cons 'mwim-next-position-function next-function-before)
               (cons 'mwim-beginning-position-functions
                     beginning-positions-before)
               (cons 'mwim-end-position-functions end-positions-before)
               (cons 'mwim-position-functions positions-before)
               (cons 'interprogram-cut-function cut-before)
               (cons 'interprogram-paste-function paste-before)
               (cons 'save-interprogram-paste-before-kill save-paste-before)
               (cons 'kill-transform-function transform-before)
               (cons 'kill-do-not-save-duplicates duplicates-before)
               (cons 'transient-mark-mode transient-mark-before))
              command-state-before))
          (attempt
           (list 'restore-variable (car entry))
           (lambda ()
             (mwim358-test-restore-variable (car entry) (cdr entry)))))
        (attempt
         '(restore-variable kill-ring)
         (lambda ()
           (if kill-ring-bound-before
               (setq kill-ring kill-ring-before)
             (makunbound 'kill-ring))))
        (attempt
         '(restore-variable kill-ring-yank-pointer)
         (lambda ()
           (if kill-yank-bound-before
               (setq kill-ring-yank-pointer kill-yank-before)
             (makunbound 'kill-ring-yank-pointer))))
        (restore-windows 'window-final)
        (setq cleanup-errors (nreverse cleanup-errors))
        (setq cleanup-state
              (list
               :new-buffers
               (seq-difference (buffer-list) buffers-before #'eq)
               :new-processes
               (seq-difference (process-list) processes-before #'eq)
               :new-timers
               (delete-dups
                (append
                 (seq-difference timer-list timers-before #'eq)
                 (seq-difference timer-idle-list idle-timers-before #'eq)))
               :owned-live
               (mapcar #'buffer-live-p mwim358-test-owned-buffers)
               :window
               (and (equal (mwim358-test-window-structure)
                           window-structure-before)
                    (compare-window-configurations
                     (current-window-configuration)
                     window-configuration-before))
               :current-buffer (eq (current-buffer) current-buffer-before)
               :selected-window (eq (selected-window)
                                    selected-window-before)
               :variables
               (and
                (equal (mwim358-test-variable-state
                        'mwim-beginning-of-line-function)
                       beginning-function-before)
                (equal (mwim358-test-variable-state
                        'mwim-end-of-line-function)
                       end-function-before)
                (equal (mwim358-test-variable-state
                        'mwim-next-position-function)
                       next-function-before)
                (equal (mwim358-test-variable-state
                        'mwim-beginning-position-functions)
                       beginning-positions-before)
                (equal (mwim358-test-variable-state
                        'mwim-end-position-functions)
                       end-positions-before)
                (equal (mwim358-test-variable-state
                        'mwim-position-functions)
                       positions-before))
               :kill-state
               (and
                (eq (boundp 'kill-ring) kill-ring-bound-before)
                (or (not kill-ring-bound-before)
                    (eq kill-ring kill-ring-before))
                (eq (boundp 'kill-ring-yank-pointer)
                    kill-yank-bound-before)
                (or (not kill-yank-bound-before)
                    (eq kill-ring-yank-pointer kill-yank-before))
                (equal (mwim358-test-variable-state
                        'interprogram-cut-function)
                       cut-before)
                (equal (mwim358-test-variable-state
                        'interprogram-paste-function)
                       paste-before)
                (equal (mwim358-test-variable-state
                        'save-interprogram-paste-before-kill)
                       save-paste-before)
                (equal (mwim358-test-variable-state
                        'kill-transform-function)
                       transform-before)
                (equal (mwim358-test-variable-state
                        'kill-do-not-save-duplicates)
                       duplicates-before))
               :transient-mark
               (equal (mwim358-test-variable-state 'transient-mark-mode)
                      transient-mark-before)
               :command-state
               (cl-every
                (lambda (entry)
                  (equal (mwim358-test-variable-state (car entry))
                         (cdr entry)))
                command-state-before)
               :body-error body-error
               :cleanup-errors cleanup-errors))))
    (when (or body-error cleanup-errors
              (plist-get cleanup-state :new-buffers)
              (plist-get cleanup-state :new-processes)
              (plist-get cleanup-state :new-timers)
              (memq t (plist-get cleanup-state :owned-live))
              (not (plist-get cleanup-state :window))
              (not (plist-get cleanup-state :current-buffer))
              (not (plist-get cleanup-state :selected-window))
              (not (plist-get cleanup-state :variables))
              (not (plist-get cleanup-state :kill-state))
              (not (plist-get cleanup-state :transient-mark))
              (not (plist-get cleanup-state :command-state)))
      (error "MWIM workflow/cleanup failure: case=%S body=%S cleanup-errors=%S state=%S"
             case-name body-error cleanup-errors cleanup-state))
    (list :result result :cleanup cleanup-state)))
"####;

fn mwim_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MWIM_MELPA_PIN, "mwim.el")
        .expect("prepare exact shallow MWIM source below ./tmp")
        .with_prelude(MWIM_TEST_PRELUDE)
        .with_timeout(MWIM_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed mwim parity test")
        .into()
}

#[test]
fn mwim_package_batch() {
    assert_oracle_batch_cases(
        mwim_oracle(),
        &current_test_name(),
        "mwim_parity",
        &workflows::workflow_batch_cases(),
    );
}
