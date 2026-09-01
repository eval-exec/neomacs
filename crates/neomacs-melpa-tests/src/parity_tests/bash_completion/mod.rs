//! Practical parity for Bash Completion's public shell and capf workflows.
//!
//! These cases register the documented hook, tokenize and requote a realistic
//! command line, complete commands and arguments through an owned Bash
//! stand-in, escape Unicode file names, and recover after disable, timeout,
//! debug, and reset.

use std::time::Duration;

use expect_test::expect;

use crate::{BASH_COMPLETION_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'comint)
(require 'bash-completion)
(set-window-configuration (current-window-configuration))

(defconst bc425-test-tree
  "b8ac4ca68002173338fd5974a1bd92ad92f56e7b")
(defconst bc425-test-manifest
  '(("bash-completion-pkg.el" . "e41ed2b66f10a892fd0b51c12293b1d36897ca4ca51ae73acd35a8f98d5c8a5b")
    ("bash-completion.el" . "fffd226753e6cbb3bd8157c6e5e1e9a299344b54c36eed0d565d1233796c96d0")))

(defvar bc425-test-case-index 0)
(defvar bc425-test-root nil)
(defvar bc425-test-root-owned nil)
(defvar bc425-test-bash nil)
(defvar bc425-test-transcript nil)
(defconst bc425-test-real-start-process (symbol-function 'start-process))
(defconst bc425-test-real-make-process (symbol-function 'make-process))

(defun bc425-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun bc425-test-source-state ()
  (let* ((located (symbol-file 'bash-completion-dynamic-complete-nocomint 'defun))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (bc425-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/bash-completion.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car bc425-test-manifest)))
      (error "Unexpected installed Bash Completion payload: %S" (or manifest files)))
    (dolist (entry bc425-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (bc425-test-sha file) (cdr entry)))
          (error "Unexpected installed Bash Completion source: %S"
                 (cons entry manifest)))))
    (list :tree bc425-test-tree
          :manifest bc425-test-manifest
          :feature (featurep 'bash-completion)
          :version "20260206.1459")))

(defun bc425-test-window-state ()
  (mapcar
   (lambda (window)
     (list window
           (eq window (selected-window))
           (window-buffer window)
           (window-point window)
           (window-start window)
           (window-hscroll window)
           (window-dedicated-p window)
           (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun bc425-test-mask (string)
  (let ((text (copy-sequence (or string "")))
        (root bc425-test-root)
        (tmp temporary-file-directory))
    (when (and root (file-name-absolute-p root))
      (setq text (replace-regexp-in-string
                  (regexp-quote root) "[ORACLE-SANDBOX]/" text t t))
      (setq text (replace-regexp-in-string
                  (regexp-quote (directory-file-name root))
                  "[ORACLE-SANDBOX]" text t t)))
    (when (and tmp (file-name-absolute-p tmp))
      (setq text (replace-regexp-in-string
                  (regexp-quote tmp) "[ORACLE-TMPDIR]/" text t t))
      (setq text (replace-regexp-in-string
                  (regexp-quote (directory-file-name tmp))
                  "[ORACLE-TMPDIR]" text t t)))
    text))

(defun bc425-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (bc425-test-mask item)
                             (copy-tree item)))
                         (cdr condition))
           :message (bc425-test-mask (error-message-string condition))))))

(defun bc425-test-write (relative contents)
  (let ((file (expand-file-name relative bc425-test-root)))
    (unless (and bc425-test-root-owned
                 (file-in-directory-p file bc425-test-root))
      (error "Refusing Bash Completion write outside owned root: %S" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix)
          (enable-local-variables nil))
      (with-temp-file file (insert contents)))
    file))

(defun bc425-test-forbid-external (operation &rest arguments)
  (error "Unexpected Bash Completion external boundary: %S %S" operation arguments))

(defun bc425-test-owned-bash-p (program)
  (and bc425-test-bash
       program
       (stringp program)
       (file-name-absolute-p program)
       (equal (file-truename program)
              (file-truename bc425-test-bash))))

