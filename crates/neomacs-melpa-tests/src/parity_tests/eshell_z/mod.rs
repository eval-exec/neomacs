use std::time::Duration;

use crate::{CachedMelpaOracle, ESHELL_Z_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ESHELL_Z_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ESHELL_Z_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'ring)
(require 'em-alias)
(require 'em-banner)
(require 'em-cmpl)
(require 'em-dirs)
(require 'em-hist)
(require 'em-prompt)
(require 'em-script)

(defconst esz-test-now "2000000000")
(defvar esz-test-owned-buffers nil)
(defvar esz-test-readonly-files nil)
(defvar esz-test-command-ledger nil)
(defvar esz-test-directory-events nil)
(defvar esz-test-completion-plan nil)
(defvar esz-test-completion-ledger nil)
(defvar esz-test-message-ledger nil)

(defun esz-test-normalize (object root)
  "Replace the validated owned ROOT prefix throughout OBJECT."
  (cond
   ((stringp object)
    (if (equal object (directory-file-name root))
        "[ROOT]"
      (replace-regexp-in-string (regexp-quote root) "[ROOT]/" object t t)))
   ((consp object)
    (cons (esz-test-normalize (car object) root)
          (esz-test-normalize (cdr object) root)))
   ((vectorp object)
    (apply #'vector (mapcar (lambda (value)
                              (esz-test-normalize value root))
                            object)))
   (t object)))

(defun esz-test-directory ()
  "Return the current directory without its trailing separator."
  (directory-file-name default-directory))

(defun esz-test-file-string (file)
  "Return FILE's exact bytes as an Emacs string, or nil when absent."
  (when (file-exists-p file)
    (with-temp-buffer
      (insert-file-contents-literally file)
      (decode-coding-string (buffer-string) 'utf-8))))

(defun esz-test-file-rows (file)
  "Return FILE's nonempty rows in deterministic lexical order."
  (sort (split-string (or (esz-test-file-string file) "") "\n" t)
        #'string<))

(defun esz-test-table-rows (&optional table)
  "Return TABLE's complete path/rank/time records in path order."
  (let ((table (or table eshell-z-freq-dir-hash-table)) values)
    (when (hash-table-p table)
      (maphash
       (lambda (_key value)
         (push (list (car value)
                     (plist-get (cdr value) :rank)
                     (plist-get (cdr value) :time))
               values))
       table))
    (sort values (lambda (left right) (string< (car left) (car right))))))

(defun esz-test-put (table path rank time)
  "Insert one exact PATH RANK TIME record into TABLE."
  (puthash path (list path :rank rank :time (format "%s" time)) table))

(defun esz-test-capture (function)
  "Return FUNCTION's exact value or nonlocal condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition) :data (cdr condition)))))

(defun esz-test-new-session (name directory)
  "Create and own a real selected Eshell session in DIRECTORY."
  (when (get-buffer name)
    (error "ESHELL-Z test buffer already exists: %s" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer esz-test-owned-buffers)
    (switch-to-buffer buffer)
    (setq default-directory (file-name-as-directory directory))
    (eshell-mode)
    (when (get-buffer-process buffer)
      (error "ESHELL-Z internal-command session unexpectedly owns a process"))
    buffer))

(defun esz-test-history ()
  "Return real newest-first Eshell history without text properties."
  (and (boundp 'eshell-history-ring)
       (ring-p eshell-history-ring)
       (mapcar #'substring-no-properties
               (ring-elements eshell-history-ring))))

(defun esz-test-send (command)
  "Submit COMMAND through real `eshell-send-input' and return its transition."
  (unless (derived-mode-p 'eshell-mode)
    (error "ESHELL-Z command submitted outside eshell-mode: %S" command))
  (let ((before (esz-test-directory))
        (start (point-max)))
    (goto-char (point-max))
    (insert command)
    (push command esz-test-command-ledger)
    (eshell-send-input)
    (when (get-buffer-process (current-buffer))
      (error "ESHELL-Z command unexpectedly started a process: %S" command))
    (list :input command
          :tail (buffer-substring-no-properties start (point-max))
          :before before :after (esz-test-directory)
          :point-at-end (= (point) (point-max))
          :history (esz-test-history))))

(defun esz-test-change-directory-observer ()
  "Record the real cwd observed before package-driven `eshell/cd'."
  (push (esz-test-directory) esz-test-directory-events))

(defun esz-test-strict-completing-read
    (prompt collection predicate require-match
            &optional initial-input history default inherit-input-method)
  "Consume one fail-closed unattended completion plan."
  (unless esz-test-completion-plan
    (error "ESHELL-Z unexpected completing-read: %S" prompt))
  (let* ((plan (pop esz-test-completion-plan))
         (choice (plist-get plan :choice))
         (record (list prompt (copy-tree collection) predicate require-match
                       initial-input history default inherit-input-method)))
    (unless (and (equal prompt (plist-get plan :prompt))
                 (null predicate) (eq require-match t)
                 (null initial-input) (null history) (null default)
                 (null inherit-input-method))
      (error "ESHELL-Z completing-read contract mismatch: %S" record))
    (unless (or (member choice collection)
                (assoc-string choice collection nil))
      (error "ESHELL-Z planned completion is not a member: %S in %S"
             choice collection))
    (push record esz-test-completion-ledger)
    choice))

(defun esz-test-message-observer (original format-string &rest arguments)
  "Record the read-only database warning and delegate to ORIGINAL."
  (let ((rendered (and format-string
                       (apply #'format-message format-string arguments))))
    (when (and rendered
               (or (string-prefix-p "Cannot write freq-dir-hash-table file "
                                    rendered)
                   (string-prefix-p "Expecting completion of delimiter "
                                    rendered)))
      (push rendered esz-test-message-ledger))
    (apply original format-string arguments)))

(defun esz-test-observe-messages (function)
  "Run FUNCTION while delegating and recording the one allowed warning seam."
  (let ((original (symbol-function 'message)))
    (cl-letf (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (apply #'esz-test-message-observer
                        original format-string arguments))))
      (funcall function))))

(defun esz-test-pcomplete (input)
  "Return real `pcomplete-completions' state for literal Eshell INPUT."
  (delete-region eshell-last-output-end (point-max))
  (goto-char (point-max))
  (insert input)
  (let ((result (pcomplete-completions)))
    (list :input input :point (point) :stub pcomplete-stub
          :result result)))

(defun esz-test-completion-at-point (input)
  "Drive public `completion-at-point' for literal Eshell INPUT."
  (delete-region eshell-last-output-end (point-max))
  (goto-char (point-max))
  (insert input)
  (let ((before (buffer-substring-no-properties
                 eshell-last-output-end (point-max)))
        (value (completion-at-point)))
    (list :before before :return value
          :after (buffer-substring-no-properties
                  eshell-last-output-end (point-max))
          :point-at-end (= (point) (point-max)))))

(defun esz-test-format-time-string
    (original format-string &optional time universal)
  "Return the fixed epoch only at the package's exact `%s' clock seam."
  (if (equal format-string "%s")
      (progn
        (unless (and (null time) (null universal))
          (error "ESHELL-Z unexpected %%s clock arguments: %S %S"
                 time universal))
        esz-test-now)
    (funcall original format-string time universal)))

(defun esz-test-process-cleanup (process)
  "Terminate one unexpected owned PROCESS and fail if it survives."
  (set-process-query-on-exit-flag process nil)
  (set-process-sentinel process nil)
  (set-process-filter process nil)
  (when (process-live-p process)
    (delete-process process))
  (when (process-live-p process)
    (error "ESHELL-Z owned process survived cleanup: %S" process)))

(defun esz-test-run (name function)
  "Run FUNCTION in one fail-closed real Eshell world named NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root) (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (let* ((root (file-name-as-directory
                  (expand-file-name name sandbox-root)))
           (root-owned nil)
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (hook-baseline (copy-sequence eshell-post-command-hook))
           (query-baseline (copy-sequence kill-emacs-query-functions))
           (original-format-time-string
            (symbol-function 'format-time-string))
           (esz-test-owned-buffers nil)
           (esz-test-readonly-files nil)
           (esz-test-command-ledger nil)
           (esz-test-directory-events nil)
           (esz-test-completion-plan nil)
           (esz-test-completion-ledger nil)
           (esz-test-message-ledger nil)
           (default-directory root)
           (eshell-directory-name (expand-file-name "state/" root))
           (eshell-history-file-name nil)
           (eshell-last-dir-ring-file-name nil)
           (eshell-aliases-file nil)
           (eshell-login-script nil)
           (eshell-rc-script nil)
           (eshell-banner-message "")
           (eshell-prompt-function (lambda () "Z> "))
           (eshell-prompt-regexp "^Z> ")
           (eshell-mode-hook nil)
           (eshell-first-time-p t)
           (eshell-z-freq-dir-hash-table-file-name nil)
           (eshell-z-freq-dir-hash-table nil)
           (eshell-z-exclude-dirs nil)
           (eshell-z--remove-p nil)
           (eshell-z-change-dir-hook nil)
           (eshell-z-change-dir-function eshell-z-change-dir-function)
           (kill-emacs-query-functions (copy-sequence query-baseline))
           result cleanup first-error)
      (when (file-exists-p root)
        (error "ESHELL-Z owned case root already exists: %s" root))
      (make-directory root)
      (setq root-owned t)
      (cl-letf (((symbol-function 'format-time-string)
                 (lambda (format-string &optional time universal)
                   (esz-test-format-time-string
                    original-format-time-string format-string time universal))))
        (condition-case condition
            (save-window-excursion
              (save-current-buffer
                (setq result (funcall function root))))
          (t (setq first-error condition))))
      (dolist (file esz-test-readonly-files)
        (condition-case condition
            (when (file-exists-p file) (set-file-modes file #o644))
          (t (unless first-error (setq first-error condition)))))
      (dolist (process (seq-difference (process-list) process-baseline #'eq))
        (condition-case condition
            (esz-test-process-cleanup process)
          (t (unless first-error (setq first-error condition)))))
      (dolist (timer (seq-difference timer-idle-list idle-timer-baseline #'eq))
        (condition-case condition
            (when (timerp timer) (cancel-timer timer))
          (t (unless first-error (setq first-error condition)))))
      (dolist (timer (seq-difference timer-list timer-baseline #'eq))
        (condition-case condition
            (when (timerp timer) (cancel-timer timer))
          (t (unless first-error (setq first-error condition)))))
      (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))
          (t (unless first-error (setq first-error condition)))))
      (setq kill-emacs-query-functions (copy-sequence query-baseline))
      (condition-case condition
          (when root-owned
            (when (file-exists-p root) (delete-directory root t))
            (unless (file-exists-p root) (setq root-owned nil)))
        (t (unless first-error (setq first-error condition))))
      ;; File decoding and recursive deletion can lazily allocate GNU's
      ;; internal conversion work buffer after the first ownership sweep.
      (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))
          (t (unless first-error (setq first-error condition)))))
      (condition-case condition
          (setq cleanup
                (list
                 :owned-reference-live
                 (and (seq-some #'buffer-live-p esz-test-owned-buffers) t)
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
                 :package-hooks-restored
                 (equal eshell-post-command-hook hook-baseline)
                 :package-hook-shape
                 (mapcar (lambda (function)
                           (if (eq function #'eshell-z--add) 'add 'remove))
                         (seq-filter
                          (lambda (function)
                            (memq function
                                  (list #'eshell-z--add
                                        #'eshell-z--remove)))
                          eshell-post-command-hook))
                 :query-functions-restored
                 (equal kill-emacs-query-functions query-baseline)
                 :completion-remaining esz-test-completion-plan
                 :commands (nreverse esz-test-command-ledger)
                 :cleanup-error first-error))
        (t (unless first-error (setq first-error condition))))
      (setq result (esz-test-normalize result root)
            cleanup (esz-test-normalize cleanup root))
      (when first-error
        (signal (car first-error) (cdr first-error)))
      (list :result result :cleanup cleanup))))
"####;

fn eshell_z_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ESHELL_Z_MELPA_PIN, "eshell-z.el")
        .expect("prepare exact shallow eshell-z source below ./tmp")
        .with_prelude(ESHELL_Z_TEST_PRELUDE)
        .with_timeout(ESHELL_Z_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed eshell-z parity test")
        .into()
}

fn assert_eshell_z_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        eshell_z_oracle(),
        &current_test_name(),
        "eshell_z_parity",
        cases,
    );
}

#[test]
fn eshell_z_package_batch() {
    assert_eshell_z_batch(&workflows::public_workflow_cases());
}
