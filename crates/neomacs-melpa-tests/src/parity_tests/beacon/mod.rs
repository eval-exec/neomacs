use crate::{BEACON_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const BEACON_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'timer)

;; Establish GNU's ordinary reserved menu-bar row before case baselines.
(set-window-configuration (current-window-configuration))

(defvar beacon359-test-owned-buffers nil)
(defvar beacon359-test-owned-overlays nil)
(defvar beacon359-test-owned-timers nil)
(defvar beacon359-test-blinks nil)

(defconst beacon359-test-state-symbols
  '(beacon-mode beacon-push-mark
    beacon-blink-when-point-moves-vertically
    beacon-blink-when-point-moves-horizontally
    beacon-blink-when-buffer-changes
    beacon-blink-when-window-scrolls
    beacon-blink-when-window-changes
    beacon-blink-when-focused beacon-blink-duration beacon-blink-delay
    beacon-size beacon-color beacon-dont-blink-predicates
    beacon-dont-blink-major-modes beacon-dont-blink-commands
    beacon-before-blink-hook beacon-lighter beacon--timer beacon--ovs
    beacon--window-scrolled beacon--previous-place
    beacon--previous-mark-head beacon--previous-window
    beacon--previous-window-start pre-command-hook post-command-hook
    before-change-functions window-scroll-functions
    after-focus-change-function unread-command-events executing-kbd-macro
    this-command real-this-command last-command real-last-command
    last-command-event last-input-event current-prefix-arg prefix-arg
    deactivate-mark)
  "Editor and Beacon state restored around each shared workflow.")

