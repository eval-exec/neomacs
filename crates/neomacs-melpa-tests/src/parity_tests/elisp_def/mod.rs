use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, ELISP_DEF_MELPA_PIN, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ELISP_DEF_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ELISP_DEF_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'xref)
(require 'timer)
(require 'cc-mode)

(defvar edt-test-owned-overlays nil)
(defvar edt-test-completion-plan nil)
(defvar edt-test-completion-ledger nil)
(defvar edt-test-xref-history nil)
(defvar edt-test-features nil)
(defvar edt-test-runtime-functions nil)

(defun edt-test-normalize (object root)
  "Replace the validated owned ROOT prefix throughout OBJECT."
  (cond
   ((stringp object)
    (if (equal object (directory-file-name root))
        "[ROOT]"
      (replace-regexp-in-string (regexp-quote root) "[ROOT]/" object t t)))
   ((consp object)
    (cons (edt-test-normalize (car object) root)
          (edt-test-normalize (cdr object) root)))
   ((vectorp object)
    (apply #'vector (mapcar (lambda (value)
                              (edt-test-normalize value root))
                            object)))
   (t object)))

(defun edt-test-write (root name contents)
  "Write exact CONTENTS to NAME below owned ROOT."
  (let ((path (expand-file-name name root)))
    (unless (string-prefix-p root path)
      (error "ELISP-DEF fixture escaped root: %s" path))
    (make-directory (file-name-directory path) t)
    (with-temp-file path (insert contents))
    path))

(defun edt-test-register-feature (feature)
  "Register an owned fixture FEATURE for unconditional teardown."
  (unless (featurep feature)
    (error "ELISP-DEF fixture feature was not loaded: %S" feature))
  (push feature edt-test-features)
  feature)

(defun edt-test-fset (symbol definition)
  "Define and own source-less runtime function SYMBOL."
  (when (fboundp symbol)
    (error "ELISP-DEF runtime function already exists: %S" symbol))
  (fset symbol definition)
  (push symbol edt-test-runtime-functions)
  symbol)

(defun edt-test-line ()
  "Return the selected line without properties."
  (buffer-substring-no-properties
   (line-beginning-position) (line-end-position)))

(defun edt-test-location ()
  "Return the complete selected source location."
  (list :buffer (buffer-name)
        :file (and buffer-file-name
                   (file-name-nondirectory buffer-file-name))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :text (edt-test-line)
        :symbol (symbol-at-point)
        :selected (eq (current-buffer) (window-buffer))))

(defun edt-test-marker (marker)
  "Return a stable complete description of MARKER."
  (let ((buffer (marker-buffer marker)))
    (and buffer
         (with-current-buffer buffer
           (list :file (and buffer-file-name
                            (file-name-nondirectory buffer-file-name))
                 :point (marker-position marker)
                 :line (line-number-at-pos marker)
                 :column (save-excursion
                           (goto-char marker)
                           (current-column)))))))