(defun bc425-test-start-process (name buffer program &rest args)
  (unless (bc425-test-owned-bash-p program)
    (apply #'bc425-test-forbid-external 'start-process name buffer program args))
  (apply bc425-test-real-start-process name buffer program args))

(defun bc425-test-make-process (&rest spec)
  (let ((program (car (plist-get spec :command))))
    (unless (bc425-test-owned-bash-p program)
      (apply #'bc425-test-forbid-external 'make-process spec))
    (apply bc425-test-real-make-process spec)))

(defun bc425-test-install-bash ()
  (let ((script (bc425-test-write
                 "bin/owned-bash"
                 "#!/usr/bin/env python3
import os, sys, time
log = os.environ.get('BC425_TRANSCRIPT', '/dev/null')
with open(log, 'a', encoding='utf-8') as fh:
    fh.write('emacs-bash-complete=%s\\n' % os.environ.get('EMACS_BASH_COMPLETE'))
    fh.write('argv:%s\\n' % ' '.join(sys.argv[1:]))
sys.stdout.reconfigure(line_buffering=True)

def reply(body=''):
    sys.stdout.write(body)
    sys.stdout.write('==emacs==ret=0==.\\n')
    sys.stdout.flush()

for raw in sys.stdin:
    with open(log, 'a', encoding='utf-8') as fh:
        fh.write('cmd:%s' % raw)
    hang = os.environ.get('BC425_HANG') == '1' and '__ebcompgen' in raw
    if hang:
        time.sleep(8)
        continue
    if 'BASH_VERSION' in raw:
        reply('5.2.15')
    elif 'COMP_WORDBREAKS' in raw:
        reply('@><=;|&(:')
    elif 'BASH_COMPLETION_VERSINFO' in raw:
        reply('2 11')
    elif 'bind -v' in raw:
        reply('set completion-ignore-case off\\n')
    elif 'complete -p' in raw:
        if ' git' in raw:
            reply(\"complete -W 'status stash stage' git\\n\")
        else:
            reply()
    elif '__ebcompgen -b -c -a' in raw:
        reply('git\\ngitk\\n')
    elif '__ebcompgen -o default' in raw:
        reply('café-note.txt\\ncafé 界.md\\n')
    elif '__ebcompgen' in raw:
        reply('stage\\nstash\\nstatus\\n')
    elif 'PS1=' in raw or 'function ' in raw or 'echo -n' in raw:
        reply()
")))
    (set-file-modes script #o755)
    (setq bc425-test-bash script
          bc425-test-transcript (expand-file-name "bash.transcript" bc425-test-root))
    (setenv "BC425_TRANSCRIPT" bc425-test-transcript)
    (setenv "BC425_HANG" nil)
    script))

(defun bc425-test-transcript-state ()
  (when (and bc425-test-transcript (file-readable-p bc425-test-transcript))
    (with-temp-buffer
      (insert-file-contents bc425-test-transcript)
      (let ((text (buffer-string)))
        (list :emacs-bash
              (and (string-match "^emacs-bash-complete=\\(.*\\)$" text)
                   (copy-sequence (match-string 1 text)))
              :argv
              (and (string-match "^argv:\\(.*\\)$" text)
                   (copy-sequence (match-string 1 text)))
              :saw-compgen
              (and (string-match-p "__ebcompgen" text) t)
              :saw-complete-p
              (and (string-match-p "complete -p" text) t))))))

(defun bc425-test-complete (text)
  (erase-buffer)
  (insert text)
  (goto-char (point-max))
  (let* ((capf (bash-completion-dynamic-complete-nocomint
                (point-min) (point) nil))
         (start (nth 0 capf))
         (end (nth 1 capf))
         (table (nth 2 capf)))
    (list :stub (and start end (buffer-substring-no-properties start end))
          :start start
          :end end
          :point (point)
          :candidates (and (listp table) (mapcar #'copy-sequence table))
          :boundary (bc425-test-transcript-state))))

(defun bc425-test-run (body)
  (let* ((index (cl-incf bc425-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "bash-completion-%d" index) sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (bc425-test-window-state))
         (source-before (bc425-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (home-before (getenv "HOME"))
         (term-before (getenv "TERM"))
         (emacs-bash-before (getenv "EMACS_BASH_COMPLETE"))
         (hang-before (getenv "BC425_HANG"))
         (transcript-before (getenv "BC425_TRANSCRIPT"))
         (enabled-before bash-completion-enabled)
         (separate-before bash-completion-use-separate-processes)
         (prog-before bash-completion-prog)
         (args-before bash-completion-args)
         (start-files-before bash-completion-start-files)
         (nospace-before bash-completion-nospace)
         (timeout-before bash-completion-process-timeout)
         (initial-before bash-completion-initial-timeout)
         (short-before bash-completion-short-command-timeout)
         (processes-var-before bash-completion-processes)
         (debug-info-before bash-completion--debug-info)
         (hooks-before shell-dynamic-complete-functions)
         (tmp-before (directory-files temporary-file-directory t))
         (bc425-test-root root)
         (bc425-test-root-owned nil)
         (bc425-test-bash nil)
         (bc425-test-transcript nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Bash Completion sandbox root"))
          (when (file-exists-p root)
            (error "Bash Completion sandbox root exists: %S" root))
          (make-directory root)
          (setq bc425-test-root-owned t
                enable-local-variables nil
                debug-on-error nil
                print-circle nil
                default-directory root
                bash-completion-enabled t
                bash-completion-use-separate-processes t
                bash-completion-start-files nil
                bash-completion-nospace nil
                bash-completion-process-timeout 0.6
                bash-completion-initial-timeout 2.5
                bash-completion-short-command-timeout 0.6
                bash-completion-processes nil
                bash-completion--debug-info nil)
          (setenv "HOME" (directory-file-name root))
          (bc425-test-install-bash)
          (setq bash-completion-prog bc425-test-bash
                bash-completion-args '("--noediting"))
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external 'call-process args)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external
                              'call-process-region args)))
                    ((symbol-function 'process-file)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external 'process-file args)))
                    ((symbol-function 'start-process)
                     #'bc425-test-start-process)
                    ((symbol-function 'start-file-process)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external
                              'start-file-process args)))
                    ((symbol-function 'make-process)
                     #'bc425-test-make-process)
                    ((symbol-function 'make-network-process)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external
                              'make-network-process args)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external 'url-retrieve args)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external
                              'url-retrieve-synchronously args)))
                    ((symbol-function 'kill-emacs)
                     (lambda (&rest args)
                       (apply #'bc425-test-forbid-external 'kill-emacs args))))
            (setq result (funcall body)))
          (setq source-after (bc425-test-source-state))
          (unless (equal source-before source-after)
            (error "Bash Completion source changed")))
          (t (setq body-error
                   (list :error (car condition)
                         :data (copy-tree (cdr condition))
                         :message (error-message-string condition)))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (setq bash-completion-enabled enabled-before
              bash-completion-use-separate-processes separate-before
              bash-completion-prog prog-before
              bash-completion-args args-before
              bash-completion-start-files start-files-before
              bash-completion-nospace nospace-before
              bash-completion-process-timeout timeout-before
              bash-completion-initial-timeout initial-before
              bash-completion-short-command-timeout short-before
              bash-completion--debug-info debug-info-before
              shell-dynamic-complete-functions hooks-before)
        (attempt 'reset-all #'bash-completion-reset-all)
        (setq bash-completion-processes processes-var-before)
        (setenv "HOME" home-before)
        (setenv "TERM" term-before)
        (setenv "EMACS_BASH_COMPLETE" emacs-bash-before)
        (setenv "BC425_HANG" hang-before)
        (setenv "BC425_TRANSCRIPT" transcript-before)
        (setq default-directory directory-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before)
        (dolist (file (directory-files temporary-file-directory t))
          (unless (member file tmp-before)
            (attempt (list 'temp file)
                     (lambda ()
                       (if (file-directory-p file)
                           (delete-directory file t)
                         (delete-file file))))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (with-current-buffer buffer
                         (let ((kill-buffer-hook nil)
                               (kill-buffer-query-functions nil))
                           (set-buffer-modified-p nil)
                           (kill-buffer buffer)))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when bc425-test-root-owned
          (attempt 'sandbox (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :hook-restored (eq shell-dynamic-complete-functions hooks-before)
                 :processes-restored (eq bash-completion-processes processes-var-before)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (bc425-test-window-state) window-state-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Bash Completion workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BASH_COMPLETION_MELPA_PIN, "bash-completion.el")
        .expect("prepare exact Bash Completion source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_setup_tokenizes_quoted_pipeline_and_quotes_unicode() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_setup_tokenizes_quoted_pipeline_and_quotes_unicode",
        r####"
(bc425-test-run
 (lambda ()
   (bash-completion-setup)
   (let ((hooked (and (memq 'bash-completion-dynamic-complete
                            shell-dynamic-complete-functions)
                      t)))
     (with-temp-buffer
       (insert "cd \"notes café\" && ls -l café\\ 界.md; git status")
       (let* ((tokens (bash-completion-tokenize (point-min) (point-max)))
              (strings (bash-completion-strings-from-tokens tokens))
              (command (bash-completion-strings-from-tokens
                        (bash-completion-parse-current-command tokens))))
         (list :hooked hooked
               :tokens (mapcar #'copy-sequence strings)
               :command (mapcar #'copy-sequence command)
               :joined (copy-sequence
                        (bash-completion-join
                         '("git" "commit" "-m" "café 界" "notes/café-note.txt")))
               :empty (bash-completion-quote "")
               :plain (bash-completion-quote "status")))))))
"####,
        expect![[
            r#"OK (:source (:tree "b8ac4ca68002173338fd5974a1bd92ad92f56e7b" :manifest (("bash-completion-pkg.el" . "e41ed2b66f10a892fd0b51c12293b1d36897ca4ca51ae73acd35a8f98d5c8a5b") ("bash-completion.el" . "fffd226753e6cbb3bd8157c6e5e1e9a299344b54c36eed0d565d1233796c96d0")) :feature t :version "20260206.1459") :result (:hooked t :tokens ("cd" "notes café" "&&" "ls" "-l" "café 界.md" ";" "git" "status") :command ("git" "status") :joined "git commit -m 'café 界' 'notes/café-note.txt'" :empty "''" :plain "status") :cleanup (:source-unchanged t :hook-restored t :processes-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_nocomint_completes_command_and_git_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_nocomint_completes_command_and_git_arguments",
        r####"
(bc425-test-run
 (lambda ()
   (bash-completion-setup)
   (with-temp-buffer
     (comint-mode)
     (insert "gi")
     (goto-char (point-max))
     (let* ((dynamic (bash-completion-dynamic-complete))
            (command
             (list :stub (buffer-substring-no-properties (nth 0 dynamic) (nth 1 dynamic))
                   :candidates (mapcar #'copy-sequence
                                       (all-completions "gi" (nth 2 dynamic)))))
            (argument (bc425-test-complete "git st"))
            (running (and (bash-completion-is-running) t)))
       (list :command command
             :argument argument
             :running running
             :hooked (and (memq 'bash-completion-dynamic-complete
                                shell-dynamic-complete-functions)
                          t)
             :boundary (bc425-test-transcript-state)
             :prog-basename
             (file-name-nondirectory bash-completion-prog)
             :args (copy-tree bash-completion-args))))))
"####,
        expect![[
            r#"OK (:source (:tree "b8ac4ca68002173338fd5974a1bd92ad92f56e7b" :manifest (("bash-completion-pkg.el" . "e41ed2b66f10a892fd0b51c12293b1d36897ca4ca51ae73acd35a8f98d5c8a5b") ("bash-completion.el" . "fffd226753e6cbb3bd8157c6e5e1e9a299344b54c36eed0d565d1233796c96d0")) :feature t :version "20260206.1459") :result (:command (:stub "gi" :candidates ("git" "gitk")) :argument (:stub "st" :start 5 :end 7 :point 7 :candidates ("stage" "stash" "status") :boundary (:emacs-bash "t" :argv "--noediting" :saw-compgen t :saw-complete-p t)) :running t :hooked t :boundary (:emacs-bash "t" :argv "--noediting" :saw-compgen t :saw-complete-p t) :prog-basename "owned-bash" :args ("--noediting")) :cleanup (:source-unchanged t :hook-restored t :processes-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_capf_completes_unicode_files_and_stays_nonexclusive() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_capf_completes_unicode_files_and_stays_nonexclusive",
        r####"
(bc425-test-run
 (lambda ()
   (with-temp-buffer
     (insert "ls café")
     (goto-char (point-max))
     (let* ((capf (bash-completion-capf-nonexclusive))
            (start (nth 0 capf))
            (end (nth 1 capf))
            (table (nth 2 capf))
            (plist (nthcdr 3 capf)))
       (list :stub (and start end (buffer-substring-no-properties start end))
             :exclusive (plist-get plist :exclusive)
             :candidates (and (functionp table)
                              (mapcar #'copy-sequence
                                      (all-completions
                                       "café"
                                       table
                                       (plist-get plist :predicate))))
             :boundary (bc425-test-transcript-state))))))
"####,
        expect![[
            r#"OK (:source (:tree "b8ac4ca68002173338fd5974a1bd92ad92f56e7b" :manifest (("bash-completion-pkg.el" . "e41ed2b66f10a892fd0b51c12293b1d36897ca4ca51ae73acd35a8f98d5c8a5b") ("bash-completion.el" . "fffd226753e6cbb3bd8157c6e5e1e9a299344b54c36eed0d565d1233796c96d0")) :feature t :version "20260206.1459") :result (:stub "café" :exclusive no :candidates ("café-note.txt" "café\\ 界.md") :boundary (:emacs-bash "t" :argv "--noediting" :saw-compgen t :saw-complete-p t)) :cleanup (:source-unchanged t :hook-restored t :processes-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_disabled_timeout_debug_and_reset_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_disabled_timeout_debug_and_reset_recover",
        r####"
(bc425-test-run
 (lambda ()
   (with-temp-buffer
     (let* ((disabled
             (progn
               (setq bash-completion-enabled nil)
               (insert "git st")
               (goto-char (point-max))
               (bash-completion-dynamic-complete-nocomint
                (point-min) (point) nil)))
            (armed
             (progn
               (setq bash-completion-enabled t)
               (bc425-test-complete "git st")))
            (debug
             (progn
               (bash-completion-debug)
               (with-current-buffer "*bash-completion-debug*"
                 (let ((text (buffer-string)))
                   (list :name (copy-sequence (buffer-name))
                         :mentions-commandline
                         (and (string-match-p "commandline" text) t)
                         :mentions-owned-bash
                         (and (string-match-p "owned-bash" text) t))))))
            (timeout
             (progn
               (bash-completion-reset)
               (setenv "BC425_HANG" "1")
               (bc425-test-condition
                (lambda () (bc425-test-complete "git st")))))
            (reset
             (progn
               (setenv "BC425_HANG" nil)
               (bash-completion-reset)
               (and (not (bash-completion-is-running)) t)))
            (recovered
             (bc425-test-complete "git st")))
       (list :disabled disabled
             :armed (plist-get armed :candidates)
             :debug debug
             :timeout timeout
             :reset reset
             :recovered recovered)))))
"####,
        expect![[
            r#"OK (:source (:tree "b8ac4ca68002173338fd5974a1bd92ad92f56e7b" :manifest (("bash-completion-pkg.el" . "e41ed2b66f10a892fd0b51c12293b1d36897ca4ca51ae73acd35a8f98d5c8a5b") ("bash-completion.el" . "fffd226753e6cbb3bd8157c6e5e1e9a299344b54c36eed0d565d1233796c96d0")) :feature t :version "20260206.1459") :result (:disabled nil :armed ("stage" "stash" "status") :debug (:name "*bash-completion-debug*" :mentions-commandline t :mentions-owned-bash nil) :timeout (:error error :data ("Bash completion failed.  M-x bash-completion-debug for details") :message "Bash completion failed.  M-x bash-completion-debug for details") :reset t :recovered (:stub "st" :start 5 :end 7 :point 7 :candidates ("stage" "stash" "status") :boundary (:emacs-bash "t" :argv "--noediting" :saw-compgen t :saw-complete-p t))) :cleanup (:source-unchanged t :hook-restored t :processes-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn bash_completion_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_setup_tokenizes_quoted_pipeline_and_quotes_unicode(),
        public_nocomint_completes_command_and_git_arguments(),
        public_capf_completes_unicode_files_and_stays_nonexclusive(),
        public_disabled_timeout_debug_and_reset_recover(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "bash-completion-rank425",
        "bash_completion_parity",
        &cases,
    );
}
