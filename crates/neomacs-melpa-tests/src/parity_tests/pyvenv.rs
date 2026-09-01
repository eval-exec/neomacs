use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PYVENV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'pyvenv)

(defun neomacs-pyvenv-test-root (name)
  "Create a deterministic sandbox directory for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "pyvenv-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-pyvenv-test-environment (root relative &optional executable)
  "Create a virtual environment below ROOT at RELATIVE.
When EXECUTABLE is non-nil, install a deterministic command with that name."
  (let* ((environment (file-name-as-directory (expand-file-name relative root)))
         (bin (expand-file-name "bin" environment)))
    (make-directory bin t)
    (with-temp-file (expand-file-name "activate" bin))
    (when executable
      (let ((program (expand-file-name executable bin)))
        (with-temp-file program
          (insert "#!/bin/sh\nprintf 'virtualenv-command:%s\\n' \"$VIRTUAL_ENV\"\n"))
        (set-file-modes program #o755)))
    environment))

(defun neomacs-pyvenv-test-clean (function)
  "Run FUNCTION with isolated process, path, mode, and Pyvenv state."
  (let ((process-environment (copy-sequence process-environment))
        (exec-path (copy-sequence exec-path))
        (saved-eshell-path-env (default-value 'eshell-path-env))
        (saved-mode-line-misc-info mode-line-misc-info)
        (saved-hack-local-variables-hook hack-local-variables-hook)
        (saved-post-command-hook (default-value 'post-command-hook))
        (pyvenv-virtual-env nil)
        (pyvenv-virtual-env-name nil)
        (pyvenv-virtual-env-path-directories nil)
        (pyvenv-old-process-environment nil)
        (pyvenv-pre-activate-hooks nil)
        (pyvenv-post-activate-hooks nil)
        (pyvenv-pre-deactivate-hooks nil)
        (pyvenv-post-deactivate-hooks nil)
        (python-shell-virtualenv-path nil)
        (python-shell-virtualenv-root nil))
    (unwind-protect
        (cl-letf (((symbol-function 'pyvenv-virtualenvwrapper-supported)
                   (lambda () nil)))
          (funcall function))
      (when pyvenv-tracking-mode
        (pyvenv-tracking-mode -1))
      (when pyvenv-mode
        (pyvenv-mode -1))
      (pyvenv-deactivate)
      (setq mode-line-misc-info saved-mode-line-misc-info
            hack-local-variables-hook saved-hack-local-variables-hook)
      (set-default 'post-command-hook saved-post-command-hook)
      (set-default 'eshell-path-env saved-eshell-path-env))))

(defun neomacs-pyvenv-test-state (environment baseline-path baseline-exec)
  "Capture stable activation state relative to ENVIRONMENT and baselines."
  (let ((bin (directory-file-name (expand-file-name "bin" environment)))
        (path-parts (split-string (or (getenv "PATH") "") path-separator)))
    (list
     :active (equal pyvenv-virtual-env
                    (file-name-as-directory (expand-file-name environment)))
     :name pyvenv-virtual-env-name
     :virtual-env (file-equal-p (getenv "VIRTUAL_ENV") environment)
     :pythonhome (getenv "PYTHONHOME")
     :path-first (equal (car path-parts) bin)
     :exec-first (equal (car exec-path) bin)
     :path-restored (equal (getenv "PATH") baseline-path)
     :exec-restored (equal exec-path baseline-exec)
     :python-shell
     (list (file-equal-p python-shell-virtualenv-path environment)
           (file-equal-p python-shell-virtualenv-root environment)))))
"####;

fn activation_routes_real_commands_and_deactivation_restores_the_process() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-pyvenv-test-clean
 (lambda ()
   (let* ((root (neomacs-pyvenv-test-root "activation"))
          (environment
           (neomacs-pyvenv-test-environment
            root "release/.venv" "release-python"))
          (baseline-path "/base/bin:/shared/bin")
          (baseline-exec '("/base/bin" "/shared/bin"))
          events output)
     (unwind-protect
         (progn
           (setenv "PATH" baseline-path)
           (setenv "PYTHONHOME" "/legacy/python")
           (setq exec-path (copy-sequence baseline-exec)
                 pyvenv-pre-activate-hooks
                 (list (lambda ()
                         (push (list 'pre-activate pyvenv-virtual-env-name) events)))
                 pyvenv-post-activate-hooks
                 (list (lambda ()
                         (push (list 'post-activate pyvenv-virtual-env-name) events)))
                 pyvenv-pre-deactivate-hooks
                 (list (lambda ()
                         (push (list 'pre-deactivate pyvenv-virtual-env-name) events)))
                 pyvenv-post-deactivate-hooks
                 (list (lambda ()
                         (push (list 'post-deactivate pyvenv-virtual-env-name) events))))
           (pyvenv-activate environment)
           (setq output
                 (with-temp-buffer
                   (call-process "release-python" nil t)
                   (replace-regexp-in-string
                    (regexp-quote (directory-file-name environment))
                    "<venv>"
                    (string-trim-right (buffer-string)) t t)))
           (let ((activated
                  (neomacs-pyvenv-test-state
                   environment baseline-path baseline-exec))
                 (found (equal (executable-find "release-python")
                               (expand-file-name "bin/release-python" environment))))
             (pyvenv-deactivate)
             (list :activated activated
                   :command output
                   :found found
                   :deactivated
                   (list :active pyvenv-virtual-env
                         :virtual-env (getenv "VIRTUAL_ENV")
                         :pythonhome (getenv "PYTHONHOME")
                         :path (getenv "PATH")
                         :exec exec-path
                         :python-shell
                         (list python-shell-virtualenv-path
                               python-shell-virtualenv-root)
                         :command-found (executable-find "release-python"))
                   :events (nreverse events))))
       (delete-directory root t)))))
"####;
    let expected = expect![[
        r#"OK (:activated (:active t :name "release" :virtual-env t :pythonhome nil :path-first t :exec-first t :path-restored nil :exec-restored nil :python-shell (t t)) :command "virtualenv-command:<venv>/" :found t :deactivated (:active nil :virtual-env nil :pythonhome "/legacy/python" :path "/base/bin:/shared/bin" :exec ("/base/bin" "/shared/bin") :python-shell (nil nil) :command-found nil) :events ((pre-activate "release") (post-activate "release") (pre-deactivate "release") (post-deactivate "release")))"#
    ]];
    ParityBatchCase::value(
        "activation_routes_real_commands_and_deactivation_restores_the_process",
        elisp_form,
        expected,
    )
}

fn switching_generic_venvs_uses_project_names_without_path_leaks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-pyvenv-test-clean
 (lambda ()
   (let* ((root (neomacs-pyvenv-test-root "switch"))
          (first (neomacs-pyvenv-test-environment root "orders/.venv"))
          (second (neomacs-pyvenv-test-environment root "analytics/venv"))
          (first-bin (directory-file-name (expand-file-name "bin" first)))
          (second-bin (directory-file-name (expand-file-name "bin" second)))
          (baseline-path "/base/bin:/shared/bin")
          (baseline-exec '("/base/bin" "/shared/bin"))
          events)
     (unwind-protect
         (progn
           (setenv "PATH" baseline-path)
           (setq exec-path (copy-sequence baseline-exec)
                 pyvenv-post-activate-hooks
                 (list (lambda () (push (list 'activate pyvenv-virtual-env-name) events)))
                 pyvenv-post-deactivate-hooks
                 (list (lambda () (push (list 'deactivate pyvenv-virtual-env-name) events))))
           (pyvenv-activate first)
           (let ((first-name pyvenv-virtual-env-name))
             (pyvenv-activate second)
             (let ((switched
                    (list :first-name first-name
                          :second-name pyvenv-virtual-env-name
                          :second-path-first
                          (string-prefix-p second-bin (getenv "PATH"))
                          :second-exec-first (equal (car exec-path) second-bin)
                          :first-path-gone
                          (not (member first-bin
                                       (split-string (getenv "PATH") path-separator)))
                          :first-exec-gone (not (member first-bin exec-path)))))
               (pyvenv-deactivate)
               (list :switched switched
                     :restored
                     (list (getenv "PATH") exec-path pyvenv-virtual-env)
                     :events (nreverse events)))))
       (delete-directory root t)))))
"####;
    let expected = expect![[
        r#"OK (:switched (:first-name "orders" :second-name "analytics" :second-path-first t :second-exec-first t :first-path-gone t :first-exec-gone t) :restored ("/base/bin:/shared/bin" ("/base/bin" "/shared/bin") nil) :events ((activate "orders") (deactivate "orders") (activate "analytics") (deactivate "analytics")))"#
    ]];
    ParityBatchCase::value(
        "switching_generic_venvs_uses_project_names_without_path_leaks",
        elisp_form,
        expected,
    )
}

fn workon_discovers_supported_layouts_and_activates_once() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-pyvenv-test-clean
 (lambda ()
   (let* ((root (neomacs-pyvenv-test-root "workon"))
          (workon (expand-file-name "environments" root))
          (process-environment (copy-sequence process-environment))
          events)
     (unwind-protect
         (progn
           (make-directory (expand-file-name "alpha/bin" workon) t)
           (with-temp-file (expand-file-name "alpha/bin/activate" workon))
           (make-directory (expand-file-name "Beta/Scripts" workon) t)
           (with-temp-file (expand-file-name "Beta/Scripts/activate.bat" workon))
           (make-directory (expand-file-name "gamma" workon) t)
           (with-temp-file (expand-file-name "gamma/python.exe" workon))
           (make-directory (expand-file-name "not-an-env" workon) t)
           (setenv "WORKON_HOME" workon)
           (setq pyvenv-post-activate-hooks
                 (list (lambda () (push pyvenv-virtual-env-name events))))
           (let ((available (pyvenv-virtualenv-list)))
             (pyvenv-workon "alpha")
             (pyvenv-workon "alpha")
             (pyvenv-workon "")
             (pyvenv-workon nil)
             (list :available available
                   :active pyvenv-virtual-env-name
                   :active-path
                   (equal pyvenv-virtual-env
                          (file-name-as-directory
                           (expand-file-name "alpha" workon)))
                   :activation-events (nreverse events))))
       (delete-directory root t)))))
"####;
    let expected = expect![[
        r#"OK (:available ("alpha" "Beta" "gamma") :active "alpha" :active-path t :activation-events ("alpha"))"#
    ]];
    ParityBatchCase::value(
        "workon_discovers_supported_layouts_and_activates_once",
        elisp_form,
        expected,
    )
}

fn tracking_mode_switches_with_buffer_local_project_configuration() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-pyvenv-test-clean
 (lambda ()
   (let* ((root (neomacs-pyvenv-test-root "tracking"))
          (api (neomacs-pyvenv-test-environment root "api/.venv"))
          (worker (neomacs-pyvenv-test-environment root "worker/.venv"))
          (api-buffer (generate-new-buffer " *pyvenv-api*"))
          (worker-buffer (generate-new-buffer " *pyvenv-worker*")))
     (unwind-protect
         (progn
           (with-current-buffer api-buffer
             (setq-local pyvenv-activate api))
           (with-current-buffer worker-buffer
             (setq-local pyvenv-activate worker))
           (pyvenv-tracking-mode 1)
           (let ((hook-installed
                  (not (null (memq #'pyvenv-track-virtualenv
                                   (default-value 'post-command-hook))))))
             (with-current-buffer api-buffer
               (run-hooks 'post-command-hook))
             (let ((first pyvenv-virtual-env-name))
               (with-current-buffer worker-buffer
                 (run-hooks 'post-command-hook))
               (let ((second pyvenv-virtual-env-name))
                 (with-current-buffer worker-buffer
                   (run-hooks 'post-command-hook))
                 (pyvenv-tracking-mode -1)
                 (list :hook-installed hook-installed
                       :first first
                       :second second
                       :same-buffer-stable pyvenv-virtual-env-name
                       :mode pyvenv-tracking-mode
                       :hook-removed
                       (not (memq #'pyvenv-track-virtualenv
                                  (default-value 'post-command-hook))))))))
       (kill-buffer api-buffer)
       (kill-buffer worker-buffer)
       (delete-directory root t)))))
"####;
    let expected = expect![[
        r#"OK (:hook-installed t :first "api" :second "worker" :same-buffer-stable "worker" :mode nil :hook-removed t)"#
    ]];
    ParityBatchCase::value(
        "tracking_mode_switches_with_buffer_local_project_configuration",
        elisp_form,
        expected,
    )
}

fn global_mode_line_and_missing_workon_failure_preserve_active_state() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-pyvenv-test-clean
 (lambda ()
   (let* ((root (neomacs-pyvenv-test-root "mode"))
          (environment (neomacs-pyvenv-test-environment root "docs/.venv"))
          (process-environment (copy-sequence process-environment)))
     (unwind-protect
         (progn
           (pyvenv-mode 1)
           (pyvenv-activate environment)
           (let* ((enabled
                   (list :mode pyvenv-mode
                         :entry
                         (not (null
                               (member
                                '(pyvenv-mode pyvenv-mode-line-indicator)
                                mode-line-misc-info)))
                         :hook
                         (not (null (memq #'pyvenv-track-virtualenv
                                         hack-local-variables-hook)))
                         :indicator-spec pyvenv-mode-line-indicator
                         :active-name pyvenv-virtual-env-name))
                  (before (list pyvenv-virtual-env pyvenv-virtual-env-name))
                  (missing (expand-file-name "missing" root))
                  error)
             (setenv "WORKON_HOME" missing)
             (condition-case error-data
                 (pyvenv-virtualenv-list)
               (error
                (setq error (list (car error-data)
                                  (error-message-string error-data)))))
             (let ((preserved
                    (equal before
                           (list pyvenv-virtual-env pyvenv-virtual-env-name))))
               (pyvenv-mode -1)
               (list :enabled enabled
                     :missing-error error
                     :active-preserved preserved
                     :disabled
                     (list :mode pyvenv-mode
                           :entry
                           (member '(pyvenv-mode pyvenv-mode-line-indicator)
                                   mode-line-misc-info)
                           :hook
                           (memq #'pyvenv-track-virtualenv
                                 hack-local-variables-hook))))))
       (delete-directory root t)))))
"####;
    let expected = expect![[
        r#"OK (:enabled (:mode t :entry t :hook t :indicator-spec (pyvenv-virtual-env-name ("[" pyvenv-virtual-env-name "] ")) :active-name "docs") :missing-error (error "Can’t find a workon home directory, set $WORKON_HOME") :active-preserved t :disabled (:mode nil :entry nil :hook nil))"#
    ]];
    ParityBatchCase::value(
        "global_mode_line_and_missing_workon_failure_preserve_active_state",
        elisp_form,
        expected,
    )
}

fn pyvenv_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYVENV_MELPA_PIN, "pyvenv.el")
        .expect("prepare pinned Pyvenv source below ./tmp")
        .with_timeout(Duration::from_secs(180))
        .with_prelude(PRELUDE)
}

#[test]
fn pyvenv_practical_workflows_batch() {
    let cases = vec![
        activation_routes_real_commands_and_deactivation_restores_the_process(),
        switching_generic_venvs_uses_project_names_without_path_leaks(),
        workon_discovers_supported_layouts_and_activates_once(),
        tracking_mode_switches_with_buffer_local_project_configuration(),
        global_mode_line_and_missing_workon_failure_preserve_active_state(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("pyvenv parity batch");
    assert_oracle_batch_cases(pyvenv_oracle(), test_name, "pyvenv parity", &cases);
}