(defun beacon359-test-copy (value)
  "Copy VALUE recursively while preserving opaque editor objects."
  (cond ((consp value)
         (cons (beacon359-test-copy (car value))
               (beacon359-test-copy (cdr value))))
        ((vectorp value)
         (apply #'vector (mapcar #'beacon359-test-copy (append value nil))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun beacon359-test-variable-state (symbol)
  "Return SYMBOL's exact boundness and copied value."
  (if (boundp symbol)
      (list :bound t :value (beacon359-test-copy (symbol-value symbol)))
    '(:bound nil)))

(defun beacon359-test-restore-variable (symbol state)
  "Restore SYMBOL to STATE returned by `beacon359-test-variable-state'."
  (if (plist-get state :bound)
      (set symbol (beacon359-test-copy (plist-get state :value)))
    (makunbound symbol)))

(defun beacon359-test-window-parameters (window)
  "Return WINDOW parameters in stable semantic order."
  (sort (copy-tree (seq-filter #'cdr (window-parameters window)))
        (lambda (left right)
          (string< (symbol-name (car left)) (symbol-name (car right))))))

(defun beacon359-test-window-state ()
  "Return ownership-relevant state for every ordinary window."
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :point (window-point window)
           :start (window-start window) :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :dedicated (window-dedicated-p window)
           :parameters (beacon359-test-window-parameters window)
           :prev-buffers (copy-tree (window-prev-buffers window))
           :next-buffers (copy-tree (window-next-buffers window))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil 'no-minibuf)))

(defun beacon359-test-restore-windows (configuration structure)
  "Restore CONFIGURATION and exact per-window STRUCTURE."
  (set-window-configuration configuration)
  (dolist (entry structure)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Beacon baseline window died: %S" window))
      (dolist (parameter (window-parameters window))
        (set-window-parameter window (car parameter) nil))
      (dolist (parameter (plist-get entry :parameters))
        (set-window-parameter window (car parameter) (cdr parameter)))
      (set-window-prev-buffers
       window (copy-tree (plist-get entry :prev-buffers)))
      (set-window-next-buffers
       window (copy-tree (plist-get entry :next-buffers)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun beacon359-test-own-buffer (suffix mode text)
  "Create and select an owned buffer for SUFFIX, MODE, and TEXT."
  (let ((name (format " *beacon359-%s*" suffix)))
    (when (get-buffer name)
      (error "Beacon refuses preexisting owned buffer: %S" name))
    (let ((buffer (generate-new-buffer name)))
      (push buffer beacon359-test-owned-buffers)
      (switch-to-buffer buffer)
      (funcall mode)
      (insert text)
      (set-buffer-modified-p nil)
      (setq buffer-undo-list nil)
      buffer)))

(defun beacon359-test-command-loop (thunk)
  "Run THUNK in a bounded real command-loop world."
  (when unread-command-events
    (error "Beacon command loop began with unread events: %S"
           unread-command-events))
  (prog1 (funcall thunk)
    (when unread-command-events
      (error "Beacon left unread command events: %S" unread-command-events))
    (when (active-minibuffer-window)
      (error "Beacon unexpectedly left an active minibuffer"))))

(defun beacon359-test-condition (thunk)
  "Call THUNK and return its exact plain condition."
  (condition-case condition
      (list :value (funcall thunk))
    (error
     (list :signal (car condition)
           :data (beacon359-test-copy (cdr condition))
           :message (substring-no-properties
                     (error-message-string condition))))))

(defun beacon359-test-record-blink ()
  "Record the exact public blink-hook command context."
  (push (list :command this-command :last last-command
              :point (point) :line (line-number-at-pos)
              :column (current-column) :window-start (window-start))
        beacon359-test-blinks))

(defun beacon359-test-jump-end ()
  "Move to the accessible end without pushing mark."
  (interactive)
  (goto-char (point-max)))

(defun beacon359-test-jump-column ()
  "Move to column twelve on the current line."
  (interactive)
  (move-to-column 12))

(defun beacon359-test-timer-state ()
  "Return stable state of the current Beacon timer."
  (if (not (timerp beacon--timer))
      '(:timer nil)
    (list :timer t :listed (and (memq beacon--timer timer-list) t)
          :function (timer--function beacon--timer)
          :repeat (timer--repeat-delay beacon--timer)
          :args (timer--args beacon--timer))))

(defun beacon359-test-overlay-state ()
  "Return stable state of every live Beacon overlay."
  (mapcar
   (lambda (overlay)
     (list :range (cons (overlay-start overlay) (overlay-end overlay))
           :beacon (overlay-get overlay 'beacon)
           :priority (overlay-get overlay 'priority)
           :selected-window
           (eq (overlay-get overlay 'window) (selected-window))
           :face (beacon359-test-copy (overlay-get overlay 'face))
           :colors (beacon359-test-copy
                    (overlay-get overlay 'beacon-colors))
           :after
           (let ((value (overlay-get overlay 'after-string)))
             (and value
                  (list :text (substring-no-properties value)
                        :cursor (get-text-property 0 'cursor value)
                        :faces
                        (mapcar
                         (lambda (position)
                           (beacon359-test-copy
                            (get-text-property position 'face value)))
                         (number-sequence 0 (1- (length value)))))))))
   (sort (seq-filter #'overlay-buffer (copy-sequence beacon--ovs))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun beacon359-test-register-action (timers-before started finished delay repeat)
  "Own and validate the timer/overlays created by one public blink."
  (let ((new-timers (seq-difference timer-list timers-before #'eq)))
    (unless (and (= (length new-timers) 1)
                 (eq (car new-timers) beacon--timer))
      (error "Beacon public blink created unexpected timers: %S" new-timers))
    (let* ((timer (car new-timers))
           (deadline (timer--time timer))
           (lower (time-add started delay))
           (upper (time-add finished delay)))
      (unless (and (not (time-less-p deadline lower))
                   (not (time-less-p upper deadline))
                   (eq (timer--function timer) #'beacon--dec)
                   (= (timer--repeat-delay timer) repeat))
        (error "Beacon timer contract mismatch: %S"
               (list deadline lower upper
                     (timer--function timer)
                     (timer--repeat-delay timer))))
      (cl-pushnew timer beacon359-test-owned-timers :test #'eq)
      (dolist (overlay beacon--ovs)
        (cl-pushnew overlay beacon359-test-owned-overlays :test #'eq))
      timer)))

(defun beacon359-test-dispatch-owned-timer (timer)
  "Deliver only the exact TIMER captured from one public blink."
  (unless (and (timerp timer)
               (memq timer beacon359-test-owned-timers)
               (eq timer beacon--timer)
               (memq timer timer-list)
               (eq (timer--function timer) #'beacon--dec))
    (error "Beacon refuses unowned timer dispatch: %S" timer))
  (timer-event-handler timer))

(defun beacon359-test-hook-count (function hook)
  "Count FUNCTION in the global value of HOOK."
  (cl-count function (default-value hook) :test #'eq))

(defun beacon359-test-run (case-name thunk)
  "Run THUNK in the owned shared-batch world CASE-NAME."
  (unless (string-match-p "\\`[a-z0-9-]+\\'" case-name)
    (error "Beacon invalid case name: %S" case-name))
  (let ((source (symbol-file 'beacon-blink 'defun)))
    (unless (and (featurep 'beacon) source
                 (string-suffix-p "/beacon.el" source)
                 (package-built-in-p 'seq '(2 24))
                 (equal load-suffixes '(".el")))
      (error "Beacon activation boundary failed: feature=%S source=%S seq=%S suffixes=%S"
             (featurep 'beacon) source
             (package-built-in-p 'seq '(2 24)) load-suffixes)))
  (let* ((buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-timers-before (copy-sequence timer-idle-list))
         (current-buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (beacon359-test-window-state))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (beacon359-test-variable-state symbol)))
                  beacon359-test-state-symbols))
         (beacon359-test-owned-buffers nil)
         (beacon359-test-owned-overlays nil)
         (beacon359-test-owned-timers nil)
         (beacon359-test-blinks nil)
         body-value body-error cleanup-errors owned-buffers owned-overlays
         owned-timers)
    (unwind-protect
        (condition-case condition
            (setq body-value (funcall thunk))
          (t (setq body-error condition)))
      (setq owned-buffers (copy-sequence beacon359-test-owned-buffers))
      (setq owned-overlays (copy-sequence beacon359-test-owned-overlays))
      (setq owned-timers (copy-sequence beacon359-test-owned-timers))
      (cl-labels
        ((attempt
          (phase function)
          (condition-case condition
              (funcall function)
            (t (push (list phase condition) cleanup-errors))))
         (sweep
          (number)
          (dolist (timer
                   (delete-dups
                    (append
                     (seq-difference timer-list timers-before #'eq)
                     (seq-difference timer-idle-list idle-timers-before #'eq))))
            (attempt (list 'timer number) (lambda () (cancel-timer timer))))
          (dolist (process (seq-difference (process-list) processes-before #'eq))
            (attempt
             (list 'process number)
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
            (attempt
             (list 'buffer number)
             (lambda ()
               (when (buffer-live-p buffer)
                 (set-buffer-modified-p nil)
                 (kill-buffer buffer)))))))
      (attempt 'disable-mode
               (lambda () (when (bound-and-true-p beacon-mode)
                            (beacon-mode -1))))
      (dolist (buffer owned-buffers)
        (when (buffer-live-p buffer)
          (dolist (overlay (with-current-buffer buffer (overlays-in (point-min)
                                                                     (point-max))))
            (when (overlay-get overlay 'beacon)
              (cl-pushnew overlay owned-overlays :test #'eq)))))
      (dolist (overlay (delete-dups
                        (append owned-overlays (copy-sequence beacon--ovs))))
        (attempt 'overlay
                 (lambda ()
                   (when (overlayp overlay) (delete-overlay overlay)))))
      (attempt 'window-first
               (lambda ()
                 (beacon359-test-restore-windows
                  configuration-before windows-before)))
      (dotimes (number 2) (sweep number))
      (dolist (entry states-before)
        (attempt (list 'variable (car entry))
                 (lambda ()
                   (beacon359-test-restore-variable
                    (car entry) (cdr entry)))))
      (attempt 'window-final
               (lambda ()
                 (beacon359-test-restore-windows
                  configuration-before windows-before)))
      (attempt
       'select-baseline
       (lambda ()
         (unless (and (buffer-live-p current-buffer-before)
                      (window-live-p selected-window-before))
           (error "Beacon selected baseline state died"))
         (select-window selected-window-before)
         (set-buffer current-buffer-before)))))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let ((cleanup-state
           (list
            :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
            :new-processes (seq-difference (process-list) processes-before #'eq)
            :new-timers
            (delete-dups
             (append (seq-difference timer-list timers-before #'eq)
                     (seq-difference timer-idle-list idle-timers-before #'eq)))
            :owned-live (mapcar #'buffer-live-p owned-buffers)
            :owned-overlays-live (mapcar #'overlay-buffer owned-overlays)
            :owned-timers-active
            (mapcar (lambda (timer)
                      (and (or (memq timer timer-list)
                               (memq timer timer-idle-list))
                           t))
                    owned-timers)
            :windows (equal (beacon359-test-window-state) windows-before)
            :configuration
            (compare-window-configurations
             (current-window-configuration) configuration-before)
            :buffer (eq (current-buffer) current-buffer-before)
            :window (eq (selected-window) selected-window-before)
            :variables
            (cl-every
             (lambda (entry)
               (equal (beacon359-test-variable-state (car entry))
                      (cdr entry)))
             states-before)
            :body-error body-error :cleanup-errors cleanup-errors)))
      (unless (and (null (plist-get cleanup-state :new-buffers))
                   (null (plist-get cleanup-state :new-processes))
                   (null (plist-get cleanup-state :new-timers))
                   (not (memq t (plist-get cleanup-state :owned-live)))
                   (not (seq-some #'identity
                                  (plist-get cleanup-state
                                             :owned-overlays-live)))
                   (not (memq t (plist-get cleanup-state
                                           :owned-timers-active)))
                   (plist-get cleanup-state :windows)
                   (plist-get cleanup-state :configuration)
                   (plist-get cleanup-state :buffer)
                   (plist-get cleanup-state :window)
                   (plist-get cleanup-state :variables)
                   (null body-error) (null cleanup-errors))
        (error "Beacon workflow/cleanup failure: %S" cleanup-state))
      (list :result body-value :cleanup cleanup-state))))
"####;

#[test]
fn beacon_package_batch() {
    let oracle = CachedMelpaOracle::new(BEACON_MELPA_PIN, "beacon.el")
        .expect("prepare exact shallow Beacon source below ./tmp")
        .with_prelude(BEACON_TEST_PRELUDE);
    assert_oracle_batch_cases(
        oracle,
        "beacon-package-batch",
        "Beacon",
        &workflows::beacon_batch_cases(),
    );
}
