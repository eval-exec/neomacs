//! Practical parity for the pinned Pipenv porcelain.
//!
//! The corpus uses the exact MELPA source at
//! `3af159749824c03f59176aff7f66ddd6a5785a10`. External Pipenv, Python, and
//! shell boundaries are owned, argv-checked, and fail closed; no ambient
//! Python installation, virtual environment, or network participates.
//!
//! The replayed CLI response lines were recorded from Pipenv 2022.5.2 under
//! CPython 3.10.20 in an owned empty Pipfile project with an in-project
//! virtualenv. The exact real observations were `pipenv, version 2022.5.2`,
//! the project and virtualenv absolute paths, `Installing dependencies from
//! Pipfile...`, and `owned run`.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PIPENV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ansi-color)
(require 'shell)
(require 'python)
(require 'pyvenv)
(require 'load-env-vars)

;; The Projectile integration is optional and undeclared.  Keep package load
;; inside its declared dependency closure.
(setq pipenv-with-projectile nil)
(require 'pipenv)

;; Establish the package-owned keymap and minor-mode tables before baselines.
(with-temp-buffer (pipenv-mode 1) (pipenv-mode -1))

(defvar pip384-test-owned-roots nil)

(defun pip384-test-write-tool (root name script)
  (let* ((bin (expand-file-name "bin" root))
         (tool (expand-file-name name bin)))
    (make-directory bin t)
    (write-region script nil tool nil 'silent)
    (set-file-modes tool #o700)
    tool))

(defun pip384-test-read-file (path)
  (if (file-exists-p path)
      (with-temp-buffer
        (insert-file-contents-literally path)
        (buffer-string))
    nil))

(defun pip384-test-file-sha256 (path)
  "Hash the bytes at PATH rather than the spelling of PATH itself."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun pip384-test-normalize (value root)
  (cond
   ((stringp value)
    (replace-regexp-in-string (regexp-quote root) "<root>" value t t))
   ((consp value)
    (cons (pip384-test-normalize (car value) root)
          (pip384-test-normalize (cdr value) root)))
   (t value)))

(defun pip384-test-note-sentinel (process &rest _)
  "Record on PROCESS that its sentinel has run."
  (process-put process 'pip384-test-sentinel-ran t))

(defun pip384-test-wait-process (process)
  "Wait until PROCESS has run its sentinel, then report how it exited.
The caller pins the shell buffer's whole text, and that text is the single
line Process shell finished -- a line the SENTINEL writes.  So the pin
cannot be taken before the sentinel has run, and waiting for the process to
die is not that moment.  It is strictly earlier, by construction: GNU reaps
the child in `handle_child_signal', setting `raw_status_new'
\(src/process.c:7748), which is all `process-status' needs to answer `exit'
\(src/process.c:1188-1189), and in the same pass calling `delete_read_fd'
\(src/process.c:7760), so the pipe stops being read at exactly that instant.
Anything still queued is recovered only by the drain loop in `status_notify'
\(src/process.c:7896-7911), immediately before `exec_sentinel'
\(src/process.c:7937).  The previous shape here spent two fixed
`accept-process-output' calls hoping to cover that gap, and said so.

Either witness is accepted: the observer firing, or PROCESS leaving
`process-list', which both editors were measured to do only once the
sentinel has run."
  (unless (process-get process 'pip384-test-sentinel-ran)
    (add-function :after (process-sentinel process)
                  #'pip384-test-note-sentinel))
  (let ((deadline (+ (float-time) 30)))
    (while (and (not (process-get process 'pip384-test-sentinel-ran))
                (memq process (process-list))
                (< (float-time) deadline))
      (accept-process-output nil 0.05)))
  (unless (or (process-get process 'pip384-test-sentinel-ran)
              (not (memq process (process-list))))
    (error "pip384-test-wait-process: %S never ran its sentinel; the shell \
buffer holds only as much of the child's output as had been read"
           (process-command process)))
  (list :status (process-status process)
        :exit (process-exit-status process)))

(defun pip384-test-wait-file (path needle process)
  (let ((attempt 0) value)
    (while (and (< attempt 100)
                (not (and (setq value (pip384-test-read-file path))
                          (string-match-p (regexp-quote needle) value))))
      (accept-process-output process 0.05)
      (setq attempt (1+ attempt)))
    (unless value
      (error "Pipenv boundary log was not written: %s" path))
    value))

(defun pip384-test-process-call (function root)
  (let* ((buffer (get-buffer-create pipenv-process-buffer-name))
         (start (with-current-buffer buffer (point-max)))
         (process (funcall function))
         (command (process-command process))
         (outcome (pip384-test-wait-process process))
         (text (with-current-buffer buffer
                 (buffer-substring-no-properties start (point-max)))))
    (list :command (pip384-test-normalize command root)
          :outcome outcome
          :output (pip384-test-normalize text root))))

(defun pip384-test-run (name body)
  (let* ((root (make-temp-file (concat "pipenv384-" name "-") t))
         (buffers-before (buffer-list))
         (frames-before (frame-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (buffer-before (current-buffer))
         (windows-before (current-window-configuration))
         (mode-line-before (copy-tree mode-line-misc-info))
         (hack-locals-before (copy-sequence hack-local-variables-hook))
         (post-command-before (copy-sequence (default-value 'post-command-hook)))
         (eshell-path-before (default-value 'eshell-path-env))
         (process-environment (copy-sequence process-environment))
         (exec-path (copy-sequence exec-path))
         (process-connection-type nil)
         (message-log-max nil)
         (inhibit-message t)
         (python-shell-virtualenv-root nil)
         (python-shell-virtualenv-path nil)
         (pyvenv-virtual-env nil)
         (pyvenv-virtual-env-name nil)
         (pyvenv-virtual-env-path-directories nil)
         (pyvenv-old-process-environment nil)
         (pyvenv-pre-activate-hooks nil)
         (pyvenv-post-activate-hooks nil)
         (pyvenv-pre-deactivate-hooks nil)
         (pyvenv-post-deactivate-hooks nil)
         (pipenv-with-flycheck nil)
         (pipenv-with-projectile nil)
         (pip384-test-owned-roots (list root))
         result body-error cleanup-errors)
    (unwind-protect
        (condition-case error
            (setq result (funcall body root))
          (error (setq body-error error)))
      (condition-case error
          (when pyvenv-virtual-env (pyvenv-deactivate))
        (error (push (list :deactivate error) cleanup-errors)))
      (condition-case error
          (progn
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (set-window-configuration windows-before))
        (error (push (list :restore-windows error) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
            (error
             (push (list :delete-process (process-name process) error)
                   cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error
             (push (list :kill-buffer (buffer-name buffer) error)
                   cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error
              (delete-frame frame t)
            (error (push (list :delete-frame error) cleanup-errors)))))
      (dolist (owned pip384-test-owned-roots)
        (condition-case error
            (when (file-exists-p owned) (delete-directory owned t))
          (error (push (list :delete-root owned error) cleanup-errors))))
      ;; Filesystem cleanup can itself trigger coding-system work buffers.
      ;; Sweep reactions before the final residue audit.
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
            (error
             (push (list :delete-reaction-process (process-name process) error)
                   cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error
             (push (list :kill-reaction-buffer (buffer-name buffer) error)
                   cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-reaction-timer error) cleanup-errors)))))
      (setq mode-line-misc-info mode-line-before
            hack-local-variables-hook hack-locals-before)
      (set-default 'post-command-hook post-command-before)
      (set-default 'eshell-path-env eshell-path-before)
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process)) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (push (list :remaining-timer t) cleanup-errors)))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (push (list :remaining-frame t) cleanup-errors)))
      (dolist (owned pip384-test-owned-roots)
        (when (file-exists-p owned)
          (push (list :remaining-root owned) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "Pipenv body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors
      (error "Pipenv cleanup failed: %S" (nreverse cleanup-errors)))
     (t (list :result result :cleanup 'clean)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PIPENV_MELPA_PIN, "pipenv.el")
        .expect("prepare exact shallow Pipenv source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn mode_keymap_project_discovery_and_source_identity() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_keymap_project_discovery_and_source_identity",
        r####"(pip384-test-run
 "mode"
 (lambda (root)
   (let* ((project (expand-file-name "project λ" root))
          (nested (expand-file-name "src/pkg" project))
          (pipenv-executable "/owned/pipenv")
          (buffer (generate-new-buffer " *pip384-mode*")))
     (make-directory nested t)
     (write-region "[packages]\n" nil (expand-file-name "Pipfile" project)
                   nil 'silent)
     (switch-to-buffer buffer)
     (setq default-directory (file-name-as-directory nested))
     (pipenv-mode 1)
     (let ((enabled
            (list :feature (featurep 'pipenv)
                  :source (pip384-test-file-sha256
                           (symbol-file 'pipenv-mode))
                  :mode pipenv-mode
                  :lighter (assq 'pipenv-mode minor-mode-alist)
                  :project
                  (equal (file-name-as-directory project) (pipenv-project-p))
                  :installed (pipenv-installed-p)
                  :aliases (list (eq (indirect-function 'pipenv-project-p)
                                     (indirect-function 'pipenv-project?))
                                 (eq (indirect-function 'pipenv-installed-p)
                                     (indirect-function 'pipenv-installed?)))
                  :keys (mapcar
                         (lambda (key) (lookup-key pipenv-mode-map (kbd key)))
                         '("C-c C-p a" "C-c C-p d" "C-c C-p s"
                           "C-c C-p o" "C-c C-p i" "C-c C-p u")))))
       (pipenv-mode -1)
       (list :enabled enabled :disabled pipenv-mode)))))"####,
        expect![[
            r#"OK (:result (:enabled (:feature t :source "abac25a652d19f8c40ccdc9d3a10e96a2ac8d63923ec7963e656e5a8b005a423" :mode t :lighter (pipenv-mode " Pipenv") :project t :installed "/owned/pipenv" :aliases (t t) :keys (pipenv-activate pipenv-deactivate pipenv-shell pipenv-open pipenv-install pipenv-uninstall)) :disabled nil) :cleanup clean)"#
        ]],
    )
}

fn public_commands_drive_exact_async_process_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_commands_drive_exact_async_process_boundaries",
        r####"(pip384-test-run
 "commands"
 (lambda (root)
   (let* ((project (expand-file-name "project" root))
          (venv (expand-file-name "venv" root))
          (log (expand-file-name "commands.log" root))
          (tool
           (pip384-test-write-tool
            root "pipenv"
            (concat
             "#!/bin/sh\n"
             "printf 'argv' >>\"$PIP384_LOG\"\n"
             "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$PIP384_LOG\"; done\n"
             "printf '\\n' >>\"$PIP384_LOG\"\n"
             "if [ \"$#\" -eq 1 ] && [ \"$1\" = --where ]; then\n"
             "  printf '%s\\nignored where line\\n' \"$PIP384_PROJECT\"\n"
             "elif [ \"$#\" -eq 1 ] && [ \"$1\" = --venv ]; then\n"
             "  printf '%s\\nignored venv line\\n' \"$PIP384_VENV\"\n"
             "elif [ \"$#\" -eq 1 ] && [ \"$1\" = install ]; then\n"
             "  printf 'Installing dependencies from Pipfile...\\n'\n"
             "elif [ \"$#\" -eq 4 ] && [ \"$1\" = run ] && "
             "[ \"$2\" = python ] && [ \"$3\" = -c ] && "
             "[ \"$4\" = 'print(\"owned run\")' ]; then\n"
             "  printf 'owned run\\n'\n"
             "elif [ \"$#\" -eq 1 ] && [ \"$1\" = --version ]; then\n"
             "  printf 'pipenv, version 2022.5.2\\n'\n"
             "else\n"
             "  printf 'UNRECORDED\\n' >>\"$PIP384_LOG\"; exit 86\n"
             "fi\n")))
          (process-environment
           (append (list (concat "PIP384_LOG=" log)
                         (concat "PIP384_PROJECT=" project)
                         (concat "PIP384_VENV=" venv))
                   process-environment))
          (pipenv-executable tool)
          (pipenv-process-name "pip384-command")
          (pipenv-process-buffer-name " *pip384-command*")
          calls)
     (make-directory project t)
     (make-directory venv t)
     (dolist (function
              (list #'pipenv-where #'pipenv-venv
                    (lambda () (pipenv-install ""))
                    (lambda ()
                      (pipenv-run '("python" "-c" "print(\"owned run\")")))
                    #'pipenv-version))
       (push (pip384-test-process-call function root) calls))
     (list :calls (nreverse calls)
           :boundary (pip384-test-read-file log)))))"####,
        expect![[
            r#"OK (:result (:calls ((:command ("<root>/bin/pipenv" "--where") :outcome (:status exit :exit 0) :output "<root>/project\n<root>/bin/pipenv --where finished") (:command ("<root>/bin/pipenv" "--venv") :outcome (:status exit :exit 0) :output "<root>/venv\n<root>/bin/pipenv --venv finished") (:command ("<root>/bin/pipenv" "install") :outcome (:status exit :exit 0) :output "Installing dependencies from Pipfile...\n<root>/bin/pipenv install finished") (:command ("<root>/bin/pipenv" "run" "python" "-c" "print(\"owned run\")") :outcome (:status exit :exit 0) :output "owned run\n<root>/bin/pipenv run python -c print(\"owned run\") finished") (:command ("<root>/bin/pipenv" "--version") :outcome (:status exit :exit 0) :output "pipenv, version 2022.5.2\n<root>/bin/pipenv --version finished")) :boundary "argv<--where>\nargv<--venv>\nargv<install>\nargv<run><python><-c><print(\"owned run\")>\nargv<--version>\n") :cleanup clean)"#
        ]],
    )
}

fn activation_loads_env_and_deactivation_restores_virtualenv_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "activation_loads_env_and_deactivation_restores_virtualenv_state",
        r####"(pip384-test-run
 "activate"
 (lambda (root)
   (let* ((project (expand-file-name "project" root))
          (nested (expand-file-name "src" project))
          (venv (file-name-as-directory (expand-file-name "venv" root)))
          (venv-bin (directory-file-name (expand-file-name "bin" venv)))
          (venv-tool (expand-file-name "pip384-lint" venv-bin))
          (log (expand-file-name "activate.log" root))
          (tool
           (pip384-test-write-tool
            root "pipenv"
            (concat
             "#!/bin/sh\n"
             "printf 'argv' >>\"$PIP384_LOG\"\n"
             "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$PIP384_LOG\"; done\n"
             "printf '\\n' >>\"$PIP384_LOG\"\n"
             "if [ \"$#\" -eq 1 ] && [ \"$1\" = --venv ]; then\n"
             "  printf '%s\\n' \"$PIP384_VENV\"\n"
             "else\n"
             "  printf 'UNRECORDED\\n' >>\"$PIP384_LOG\"; exit 86\n"
             "fi\n")))
          (process-environment
           (append (list (concat "PIP384_LOG=" log)
                         (concat "PIP384_VENV=" (directory-file-name venv)))
                   process-environment))
          (pipenv-executable tool)
          (pipenv-process-name "pip384-activate")
          (pipenv-process-buffer-name " *pip384-activate*")
          (baseline-path "/base/bin:/shared/bin")
          (baseline-exec '("/base/bin" "/shared/bin")))
     (make-directory nested t)
     (make-directory venv-bin t)
     (write-region "#!/bin/sh\nexit 0\n" nil venv-tool nil 'silent)
     (set-file-modes venv-tool #o700)
     (write-region "[packages]\n" nil (expand-file-name "Pipfile" project)
                   nil 'silent)
     (write-region
      "export PIP384_ALPHA=one\nPIP384_UNICODE='λ界'\n# ignored\n"
      nil (expand-file-name ".env" project) nil 'silent)
     (setq default-directory (file-name-as-directory nested)
           exec-path (copy-sequence baseline-exec))
     (setenv "PATH" baseline-path)
     (setenv "PIP384_ALPHA" nil)
     (setenv "PIP384_UNICODE" nil)
     (let* ((activated-value (pipenv-activate))
            (activated
             (list :value activated-value
                   :project (equal (file-name-as-directory project)
                                   (pipenv-project?))
                   :pyvenv (equal pyvenv-virtual-env venv)
                   :python-root (equal python-shell-virtualenv-root
                                       (directory-file-name venv))
                   :virtual-env (file-equal-p (getenv "VIRTUAL_ENV") venv)
                   :path-first (equal (car (split-string (getenv "PATH")
                                                          path-separator))
                                      venv-bin)
                   :exec-first (equal (car exec-path) venv-bin)
                   :executable (equal (pipenv-executable-find "pip384-lint")
                                      venv-tool)
                   :env (list (getenv "PIP384_ALPHA")
                              (getenv "PIP384_UNICODE"))
                   :process-buffer
                   (pip384-test-normalize
                    (with-current-buffer pipenv-process-buffer-name
                      (buffer-string)) root))))
       (let ((deactivated-value (pipenv-deactivate)))
         (list :activated activated
               :deactivated
               (list :value deactivated-value
                     :pyvenv pyvenv-virtual-env
                     :python-root python-shell-virtualenv-root
                     :virtual-env (getenv "VIRTUAL_ENV")
                     :path (getenv "PATH") :exec exec-path
                     :env-persists (list (getenv "PIP384_ALPHA")
                                         (getenv "PIP384_UNICODE")))
               :boundary (pip384-test-read-file log)))))))"####,
        expect![[
            r#"OK (:result (:activated (:value t :project t :pyvenv t :python-root t :virtual-env t :path-first t :exec-first t :executable t :env ("one" "λ界") :process-buffer "<root>/venv") :deactivated (:value t :pyvenv nil :python-root nil :virtual-env nil :path "/base/bin:/shared/bin" :exec ("/base/bin" "/shared/bin") :env-persists ("one" "λ界")) :boundary "argv<--venv>\n") :cleanup clean)"#
        ]],
    )
}

fn public_open_resolves_owned_python_module_without_ambient_lookup() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_open_resolves_owned_python_module_without_ambient_lookup",
        r####"(pip384-test-run
 "open"
 (lambda (root)
   (let* ((project (expand-file-name "project" root))
          (module (expand-file-name "lib/demo.py" project))
          (reported (expand-file-name "lib/demo.pyc" project))
          (missing (expand-file-name "bin/missing-python" root))
          (log (expand-file-name "python.log" root))
          (tool
           (pip384-test-write-tool
            root "python"
            (concat
             "#!/bin/sh\n"
             "printf 'argv' >>\"$PIP384_LOG\"\n"
             "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$PIP384_LOG\"; done\n"
             "printf '\\n' >>\"$PIP384_LOG\"\n"
             "if [ \"$#\" -eq 2 ] && [ \"$1\" = -c ] && "
             "[ \"$2\" = 'import demo as mod; print(mod.__file__)' ]; then\n"
             "  printf '%s\\n' \"$PIP384_MODULE\"\n"
             "else\n"
             "  printf 'UNRECORDED\\n' >>\"$PIP384_LOG\"; exit 86\n"
             "fi\n")))
          (process-environment
           (append (list (concat "PIP384_LOG=" log)
                         (concat "PIP384_MODULE=" reported))
                   process-environment))
          (python-shell-interpreter missing)
          (enable-dir-local-variables nil))
     (make-directory (file-name-directory module) t)
     (write-region "value = 'λ界'\n" nil module nil 'silent)
     (setq default-directory (file-name-as-directory project))
     (let* ((origin (current-buffer))
            (failure
             (condition-case error
                 (progn (pipenv-open "demo") 'unexpected-success)
               (error
                (list :type (car error)
                      :launch (string-prefix-p "Doing vfork: "
                                                (error-message-string error))
                      :missing
                      (and (string-match-p "No such file or directory"
                                           (error-message-string error))
                           t)))))
            (failure-state
             (list :selected (eq origin (window-buffer (selected-window)))
                   :buffer (eq origin (current-buffer))
                   :log (pip384-test-read-file log))))
       (setq python-shell-interpreter tool)
     (pipenv-open "demo")
       (list :failure failure :failure-state failure-state
             :recovery
             (list :selected (eq (current-buffer)
                                 (window-buffer (selected-window)))
                   :file (file-relative-name buffer-file-name root)
                   :mode major-mode
                   :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :point (point))
             :boundary (pip384-test-normalize
                        (pip384-test-read-file log) root))))))"####,
        expect![[
            r#"OK (:result (:failure (:type file-missing :launch t :missing t) :failure-state (:selected t :buffer t :log nil) :recovery (:selected t :file "project/lib/demo.py" :mode python-mode :text "value = 'λ界'\n" :point 1) :boundary "argv<-c><import demo as mod; print(mod.__file__)>\n") :cleanup clean)"#
        ]],
    )
}

/// The stand-in shell writes exactly one line to stdout, and it writes it
/// before the init command is sent.
///
/// `pipenv-shell' ends with `comint-send-input' immediately followed by
/// `comint-clear-buffer' (pipenv.el:314-323), so the pinned buffer text is
/// decided INSIDE the package, before this fixture regains control -- there is
/// no instant at which a wait could be inserted.  What survives the clear is
/// therefore whatever the child wrote after Emacs's last read, and the only
/// read the fixture can order against is the untimed `accept-process-output'
/// inside `comint-send-input''s echo loop (lisp/comint.el:2065-2079), which
/// runs because `pipenv-shell' sets `comint-process-echoes'.
///
/// `ready\n' is written at child startup, so it precedes the tty echo in the
/// same stream and any read that delivers the echo has already delivered it:
/// it is deterministically erased, which is what makes the empty pre-sentinel
/// buffer a positive witness that `comint-clear-buffer' ran.  An `ack:' line
/// written in response to the init command is on the other side of that
/// boundary -- the child must be scheduled after Emacs's write -- so pinning a
/// buffer that may or may not contain it pinned a race.  Entry 169 measured it
/// at 8 passed / 4 failed over twelve solo runs on BOTH editors, with
/// byte-identical failing text; the boundary log still proves the child read
/// the init command on stdin.
fn public_shell_sends_init_command_and_closes_owned_comint_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_shell_sends_init_command_and_closes_owned_comint_process",
        r####"(pip384-test-run
 "shell"
 (lambda (root)
   (let* ((log (expand-file-name "shell.log" root))
          (tool
           (pip384-test-write-tool
            root "owned-shell"
            (concat
             "#!/bin/sh\n"
             "printf 'argv' >>\"$PIP384_LOG\"\n"
             "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$PIP384_LOG\"; done\n"
             "printf '\\n' >>\"$PIP384_LOG\"\n"
             "if ! [ \"$#\" -eq 1 ] || ! [ \"$1\" = -i ]; then\n"
             "  printf 'UNRECORDED\\n' >>\"$PIP384_LOG\"; exit 86\n"
             "fi\n"
             "printf 'ready\\n'\n"
             "while IFS= read -r line; do\n"
             "  if [ \"$line\" = 'exec pipenv shell' ]; then\n"
             "    printf 'stdin<%s>\\n' \"$line\" >>\"$PIP384_LOG\"\n"
             "  else\n"
             "    printf 'UNRECORDED-STDIN<%s>\\n' \"$line\" >>\"$PIP384_LOG\"\n"
             "    exit 86\n"
             "  fi\n"
             "done\n")))
          (process-environment
           (cons (concat "PIP384_LOG=" log) process-environment))
          (shell-file-name tool)
          (explicit-shell-file-name tool)
          (shell-mode-hook nil)
          (comint-mode-hook nil)
          (pipenv-shell-buffer-name " *pip384-shell*")
          (pipenv-shell-buffer-init-command "exec pipenv shell"))
     (setenv "ESHELL" tool)
     (pipenv-shell)
     (let* ((buffer (current-buffer))
            (process (get-buffer-process buffer)))
       (pip384-test-wait-file log "stdin<exec pipenv shell>" process)
       (let ((live-state
              (list :selected (eq buffer (window-buffer (selected-window)))
                    :mode major-mode
                    :name (buffer-name)
                    :echoes comint-process-echoes
                    :command (pip384-test-normalize
                              (process-command process) root)
                    :live (and (process-live-p process) t))))
         (comint-send-eof)
         (let ((outcome (pip384-test-wait-process process)))
           (list :live-state live-state
                 :outcome outcome
                 :output (buffer-substring-no-properties
                          (point-min) (point-max))
                 :boundary (pip384-test-normalize
                            (pip384-test-read-file log) root))))))))"####,
        expect![[
            r#"OK (:result (:live-state (:selected t :mode shell-mode :name " *pip384-shell*" :echoes t :command ("<root>/bin/owned-shell" "-i") :live t) :outcome (:status exit :exit 0) :output "\nProcess shell finished\n" :boundary "argv<-i>\nstdin<exec pipenv shell>\n") :cleanup clean)"#
        ]],
    )
}

#[test]
fn pipenv_package_batch() {
    let cases = vec![
        mode_keymap_project_discovery_and_source_identity(),
        public_commands_drive_exact_async_process_boundaries(),
        activation_loads_env_and_deactivation_restores_virtualenv_state(),
        public_open_resolves_owned_python_module_without_ambient_lookup(),
        public_shell_sends_init_command_and_closes_owned_comint_process(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed pipenv parity test");
    assert_oracle_batch_cases(oracle(), test_name, "pipenv_parity", &cases);
}
