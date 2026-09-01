use std::time::Duration;

use crate::{CachedMelpaOracle, GIT_COMMIT_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GIT_COMMIT_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'ring)
(require 'log-edit)
(require 'diff-mode)
(require 'git-commit-mode)

;; Unicode process decoding and filesystem teardown use this GNU scratch
;; buffer lazily.  Treat that editor infrastructure as common-prelude state.
(get-buffer-create " *code-conversion-work*")

;; GNU batch realizes its reserved menu-bar row on the first file visit.
;; Stabilize that one-time frame transition before any per-case baseline.
(set-window-configuration (current-window-configuration))

(defconst git363-test-source-sha256
  "4c7eb92813c4c001b8776cef1edc9f491087b0cee8ee43fe8b989a1135b20dab")

(defconst git363-test-installed-sha256
  "11d673f5934a2d3d74955b5eee4d7dc1a076ef6ceb627237dac0890a2948597d")

(defconst git363-test-state-symbols
  '(find-file-hook after-change-major-mode-hook global-git-commit-mode
    git-commit-major-mode git-commit-setup-hook
    git-commit-finish-query-functions git-commit-summary-max-length
    git-commit-fill-column git-commit-known-pseudo-headers
    log-edit-comment-ring log-edit-comment-ring-index
    log-edit-last-comment-match log-edit-maximum-comment-ring-size
    recentf-exclude save-place
    process-environment exec-path default-directory
    user-full-name user-mail-address
    minibuffer-history file-name-history extended-command-history
    command-history kill-ring kill-ring-yank-pointer
    interprogram-cut-function suggest-key-bindings
    execute-extended-command--binding-timer
    undo-auto-current-boundary-timer undo-auto--undoably-changed-buffers
    unread-command-events executing-kbd-macro this-command real-this-command
    last-command real-last-command last-command-event last-input-event
    current-prefix-arg prefix-arg deactivate-mark
    enable-local-variables enable-dir-local-variables create-lockfiles
    vc-handled-backends)
  "Mutable editor/package state restored after every historical workflow.")

(defconst git363-test-terminal-state-symbols
  '(undo-auto-current-boundary-timer undo-auto--undoably-changed-buffers)
  "Undo state restored only after final window/diagnostic reactions settle.")

(defconst git363-test-forbidden-external-functions
  '(call-process call-process-region process-file start-process
    start-file-process make-process make-network-process
    open-network-stream url-retrieve url-retrieve-synchronously)
  "External boundaries forbidden except the exact delegated Git lookup.")

(defvar git363-test-world nil)
(defvar git363-test-process-records nil)
(defvar git363-test-external-events nil)
(defvar git363-test-external-advices nil)
(defvar git363-test-message-events nil)
(defvar git363-test-prompt-events nil)
(defvar git363-test-prompt-answers nil)
(defvar git363-test-read-answers nil)
(defvar git363-test-setup-external nil)
(defvar git363-test-inside-process-lines nil)
(defvar git363-test-history-events nil)
(defvar git363-test-edit-events nil)
(defvar git363-test-timer-events nil)
(defvar git363-test-read-events nil)
(defvar git363-test-git-status nil)
(defvar git363-test-diff-records nil)
(defvar git363-test-diff-paths nil)
(defvar git363-test-usage-buffer nil)
(defvar git363-test-usage-timer nil)

(defun git363-test-variable-state (symbol)
  "Return SYMBOL's exact boundness and value identity."
  (if (boundp symbol)
      (list :bound t :value (symbol-value symbol))
    '(:bound nil)))

(defun git363-test-restore-variable (symbol state)
  "Restore SYMBOL to exact STATE."
  (if (plist-get state :bound)
      (set symbol (plist-get state :value))
    (makunbound symbol)))

(defun git363-test-variable-restored-p (symbol state)
  "Return non-nil when SYMBOL has exact STATE identity."
  (if (plist-get state :bound)
      (and (boundp symbol) (eq (symbol-value symbol) (plist-get state :value)))
    (not (boundp symbol))))

(defun git363-test-window-state ()
  "Return stable ownership state for ordinary windows."
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :point (window-point window)
           :start (window-start window) :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :prev (copy-tree (window-prev-buffers window))
           :next (copy-tree (window-next-buffers window))))
   (window-list nil 'no-minibuf)))

(defun git363-test-restore-windows (configuration state)
  "Restore window CONFIGURATION and semantic STATE."
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Git Commit Mode baseline window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun git363-test-buffer-content-state (name)
  "Return exact mutable state for existing diagnostic buffer NAME."
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((minimum (point-min)) (maximum (point-max)))
        (list :buffer buffer
              :text (save-restriction (widen) (buffer-string))
              :point (point) :modified (buffer-modified-p)
              :undo (copy-tree buffer-undo-list) :read-only buffer-read-only
              :mode major-mode :min minimum :max maximum)))))

(defun git363-test-restore-buffer-content (state)
  "Restore an existing diagnostic buffer to exact STATE."
  (when state
    (let ((buffer (plist-get state :buffer)))
      (unless (buffer-live-p buffer)
        (error "Git Commit Mode diagnostic baseline died: %S" buffer))
      (with-current-buffer buffer
        (let ((inhibit-read-only t))
          (widen)
          (erase-buffer)
          (insert (plist-get state :text)))
        (goto-char (min (plist-get state :point) (point-max)))
        (setq buffer-undo-list (copy-tree (plist-get state :undo))
              buffer-read-only (plist-get state :read-only))
        (set-buffer-modified-p (plist-get state :modified))
        (narrow-to-region (plist-get state :min) (plist-get state :max))))))

(defun git363-test-buffer-content-restored-p (state)
  "Return non-nil when diagnostic buffer STATE is restored."
  (or (null state)
      (let ((buffer (plist-get state :buffer)))
        (and (buffer-live-p buffer)
             (with-current-buffer buffer
               (and (equal (save-restriction (widen) (buffer-string))
                           (plist-get state :text))
                    (= (point) (plist-get state :point))
                    (eq (buffer-modified-p) (plist-get state :modified))
                    (equal buffer-undo-list (plist-get state :undo))
                    (eq buffer-read-only (plist-get state :read-only))
                    (= (point-min) (plist-get state :min))
                    (= (point-max) (plist-get state :max))))))))

(defun git363-test-condition-state (condition)
  "Return stable exact CONDITION state."
  (list :symbol (car condition)
        :data (mapcar (lambda (datum)
                        (cond ((bufferp datum) :buffer)
                              ((markerp datum)
                               (list :marker (marker-position datum)))
                              ((stringp datum)
                               (git363-test-normalize-string datum))
                              (t datum)))
                      (cdr condition))
        :message (git363-test-normalize-string
                  (error-message-string condition))))

(defun git363-test-attempt (phase thunk errors)
  "Run THUNK for cleanup PHASE and return updated ERRORS."
  (condition-case condition
      (progn (funcall thunk) errors)
    (t (cons (list phase (git363-test-condition-state condition)) errors))))

(defun git363-test-normalize-string (value)
  "Normalize only the exact owned root and Git executable in VALUE."
  (if (not (stringp value)) value
    (let* ((root (and git363-test-world (plist-get git363-test-world :root)))
           (git (and git363-test-world (plist-get git363-test-world :git)))
           (normalized value))
      (when root
        (setq normalized
              (replace-regexp-in-string
               (regexp-quote root) "[ROOT]/" normalized t t)))
      (when git
        (setq normalized
              (replace-regexp-in-string
               (regexp-quote git) "[GIT]" normalized t t)))
      normalized)))

(defun git363-test-path (relative)
  "Return owned RELATIVE path in the current world."
  (unless git363-test-world
    (error "Git Commit Mode has no active world"))
  (expand-file-name relative (plist-get git363-test-world :root)))

(defun git363-test-write (relative text)
  "Write exact UTF-8 TEXT to owned RELATIVE path."
  (let ((path (git363-test-path relative))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-file path (insert text))
    (unless (equal (git363-test-file-bytes path) text)
      (error "Git Commit Mode fixture write mismatch: %S" relative))
    path))

(defun git363-test-file-bytes (path)
  "Return exact UTF-8 text bytes from owned PATH, or :missing."
  (if (not (file-exists-p path)) :missing
    (let ((coding-system-for-read 'utf-8-unix))
      (with-temp-buffer
        (insert-file-contents path)
        (buffer-substring-no-properties (point-min) (point-max))))))

(defun git363-test-run-git (directory &rest arguments)
  "Run exact owned Git ARGUMENTS in DIRECTORY for fixture setup."
  (let ((default-directory (file-name-as-directory directory))
        (buffer (generate-new-buffer " *git363 setup stdout*"))
        (git363-test-setup-external :outer))
    (unwind-protect
        (let ((status (apply #'process-file
                             (plist-get git363-test-world :git)
                             nil buffer nil arguments)))
          (unless (eq status 0)
            (error "Git Commit Mode setup Git failed: %S %S %S"
                   status arguments
                   (with-current-buffer buffer (buffer-string))))
          (with-current-buffer buffer
            (buffer-substring-no-properties (point-min) (point-max))))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(defun git363-test-make-repo (relative comment-character)
  "Create real owned Git repo RELATIVE with COMMENT-CHARACTER."
  (let ((repo (file-name-as-directory (git363-test-path relative))))
    (make-directory repo t)
    (git363-test-run-git repo "-c" "init.defaultBranch=main" "init" "-q")
    (git363-test-run-git repo "config" "user.name" "Config User")
    (git363-test-run-git repo "config" "user.email" "config@example.test")
    (git363-test-run-git repo "config" "core.commentChar" comment-character)
    repo))

(defun git363-test-allocate-world (case-name)
  "Allocate and return an owned CASE-NAME world after Git preflight."
  (unless (string-match-p "\\`[a-z0-9-]+\\'" case-name)
    (error "Git Commit Mode invalid case name: %S" case-name))
  (let ((git (executable-find "git"))
        (diff (executable-find "diff"))
        (raw-workspace (getenv "NEOMACS_TEST_WORKSPACE_ROOT"))
        (raw-owner (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
        (raw-tmp (getenv "TMPDIR")))
    (unless (and git (file-name-absolute-p git) (file-executable-p git))
      (error "Git Commit Mode needs an absolute real Git: %S" git))
    (unless (and diff (file-name-absolute-p diff) (file-executable-p diff))
      (error "Git Commit Mode needs an absolute real diff: %S" diff))
    (unless (and raw-workspace (not (string-empty-p raw-workspace))
                 (file-name-absolute-p raw-workspace)
                 (file-directory-p raw-workspace))
      (error "Git Commit Mode workspace root is unsafe: %S" raw-workspace))
    (unless (and raw-owner (not (string-empty-p raw-owner))
                 (file-name-absolute-p raw-owner)
                 (file-directory-p raw-owner))
      (error "Git Commit Mode sandbox root is unsafe: %S" raw-owner))
    (unless (and raw-tmp (not (string-empty-p raw-tmp))
                 (file-name-absolute-p raw-tmp) (file-directory-p raw-tmp))
      (error "Git Commit Mode TMPDIR is unsafe: %S" raw-tmp))
    (let* ((workspace
            (file-name-as-directory (file-truename raw-workspace)))
           (workspace-tmp
            (file-name-as-directory
             (file-truename (expand-file-name "tmp/" workspace))))
           (owner (file-name-as-directory (file-truename raw-owner)))
           (tmp (file-name-as-directory (file-truename raw-tmp)))
           (root (expand-file-name
                  (format "git-commit-mode363-%s project 界/" case-name)
                  owner)))
      (unless (and (file-in-directory-p owner workspace-tmp)
                   (file-in-directory-p tmp workspace-tmp))
        (error "Git Commit Mode scratch paths escape workspace tmp: %S"
               (list workspace-tmp owner tmp)))
      (unless (and (file-name-absolute-p root)
                   (not (equal owner root))
                   (string-prefix-p owner root)
                   (not (file-exists-p root)))
        (error "Git Commit Mode refuses owned root: %S" (list owner root)))
      (make-directory root)
      (list :owner owner :root root :git (file-truename git)
            :diff (file-truename diff)
            :bin (file-name-directory (file-truename git))
            :diff-bin (file-name-directory (file-truename diff))
            :tmp tmp
            :empty-bin (expand-file-name "empty-bin/" root)
            :home (expand-file-name "home/" root)
            :global-config (expand-file-name "global.gitconfig" root)))))

(defun git363-test-configure-world ()
  "Install deterministic reversible package and Git configuration."
  (make-directory (plist-get git363-test-world :empty-bin) t)
  (make-directory (plist-get git363-test-world :home) t)
  (git363-test-write "global.gitconfig" "")
  (setq process-environment (copy-sequence process-environment)
        exec-path (delete-dups
                   (list (directory-file-name
                          (plist-get git363-test-world :bin))
                         (directory-file-name
                          (plist-get git363-test-world :diff-bin))))
        default-directory (plist-get git363-test-world :root)
        enable-local-variables nil
        enable-dir-local-variables nil
        create-lockfiles nil
        vc-handled-backends nil
        user-full-name "Fallback User"
        user-mail-address "fallback@example.test"
        minibuffer-history nil
        file-name-history nil
        extended-command-history nil
        command-history nil
        kill-ring nil
        kill-ring-yank-pointer nil
        interprogram-cut-function nil
        suggest-key-bindings nil
        ;; Preserve the already-gated source defaults, forking only mutable
        ;; list values so a case cannot mutate the shared baseline object.
        git-commit-setup-hook (copy-sequence git-commit-setup-hook)
        git-commit-finish-query-functions
        (copy-sequence git-commit-finish-query-functions)
        git-commit-known-pseudo-headers
        (copy-sequence git-commit-known-pseudo-headers)
        log-edit-maximum-comment-ring-size 20
        log-edit-comment-ring (make-ring 20)
        log-edit-comment-ring-index 0
        log-edit-last-comment-match nil
        find-file-hook (copy-sequence find-file-hook)
        after-change-major-mode-hook
        (copy-sequence after-change-major-mode-hook))
  (setq process-environment
        (seq-remove (lambda (entry) (string-prefix-p "GIT_" entry))
                    process-environment))
  (setenv "HOME" (directory-file-name (plist-get git363-test-world :home)))
  (setenv "GIT_CONFIG_GLOBAL" (plist-get git363-test-world :global-config))
  (setenv "GIT_CONFIG_NOSYSTEM" "1")
  (setenv "PATH"
          (mapconcat #'identity exec-path path-separator))
  (setenv "LC_ALL" "C.UTF-8")
  (setenv "TZ" "UTC")
  (setenv "GIT_AUTHOR_NAME" nil)
  (setenv "GIT_AUTHOR_EMAIL" nil)
  (setenv "GIT_COMMITTER_NAME" nil)
  (setenv "GIT_COMMITTER_EMAIL" nil)
  (setenv "EMAIL" nil)
  (global-git-commit-mode 1))

(defun git363-test-message-observer (original format-string &rest arguments)
  "Record exact message then call ORIGINAL."
  (let ((rendered (and format-string
                       (apply #'format-message format-string arguments))))
    (when rendered
      (push (substring-no-properties
             (git363-test-normalize-string rendered))
            git363-test-message-events))
    (apply original format-string arguments)))

(defun git363-test-run-at-time-observer
    (original time repeat function &rest arguments)
  "Record exact real timer registration and delegate to ORIGINAL."
  (let* ((role (cond ((and (eq function #'track-changes--call-signal)
                           (equal time 0) (null repeat)
                           (= (length arguments) 2))
                      :track-changes)
                     ((and (eq function #'undo-auto--boundary-timer)
                           (equal time 10) (null repeat)
                           (null arguments))
                      :undo-boundary)
                     ((and (equal time 0.05) (null repeat)
                           (null arguments))
                      :with-editor-usage)
                     (t :unexpected)))
         (timer (apply original time repeat function arguments)))
    (push (list :timer timer :time time :repeat repeat :role role
                :arguments (length arguments)
                :buffer (and (eq role :track-changes)
                             (bufferp (car arguments)) (car arguments)))
          git363-test-timer-events)
    timer))

(defun git363-test-with-editor-usage-observer (original)
  "Capture exact timer identity returned by real With-Editor usage setup."
  (let ((buffer (current-buffer))
        (timer (funcall original)))
    (setq git363-test-usage-buffer buffer
          git363-test-usage-timer timer)
    timer))

(defun git363-test-y-or-n-observer (_original prompt)
  "Record PROMPT and return the next exact scripted answer."
  (unless git363-test-prompt-answers
    (error "Unexpected Git Commit Mode prompt: %S" prompt))
  (let* ((entry (pop git363-test-prompt-answers))
         (expected (car entry))
         (answer (cdr entry)))
    (unless (equal prompt expected)
      (error "Git Commit Mode prompt mismatch: expected %S, got %S"
             expected prompt))
    (push (list :prompt prompt :answer answer) git363-test-prompt-events)
    answer))

(defun git363-test-read-string-observer (original prompt &rest arguments)
  "Observe a real minibuffer read through ORIGINAL."
  (unless git363-test-read-answers
    (error "Unexpected Git Commit Mode read-string: %S" prompt))
  (let* ((entry (pop git363-test-read-answers))
         (expected-prompt (car entry))
         (expected-answer (cdr entry)))
    (unless (equal prompt expected-prompt)
      (error "Git Commit Mode read prompt mismatch: expected %S, got %S"
             expected-prompt prompt))
    (let ((answer (apply original prompt arguments)))
      (unless (equal answer expected-answer)
        (error "Git Commit Mode read answer mismatch: expected %S, got %S"
               expected-answer answer))
    (push (list :prompt prompt :answer (copy-sequence answer)
                :history-after (copy-tree minibuffer-history))
          git363-test-read-events)
      answer)))

(defun git363-test-file-sha256 (path)
  "Return the exact SHA-256 digest of PATH bytes."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun git363-test-diff-semantic-output (output file1 file2)
  "Validate and return stable normal-diff OUTPUT for FILE1 and FILE2."
  (when (or (string-match-p (regexp-quote file1) output)
            (string-match-p (regexp-quote file2) output))
    (error "Git Commit Mode diff exposed owned nonce paths: %S" output))
  (unless (and (stringp output) (not (string-empty-p output)))
    (error "Git Commit Mode diff produced no semantic output"))
  output)

(defun git363-test-reject-external (operation original &rest arguments)
  "Reject OPERATION except exact owned setup, package Git, and GNU diff calls."
  (cond
   ((and (eq git363-test-setup-external :outer)
         (eq operation 'process-file))
    (pcase-let ((`(,program ,infile ,destination ,display . ,argv) arguments))
      (let ((cwd (file-name-as-directory (file-truename default-directory)))
            (root (plist-get git363-test-world :root)))
        (unless
            (and (file-name-absolute-p program)
                 (equal (file-truename program)
                        (plist-get git363-test-world :git))
                 (null infile) (buffer-live-p destination) (null display)
                 (string-prefix-p root cwd)
                 (or (equal argv
                            '("-c" "init.defaultBranch=main" "init" "-q"))
                     (equal argv '("config" "user.name" "Config User"))
                     (equal argv
                            '("config" "user.email" "config@example.test"))
                     (and (equal (butlast argv)
                                 '("config" "core.commentChar"))
                          (= (length argv) 3)
                          (member (car (last argv)) '(";" "#")))))
          (error "Unexpected Git Commit Mode setup boundary: %S" arguments))
        (let ((token (vector (copy-tree arguments) 0)))
          (let ((git363-test-setup-external token))
            (prog1 (apply original arguments)
              (unless (memq (aref token 1) '(0 1))
                (error "Git Commit Mode setup nesting count drifted: %S"
                       (aref token 1)))))))))
   ((vectorp git363-test-setup-external)
    (unless (and (eq operation 'call-process)
                 (equal arguments (aref git363-test-setup-external 0))
                 (= (aref git363-test-setup-external 1) 0))
      (error "Unexpected nested Git Commit Mode setup boundary: %S %S"
             operation arguments))
    (aset git363-test-setup-external 1 1)
    (apply original arguments))
   ((and git363-test-inside-process-lines (eq operation 'call-process))
    (pcase-let ((`(,program ,infile ,destination ,display . ,argv) arguments))
      (unless (and (equal program "git") (null infile)
                   (buffer-live-p destination)
                   (eq destination (current-buffer))
                   (null (buffer-file-name destination))
                   (string-prefix-p " *temp*" (buffer-name destination))
                   (null display)
                   (equal argv '("config" "core.commentchar")))
        (error "Unexpected Git Commit Mode process-lines boundary: %S"
               arguments))
      (setq git363-test-git-status (apply original arguments))))
   ((and (eq operation 'call-process)
         (equal (car arguments) "diff"))
    (pcase-let ((`(,program ,infile ,destination ,display
                            ,option ,file1 ,file2) arguments))
      (let* ((resolved (executable-find program))
             (tmp (plist-get git363-test-world :tmp))
             (cwd (file-name-as-directory (file-truename default-directory)))
             (root (plist-get git363-test-world :root))
             (start (point)) status output semantic)
        (dolist (path (list file1 file2))
          (unless (and (stringp path) (file-name-absolute-p path)
                       (file-exists-p path)
                       (string-prefix-p tmp (file-truename path)))
            (error "Unowned Git Commit Mode diff input: %S" path))
          (push path git363-test-diff-paths))
        (unless (and resolved
                     (equal (file-truename resolved)
                            (plist-get git363-test-world :diff))
                     (buffer-live-p (current-buffer))
                     (null buffer-file-name)
                     (string-prefix-p " *temp*" (buffer-name))
                     (null infile) (eq destination t) (null display)
                     (equal option "-ad")
                     (string-prefix-p root cwd)
                     (string-suffix-p "/.git/" cwd)
                     (string-match-p "\\`diff1[A-Za-z0-9]+\\'"
                                     (file-name-nondirectory file1))
                     (string-match-p "\\`diff2[A-Za-z0-9]+\\'"
                                     (file-name-nondirectory file2)))
          (error "Unexpected Git Commit Mode diff boundary: %S" arguments))
        (setq status (apply original arguments)
              output (buffer-substring-no-properties start (point))
              semantic (git363-test-diff-semantic-output output file1 file2))
        (push
         (list :paths (list file1 file2)
               :program "[DIFF]" :argv '("-ad" :old :new)
               :cwd (git363-test-normalize-string cwd)
               :buffer :temporary :status status
               :streams :combined :combined-output semantic
               :headers :normal-diff-emits-no-paths
               :old (list :sha256 (git363-test-file-sha256 file1)
                          :text (git363-test-file-bytes file1))
               :new (list :sha256 (git363-test-file-sha256 file2)
                          :text (git363-test-file-bytes file2)))
         git363-test-diff-records)
        status)))
   (t
    (push (list :operation operation :arguments (copy-tree arguments))
          git363-test-external-events)
    (error "Unexpected Git Commit Mode external operation: %S %S"
           operation arguments))))

(defun git363-test-process-lines-observer (original program &rest arguments)
  "Validate, delegate, and record real package PROGRAM invocation."
  (unless (equal program "git")
    (error "Git Commit Mode used unexpected process-lines program: %S" program))
  (unless (equal arguments '("config" "core.commentchar"))
    (error "Git Commit Mode used unexpected Git argv: %S" arguments))
  (let ((resolved (executable-find program))
        (cwd (git363-test-normalize-string
              (file-name-as-directory (file-truename default-directory)))))
    (if (null resolved)
        (condition-case condition
            (let ((git363-test-inside-process-lines t))
              (apply original program arguments))
          (t
           (push (list :program "git" :argv (copy-tree arguments)
                       :cwd cwd :launched nil
                       :condition (git363-test-condition-state condition))
                 git363-test-process-records)
           (signal (car condition) (cdr condition))))
      (unless (equal (file-truename resolved)
                     (plist-get git363-test-world :git))
        (error "Git Commit Mode resolved unowned Git: %S" resolved))
      (let* ((git363-test-inside-process-lines t)
             (git363-test-git-status nil)
             (lines (apply original program arguments)))
        (push (list :program "[GIT]" :argv (copy-tree arguments)
                    :cwd cwd :launched t :status git363-test-git-status
                    :streams :combined
                    :combined-output (copy-tree lines))
              git363-test-process-records)
        lines))))

(defun git363-test-install-observers ()
  "Install fail-closed process and unattended-input observers."
  (setq git363-test-external-advices nil)
  (dolist (operation git363-test-forbidden-external-functions)
    (unless (fboundp operation)
      (error "Missing Git Commit Mode boundary: %S" operation))
    (let ((advice (apply-partially #'git363-test-reject-external operation)))
      (advice-add operation :around advice)
      (push (cons operation advice) git363-test-external-advices)))
  (advice-add 'process-lines :around #'git363-test-process-lines-observer)
  (advice-add 'message :around #'git363-test-message-observer)
  (advice-add 'y-or-n-p :around #'git363-test-y-or-n-observer)
  (advice-add 'read-string :around #'git363-test-read-string-observer)
  (advice-add 'with-editor-usage-message :around
              #'git363-test-with-editor-usage-observer)
  (advice-add 'run-at-time :around #'git363-test-run-at-time-observer))

(defun git363-test-remove-observers ()
  "Remove and verify every installed observer."
  (let (errors survivors)
    (dolist (entry git363-test-external-advices)
      (condition-case condition
          (progn
            (advice-remove (car entry) (cdr entry))
            (when (advice-member-p (cdr entry) (car entry))
              (push (car entry) survivors)))
        (t (push (list (car entry) condition) errors))))
    (dolist (entry '((process-lines . git363-test-process-lines-observer)
                     (message . git363-test-message-observer)
                     (y-or-n-p . git363-test-y-or-n-observer)
                     (read-string . git363-test-read-string-observer)
                     (with-editor-usage-message
                      . git363-test-with-editor-usage-observer)
                     (run-at-time . git363-test-run-at-time-observer)))
      (condition-case condition
          (progn
            (advice-remove (car entry) (cdr entry))
            (when (advice-member-p (cdr entry) (car entry))
              (push (car entry) survivors)))
        (t (push (list (car entry) condition) errors))))
    (when (or errors survivors)
      (error "Git Commit Mode observer cleanup failed: %S"
             (list :errors errors :survivors survivors)))))

(defun git363-test-new-timers (before idle-before)
  "Return timers created after BEFORE and IDLE-BEFORE."
  (delete-dups
   (append (seq-difference timer-list before #'eq)
           (seq-difference timer-idle-list idle-before #'eq))))

(defun git363-test-settle-usage-timer (before idle-before)
  "Settle only the exact real With-Editor timer and own core diff timers."
  (let* ((owned (git363-test-new-timers before idle-before))
         (events (nreverse (copy-sequence git363-test-timer-events)))
         (usage-events (seq-filter
                        (lambda (entry)
                          (eq (plist-get entry :role) :with-editor-usage))
                        events))
         (track-events (seq-filter
                        (lambda (entry)
                          (eq (plist-get entry :role) :track-changes))
                        events))
         (undo-events (seq-filter
                       (lambda (entry)
                         (eq (plist-get entry :role) :undo-boundary))
                       events))
         (other-owned (delq git363-test-usage-timer
                            (copy-sequence owned)))
         (track-timers
          (seq-filter
           (lambda (timer)
             (eq (timer--function timer) #'track-changes--call-signal))
           other-owned))
         (undo-timers
          (seq-filter
           (lambda (timer)
             (eq (timer--function timer) #'undo-auto--boundary-timer))
           other-owned))
         (unmatched
          (seq-difference other-owned
                          (append track-timers undo-timers) #'eq)))
    (unless (and (= (length usage-events) 1)
                 (eq git363-test-usage-buffer (current-buffer))
                 (buffer-live-p git363-test-usage-buffer)
                 (eq git363-test-usage-timer
                     (plist-get (car usage-events) :timer))
                 (memq git363-test-usage-timer owned)
                 (null unmatched)
                 (cl-every
                  (lambda (timer)
                    (let ((arguments (timer--args timer)))
                      (and (null (timer--repeat-delay timer))
                           (= (length arguments) 2)
                           (bufferp (car arguments))
                           (not (buffer-live-p (car arguments))))))
                  track-timers)
                 (cl-every
                  (lambda (timer)
                    (and (null (timer--repeat-delay timer))
                         (null (timer--args timer))))
                  undo-timers)
                 (or (null undo-timers)
                     (and (boundp 'undo-auto-current-boundary-timer)
                          (memq undo-auto-current-boundary-timer undo-timers)))
                 (cl-every (lambda (entry)
                             (memq (plist-get entry :timer) track-timers))
                           track-events)
                 (cl-every (lambda (entry)
                             (memq (plist-get entry :timer) undo-timers))
                           undo-events)
                 (not (seq-some (lambda (entry)
                                  (eq (plist-get entry :role) :unexpected))
                                events)))
      (error "Unexpected With-Editor/core timer ownership: %S"
             (list :owned owned :events events
                   :usage git363-test-usage-timer
                   :buffer git363-test-usage-buffer)))
    (when (seq-some
           (lambda (event)
             (string-prefix-p "Type C-c C-c to finish" event))
           git363-test-message-events)
      (error "With-Editor usage message arrived before owned timer delivery"))
    ;; The killed diff temp-buffer callbacks are owned but semantically inert;
    ;; cancel them exactly.  Wait until the captured 0.05 timer is naturally
    ;; due without pumping any ambient timer, then invoke GNU's real handler
    ;; on that one identity.
    (dolist (timer track-timers)
      (cancel-timer timer))
    (let ((deadline (+ (float-time) 1.0)))
      (while (and (time-less-p (current-time)
                               (timer--time git363-test-usage-timer))
                  (< (float-time) deadline))
        nil))
    (when (time-less-p (current-time) (timer--time git363-test-usage-timer))
      (error "With-Editor usage timer never became due"))
    (timer-event-handler git363-test-usage-timer)
    (when (cl-some (lambda (timer)
                     (or (memq timer timer-list)
                         (memq timer timer-idle-list)))
                   (cons git363-test-usage-timer track-timers))
      (error "With-Editor usage timer did not settle"))
    (let ((usage
           (seq-filter
            (lambda (event)
              (string-prefix-p "Type C-c C-c to finish" event))
            (nreverse (copy-sequence git363-test-message-events)))))
      (unless (equal usage
                     '("Type C-c C-c to finish, or C-c C-k to cancel"))
        (error "Unexpected With-Editor usage messages: %S" usage))
      (list :registration
            (list :time 0.05 :repeat nil :buffer-live t
                  :identity-captured t)
            :track-changes
            (list :created (length track-timers)
                  :buffers-live nil :cancelled (length track-timers))
            :undo-boundary
            (list :created (length undo-timers)
                  :pending-for-cleanup (length undo-timers))
            :pre-message nil :messages usage
            :usage-pending 0))))

(defun git363-test-visit (path &optional settle-usage)
  "Publicly visit PATH and optionally SETTLE-USAGE timer."
  (let ((before (copy-sequence timer-list))
        (idle-before (copy-sequence timer-idle-list)))
    (setq git363-test-message-events nil
          git363-test-timer-events nil)
    (find-file path)
    (let ((usage (and settle-usage
                      (git363-test-settle-usage-timer before idle-before))))
      (list :buffer (current-buffer) :usage usage))))

(defun git363-test-kbd (&rest chunks)
  "Run one real selected-buffer command loop from CHUNKS.
Each chunk is either (:text STRING) or (:keys KBD-SYNTAX)."
  (unless (eq (current-buffer) (window-buffer (selected-window)))
    (error "Git Commit Mode command buffer is not selected"))
  (when unread-command-events
    (error "Git Commit Mode inherited unread input: %S" unread-command-events))
  (let ((events
         (apply #'vconcat
                (mapcar
                 (lambda (chunk)
                   (pcase (car chunk)
                     (:text (string-to-vector (cadr chunk)))
                     (:keys (kbd (cadr chunk)))
                     (_ (error "Bad Git Commit Mode input chunk: %S" chunk))))
                 chunks))))
    (execute-kbd-macro events)
    (when unread-command-events
      (error "Git Commit Mode left unread input: %S" unread-command-events))
    (list :events (length events) :unread nil)))

(defun git363-test-edit-post-command ()
  "Record meaningful public edit commands at their command-loop boundary."
  (when (memq this-command
              '(git-commit-signoff git-commit-ack git-commit-cc
                git-commit-save-message undo kill-region))
    (push (list :command this-command :state (git363-test-buffer-state))
          git363-test-edit-events)))

(defun git363-test-edit-macro (&rest chunks)
  "Run CHUNKS contiguously and return meaningful post-command states."
  (setq git363-test-edit-events nil)
  (add-hook 'post-command-hook #'git363-test-edit-post-command nil t)
  (unwind-protect
      (apply #'git363-test-kbd chunks)
    (remove-hook 'post-command-hook #'git363-test-edit-post-command t))
  (nreverse (copy-tree git363-test-edit-events)))

(defun git363-test-buffer-state ()
  "Return exact stable current buffer state."
  (list :file (and buffer-file-name (file-name-nondirectory buffer-file-name))
        :mode major-mode :git-commit git-commit-mode
        :with-editor with-editor-mode
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point) :mark (mark t) :active mark-active
        :modified (buffer-modified-p) :read-only buffer-read-only
        :narrowed (buffer-narrowed-p)
        :undo (cond ((eq buffer-undo-list t) :disabled)
                    ((null buffer-undo-list) :empty)
                    (t :present))))

(defun git363-test-activation-state ()
  "Return practical public activation and key state."
  (list :major major-mode :git-commit git-commit-mode
        :with-editor with-editor-mode :comment comment-start
        :comment-skip comment-start-skip
        :fill fill-column :auto-fill auto-fill-function
        :finish-hooks (copy-sequence with-editor-finish-query-functions)
        :cancel-hooks (copy-sequence with-editor-pre-cancel-hook)
        :cancel-message with-editor-cancel-message
        :kill-query (copy-sequence kill-buffer-query-functions)
        :keys
        (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                '("C-c C-s" "C-c C-a" "C-c C-o" "C-c M-s"
                  "M-p" "M-n" "C-c C-c" "C-c C-k"))))

(defun git363-test-property-runs ()
  "Return every non-nil semantic face/property span."
  (font-lock-ensure (point-min) (point-max))
  (let ((position (point-min)) rows)
    (while (< position (point-max))
      (let* ((next (or (next-property-change position nil (point-max))
                       (point-max)))
             (face (get-text-property position 'face))
             (font-lock-face
              (get-text-property position 'font-lock-face)))
        (when (or face font-lock-face)
          (push
           (list (line-number-at-pos position)
                 (save-excursion (goto-char position) (current-column))
                 (line-number-at-pos next)
                 (save-excursion (goto-char next) (current-column))
                 face font-lock-face
                 (buffer-substring-no-properties position next))
           rows))
        (setq position next)))
    (nreverse rows)))

(defun git363-test-ring-state ()
  "Return exact global comment ring and local index."
  (list :length (ring-length log-edit-comment-ring)
        :elements (mapcar #'substring-no-properties
                          (ring-elements log-edit-comment-ring))
        :index log-edit-comment-ring-index))

(defun git363-test-process-state ()
  "Return exact package Git process observations in call order."
  (list
   :git (nreverse (copy-tree git363-test-process-records))
   :diff
   (mapcar
    (lambda (entry)
      (list :program (plist-get entry :program)
            :argv (plist-get entry :argv)
            :cwd (plist-get entry :cwd)
            :status (plist-get entry :status)
            :buffer (plist-get entry :buffer)
            :streams (plist-get entry :streams)
            :combined-output (plist-get entry :combined-output)
            :headers (plist-get entry :headers)
            :old (copy-tree (plist-get entry :old))
            :new (copy-tree (plist-get entry :new))
            :temp-clean
            (cl-every (lambda (path) (not (file-exists-p path)))
                      (plist-get entry :paths))))
    (nreverse (copy-sequence git363-test-diff-records)))))

(defun git363-test-prompt-state ()
  "Return exact prompt ledger and require answer exhaustion."
  (when git363-test-prompt-answers
    (error "Unused Git Commit Mode prompt answers: %S"
           git363-test-prompt-answers))
  (nreverse (copy-tree git363-test-prompt-events)))

(defun git363-test-history-post-command ()
  "Record each public history navigation after command execution."
  (when (memq this-command '(git-commit-prev-message git-commit-next-message))
    (push (list :command this-command
                :state (git363-test-buffer-state)
                :ring (git363-test-ring-state)
                :message (car git363-test-message-events))
          git363-test-history-events)))

(defun git363-test-history-macro (keys)
  "Run contiguous history KEYS and return every command result."
  (setq git363-test-history-events nil
        git363-test-message-events nil)
  (add-hook 'post-command-hook #'git363-test-history-post-command nil t)
  (unwind-protect
      (git363-test-kbd (list :keys keys))
    (remove-hook 'post-command-hook #'git363-test-history-post-command t))
  (nreverse (copy-tree git363-test-history-events)))

(defun git363-test-provenance ()
  "Return exact historical implementation provenance."
  (let* ((source (symbol-file 'git-commit-setup 'defun))
         (installed-digest
          (and source
               (with-temp-buffer
                 (set-buffer-multibyte nil)
                 (insert-file-contents-literally source)
                 (secure-hash 'sha256 (current-buffer))))))
    (unless (and source (string-suffix-p "/git-commit-mode.el" source)
                 (equal installed-digest git363-test-installed-sha256)
                 (not (featurep 'git-commit)))
      (error "Git Commit Mode provenance mismatch: %S"
             (list source installed-digest
                   (featurep 'git-commit))))
    (unless
        (and global-git-commit-mode
             (= (cl-count #'git-commit-setup-check-buffer find-file-hook
                          :test #'eq) 1)
             (= (cl-count #'git-commit-setup-font-lock-in-buffer
                          after-change-major-mode-hook :test #'eq) 1)
             (eq git-commit-major-mode 'text-mode)
             (equal git-commit-setup-hook
                    '(git-commit-save-message
                      git-commit-setup-changelog-support
                      git-commit-turn-on-auto-fill
                      git-commit-propertize-diff
                      with-editor-usage-message))
             (equal git-commit-finish-query-functions
                    '(git-commit-check-style-conventions))
             (= git-commit-summary-max-length 50)
             (= git-commit-fill-column 72)
             (equal git-commit-known-pseudo-headers
                    '("Signed-off-by" "Acked-by" "Cc" "Suggested-by"
                      "Reported-by" "Tested-by" "Reviewed-by")))
      (error "Git Commit Mode source defaults/registrations drifted: %S"
             (list global-git-commit-mode find-file-hook
                   after-change-major-mode-hook git-commit-major-mode
                   git-commit-setup-hook git-commit-finish-query-functions
                   git-commit-summary-max-length git-commit-fill-column
                   git-commit-known-pseudo-headers)))
    (list :library (file-name-nondirectory source)
          :source-sha256 git363-test-source-sha256
          :installed-sha256 installed-digest
          :feature (featurep 'git-commit-mode)
          :modern-feature (featurep 'git-commit)
          :global global-git-commit-mode
          :find-registration 1 :font-registration 1
          :major-default git-commit-major-mode
          :setup-default (copy-sequence git-commit-setup-hook)
          :finish-default (copy-sequence git-commit-finish-query-functions)
          :summary-default git-commit-summary-max-length
          :fill-default git-commit-fill-column
          :headers-default (copy-sequence git-commit-known-pseudo-headers))))

(defun git363-test-run (case-name thunk)
  "Run THUNK in one fully owned and reversible CASE-NAME world."
  (git363-test-provenance)
  (let* ((git363-test-world nil)
         (git363-test-process-records nil)
         (git363-test-external-events nil)
         (git363-test-external-advices nil)
         (git363-test-message-events nil)
         (git363-test-prompt-events nil)
         (git363-test-prompt-answers nil)
         (git363-test-read-answers nil)
         (git363-test-history-events nil)
         (git363-test-edit-events nil)
         (git363-test-timer-events nil)
         (git363-test-read-events nil)
         (git363-test-diff-records nil)
         (git363-test-diff-paths nil)
         (git363-test-usage-buffer nil)
         (git363-test-usage-timer nil)
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-timers-before (copy-sequence timer-idle-list))
         (buffer-before (current-buffer))
         (window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (git363-test-window-state))
         (warnings-before (git363-test-buffer-content-state "*Warnings*"))
         (messages-before (git363-test-buffer-content-state "*Messages*"))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (git363-test-variable-state symbol)))
                  git363-test-state-symbols))
         body-value body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (setq git363-test-world (git363-test-allocate-world case-name))
              (git363-test-configure-world)
              (git363-test-install-observers)
              (setq body-value (funcall thunk git363-test-world)))
          (t (setq body-error (git363-test-condition-state condition))))
      (setq cleanup-errors
            (git363-test-attempt
             'remove-observers #'git363-test-remove-observers cleanup-errors))
      ;; Quiesce all case-owned async resources before restoring ambient state.
      (let ((index 0))
        (dolist (timer (git363-test-new-timers timers-before idle-timers-before))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'cancel-timer index)
                 (lambda () (cancel-timer timer)) cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (process (seq-difference (process-list) processes-before #'eq))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'reap-process index)
                 (lambda ()
                   (set-process-query-on-exit-flag process nil)
                   (when (process-live-p process) (delete-process process))
                   (let ((deadline (+ (float-time) 1.0)))
                     (while (and (process-live-p process)
                                 (< (float-time) deadline))
                       (accept-process-output process 0.01)))
                   (when (process-live-p process)
                     (error "Git Commit Mode process survived: %S" process)))
                 cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'kill-buffer index (buffer-name buffer))
                 (lambda ()
                   (when (buffer-live-p buffer)
                     (with-current-buffer buffer
                       (remove-hook 'kill-buffer-query-functions
                                    #'with-editor-kill-buffer-noop t)
                       (set-buffer-modified-p nil))
                     (kill-buffer buffer)))
                 cleanup-errors))
          (setq index (1+ index))))
      ;; A kill hook can create a late resource; every sibling is attempted.
      (let ((index 0))
        (dolist (timer (git363-test-new-timers timers-before idle-timers-before))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'second-timer index)
                 (lambda () (cancel-timer timer)) cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (process (seq-difference (process-list) processes-before #'eq))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'second-process index)
                 (lambda ()
                   (when (process-live-p process) (delete-process process))
                   (when (process-live-p process)
                     (error "Git Commit Mode late process survived: %S" process)))
                 cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'second-buffer index (buffer-name buffer))
                 (lambda ()
                   (when (buffer-live-p buffer)
                     (with-current-buffer buffer
                       (remove-hook 'kill-buffer-query-functions
                                    #'with-editor-kill-buffer-noop t)
                       (set-buffer-modified-p nil))
                     (kill-buffer buffer)))
                 cleanup-errors))
          (setq index (1+ index))))
      ;; Subject/GNU diff owns its temp-file unwind.  Record any violation,
      ;; then still attempt every exact validated sibling so the suite itself
      ;; never leaks outside the per-case project root.
      (let ((index 0))
        (dolist (path (delete-dups (copy-sequence git363-test-diff-paths)))
          (when (file-exists-p path)
            (setq cleanup-errors
                  (cons (list (list 'diff-temp-survived index)
                              (list :path :owned-tmp))
                        cleanup-errors)))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'delete-diff-temp index)
                 (lambda () (when (file-exists-p path) (delete-file path)))
                 cleanup-errors))
          (setq index (1+ index))))
      (when git363-test-world
        (setq cleanup-errors
              (git363-test-attempt
               'delete-root
               (lambda ()
                 (let* ((root (plist-get git363-test-world :root))
                        (owner (plist-get git363-test-world :owner))
                        (true-root
                         (and (file-exists-p root)
                              (file-name-as-directory (file-truename root)))))
                   (when true-root
                     (unless (and (file-name-absolute-p root)
                                  (not (equal true-root owner))
                                  (string-prefix-p owner true-root))
                       (error "Git Commit Mode refuses root deletion: %S"
                              (list owner root)))
                     (delete-directory root t))))
               cleanup-errors)))
      ;; Unicode filesystem teardown can lazily create a conversion buffer or
      ;; undo boundary.  Sweep those reactions before the one final baseline
      ;; restoration, so no callback or kill hook runs afterward.
      (let ((index 0))
        (dolist (timer (git363-test-new-timers timers-before idle-timers-before))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'after-root-timer index)
                 (lambda () (cancel-timer timer)) cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (process (seq-difference (process-list) processes-before #'eq))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'after-root-process index)
                 (lambda ()
                   (when (process-live-p process) (delete-process process))
                   (when (process-live-p process)
                     (error "Git Commit Mode after-root process survived: %S"
                            process)))
                 cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'after-root-buffer index (buffer-name buffer))
                 (lambda ()
                   (when (buffer-live-p buffer)
                     (with-current-buffer buffer (set-buffer-modified-p nil))
                     (kill-buffer buffer)))
                 cleanup-errors))
          (setq index (1+ index))))
      (dolist (entry states-before)
        (unless (memq (car entry) git363-test-terminal-state-symbols)
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'restore-variable (car entry))
                 (lambda ()
                   (git363-test-restore-variable (car entry) (cdr entry)))
                 cleanup-errors))))
      (setq cleanup-errors
            (git363-test-attempt
             'restore-warnings
             (lambda () (git363-test-restore-buffer-content warnings-before))
             cleanup-errors))
      (setq cleanup-errors
            (git363-test-attempt
             'restore-messages
             (lambda () (git363-test-restore-buffer-content messages-before))
             cleanup-errors))
      (setq cleanup-errors
            (git363-test-attempt
             'restore-windows
             (lambda ()
               (git363-test-restore-windows configuration-before windows-before)
               (select-window window-before)
               (set-buffer buffer-before))
             cleanup-errors)))
      ;; Neo can schedule an undo boundary while the exact baseline window and
      ;; diagnostic buffers are being restored.  Validate/cancel only that
      ;; known core reaction, then restore its two controlling globals last.
      ;; No callback, buffer kill, or window mutation occurs after this point.
      (let ((index 0))
        (dolist (timer (git363-test-new-timers timers-before idle-timers-before))
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'restore-reaction-timer index)
                 (lambda ()
                   (unwind-protect
                       (unless (and
                                (eq (timer--function timer)
                                    #'undo-auto--boundary-timer)
                                (null (timer--repeat-delay timer))
                                (null (timer--args timer)))
                         (error "Unexpected timer after baseline restore: %S"
                                timer))
                     (cancel-timer timer)))
                 cleanup-errors))
          (setq index (1+ index))))
      (dolist (entry states-before)
        (when (memq (car entry) git363-test-terminal-state-symbols)
          (setq cleanup-errors
                (git363-test-attempt
                 (list 'restore-terminal-variable (car entry))
                 (lambda ()
                   (git363-test-restore-variable (car entry) (cdr entry)))
                 cleanup-errors))))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let* ((variable-mismatches
            (delq nil
                  (mapcar
                   (lambda (entry)
                     (unless (git363-test-variable-restored-p
                              (car entry) (cdr entry))
                       (car entry)))
                   states-before)))
           (cleanup-state
           (list
            :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
            :new-processes (seq-difference (process-list) processes-before #'eq)
            :new-timers (git363-test-new-timers timers-before idle-timers-before)
            :variables (null variable-mismatches)
            :variable-mismatches variable-mismatches
            :warnings (git363-test-buffer-content-restored-p warnings-before)
            :messages (git363-test-buffer-content-restored-p messages-before)
            :windows (equal (git363-test-window-state) windows-before)
            :configuration
            (compare-window-configurations
             (current-window-configuration) configuration-before)
            :buffer (eq (current-buffer) buffer-before)
            :window (eq (selected-window) window-before)
            :external-events git363-test-external-events
            :prompt-answers git363-test-prompt-answers
            :read-answers git363-test-read-answers
            :diff-temps
            (cl-every (lambda (path) (not (file-exists-p path)))
                      git363-test-diff-paths)
            :root (and git363-test-world
                       (not (file-exists-p
                             (plist-get git363-test-world :root))))
            :body-error body-error :cleanup-errors cleanup-errors)))
      (unless (and (null (plist-get cleanup-state :new-buffers))
                   (null (plist-get cleanup-state :new-processes))
                   (null (plist-get cleanup-state :new-timers))
                   (plist-get cleanup-state :variables)
                   (plist-get cleanup-state :warnings)
                   (plist-get cleanup-state :messages)
                   (plist-get cleanup-state :windows)
                   (plist-get cleanup-state :configuration)
                   (plist-get cleanup-state :buffer)
                   (plist-get cleanup-state :window)
                   (null (plist-get cleanup-state :external-events))
                   (null (plist-get cleanup-state :prompt-answers))
                   (null (plist-get cleanup-state :read-answers))
                   (plist-get cleanup-state :diff-temps)
                   (plist-get cleanup-state :root)
                   (null body-error) (null cleanup-errors))
        (error "Git Commit Mode workflow/cleanup failure: %S" cleanup-state))
      (list :result body-value :cleanup 'clean))))
"####;

fn git_commit_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_COMMIT_MODE_MELPA_PIN, "git-commit-mode.el")
        .expect("prepare terminal historical Git Commit Mode and exact dependencies below ./tmp")
        .with_prelude(GIT_COMMIT_MODE_TEST_PRELUDE)
        .with_timeout(Duration::from_secs(300))
}

#[test]
fn git_commit_mode_package_batch() {
    assert_oracle_batch_cases(
        git_commit_mode_oracle(),
        "git-commit-mode-package-batch",
        "Git Commit Mode",
        &workflows::git_commit_mode_batch_cases(),
    );
}
