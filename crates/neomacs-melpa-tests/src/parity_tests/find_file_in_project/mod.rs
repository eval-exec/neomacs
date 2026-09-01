use std::time::Duration;

use crate::{CachedMelpaOracle, FIND_FILE_IN_PROJECT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const FIND_FILE_IN_PROJECT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'dired)
(require 'diff-mode)
(require 'js)
(require 'find-file-in-project)

;; External UTF-8 process decoding uses this GNU scratch buffer lazily.  Make
;; it shared-prelude baseline rather than misclassifying it as package state.
(get-buffer-create " *code-conversion-work*")
(get-buffer-create "*scratch*")
;; The first real minibuffer session creates this alternate internal buffer.
;; Establish it as editor infrastructure before per-case ownership begins.
(get-buffer-create " *Minibuf-1*")
;; Completion and Eldoc install editor-global idle timers on the first genuine
;; minibuffer session.  Prime that editor infrastructure before case baselines
;; so package teardown never claims or cancels those unrelated timers.
(let ((executing-kbd-macro t)
      (unread-command-events (listify-key-sequence (kbd "RET"))))
  (minibuffer-with-setup-hook
      (lambda () (insert "baseline"))
    (completing-read "FFIP infrastructure baseline: "
                     '("baseline") nil t)))
;; The first set-window-configuration in a batch frame accounts for the
;; frame's reserved menu-bar row.  Establish that real GNU/Neo frame state as
;; prelude baseline rather than attributing it to the first package case.
(set-window-configuration (current-window-configuration))

(defvar ffip356-test-world-root nil)
(defvar ffip356-test-owned-buffers nil)
(defvar ffip356-test-find-plan nil)
(defvar ffip356-test-find-trace nil)
(defvar ffip356-test-git-plan nil)
(defvar ffip356-test-git-trace nil)
(defvar ffip356-test-git-expect nil)
(defvar ffip356-test-git-request nil)
(defvar ffip356-test-search-ledger nil)
(defvar ffip356-test-real-find nil)
(defvar ffip356-test-real-git nil)
(defvar ffip356-test-real-shell nil)
(defvar ffip356-test-real-cmp nil)
(defvar ffip356-test-input-plan nil)
(defvar ffip356-test-input-ledger nil)

(defun ffip356-test-delegating-search (find-command)
  "Validate FIND-COMMAND before delegating to the package's real shell seam."
  (let ((plan ffip356-test-find-plan)
        (cwd (ffip356-test-relative
              (directory-file-name default-directory)
              ffip356-test-world-root)))
    (unless plan
      (error "FIND-FILE-IN-PROJECT search ran without an armed plan: %S"
             find-command))
    (unless (and (equal find-command (plist-get plan :command))
                 (equal cwd (plist-get plan :cwd)))
      (error "FIND-FILE-IN-PROJECT rejected shell command before execution: expected=%S actual=%S cwd=%S"
             (plist-get plan :command) find-command cwd))
    (push (list :command find-command :cwd cwd)
          ffip356-test-search-ledger)
    (ffip-project-search-default-function find-command)))

(defun ffip356-test-relative (path root)
  "Return PATH relative to ROOT while retaining a directory suffix."
  (when path
    (let ((relative (file-relative-name path root)))
      (if (and (string-suffix-p "/" path)
               (not (string-suffix-p "/" relative)))
          (concat relative "/")
        relative))))

(defun ffip356-test-owned-path (root relative)
  "Resolve RELATIVE below owned ROOT or fail closed."
  (unless (and (stringp relative)
               (not (file-name-absolute-p relative))
               (not (equal relative ".."))
               (not (string-prefix-p "../" relative)))
    (error "FIND-FILE-IN-PROJECT invalid fixture path: %S" relative))
  (let* ((path (expand-file-name relative root))
         (parent (file-name-directory path)))
    (make-directory parent t)
    (unless (file-in-directory-p (file-truename parent)
                                 (file-truename root))
      (error "FIND-FILE-IN-PROJECT fixture escaped root: %s" path))
    path))

(defun ffip356-test-write-file (root relative contents)
  "Write CONTENTS to owned RELATIVE path under ROOT."
  (let ((path (ffip356-test-owned-path root relative)))
    (write-region contents nil path nil 'silent)
    path))

(defun ffip356-test-file-bytes (file)
  "Read FILE as exact text bytes using UTF-8 Unix decoding."
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents file))
    (buffer-substring-no-properties (point-min) (point-max))))

(defun ffip356-test-own-buffer (buffer root)
  "Register BUFFER after proving its file or directory belongs to ROOT."
  (unless (buffer-live-p buffer)
    (error "FIND-FILE-IN-PROJECT cannot own dead buffer: %S" buffer))
  (with-current-buffer buffer
    (let ((file buffer-file-name)
          (directory default-directory))
      (unless (or (equal (buffer-name buffer) "*ffip-diff*")
                  (and file
                       (file-in-directory-p
                        (file-truename (file-name-directory file))
                        (file-truename root)))
                  (and directory
                       (file-in-directory-p (file-truename directory)
                                            (file-truename root))))
        (error "FIND-FILE-IN-PROJECT refuses unowned buffer: %S file=%S dir=%S"
               (buffer-name buffer) file directory))))
  (cl-pushnew buffer ffip356-test-owned-buffers :test #'eq)
  buffer)

(defun ffip356-test-visit (root relative)
  "Visit owned RELATIVE file below ROOT with ambient locals disabled."
  (let ((enable-local-variables nil)
        (enable-dir-local-variables nil)
        (enable-local-eval nil))
    (ffip356-test-own-buffer
     (find-file-noselect (ffip356-test-owned-path root relative)) root)))

(defun ffip356-test-capture (function)
  "Return FUNCTION's value or its exact signaled condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition)
             :data (cdr condition)
             :message (error-message-string condition)))))

(defun ffip356-test-observe-messages (function)
  "Call FUNCTION while recording and delegating every real `message'."
  (let ((original (symbol-function 'message)) messages outcome)
    (cl-letf (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((text (and format-string
                                  (apply #'format-message
                                         format-string arguments))))
                   (push text messages)
                   (apply original format-string arguments)))))
      (setq outcome (ffip356-test-capture function)))
    (list :outcome outcome :messages (nreverse messages))))

(defun ffip356-test-install-wrappers (root)
  "Install owned pass-through Find and Git executables below ROOT."
  (unless (and (stringp ffip356-test-real-shell)
               (file-name-absolute-p ffip356-test-real-shell)
               (file-executable-p ffip356-test-real-shell))
    (error "FIND-FILE-IN-PROJECT has no absolute executable shell"))
  (dolist (tool '(find git))
    (let* ((name (format "ffip356-%s" tool))
           (script (ffip356-test-owned-path root (concat "bin/" name)))
           (real-var (if (eq tool 'find)
                         "FFIP356_REAL_FIND" "FFIP356_REAL_GIT"))
           (trace-var (if (eq tool 'find)
                          "FFIP356_FIND_TRACE" "FFIP356_GIT_TRACE"))
           (mode-var (if (eq tool 'find)
                         "FFIP356_FIND_MODE" "FFIP356_GIT_MODE")))
      (write-region
       (concat
        "#!" ffip356-test-real-shell "\n"
        "set -u\n"
        "mode=${" mode-var "-}\n"
        "trace=${" trace-var "-}\n"
        "real=${" real-var "-}\n"
        "if [ -z \"$mode\" ] || [ -z \"$trace\" ] || [ -z \"$real\" ]; then exit 98; fi\n"
        (if (eq tool 'git)
            (concat
             "expected=${FFIP356_GIT_EXPECT-}\n"
             "request=${FFIP356_GIT_REQUEST-}\n"
             "cmp=${FFIP356_REAL_CMP-}\n"
             "if [ -z \"$expected\" ] || [ -z \"$request\" ] || [ -z \"$cmp\" ]; then exit 98; fi\n"
             "{ printf '%s\\n%s\\n' \"$PWD\" \"$#\"; for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done; } > \"$request\"\n"
             "if ! \"$cmp\" \"$expected\" \"$request\" >/dev/null 2>&1; then exit 97; fi\n")
          "")
        "{ printf 'CALL\\0%s\\0%s\\0%s\\0' \"$mode\" \"$PWD\" \"$#\"; "
        "for arg in \"$@\"; do printf '%s\\0' \"$arg\"; done; } >> \"$trace\"\n"
        "case \"$mode\" in\n"
        "  delegate) exec \"$real\" \"$@\" ;;\n"
        "  quiet) exit 47 ;;\n"
        "  diagnostic) printf 'controlled find failure Ω\\n' >&2; exit 47 ;;\n"
        "  *) printf 'unexpected owned adapter mode: %s\\n' \"$mode\" >&2; exit 97 ;;\n"
        "esac\n")
       nil script nil 'silent)
      (set-file-modes script #o700)))
  (let ((bin (file-name-as-directory (expand-file-name "bin" root))))
    (setq exec-path (cons (directory-file-name bin) exec-path))
    (setenv "PATH" (concat (directory-file-name bin)
                            path-separator (or (getenv "PATH") "")))
    (setenv "FFIP356_REAL_FIND" ffip356-test-real-find)
    (setenv "FFIP356_REAL_GIT" ffip356-test-real-git)
    (setenv "FFIP356_FIND_TRACE" ffip356-test-find-trace)
    (setenv "FFIP356_GIT_TRACE" ffip356-test-git-trace))
    (setenv "FFIP356_GIT_EXPECT" ffip356-test-git-expect)
    (setenv "FFIP356_GIT_REQUEST" ffip356-test-git-request)
    (setenv "FFIP356_REAL_CMP" ffip356-test-real-cmp))

(defun ffip356-test-arm-tool (tool mode cwd argv &optional command)
  "Arm TOOL for one exact MODE, CWD, ARGV, and Find COMMAND call."
  (unless (and (memq tool '(find git))
               (memq mode '(delegate quiet diagnostic))
               (stringp cwd) (listp argv) (seq-every-p #'stringp argv))
    (error "FIND-FILE-IN-PROJECT invalid tool plan: %S %S %S %S"
           tool mode cwd argv))
  (let* ((trace (if (eq tool 'find)
                    ffip356-test-find-trace ffip356-test-git-trace))
         (plan-symbol (if (eq tool 'find)
                          'ffip356-test-find-plan 'ffip356-test-git-plan))
         (mode-variable (if (eq tool 'find)
                            "FFIP356_FIND_MODE" "FFIP356_GIT_MODE")))
    (when (symbol-value plan-symbol)
      (error "FIND-FILE-IN-PROJECT tool plan already armed: %S"
             (symbol-value plan-symbol)))
    (when (file-exists-p trace)
      (unless (= (file-attribute-size (file-attributes trace)) 0)
        (error "FIND-FILE-IN-PROJECT stale tool trace: %s" trace))
      (delete-file trace))
    (when (and (eq tool 'find) (not (stringp command)))
      (error "FIND-FILE-IN-PROJECT Find plan lacks exact shell command"))
    (when (eq tool 'git)
      (unless (and (stringp ffip356-test-real-cmp)
                   (file-name-absolute-p ffip356-test-real-cmp)
                   (file-executable-p ffip356-test-real-cmp)
                   (seq-every-p
                    (lambda (value) (not (string-match-p "[\n\r]" value)))
                    (cons cwd argv)))
        (error "FIND-FILE-IN-PROJECT invalid Git pre-exec plan"))
      (write-region
       (concat
        (mapconcat #'identity
                   (append
                    (list (directory-file-name
                           (expand-file-name cwd ffip356-test-world-root))
                          (number-to-string (length argv)))
                    argv)
                   "\n")
        "\n")
       nil ffip356-test-git-expect nil 'silent))
    (set plan-symbol (append (list :tool tool :mode mode :cwd cwd :argv argv)
                             (and command (list :command command))))
    (setenv mode-variable (symbol-name mode))))

(defun ffip356-test-read-tool-trace (tool)
  "Parse TOOL's exact NUL-delimited owned trace."
  (let ((trace (if (eq tool 'find)
                   ffip356-test-find-trace ffip356-test-git-trace)))
    (unless (file-exists-p trace)
      (error "FIND-FILE-IN-PROJECT missing %s trace" tool))
    (let* ((bytes (with-temp-buffer
                    (set-buffer-multibyte nil)
                    (insert-file-contents-literally trace)
                    (buffer-string)))
           (fields (split-string (decode-coding-string bytes 'utf-8) "\0" t))
           records)
      (while fields
        (unless (equal (pop fields) "CALL")
          (error "FIND-FILE-IN-PROJECT malformed %s trace marker" tool))
        (let* ((mode (intern (or (pop fields) "")))
               (cwd (or (pop fields) ""))
               (argc-string (or (pop fields) ""))
               (argc (string-to-number argc-string))
               argv)
          (unless (equal argc-string (number-to-string argc))
            (error "FIND-FILE-IN-PROJECT malformed argc: %S" argc-string))
          (dotimes (_ argc)
            (unless fields
              (error "FIND-FILE-IN-PROJECT truncated %s trace" tool))
            (push (pop fields) argv))
          (push (list :tool tool :mode mode
                      :cwd (ffip356-test-relative cwd ffip356-test-world-root)
                      :argv (nreverse argv))
                records)))
      (nreverse records))))

(defun ffip356-test-finish-tool (tool)
  "Validate and consume TOOL's one armed invocation."
  (let* ((plan-symbol (if (eq tool 'find)
                          'ffip356-test-find-plan 'ffip356-test-git-plan))
         (plan (symbol-value plan-symbol))
         (trace-plan (copy-sequence plan))
         (records (ffip356-test-read-tool-trace tool))
         (trace (if (eq tool 'find)
                    ffip356-test-find-trace ffip356-test-git-trace)))
    (unless plan
      (error "FIND-FILE-IN-PROJECT %s ran without a plan: %S" tool records))
    (setq trace-plan (plist-put trace-plan :command nil))
    (setq trace-plan (cl-loop for (key value) on trace-plan by #'cddr
                              unless (eq key :command)
                              append (list key value)))
    (unless (equal records (list trace-plan))
      (error "FIND-FILE-IN-PROJECT %s contract mismatch: expected=%S actual=%S"
             tool trace-plan records))
    (when (eq tool 'find)
      (unless (equal (nreverse ffip356-test-search-ledger)
                     (list (list :command (plist-get plan :command)
                                 :cwd (plist-get plan :cwd))))
        (error "FIND-FILE-IN-PROJECT search call mismatch: %S"
               ffip356-test-search-ledger))
      (setq ffip356-test-search-ledger nil))
    (set plan-symbol nil)
    (delete-file trace)
    (when (eq tool 'git)
      (unless (file-exists-p ffip356-test-git-request)
        (error "FIND-FILE-IN-PROJECT Git request evidence is missing"))
      (when (file-exists-p ffip356-test-git-expect)
        (delete-file ffip356-test-git-expect))
      (when (file-exists-p ffip356-test-git-request)
        (delete-file ffip356-test-git-request)))
    records))

(defun ffip356-test-assert-no-tool-call (tool)
  "Prove TOOL has no armed or recorded call."
  (let ((plan (if (eq tool 'find)
                  ffip356-test-find-plan ffip356-test-git-plan))
        (trace (if (eq tool 'find)
                   ffip356-test-find-trace ffip356-test-git-trace)))
    (when (or plan (and trace (file-exists-p trace)))
      (error "FIND-FILE-IN-PROJECT unexpected %s call: plan=%S trace=%S"
             tool plan (and trace (file-exists-p trace)))))
  t)

(defun ffip356-test-minibuffer-exit ()
  "Append exact final input to the current real minibuffer observation."
  (unless ffip356-test-input-ledger
    (error "FIND-FILE-IN-PROJECT minibuffer exited without setup"))
  (setcar ffip356-test-input-ledger
          (append (car ffip356-test-input-ledger)
                  (list :final-input
                        (minibuffer-contents-no-properties)))))

(defun ffip356-test-minibuffer-setup ()
  "Observe a real minibuffer and feed the next fail-closed input plan."
  (unless ffip356-test-input-plan
    (error "FIND-FILE-IN-PROJECT unexpected extra minibuffer: %S"
           (minibuffer-prompt)))
  (let* ((input (pop ffip356-test-input-plan))
         (table minibuffer-completion-table)
         (predicate minibuffer-completion-predicate)
         (metadata (and table (completion-metadata "" table predicate)))
         (candidates
          (and table
               (sort (mapcar #'substring-no-properties
                             (all-completions "" table predicate))
                     #'string<))))
    (push (list :prompt (minibuffer-prompt)
                :initial-input (minibuffer-contents-no-properties)
                :require-match minibuffer--require-match
                :category (and metadata
                               (completion-metadata-get metadata 'category))
                :candidates candidates)
          ffip356-test-input-ledger)
    (add-hook 'minibuffer-exit-hook #'ffip356-test-minibuffer-exit nil t)
    (setq unread-command-events
          (append (string-to-list (plist-get input :text))
                  (listify-key-sequence
                   (kbd (or (plist-get input :keys) "RET")))
                  unread-command-events))))

(defun ffip356-test-drive-input (function inputs)
  "Call FUNCTION through real minibuffers using exact INPUTS."
  (when (or unread-command-events (active-minibuffer-window))
    (error "FIND-FILE-IN-PROJECT dirty input before session"))
  (let ((executing-kbd-macro t)
        (completing-read-function #'completing-read-default)
        (ffip356-test-input-plan (copy-tree inputs))
        (ffip356-test-input-ledger nil))
    (minibuffer-with-setup-hook #'ffip356-test-minibuffer-setup
      (funcall function))
    (when (or ffip356-test-input-plan unread-command-events
              (active-minibuffer-window))
      (error "FIND-FILE-IN-PROJECT incomplete input: plan=%S events=%S minibuffer=%S"
             ffip356-test-input-plan unread-command-events
             (active-minibuffer-window)))
    (nreverse ffip356-test-input-ledger)))

(defun ffip356-test-git (directory &rest argv)
  "Run absolute real Git with ARGV in owned DIRECTORY or fail closed."
  (unless (and (stringp ffip356-test-real-git)
               (file-name-absolute-p ffip356-test-real-git)
               (file-executable-p ffip356-test-real-git))
    (error "FIND-FILE-IN-PROJECT has no absolute executable Git"))
  (unless (file-in-directory-p (file-truename directory)
                               (file-truename ffip356-test-world-root))
    (error "FIND-FILE-IN-PROJECT Git cwd escaped root: %S" directory))
  (let ((default-directory (file-name-as-directory directory))
        (process-environment (copy-sequence process-environment))
        (coding-system-for-read 'utf-8-unix)
        (coding-system-for-write 'utf-8-unix)
        status output)
    (setenv "FFIP356_GIT_TRACE" nil)
    (with-temp-buffer
      (setq status
            (apply #'process-file ffip356-test-real-git nil
                   (current-buffer) nil argv)
            output (buffer-string)))
    (unless (and (integerp status) (zerop status))
      (error "FIND-FILE-IN-PROJECT fixture Git failed: cwd=%S argv=%S status=%S output=%S"
             (ffip356-test-relative directory ffip356-test-world-root)
             argv status output))
    output))

(defun ffip356-test-init-git (root relative)
  "Initialize and commit a deterministic real Git repository."
  (let* ((repo (file-name-as-directory
                (ffip356-test-owned-path root relative)))
         (process-environment (copy-sequence process-environment)))
    (make-directory repo t)
    (setenv "GIT_AUTHOR_NAME" "Parity Author")
    (setenv "GIT_AUTHOR_EMAIL" "parity@example.invalid")
    (setenv "GIT_COMMITTER_NAME" "Parity Author")
    (setenv "GIT_COMMITTER_EMAIL" "parity@example.invalid")
    (setenv "GIT_AUTHOR_DATE" "2001-02-03T04:05:06+0000")
    (setenv "GIT_COMMITTER_DATE" "2001-02-03T04:05:06+0000")
    (ffip356-test-git repo "init" "--quiet" "--initial-branch=main")
    (ffip356-test-git repo "config" "user.name" "Parity Author")
    (ffip356-test-git repo "config" "user.email" "parity@example.invalid")
    (ffip356-test-git repo "config" "commit.gpgsign" "false")
    (ffip356-test-git repo "add" "--all")
    (ffip356-test-git repo "commit" "--quiet" "--no-gpg-sign" "-m" "baseline")
    repo))

(defun ffip356-test-property-runs (start end)
  "Return exact relevant property runs between START and END."
  (font-lock-ensure start end)
  (let ((position start) runs)
    (while (< position end)
      (let* ((face (get-text-property position 'face))
             (font-lock-face (get-text-property position 'font-lock-face))
             (next (or (next-property-change position nil end) end)))
        (push (list :text (buffer-substring-no-properties position next)
                    :face face :font-lock-face font-lock-face)
              runs)
        (setq position next)))
    (nreverse runs)))

(defun ffip356-test-window-structure ()
  "Return the stable structure and ownership-relevant state of every window."
  (mapcar
   (lambda (window)
     (list :window window
           :buffer (window-buffer window)
           :edges (window-edges window)
           :dedicated (window-dedicated-p window)
           :parameters
           (copy-tree
            (seq-filter #'cdr (window-parameters window)))
           :prev-buffers (copy-tree (window-prev-buffers window))
           :next-buffers (copy-tree (window-next-buffers window))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil 'no-minibuf)))

(defun ffip356-test-restore-window-parameters (baseline)
  "Restore every window parameter represented by BASELINE."
  (dolist (entry baseline)
    (let* ((window (plist-get entry :window))
           (expected (plist-get entry :parameters)))
      (unless (window-live-p window)
        (error "baseline window died before parameter restoration: %S" window))
      (dolist (parameter (window-parameters window))
        (set-window-parameter window (car parameter) nil))
      (dolist (parameter expected)
        (set-window-parameter window (car parameter) (cdr parameter)))
      (set-window-prev-buffers
       window (copy-tree (plist-get entry :prev-buffers)))
      (set-window-next-buffers
       window (copy-tree (plist-get entry :next-buffers))))))

(defun ffip356-test-normalize (value root)
  "Normalize owned paths and executable identities recursively in VALUE."
  (cond
   ((stringp value)
    (let* ((root-directory (file-name-as-directory root))
           (root-name (directory-file-name root-directory))
           (result (copy-sequence value)))
      (setq result (replace-regexp-in-string
                    (regexp-quote root-directory) "[ROOT]/" result t t))
      (setq result (replace-regexp-in-string
                    (regexp-quote root-name) "[ROOT]" result t t))
      (when (equal result ffip356-test-real-find) (setq result "[FIND]"))
      (when (equal result ffip356-test-real-git) (setq result "[GIT]"))
      result))
   ((consp value)
    (cons (ffip356-test-normalize (car value) root)
          (ffip356-test-normalize (cdr value) root)))
   ((vectorp value)
    (apply #'vector
           (mapcar (lambda (item) (ffip356-test-normalize item root))
                   value)))
   (t value)))

(defun ffip356-test-run (case-name body)
  "Run BODY in one owned shared-batch world named CASE-NAME."
  (let ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox) (not (string-empty-p sandbox))
                 (file-name-absolute-p sandbox)
                 (file-directory-p sandbox))
      (error "FIND-FILE-IN-PROJECT requires absolute NEOMACS_TEST_SANDBOX_ROOT: %S"
             sandbox))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" case-name)
      (error "FIND-FILE-IN-PROJECT invalid case name: %S" case-name))
    (let* ((root (file-name-as-directory
                  (expand-file-name (concat "find-file-in-project-" case-name)
                                    sandbox)))
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (current-buffer-baseline (current-buffer))
           (selected-window-baseline (selected-window))
           (window-buffer-baseline (window-buffer))
           (window-configuration-baseline (current-window-configuration))
           (window-list-baseline (window-list nil 'no-minibuf))
           (window-structure-baseline (ffip356-test-window-structure))
           (dired-buffers-baseline (copy-tree dired-buffers))
           (transient-mark-baseline transient-mark-mode)
           (advice-baseline
            (let ((count 0))
              (advice-mapc
               (lambda (function _props)
                 (when (eq function #'ffip-read-file-name-hack)
                   (setq count (1+ count))))
               'read-file-name)
              count))
           (root-owned nil)
           restored-window-configuration
           owned-buffers owned-processes owned-timers
           result body-error cleanup cleanup-errors)
      (when (file-exists-p root)
        (error "FIND-FILE-IN-PROJECT refuses preexisting case root: %s" root))
      (when (get-buffer "*ffip-diff*")
        (error "FIND-FILE-IN-PROJECT refuses preexisting *ffip-diff* buffer"))
      (make-directory root)
      (setq root-owned t)
      (let* ((ffip356-test-world-root root)
             (ffip356-test-owned-buffers nil)
             (ffip356-test-find-plan nil)
             (ffip356-test-git-plan nil)
             (ffip356-test-search-ledger nil)
             (ffip356-test-find-trace
              (expand-file-name "find.trace" root))
             (ffip356-test-git-trace
              (expand-file-name "git.trace" root))
             (ffip356-test-git-expect
              (expand-file-name "git.expected" root))
             (ffip356-test-git-request
              (expand-file-name "git.request" root))
             (ffip356-test-real-find (executable-find "find" t))
             (ffip356-test-real-git (executable-find "git" t))
             (ffip356-test-real-shell (executable-find "sh" t))
             (ffip356-test-real-cmp (executable-find "cmp" t))
             (process-environment (copy-sequence process-environment))
             (exec-path (copy-sequence exec-path))
             (temporary-file-directory
              (file-name-as-directory
               (ffip356-test-owned-path root "emacs-temp")))
             (default-directory root)
             (ffip-project-root nil)
             (ffip-project-root-function nil)
             (ffip-project-search-function
              #'ffip356-test-delegating-search)
             (ffip-project-file '(".svn" ".hg" ".git"))
             (ffip-find-executable "ffip356-find")
             (ffip-use-rust-fd nil)
             (ffip-find-options "")
             (ffip-find-pre-path-options "")
             (ffip-patterns nil)
             (ffip-prune-patterns '("*/.git"))
             (ffip-ignore-filenames nil)
             (ffip-match-path-instead-of-filename nil)
             (ffip-prefer-ido-mode nil)
             (ffip-find-files-history nil)
             (ffip-filename-history nil)
             (ffip-find-files-history-max-items 4)
             (ffip-find-relative-path-callback #'ffip-copy-without-change)
             (ffip-diff-backends nil)
             (ffip-diff-find-file-before-hook nil)
             (ffip-diff-apply-hunk-hook nil)
             (ffip-diff-find-file-by-file-name-p nil)
             (ffip-read-file-name-hijacked-p nil)
             (ffip-debug nil)
             (unread-command-events nil)
             (executing-kbd-macro nil)
             (completing-read-function #'completing-read-default)
             (minibuffer-setup-hook (copy-sequence minibuffer-setup-hook))
             (minibuffer-exit-hook (copy-sequence minibuffer-exit-hook))
             (ido-setup-hook (copy-sequence ido-setup-hook))
             (ido-minibuffer-setup-hook
              (copy-sequence ido-minibuffer-setup-hook))
             (kill-ring (copy-tree kill-ring))
             (kill-ring-yank-pointer kill-ring)
             (transient-mark-mode transient-mark-baseline)
             (interprogram-cut-function nil)
             (interprogram-paste-function nil)
             (save-interprogram-paste-before-kill nil)
             (kill-transform-function nil)
             (kill-do-not-save-duplicates nil)
             (file-name-history (copy-tree file-name-history))
             (minibuffer-history (copy-tree minibuffer-history))
             (enable-local-variables nil)
             (enable-dir-local-variables nil)
             (enable-local-eval nil))
        (setenv "TMPDIR" temporary-file-directory)
        (make-directory temporary-file-directory t)
        (setenv "HOME" (ffip356-test-owned-path root "home"))
        (make-directory (getenv "HOME") t)
        (setenv "GIT_CONFIG_NOSYSTEM" "1")
        (setenv "GIT_CONFIG_GLOBAL" (expand-file-name ".gitconfig" (getenv "HOME")))
        (unless (and (file-name-absolute-p ffip356-test-real-find)
                     (file-executable-p ffip356-test-real-find)
                     (file-name-absolute-p ffip356-test-real-git)
                     (file-executable-p ffip356-test-real-git))
          (error "FIND-FILE-IN-PROJECT missing real tools: find=%S git=%S"
                 ffip356-test-real-find ffip356-test-real-git))
        (unless (and (file-name-absolute-p ffip356-test-real-cmp)
                     (file-executable-p ffip356-test-real-cmp))
          (error "FIND-FILE-IN-PROJECT missing absolute cmp: %S"
                 ffip356-test-real-cmp))
        (ffip356-test-install-wrappers root)
        (cl-labels
            ((attempt
              (phase function)
              (condition-case condition
                  (funcall function)
                (t (push (list phase condition) cleanup-errors) nil)))
             (inside-root-p
              (path)
              (and (stringp path)
                   (file-name-absolute-p path)
                   (file-in-directory-p (expand-file-name path)
                                        (expand-file-name root))))
             (drain
              (phase)
              (attempt phase
                       (lambda ()
                         (dotimes (_ 3)
                           (accept-process-output nil 0.01)))))
             (stop-processes
              (phase)
              (dolist (process (seq-difference (process-list)
                                               process-baseline #'eq))
                (attempt
                 phase
                 (lambda ()
                   (let* ((command (process-command process))
                          (buffer (process-buffer process))
                          (directory
                           (and (buffer-live-p buffer)
                                (buffer-local-value 'default-directory
                                                    buffer)))
                          (program (car-safe command))
                          (allowed
                           (delq nil
                                 (list ffip356-test-real-find
                                       ffip356-test-real-git
                                       ffip356-test-real-shell
                                       (expand-file-name
                                        "bin/ffip356-find" root)
                                       (expand-file-name
                                        "bin/ffip356-git" root)))))
                     (unless (and (stringp program)
                                  (seq-some
                                   (lambda (item)
                                     (and (file-exists-p item)
                                          (file-exists-p program)
                                          (file-equal-p program item)))
                                   allowed)
                                  (inside-root-p directory))
                       (error "refusing to stop unowned process: %S command=%S directory=%S"
                              (process-name process) command directory)))
                   (cl-pushnew process owned-processes :test #'eq)
                   (set-process-query-on-exit-flag process nil)
                   (when (process-live-p process) (delete-process process))
                   (let ((deadline (+ (float-time) 1.0)))
                     (while (and (process-live-p process)
                                 (< (float-time) deadline))
                       (accept-process-output process 0.01)))
                   (when (process-live-p process)
                     (error "owned process remained live: %S" process))))))
             (cancel-timers
              (phase)
              (dolist (timer
                       (delete-dups
                        (append
                         (seq-difference timer-list timer-baseline #'eq)
                         (seq-difference timer-idle-list
                                         idle-timer-baseline #'eq))))
                (attempt
                 phase
                 (lambda ()
                   (let ((arguments (timer--args timer)))
                     (unless
                         (or (memq timer owned-timers)
                             (and (eq (timer--function timer)
                                      'undo-auto--boundary-timer)
                                  ffip356-test-owned-buffers)
                             (and (eq (timer--function timer)
                                      'completions--background-update)
                                  (equal arguments '(t)))
                             (seq-some
                              (lambda (argument)
                                (and (bufferp argument)
                                     (memq argument
                                           ffip356-test-owned-buffers)))
                              arguments))
                       (error "refusing to cancel unowned timer: function=%S args=%S idle=%S"
                              (timer--function timer) arguments
                              (memq timer timer-idle-list))))
                   (cl-pushnew timer owned-timers :test #'eq)
                   (cancel-timer timer)))))
             (kill-buffers
              (phase)
              (let ((candidates
                     (delete-dups
                      (append (copy-sequence ffip356-test-owned-buffers)
                              (seq-difference (buffer-list)
                                              buffer-baseline #'eq)))))
                (dolist (buffer candidates)
                  (attempt
                   phase
                   (lambda ()
                     (when (buffer-live-p buffer)
                       (let ((file (buffer-local-value 'buffer-file-name
                                                       buffer))
                             (directory
                              (buffer-local-value 'default-directory buffer)))
                         (when (or (inside-root-p file)
                                   (inside-root-p directory)
                                   (equal (buffer-name buffer)
                                          "*ffip-diff*"))
                           (cl-pushnew buffer ffip356-test-owned-buffers
                                       :test #'eq))
                         (unless (memq buffer ffip356-test-owned-buffers)
                           (error "refusing to kill unowned buffer: %S file=%S directory=%S"
                                  (buffer-name buffer) file directory)))
                       (with-current-buffer buffer
                         (set-buffer-modified-p nil))
                       (kill-buffer buffer)))))))
             (restore-windows
              (phase)
              (attempt
               phase
               (lambda ()
                 (set-window-configuration window-configuration-baseline)
                 (ffip356-test-restore-window-parameters
                  window-structure-baseline)
                 (unless (and (window-live-p selected-window-baseline)
                              (buffer-live-p window-buffer-baseline))
                   (error "baseline window or buffer died during workflow"))
                 (unless (equal (ffip356-test-window-structure)
                                window-structure-baseline)
                   (error "window structure was not restored: baseline=%S actual=%S"
                          window-structure-baseline
                          (ffip356-test-window-structure)))
                 (unless (compare-window-configurations
                          (current-window-configuration)
                          window-configuration-baseline)
                   (error "baseline window configuration was not restored"))
                 (setq restored-window-configuration
                       (current-window-configuration))))))
          (unwind-protect
              (condition-case condition
                  (setq result (funcall body root))
                (t (setq body-error condition)))
            (attempt
             'input
             (lambda ()
               (setq unread-command-events nil)
               (when (active-minibuffer-window)
                 (error "active minibuffer remains during cleanup"))))
            (restore-windows 'window-first)
            (attempt
             'tool-ledger
             (lambda ()
               (when (or ffip356-test-find-plan ffip356-test-git-plan
                         ffip356-test-search-ledger)
                 (error "unconsumed plans: find=%S git=%S search=%S"
                        ffip356-test-find-plan ffip356-test-git-plan
                        ffip356-test-search-ledger))
               (when (or (file-exists-p ffip356-test-find-trace)
                         (file-exists-p ffip356-test-git-trace)
                         (file-exists-p ffip356-test-git-expect)
                         (file-exists-p ffip356-test-git-request))
                 (error "unconsumed tool artifact remains"))))
            (drain 'drain-first)
            (stop-processes 'processes-first)
            (cancel-timers 'timers-first)
            (kill-buffers 'buffers-first)
            (drain 'drain-second)
            (stop-processes 'processes-second)
            (cancel-timers 'timers-second)
            (kill-buffers 'buffers-second)
            (attempt
             'current-buffer
             (lambda ()
               (when (buffer-live-p current-buffer-baseline)
                 (set-buffer current-buffer-baseline))))
            (drain 'drain-final)
            (stop-processes 'processes-final)
            (cancel-timers 'timers-final)
            (kill-buffers 'buffers-final)
            (restore-windows 'window-final)
            (attempt
             'dired-registry
             (lambda ()
               (setq dired-buffers (copy-tree dired-buffers-baseline))))
            (attempt
             'root
             (lambda ()
               (when root-owned
                 (unless (and (file-name-absolute-p root)
                              (file-equal-p
                               (file-name-directory
                                (directory-file-name root))
                               sandbox))
                   (error "refusing unsafe root deletion: %S" root))
                 (when (file-exists-p root) (delete-directory root t))
                 (unless (file-exists-p root) (setq root-owned nil)))))
            (attempt
             'state
             (lambda ()
               (let ((advice-after 0))
                 (advice-mapc
                  (lambda (function _props)
                    (when (eq function #'ffip-read-file-name-hack)
                      (setq advice-after (1+ advice-after))))
                  'read-file-name)
                 (setq cleanup
                       (list
                        :new-buffers
                        (mapcar #'buffer-name
                                (seq-difference (buffer-list)
                                                buffer-baseline #'eq))
                        :new-processes
                        (mapcar #'process-name
                                (seq-difference (process-list)
                                                process-baseline #'eq))
                        :new-timers
                        (+ (length (seq-difference timer-list
                                                    timer-baseline #'eq))
                           (length (seq-difference timer-idle-list
                                                    idle-timer-baseline #'eq)))
                        :timer-details
                        (mapcar
                         (lambda (timer)
                           (list (timer--function timer)
                                 (timer--args timer)
                                 (and (memq timer timer-idle-list) t)))
                         (delete-dups
                          (append
                           (seq-difference timer-list timer-baseline #'eq)
                           (seq-difference timer-idle-list
                                           idle-timer-baseline #'eq))))
                        :owned-buffer-live
                        (seq-some #'buffer-live-p ffip356-test-owned-buffers)
                        :owned-process-live
                        (seq-some #'process-live-p owned-processes)
                        :owned-timer-live
                        (seq-some
                         (lambda (timer)
                           (or (memq timer timer-list)
                               (memq timer timer-idle-list)))
                         owned-timers)
                        :root-exists (file-exists-p root)
                        :root-owned root-owned
                        :current-buffer-restored
                        (eq (current-buffer) current-buffer-baseline)
                        :window-restored
                        (and (eq (selected-window) selected-window-baseline)
                             (eq (window-buffer) window-buffer-baseline)
                             (equal (window-list nil 'no-minibuf)
                                    window-list-baseline)
                             (equal (ffip356-test-window-structure)
                                    window-structure-baseline)
                             (compare-window-configurations
                              (current-window-configuration)
                              restored-window-configuration))
                        :transient-mark-restored
                        (eq transient-mark-mode transient-mark-baseline)
                        :dired-restored
                        (equal dired-buffers dired-buffers-baseline)
                        :advice-count advice-after
                        :advice-restored (= advice-after advice-baseline)
                        :hijack ffip-read-file-name-hijacked-p
                        :unread-events unread-command-events
                        :active-minibuffer
                        (and (active-minibuffer-window) t)
                        :body-error body-error
                        :cleanup-errors (nreverse cleanup-errors))))))))
        (let ((dirty
               (or body-error cleanup-errors
                   (plist-get cleanup :new-buffers)
                   (plist-get cleanup :new-processes)
                   (not (= (or (plist-get cleanup :new-timers) -1) 0))
                   (plist-get cleanup :owned-buffer-live)
                   (plist-get cleanup :owned-process-live)
                   (plist-get cleanup :owned-timer-live)
                   (plist-get cleanup :root-exists)
                   (plist-get cleanup :root-owned)
                   (not (plist-get cleanup :current-buffer-restored))
                   (not (plist-get cleanup :window-restored))
                   (not (plist-get cleanup :transient-mark-restored))
                   (not (plist-get cleanup :dired-restored))
                   (not (plist-get cleanup :advice-restored))
                   (plist-get cleanup :hijack)
                   (plist-get cleanup :unread-events)
                   (plist-get cleanup :active-minibuffer))))
          (when dirty
            (error "FIND-FILE-IN-PROJECT world failed: body=%S cleanup=%S phase-errors=%S"
                   body-error cleanup cleanup-errors))
          (list :result (ffip356-test-normalize result root)
                :cleanup cleanup))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FIND_FILE_IN_PROJECT_MELPA_PIN, "find-file-in-project.el")
        .expect("prepare exact shallow Find File in Project source below ./tmp")
        .with_prelude(FIND_FILE_IN_PROJECT_TEST_PRELUDE)
        .with_timeout(Duration::from_secs(240))
}

#[test]
fn find_file_in_project_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "find-file-in-project-package-batch",
        "Find File in Project",
        &workflows::workflow_batch_cases(),
    );
}