(defun edt-test-xref-state ()
  "Return both sides of the isolated real xref history."
  (list :backward (mapcar #'edt-test-marker (car edt-test-xref-history))
        :forward (mapcar #'edt-test-marker (cdr edt-test-xref-history))))

(defun edt-test-clear-xref ()
  "Clear every marker in the isolated real xref history."
  (dolist (stack (list (car edt-test-xref-history)
                       (cdr edt-test-xref-history)))
    (dolist (marker stack)
      (set-marker marker nil nil)))
  (setcar edt-test-xref-history nil)
  (setcdr edt-test-xref-history nil))

(defun edt-test-highlight-overlays (&optional buffer point)
  "Describe and own exact highlight overlays at POINT in BUFFER."
  (with-current-buffer (or buffer (current-buffer))
    (save-excursion
      (when point (goto-char point))
      (let ((overlays
             (sort
              (seq-filter
               (lambda (overlay)
                 (eq (overlay-get overlay 'face) 'highlight))
               (overlays-at (point)))
              (lambda (left right)
                (< (overlay-start left) (overlay-start right))))))
        (dolist (overlay overlays)
          (cl-pushnew overlay edt-test-owned-overlays :test #'eq))
        (mapcar
         (lambda (overlay)
           (list :start (overlay-start overlay)
                 :end (overlay-end overlay)
                 :face (overlay-get overlay 'face)
                 :text (buffer-substring-no-properties
                        (overlay-start overlay) (overlay-end overlay))))
         overlays)))))

(defun edt-test-position (buffer needle &optional occurrence offset)
  "Select BUFFER and put point at NEEDLE plus OFFSET."
  (switch-to-buffer buffer)
  (goto-char (point-min))
  (let ((case-fold-search nil))
    (dotimes (_ (or occurrence 1))
      (unless (search-forward needle nil t)
        (error "ELISP-DEF missing fixture needle: %S" needle))))
  (goto-char (+ (match-beginning 0) (or offset 0)))
  (point))

(defun edt-test-plain (object)
  "Remove text properties recursively from OBJECT."
  (cond
   ((stringp object) (substring-no-properties object))
   ((consp object)
    (cons (edt-test-plain (car object))
          (edt-test-plain (cdr object))))
   ((vectorp object)
    (apply #'vector (mapcar #'edt-test-plain object)))
   (t object)))

(defun edt-test-reset-navigation (buffer)
  "Reset isolated xref and BUFFER's mark before an independent episode."
  (edt-test-clear-xref)
  (with-current-buffer buffer
    (set-marker (mark-marker) nil nil)
    (setq mark-active nil)))

(defun edt-test-jump (invocation)
  "Invoke public `elisp-def' by INVOCATION and prove jump/return/timer state."
  (let* ((origin-buffer (current-buffer))
         (origin-point (point))
         (origin-before (edt-test-location))
         (timers-before (copy-sequence timer-list))
         (value (pcase invocation
                  ('key (execute-kbd-macro (kbd "M-.")))
                  ('command (call-interactively #'elisp-def))
                  (_ (error "ELISP-DEF unknown invocation: %S" invocation))))
         (return-time (float-time))
         (new-timers (seq-difference timer-list timers-before #'eq)))
    (unless (= (length new-timers) 1)
      (error "ELISP-DEF expected one new timer, got: %S" new-timers))
    (let* ((timer (car new-timers))
           (scheduled-before (and (memq timer timer-list) t))
           (delay-tenths
            (round (* 10 (- (float-time (timer--time timer))
                            return-time))))
           (target-buffer (current-buffer))
           (target-point (point))
           (target (edt-test-location))
           (highlight (edt-test-highlight-overlays target-buffer target-point))
           (jump-history (edt-test-xref-state))
           (origin-mark
            (with-current-buffer origin-buffer
              (list :mark (mark t)
                    :active (and mark-active t))))
           back after-dispatch)
      (unless (= (length highlight) 1)
        (error "ELISP-DEF expected one highlight overlay: %S" highlight))
      (pcase invocation
        ('key (execute-kbd-macro (kbd "M-,")))
        ('command (xref-go-back)))
      (setq back
            (list :location (edt-test-location)
                  :same-buffer (eq (current-buffer) origin-buffer)
                  :same-point (= (point) origin-point)
                  :history (edt-test-xref-state)))
      (timer-event-handler timer)
      (setq after-dispatch
            (list :scheduled (and (memq timer timer-list) t)
                  :highlight
                  (edt-test-highlight-overlays target-buffer target-point)))
      (list :invocation invocation
            :origin origin-before
            :public-return
            (list :timerp (timerp value)
                  :same-as-new-timer (eq value timer))
            :timer (list :new-count 1
                         :scheduled-before scheduled-before
                         :remaining-delay-tenths delay-tenths)
            :target target
            :highlight highlight
            :origin-mark origin-mark
            :jump-history jump-history
            :back back
            :after-dispatch after-dispatch))))

(defun edt-test-failure ()
  "Invoke public `elisp-def' and return its exact failure boundary."
  (let* ((origin-buffer (current-buffer))
         (origin-point (point))
         (before (list :location (edt-test-location)
                       :mark (mark t)
                       :mark-active (and mark-active t)
                       :xref (edt-test-xref-state)))
         (timers-before (copy-sequence timer-list))
         condition)
    (condition-case error-data
        (call-interactively #'elisp-def)
      (error (setq condition error-data)))
    (unless condition
      (error "ELISP-DEF expected public navigation to fail"))
    (list
     :condition
     (list :signal (car condition)
           :data (edt-test-plain (cdr condition))
           :message (substring-no-properties
                     (error-message-string condition)))
     :before before
     :after (list :location (edt-test-location)
                  :same-buffer (eq (current-buffer) origin-buffer)
                  :same-point (= (point) origin-point)
                  :mark (mark t)
                  :mark-active (and mark-active t)
                  :xref (edt-test-xref-state))
     :new-timers (length (seq-difference timer-list timers-before #'eq))
     :highlight (edt-test-highlight-overlays))))

(defun edt-test-strict-completing-read
    (prompt collection predicate require-match
            &optional initial-input history default inherit-input-method)
  "Consume one exact fail-closed completion plan."
  (unless edt-test-completion-plan
    (error "ELISP-DEF unexpected completing-read: %S" prompt))
  (let* ((plan (pop edt-test-completion-plan))
         (choice (plist-get plan :choice))
         (record (list prompt (copy-tree collection) predicate require-match
                       initial-input history default inherit-input-method)))
    (unless (and (equal prompt (plist-get plan :prompt))
                 (equal (mapcar (lambda (candidate)
                                  (if (symbolp candidate)
                                      (symbol-name candidate)
                                    candidate))
                                collection)
                        (plist-get plan :candidates))
                 (null predicate) (eq require-match t)
                 (null initial-input) (null history) (null default)
                 (null inherit-input-method)
                 (member choice collection))
      (error "ELISP-DEF completing-read contract mismatch: %S" record))
    (push record edt-test-completion-ledger)
    (symbol-name choice)))

(defun edt-test-completion-calls ()
  "Return completion calls in user order without sharing list structure."
  (reverse (copy-tree edt-test-completion-ledger)))

(defun edt-test-process-cleanup (process)
  "Terminate one unexpected owned PROCESS and fail if it survives."
  (set-process-query-on-exit-flag process nil)
  (set-process-sentinel process nil)
  (set-process-filter process nil)
  (when (process-live-p process) (delete-process process))
  (when (process-live-p process)
    (error "ELISP-DEF process survived cleanup: %S" process)))

(defun edt-test-prefixed-symbols ()
  "Return every interned symbol in the owned ed349 namespace."
  (let (symbols)
    (mapatoms (lambda (symbol)
                (when (or (string-prefix-p "ed349/" (symbol-name symbol))
                          (string-prefix-p "ed349-" (symbol-name symbol))
                          (string-prefix-p "make-ed349/" (symbol-name symbol)))
                  (push symbol symbols))))
    symbols))

(defun edt-test-run (name function)
  "Run FUNCTION in one fully owned definition-navigation world NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root) (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" name)
      (error "ELISP-DEF invalid case name: %S" name))
    (let* ((root (file-name-as-directory
                  (expand-file-name name sandbox-root)))
           (root-owned nil)
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (window-buffer-baseline (window-buffer))
           (completion-baseline completing-read-function)
           (hook-baseline (copy-sequence emacs-lisp-mode-hook))
           (load-history-baseline (copy-tree load-history))
           (placeholder-baseline elisp-def--placeholder-num)
           (prefixed-symbol-baseline (edt-test-prefixed-symbols))
           (edt-test-owned-overlays nil)
           (edt-test-completion-plan nil)
           (edt-test-completion-ledger nil)
           (edt-test-xref-history (xref--make-xref-history))
           (edt-test-features nil)
           (edt-test-runtime-functions nil)
           (xref-history-storage (lambda () edt-test-xref-history))
           (global-mark-ring nil)
           (emacs-lisp-mode-hook (copy-sequence hook-baseline))
           (load-history (copy-tree load-history-baseline))
           (load-path (cons root load-path))
           (default-directory root)
           (enable-local-variables nil)
           (enable-local-eval nil)
           (timer-event-last timer-event-last)
           (timer-event-last-1 timer-event-last-1)
           (timer-event-last-2 timer-event-last-2)
           result cleanup body-error cleanup-errors)
      (when (file-exists-p root)
        (error "ELISP-DEF owned case root already exists: %s" root))
      (cl-labels
          ((attempt
            (phase function)
            (condition-case condition
                (funcall function)
              (t (push (list phase condition) cleanup-errors) nil))))
        (unwind-protect
            (condition-case condition
                (progn
                  (unwind-protect
                      (make-directory root)
                    (when (file-directory-p root) (setq root-owned t)))
                  (unless root-owned
                    (error "ELISP-DEF failed to own case root: %s" root))
                  (save-window-excursion
                    (save-current-buffer
                      (setq result (funcall function root)))))
              (t (setq body-error condition)))
          (dolist (overlay edt-test-owned-overlays)
            (attempt 'overlay
                     (lambda ()
                       (when (overlayp overlay) (delete-overlay overlay)))))
          (dolist (timer (seq-difference timer-idle-list
                                          idle-timer-baseline #'eq))
            (attempt 'idle-timer
                     (lambda ()
                       (when (timerp timer) (cancel-timer timer)))))
          (dolist (timer (seq-difference timer-list timer-baseline #'eq))
            (attempt 'timer
                     (lambda ()
                       (when (timerp timer) (cancel-timer timer)))))
          (attempt 'xref #'edt-test-clear-xref)
          (dolist (feature edt-test-features)
            (attempt 'feature
                     (lambda ()
                       (when (featurep feature) (unload-feature feature t)))))
          (dolist (function edt-test-runtime-functions)
            (attempt 'runtime-function
                     (lambda ()
                       (when (fboundp function) (fmakunbound function)))))
          (dolist (symbol (seq-difference (edt-test-prefixed-symbols)
                                          prefixed-symbol-baseline #'eq))
            (attempt 'symbol
                     (lambda () (unintern (symbol-name symbol) obarray))))
          (dolist (process (seq-difference (process-list)
                                           process-baseline #'eq))
            (attempt 'process
                     (lambda () (edt-test-process-cleanup process))))
          (dolist (buffer (seq-difference (buffer-list)
                                          buffer-baseline #'eq))
            (attempt 'buffer
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer (set-buffer-modified-p nil))
                         (kill-buffer buffer)))))
          (attempt 'restore-globals
                   (lambda ()
                     (setq elisp-def--placeholder-num placeholder-baseline
                           load-history (copy-tree load-history-baseline))))
          (attempt 'root
                   (lambda ()
                     (when root-owned
                       (when (file-exists-p root) (delete-directory root t))
                       (unless (file-exists-p root) (setq root-owned nil)))))
          (dolist (buffer (seq-difference (buffer-list)
                                          buffer-baseline #'eq))
            (attempt 'late-buffer
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer (set-buffer-modified-p nil))
                         (kill-buffer buffer)))))
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
                    :highlight-overlays-live
                    (and (seq-some #'overlay-buffer edt-test-owned-overlays) t)
                    :xref (edt-test-xref-state)
                    :fixture-features-live
                    (delq nil (mapcar (lambda (feature)
                                        (and (featurep feature) feature))
                                      edt-test-features))
                    :runtime-functions-live
                    (delq nil (mapcar (lambda (function)
                                        (and (fboundp function) function))
                                      edt-test-runtime-functions))
                    :prefixed-symbols-live
                    (seq-difference (edt-test-prefixed-symbols)
                                    prefixed-symbol-baseline #'eq)
                    :root-exists (file-exists-p root)
                    :root-owned root-owned
                    :window-restored (eq (window-buffer)
                                         window-buffer-baseline)
                    :hook-restored (equal emacs-lisp-mode-hook hook-baseline)
                    :load-history-restored
                    (equal load-history load-history-baseline)
                    :placeholder-restored
                    (= elisp-def--placeholder-num placeholder-baseline)
                    :completion-adapter-restored
                    (eq completing-read-function completion-baseline)
                    :local-variables-disabled
                    (and (null enable-local-variables)
                         (null enable-local-eval))
                    :completion-remaining edt-test-completion-plan
                    :completion-calls (nreverse edt-test-completion-ledger)
                    :body-error body-error
                    :cleanup-errors (reverse (copy-tree cleanup-errors))))))
          (attempt 'normalize
                   (lambda ()
                     (setq result (edt-test-normalize result root)
                           cleanup (edt-test-normalize cleanup root)))))
      (when cleanup-errors
        (error "ELISP-DEF teardown failed: body=%S cleanup=%S"
               body-error (reverse cleanup-errors)))
      (when body-error
        (signal (car body-error) (cdr body-error)))
      (list :result result :cleanup cleanup)))))
"####;

fn elisp_def_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELISP_DEF_MELPA_PIN, "elisp-def.el")
        .expect("prepare exact shallow elisp-def source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact Dash dependency below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare exact f dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare exact s dependency below ./tmp")
        .with_prelude(ELISP_DEF_TEST_PRELUDE)
        .with_timeout(ELISP_DEF_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed elisp-def parity test")
        .into()
}

fn assert_elisp_def_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        elisp_def_oracle(),
        &current_test_name(),
        "elisp_def_parity",
        cases,
    );
}

#[test]
fn elisp_def_package_batch() {
    assert_elisp_def_batch(&workflows::public_workflow_cases());
}
