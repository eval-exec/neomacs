use std::time::Duration;

use crate::{BROWSE_AT_REMOTE_MELPA_PIN, CachedMelpaOracle, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const BROWSE_AT_REMOTE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'json)
(require 'browse-url)
(require 'dired)
(require 'vc)
(require 'vc-git)
(require 'vc-annotate)
(require 'browse-at-remote)

(unless (and (eq (indirect-function 'bar-browse)
                 (indirect-function 'browse-at-remote))
             (eq (indirect-function 'bar-to-clipboard)
                 (indirect-function 'browse-at-remote-kill)))
  (error "Browse At Remote public aliases do not target their documented commands"))

(defvar bar355-test-browser-plan nil)
(defvar bar355-test-browser-events nil)
(defvar bar355-test-world-root nil)
(defvar bar355-test-git-program nil)
(defvar bar355-test-trace-file nil)
(defvar bar355-test-owned-buffers nil)
(defvar bar355-test-owned-timers nil)

(defun bar355-test-relative (path root)
  "Return PATH relative to owned ROOT, retaining a directory suffix."
  (when path
    (let ((relative (file-relative-name path root)))
      (if (and (string-suffix-p "/" path)
               (not (string-suffix-p "/" relative)))
          (concat relative "/")
        relative))))

(defun bar355-test-normalize-string (string root)
  "Normalize exact owned ROOT and Git executable occurrences in STRING."
  (let* ((root-directory (file-name-as-directory root))
         (root-name (directory-file-name root-directory))
         (result (copy-sequence string)))
    (setq result (replace-regexp-in-string
                  (regexp-quote root-directory) "[ROOT]/" result t t))
    (setq result (replace-regexp-in-string
                  (regexp-quote root-name) "[ROOT]" result t t))
    (when (and bar355-test-git-program
               (equal result bar355-test-git-program))
      (setq result "git"))
    result))

(defun bar355-test-owned-path (root relative)
  "Resolve RELATIVE below ROOT with semantic containment checks."
  (when (or (file-name-absolute-p relative)
            (equal relative "..")
            (string-prefix-p "../" relative))
    (error "Browse At Remote fixture path is not relative: %S" relative))
  (let* ((path (expand-file-name relative root))
         (parent (file-name-directory path)))
    (make-directory parent t)
    (unless (file-in-directory-p (file-truename parent)
                                 (file-truename root))
      (error "Browse At Remote fixture escaped owned root: %s" path))
    path))

(defun bar355-test-write-file (root relative contents)
  "Write CONTENTS to owned RELATIVE path below ROOT."
  (let ((path (bar355-test-owned-path root relative)))
    (write-region contents nil path nil 'silent)
    path))

(defun bar355-test-git (directory &rest args)
  "Run the real owned Git in DIRECTORY with exact ARGS or fail closed."
  (unless (and (stringp bar355-test-git-program)
               (file-name-absolute-p bar355-test-git-program)
               (file-executable-p bar355-test-git-program))
    (error "Browse At Remote has no absolute executable Git"))
  (unless (and (file-directory-p directory)
               (file-in-directory-p (file-truename directory)
                                     (file-truename bar355-test-world-root)))
    (error "Browse At Remote Git cwd is not owned: %S" directory))
  (unless (seq-every-p #'stringp args)
    (error "Browse At Remote Git argv is not all strings: %S" args))
  (let* ((stderr-file
          (bar355-test-owned-path bar355-test-world-root ".fixture-git.stderr"))
         (default-directory (file-name-as-directory directory))
         (process-environment (copy-sequence process-environment))
         (coding-system-for-read 'utf-8-unix)
         (coding-system-for-write 'utf-8-unix)
         status stdout stderr)
    ;; Fixture construction must never pollute the package-call trace.
    (setenv "GIT_TRACE2_EVENT" nil)
    (when (file-exists-p stderr-file) (delete-file stderr-file))
    (with-temp-buffer
      (setq status
            (apply #'process-file bar355-test-git-program nil
                   (list (current-buffer) stderr-file) nil args))
      (setq stdout (buffer-string)))
    (setq stderr
          (if (file-exists-p stderr-file)
              (with-temp-buffer
                (insert-file-contents stderr-file)
                (buffer-string))
            ""))
    (when (file-exists-p stderr-file) (delete-file stderr-file))
    (unless (and (integerp status) (zerop status))
      (error "Browse At Remote fixture Git failed: cwd=%S argv=%S status=%S stdout=%S stderr=%S"
             (bar355-test-relative directory bar355-test-world-root)
             args status stdout stderr))
    (list :status status :stdout stdout :stderr stderr)))

(defun bar355-test-git-stdout (directory &rest args)
  "Run fixture Git and return its stdout without a trailing newline."
  (string-trim-right
   (plist-get (apply #'bar355-test-git directory args) :stdout)))

(defun bar355-test-config (repo key value)
  "Set real repository-local Git KEY to VALUE in REPO."
  (bar355-test-git repo "config" "--local" key value))

(defun bar355-test-unset-config (repo key)
  "Remove real repository-local Git KEY, accepting only Git's 0/5 statuses."
  (let ((default-directory (file-name-as-directory repo))
        (process-environment (copy-sequence process-environment))
        status)
    (setenv "GIT_TRACE2_EVENT" nil)
    (setq status
          (process-file bar355-test-git-program nil nil nil
                        "config" "--local" "--unset-all" key))
    (unless (memq status '(0 5))
      (error "Browse At Remote could not unset Git config %S: status=%S"
             key status))
    status))

(defun bar355-test-make-repo (root remote &optional branch)
  "Create a deterministic real Git repository below ROOT using REMOTE."
  (let* ((branch (or branch "main"))
         (repo (file-name-as-directory
                (bar355-test-owned-path root "repo")))
         (process-environment (copy-sequence process-environment)))
    (make-directory repo)
    (setenv "GIT_AUTHOR_NAME" "Parity Author")
    (setenv "GIT_AUTHOR_EMAIL" "parity@example.invalid")
    (setenv "GIT_COMMITTER_NAME" "Parity Author")
    (setenv "GIT_COMMITTER_EMAIL" "parity@example.invalid")
    (setenv "GIT_AUTHOR_DATE" "2001-02-03T04:05:06+0000")
    (setenv "GIT_COMMITTER_DATE" "2001-02-03T04:05:06+0000")
    (bar355-test-git repo "init" "--quiet" "--object-format=sha1"
                        (format "--initial-branch=%s" branch) ".")
    (bar355-test-config repo "user.name" "Parity Author")
    (bar355-test-config repo "user.email" "parity@example.invalid")
    (bar355-test-config repo "commit.gpgsign" "false")
    (bar355-test-write-file
     root "repo/docs/Release Notes.md"
     "alpha\nbeta界\ngamma\ndelta\nepsilon\n")
    (bar355-test-write-file
     root "repo/src/demo.el"
     ";;; demo.el\n(defun demo ()\n  \"hello\")\n")
    (bar355-test-write-file
     root "repo/README.md"
     "# Widget Kit\n\nRead me.\nMore details.\n")
    (bar355-test-git repo "add" "--all")
    (bar355-test-git repo "commit" "--quiet" "--no-gpg-sign"
                        "-m" "Initial deterministic fixture")
    (bar355-test-git repo "remote" "add" "origin" remote)
    (bar355-test-git repo "update-ref"
                        (format "refs/remotes/origin/%s" branch) "HEAD")
    (bar355-test-config repo (format "branch.%s.remote" branch) "origin")
    (bar355-test-config repo (format "branch.%s.merge" branch)
                        (format "refs/heads/%s" branch))
    (let ((head (bar355-test-git-stdout repo "rev-parse" "HEAD")))
      (unless (string-match-p "\\`[0-9a-f]\\{40\\}\\'" head)
        (error "Browse At Remote fixture HEAD is not SHA-1: %S" head))
      (list :repo repo
            :head head
            :notes (expand-file-name "docs/Release Notes.md" repo)
            :demo (expand-file-name "src/demo.el" repo)
            :readme (expand-file-name "README.md" repo)))))

(defun bar355-test-second-commit (root repo)
  "Change one line in REPO and create a second deterministic commit."
  (let ((process-environment (copy-sequence process-environment)))
    (bar355-test-write-file
     root "repo/docs/Release Notes.md"
     "alpha\nbeta界 updated\ngamma\ndelta\nepsilon\n")
    (setenv "GIT_AUTHOR_DATE" "2002-03-04T05:06:07+0000")
    (setenv "GIT_COMMITTER_DATE" "2002-03-04T05:06:07+0000")
    (bar355-test-git repo "add" "docs/Release Notes.md")
    (bar355-test-git repo "commit" "--quiet" "--no-gpg-sign"
                        "-m" "Update Unicode line")
    (bar355-test-git-stdout repo "rev-parse" "HEAD")))

(defun bar355-test-capture (function)
  "Return FUNCTION's value or exact signaled condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition)
             :data (cdr condition)
             :message (error-message-string condition)))))

(defun bar355-test-string-occurrences (string needle)
  "Count nonoverlapping literal NEEDLE occurrences in STRING."
  (unless (and (stringp string) (stringp needle) (> (length needle) 0))
    (error "Browse At Remote invalid occurrence inputs: %S %S" string needle))
  (let ((start 0) (count 0) (regexp (regexp-quote needle)))
    (while (string-match regexp string start)
      (setq count (1+ count)
            start (match-end 0)))
    count))

(defun bar355-test-normalize-head (value head)
  "Validate exact SHA-1 HEAD in VALUE and replace just it by [HEAD]."
  (unless (and (stringp head)
               (string-match-p "\\`[0-9a-f]\\{40\\}\\'" head))
    (error "Browse At Remote invalid dynamic HEAD: %S" head))
  (unless (and (stringp value)
               (= (bar355-test-string-occurrences value head) 1))
    (error "Browse At Remote expected HEAD exactly once in: %S" value))
  (replace-regexp-in-string (regexp-quote head) "[HEAD]" value t t))

(defun bar355-test-normalize-abbrev (value abbrev head)
  "Validate ABBREV as HEAD's prefix and replace its exact VALUE occurrence."
  (unless (and (stringp abbrev) (>= (length abbrev) 7)
               (string-prefix-p abbrev head))
    (error "Browse At Remote invalid annotated revision: %S for %S"
           abbrev head))
  (unless (and (stringp value)
               (= (bar355-test-string-occurrences value abbrev) 1))
    (error "Browse At Remote annotation value lacks exactly one revision %S: %S"
           abbrev value))
  (replace-regexp-in-string (regexp-quote abbrev)
                            "[ABBREV-HEAD]" value t t))

(defun bar355-test-normalize-trace-head (trace head)
  "Replace the sole exact HEAD argv in copied Git TRACE."
  (let ((normalized (copy-tree trace)) (count 0))
    (dolist (record normalized)
      (setf (plist-get record :argv)
            (mapcar
             (lambda (arg)
               (if (equal arg head)
                   (progn (setq count (1+ count)) "[HEAD]")
                 arg))
             (plist-get record :argv))))
    (unless (= count 1)
      (error "Browse At Remote expected one exact HEAD trace argv, saw %d: %S"
             count trace))
    normalized))

(defun bar355-test-validate-provider-trace
    (traced worktree branch actual-host-status type-status)
  "Validate one exact provider TRACED call and return its public URL."
  (let* ((outcome (plist-get traced :outcome))
         (trace (plist-get traced :git))
         (expected
          (list
           (list :argv '("git" "--no-pager" "symbolic-ref" "HEAD")
                 :worktree worktree :exit 0)
           (list :argv
                 (list "git" "--no-pager" "config"
                       (format "branch.%s.pushRemote" branch))
                 :worktree worktree :exit 1)
           (list :argv
                 (list "git" "--no-pager" "rev-parse"
                       "--symbolic-full-name" "--abbrev-ref"
                       (format "%s@{upstream}" branch))
                 :worktree worktree :exit 0)
           (list :argv '("git" "--no-pager" "ls-remote" "--get-url" "origin")
                 :worktree worktree :exit 0)
           (list :argv
                 '("git" "--no-pager" "config" "--get"
                   "browseAtRemote.actualHost")
                 :worktree worktree :exit actual-host-status)
           (list :argv
                 '("git" "--no-pager" "config" "--get"
                   "browseAtRemote.type")
                 :worktree worktree :exit type-status))))
    (unless (equal trace expected)
      (error "Browse At Remote provider Git contract mismatch: expected=%S actual=%S"
             expected trace))
    (unless (and (eq (car-safe outcome) :value)
                 (stringp (cadr outcome)))
      (error "Browse At Remote provider route did not return a URL: %S" outcome))
    (copy-sequence (cadr outcome))))

(defun bar355-test-browser (url &rest args)
  "Consume one exact planned browser launch for URL and ARGS."
  (unless bar355-test-browser-plan
    (error "Browse At Remote made an unexpected browser launch: %S %S"
           url args))
  (let* ((entry (pop bar355-test-browser-plan))
         (expected-url (plist-get entry :url))
         (expected-args (plist-get entry :args))
         (event
          (list :url (copy-sequence url)
                :args (copy-tree args)
                :mode major-mode
                :buffer (copy-sequence (buffer-name))
                :file (bar355-test-relative buffer-file-name
                                            bar355-test-world-root)
                :directory (bar355-test-relative default-directory
                                                 bar355-test-world-root))))
    (push event bar355-test-browser-events)
    (unless (and (equal url expected-url) (equal args expected-args))
      (error "Browse At Remote browser contract mismatch: expected=%S actual=%S"
             entry event))
    (if-let ((condition (plist-get entry :signal)))
        (signal (car condition) (cdr condition))
      (plist-get entry :return))))

(defun bar355-test-trace-records (root &optional allow-empty)
  "Parse and validate complete real Git Trace2 records below ROOT."
  (unless (or (file-exists-p bar355-test-trace-file) allow-empty)
    (error "Browse At Remote package call emitted no Git Trace2 file"))
  (let ((records (make-hash-table :test #'equal))
        order)
    (when (file-exists-p bar355-test-trace-file)
      (with-temp-buffer
        (insert-file-contents bar355-test-trace-file)
        (goto-char (point-min))
        (while (< (point) (point-max))
          (let* ((line (buffer-substring-no-properties
                        (line-beginning-position) (line-end-position)))
                 (object
                  (condition-case condition
                      (json-parse-string line :object-type 'alist
                                         :array-type 'list
                                         :null-object nil :false-object nil)
                    (t (error "Browse At Remote malformed Git Trace2 line: %S %S"
                              line condition))))
                 (event (alist-get 'event object))
                 (sid (alist-get 'sid object)))
            (cond
             ((equal event "start")
              (when (gethash sid records)
                (error "Browse At Remote duplicate Git Trace2 start: %S" sid))
              (puthash sid
                       (list :argv (copy-sequence (alist-get 'argv object))
                             :worktree nil :exit :missing)
                       records)
              (push sid order))
             ((equal event "def_repo")
              (when-let ((record (gethash sid records)))
                (setf (plist-get record :worktree)
                      (copy-sequence (alist-get 'worktree object)))))
             ((equal event "exit")
              (when-let ((record (gethash sid records)))
                (setf (plist-get record :exit) (alist-get 'code object)))))
            (forward-line 1)))))
    (setq order (nreverse order))
    (when (and (null order) (not allow-empty))
      (error "Browse At Remote package call emitted no Git starts"))
    (mapcar
     (lambda (sid)
       (let* ((record (gethash sid records))
              (argv (plist-get record :argv))
              (worktree (plist-get record :worktree))
              (exit (plist-get record :exit)))
         (unless (and (listp argv) (seq-every-p #'stringp argv)
                      (integerp exit))
           (error "Browse At Remote incomplete Git Trace2 session: %S" record))
         (when worktree
           (unless (file-in-directory-p (file-truename worktree)
                                        (file-truename root))
             (error "Browse At Remote Git escaped owned root: %S" worktree)))
         (list :argv
               (mapcar (lambda (arg)
                         (bar355-test-normalize-string arg root))
                       argv)
               :worktree (bar355-test-relative worktree root)
               :exit exit)))
     order)))

(defun bar355-test-traced (root function &optional allow-empty)
  "Call FUNCTION with real Git Trace2 enabled and return outcome plus trace."
  (when (file-exists-p bar355-test-trace-file)
    (delete-file bar355-test-trace-file))
  (let ((process-environment (copy-sequence process-environment)) outcome trace)
    (setenv "GIT_TRACE2_EVENT" bar355-test-trace-file)
    (setq outcome (bar355-test-capture function))
    (setq trace (bar355-test-trace-records root allow-empty))
    (when (file-exists-p bar355-test-trace-file)
      (delete-file bar355-test-trace-file))
    (list :outcome outcome :git trace)))

(defun bar355-test-visit (file)
  "Visit owned FILE with local and directory variables disabled."
  (let ((enable-local-variables nil)
        (enable-dir-local-variables nil)
        (enable-local-eval nil))
    (let ((buffer (find-file-noselect file)))
      (cl-pushnew buffer bar355-test-owned-buffers :test #'eq)
      buffer)))

(defun bar355-test-own-buffer (buffer)
  "Register exact BUFFER as owned by the active case."
  (unless (buffer-live-p buffer)
    (error "Browse At Remote cannot own dead buffer: %S" buffer))
  (cl-pushnew buffer bar355-test-owned-buffers :test #'eq)
  buffer)

(defun bar355-test-wait-annotate (buffer)
  "Wait boundedly for real VC annotation and a stable terminal BUFFER."
  (let* ((process (get-buffer-process buffer))
         (deadline (+ (float-time) 5.0))
         previous-state
         (stable-rounds 0))
    (while (and process (process-live-p process) (< (float-time) deadline))
      (accept-process-output process 0.02))
    (when (and process (process-live-p process))
      (error "Browse At Remote VC annotate timed out: process=%S status=%S buffer=%S mode=%S"
             (process-name process) (process-status process)
             (and (buffer-live-p buffer) (buffer-name buffer))
             (and (buffer-live-p buffer)
                  (buffer-local-value 'major-mode buffer))))
    ;; A process may become non-live before its final output and sentinel have
    ;; been delivered.  Require three unchanged drain rounds before observing.
    (while (and (< stable-rounds 3) (< (float-time) deadline))
      (accept-process-output nil 0.02)
      (let ((state
             (and (buffer-live-p buffer)
                  (with-current-buffer buffer
                    (list major-mode (buffer-string)
                          (point-min) (point-max)
                          (buffer-modified-tick))))))
        (if (equal state previous-state)
            (setq stable-rounds (1+ stable-rounds))
          (setq previous-state state stable-rounds 0))))
    (unless (= stable-rounds 3)
      (error "Browse At Remote VC annotate never stabilized: process=%S state=%S"
             process previous-state))
    (unless (and (buffer-live-p buffer)
                 (eq (buffer-local-value 'major-mode buffer) 'vc-annotate-mode)
                 (> (buffer-size buffer) 0)
                 (file-exists-p bar355-test-trace-file)
                 (> (file-attribute-size
                     (file-attributes bar355-test-trace-file))
                    0))
      (error "Browse At Remote VC annotate has no completed terminal evidence: process=%S state=%S"
             process previous-state))
    (when (and process
               (not (and (eq (process-status process) 'exit)
                         (zerop (process-exit-status process)))))
      (error "Browse At Remote VC annotate failed: status=%S exit=%S"
             (process-status process) (process-exit-status process)))
    (let* ((trace (bar355-test-trace-records bar355-test-world-root))
           (blame (car trace))
           (attached (get-buffer-process buffer)))
      (unless (and (= (length trace) 1)
                   (equal (seq-take (plist-get blame :argv) 4)
                          '("git" "--no-pager" "blame" "--date=short"))
                   (zerop (plist-get blame :exit))
                   (not (and attached (process-live-p attached))))
        (error "Browse At Remote VC annotate terminal contract failed: trace=%S attached=%S"
               trace attached))
      (list :status 'exit :exit 0 :live nil
            :attached-live nil :stable-rounds stable-rounds
            :buffer-live (buffer-live-p buffer)
            :trace-terminal t))))

(defun bar355-test-run (name function)
  "Run FUNCTION in a fail-closed owned Git/editor world named NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root) (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" name)
      (error "Browse At Remote invalid owned case name: %S" name))
    (let* ((root (file-name-as-directory (expand-file-name name sandbox-root)))
           (root-owned nil)
           (git-program (executable-find "git" t))
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (current-buffer-baseline (current-buffer))
           (window-buffer-baseline (window-buffer))
           (dired-buffers-baseline (copy-tree dired-buffers))
           (vc-annotate-display-mode-baseline vc-annotate-display-mode)
           (kill-ring-baseline (copy-tree kill-ring))
           (kill-ring-yank-pointer-baseline kill-ring-yank-pointer)
           owned-buffers owned-timers result body-error cleanup cleanup-errors)
      (unless (and (stringp git-program) (file-name-absolute-p git-program)
                   (file-executable-p git-program))
        (error "Browse At Remote requires an absolute executable Git"))
      (when (file-exists-p root)
        (error "Browse At Remote owned case root already exists: %s" root))
      (cl-labels
          ((attempt
            (phase callback)
            (condition-case condition
                (funcall callback)
              (t (push (list phase condition) cleanup-errors) nil)))
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
                              (buffer-local-value 'default-directory buffer))))
                   (unless
                       (and (consp command)
                            (stringp (car command))
                            (equal (file-truename (car command))
                                   (file-truename git-program))
                            (buffer-live-p buffer)
                            (not (memq buffer buffer-baseline))
                            (stringp directory)
                            (file-directory-p directory)
                            (file-in-directory-p (file-truename directory)
                                                 (file-truename root)))
                     (error "refusing to stop unowned process: name=%S command=%S buffer=%S directory=%S"
                            (process-name process) command buffer directory)))
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
            (dolist (timer (seq-difference timer-idle-list
                                           idle-timer-baseline #'eq))
              (attempt
               phase
               (lambda ()
                 (when (and (not (memq timer owned-timers))
                            (eq (timer--function timer)
                                'undo-auto--boundary-timer)
                            (seq-some #'buffer-live-p owned-buffers))
                   (push timer owned-timers))
                 (unless (memq timer owned-timers)
                   (error "refusing to cancel unowned idle timer: %S" timer))
                 (cancel-timer timer))))
            (dolist (timer (seq-difference timer-list timer-baseline #'eq))
              (attempt
               phase
               (lambda ()
                 (when (and (not (memq timer owned-timers))
                            (eq (timer--function timer)
                                'undo-auto--boundary-timer)
                            (seq-some #'buffer-live-p owned-buffers))
                   (push timer owned-timers))
                 (unless (memq timer owned-timers)
                   (error "refusing to cancel unowned timer: %S" timer))
                 (cancel-timer timer)))))
           (kill-buffers
            (phase)
            (dolist (buffer (seq-difference (buffer-list)
                                            buffer-baseline #'eq))
              (attempt
               phase
               (lambda ()
                 (when (buffer-live-p buffer)
                   (let* ((file (buffer-local-value 'buffer-file-name buffer))
                          (directory
                           (buffer-local-value 'default-directory buffer))
                          (root-truename (file-truename root))
                          (semantically-owned
                           (or (and (stringp file)
                                    (file-exists-p file)
                                    (file-in-directory-p
                                     (file-truename file) root-truename))
                               (and (stringp directory)
                                    (file-directory-p directory)
                                    (file-in-directory-p
                                     (file-truename directory)
                                     root-truename)))))
                     (when semantically-owned
                       (cl-pushnew buffer owned-buffers :test #'eq)))
                   (unless (memq buffer owned-buffers)
                     (error "refusing to kill unowned buffer: %S file=%S directory=%S"
                            (buffer-name buffer)
                            (buffer-local-value 'buffer-file-name buffer)
                            (buffer-local-value 'default-directory buffer)))
                   (with-current-buffer buffer (set-buffer-modified-p nil))
                   (kill-buffer buffer))))))
           (drain-events
            (phase)
            (attempt
             phase
             (lambda ()
               (dotimes (_ 3) (accept-process-output nil 0.01))))))
        (unwind-protect
            (condition-case condition
                (progn
                  (make-directory root)
                  (setq root-owned t)
                  (let* ((bar355-test-world-root root)
                         (bar355-test-git-program git-program)
                         (bar355-test-trace-file
                          (expand-file-name ".package-git-trace.json" root))
                         (bar355-test-browser-plan nil)
                         (bar355-test-browser-events nil)
                         (bar355-test-owned-buffers nil)
                         (bar355-test-owned-timers nil)
                         (vc-git-program git-program)
                         (vc-file-prop-obarray (obarray-make 17))
                         (browse-url-browser-function #'bar355-test-browser)
                         (browse-url-handlers nil)
                         (browse-url-default-handlers nil)
                         (browse-url-transform-alist nil)
                         (browse-url-new-window-flag nil)
                         (kill-ring (copy-tree kill-ring-baseline))
                         (kill-ring-yank-pointer kill-ring)
                         (interprogram-cut-function nil)
                         (interprogram-paste-function nil)
                         (save-interprogram-paste-before-kill nil)
                         (kill-transform-function nil)
                         (kill-do-not-save-duplicates nil)
                         (dired-buffers (copy-tree dired-buffers-baseline))
                         (vc-annotate-display-mode vc-annotate-display-mode-baseline)
                         (enable-local-variables nil)
                         (enable-dir-local-variables nil)
                         (enable-local-eval nil)
                         (process-environment (copy-sequence process-environment))
                         (default-directory root))
                    (unwind-protect
                        (progn
                          (setenv "GIT_CONFIG_NOSYSTEM" "1")
                          (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
                          (setenv "GIT_CONFIG_SYSTEM" "/dev/null")
                          (setenv "LC_ALL" "C")
                          (setenv "TZ" "UTC")
                          (save-window-excursion
                            (save-current-buffer
                              (setq result (funcall function root))))
                          (unless (null bar355-test-browser-plan)
                            (error "Browse At Remote left browser fixtures unused: %S"
                                   bar355-test-browser-plan)))
                      (setq owned-buffers
                            (copy-sequence bar355-test-owned-buffers)
                            owned-timers
                            (copy-sequence bar355-test-owned-timers)))))
              (t (setq body-error condition)))
          (stop-processes 'processes-first)
          (drain-events 'drain-first)
          (cancel-timers 'timers-first)
          (kill-buffers 'buffers-first)
          (drain-events 'drain-second)
          (stop-processes 'processes-second)
          (cancel-timers 'timers-second)
          (kill-buffers 'buffers-second)
          (drain-events 'drain-third)
          (stop-processes 'processes-third)
          (cancel-timers 'timers-third)
          (kill-buffers 'buffers-third)
          (attempt
           'root
           (lambda ()
             (when root-owned
               (when (file-exists-p root) (delete-directory root t))
               (unless (file-exists-p root) (setq root-owned nil)))))
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
                    :owned-buffer-live
                    (delq nil
                          (mapcar (lambda (buffer)
                                    (and (buffer-live-p buffer)
                                         (buffer-name buffer)))
                                  owned-buffers))
                    :owned-timer-live
                    (seq-some (lambda (timer)
                                (or (memq timer timer-list)
                                    (memq timer timer-idle-list)))
                              owned-timers)
                    :root-exists (file-exists-p root)
                    :root-owned root-owned
                    :current-buffer-restored
                    (eq (current-buffer) current-buffer-baseline)
                    :window-restored
                    (eq (window-buffer) window-buffer-baseline)
                    :dired-buffers-restored
                    (equal dired-buffers dired-buffers-baseline)
                    :vc-annotate-display-restored
                    (equal vc-annotate-display-mode
                           vc-annotate-display-mode-baseline)
                    :kill-ring-restored (equal kill-ring kill-ring-baseline)
                    :kill-yank-restored
                    (eq kill-ring-yank-pointer kill-ring-yank-pointer-baseline)
                    :body-error body-error
                    :cleanup-errors (nreverse cleanup-errors))))))
        (let ((dirty
               (or body-error cleanup-errors
                   (plist-get cleanup :new-buffers)
                   (plist-get cleanup :new-processes)
                   (not (= (plist-get cleanup :new-timers) 0))
                   (plist-get cleanup :owned-buffer-live)
                   (plist-get cleanup :owned-timer-live)
                   (plist-get cleanup :root-exists)
                   (plist-get cleanup :root-owned)
                   (not (plist-get cleanup :current-buffer-restored))
                   (not (plist-get cleanup :window-restored))
                   (not (plist-get cleanup :dired-buffers-restored))
                   (not (plist-get cleanup :vc-annotate-display-restored))
                   (not (plist-get cleanup :kill-ring-restored))
                   (not (plist-get cleanup :kill-yank-restored)))))
          (when dirty
            (error "Browse At Remote world failed: body=%S cleanup=%S phase-errors=%S"
                   body-error cleanup cleanup-errors))
          (list :result result :cleanup cleanup))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BROWSE_AT_REMOTE_MELPA_PIN, "browse-at-remote.el")
        .expect("prepare exact shallow Browse At Remote source below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare exact shallow f dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare exact shallow s dependency below ./tmp")
        .with_prelude(BROWSE_AT_REMOTE_TEST_PRELUDE)
        .with_timeout(Duration::from_secs(300))
}

#[test]
fn browse_at_remote_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "browse-at-remote-package-batch",
        "Browse At Remote",
        &workflows::workflow_batch_cases(),
    );
}
