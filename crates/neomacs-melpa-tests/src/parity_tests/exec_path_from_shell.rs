use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EXEC_PATH_FROM_SHELL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EXEC_PATH_FROM_SHELL_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const EXEC_PATH_FROM_SHELL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'esh-util)
(require 'exec-path-from-shell)

(global-set-key (kbd "C-c e") #'exec-path-from-shell-copy-env)

(defun neomacs-exec-path-test-root (name)
  "Create and return a deterministic workspace-temporary directory for NAME."
  (let ((root (expand-file-name
               (format "exec-path-from-shell-%s-fixture/" name)
               temporary-file-directory)))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-exec-path-test-write-program (root name contents)
  "Write executable CONTENTS below ROOT under NAME and return its path."
  (let ((path (expand-file-name name root)))
    (with-temp-file path
      (insert contents))
    (set-file-modes path #o755)
    path))

(defun neomacs-exec-path-test-shell (root name)
  "Create a deterministic login-shell fixture below ROOT named NAME."
  (neomacs-exec-path-test-write-program
   root name
   "#!/bin/sh\nprintf 'call\\n' >> \"$EPFS_TRACE\"\nexport PATH='/workspace tools/bin:/usr/local/bin:/usr/bin'\nexport MANPATH='/opt/neomacs/man:/usr/share/man'\nexport DEPLOY_TOKEN='release \"blue\" $literal'\nexport MULTILINE='alpha\nbeta'\nexport EMPTY=''\nunset ABSENT\nexec /bin/sh \"$@\"\n"))

(defun neomacs-exec-path-test-trace (trace)
  "Return TRACE as stable non-empty lines."
  (if (file-exists-p trace)
      (with-temp-buffer
        (insert-file-contents trace)
        (split-string (buffer-string) "\n" t))
    nil))

(defun neomacs-exec-path-test-error (thunk root)
  "Run THUNK and return a normalized signal description relative to ROOT."
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal (car error-data)
           :data
           (mapcar
            (lambda (value)
              (if (stringp value)
                  (replace-regexp-in-string
                   (regexp-quote root) "[FIXTURE]/" value t t)
                value))
            (cdr error-data))
           :message
           (replace-regexp-in-string
            (regexp-quote root) "[FIXTURE]/"
            (error-message-string error-data) t t)))))
"##;

fn exec_path_from_shell_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EXEC_PATH_FROM_SHELL_MELPA_PIN, "exec-path-from-shell.el")
        .expect("prepare revision-pinned Exec Path From Shell source below ./tmp")
        .with_prelude(EXEC_PATH_FROM_SHELL_TEST_PRELUDE)
        .with_timeout(EXEC_PATH_FROM_SHELL_TEST_TIMEOUT)
}

fn one_shell_session_imports_complex_empty_and_missing_environment_values() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-exec-path-test-root "read"))
       (trace (expand-file-name "calls.log" root))
       (shell (neomacs-exec-path-test-shell root "login-shell"))
       (process-environment (copy-sequence process-environment)))
  (unwind-protect
      (progn
        (setenv "EPFS_TRACE" trace)
        (let* ((exec-path-from-shell-shell-name shell)
               (exec-path-from-shell-arguments nil)
               (exec-path-from-shell-warn-duration-millis 10000)
               (names '("PATH" "MANPATH" "DEPLOY_TOKEN" "EMPTY" "ABSENT"))
               (values (exec-path-from-shell-getenvs names)))
          (list :values (copy-tree values)
                :path (cdr (assoc "PATH" values))
                :token (cdr (assoc "DEPLOY_TOKEN" values))
                :empty (cdr (assoc "EMPTY" values))
                :absent (copy-tree (assoc "ABSENT" values))
                :calls (neomacs-exec-path-test-trace trace))))
    (when (file-exists-p root)
      (delete-directory root t))))
"##;
    let expected = expect![[
        r####"OK (:values (("ABSENT") ("EMPTY" . "") ("DEPLOY_TOKEN" . "release \"blue\" $literal") ("MANPATH" . "/opt/neomacs/man:/usr/share/man") ("PATH" . "/workspace tools/bin:/usr/local/bin:/usr/bin")) :path "/workspace tools/bin:/usr/local/bin:/usr/bin" :token "release \"blue\" $literal" :empty "" :absent ("ABSENT") :calls ("call"))"####
    ]];
    ParityBatchCase::value(
        "one_shell_session_imports_complex_empty_and_missing_environment_values",
        elisp_form,
        expected,
    )
}

fn initialization_synchronizes_process_path_exec_path_eshell_and_custom_variables()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-exec-path-test-root "initialize"))
       (trace (expand-file-name "calls.log" root))
       (shell (neomacs-exec-path-test-shell root "login-shell"))
       (process-environment (copy-sequence process-environment))
       (exec-path '("/stale/bin/"))
       (old-eshell-path (default-value 'eshell-path-env)))
  (unwind-protect
      (progn
        (setenv "EPFS_TRACE" trace)
        (setenv "PATH" "/stale/bin")
        (setenv "MANPATH" "/stale/man")
        (setenv "DEPLOY_TOKEN" "stale-token")
        (set-default 'eshell-path-env "/stale/bin")
        (let ((exec-path-from-shell-shell-name shell)
              (exec-path-from-shell-arguments nil)
              (exec-path-from-shell-warn-duration-millis 10000)
              (exec-path-from-shell-variables
               '("PATH" "MANPATH" "DEPLOY_TOKEN")))
          (let ((result (exec-path-from-shell-initialize)))
            (list
             :result result
             :environment
             (mapcar (lambda (name) (cons name (getenv name)))
                     exec-path-from-shell-variables)
             :exec-path (butlast exec-path)
             :exec-directory-tail (equal (car (last exec-path)) exec-directory)
             :eshell-path (default-value 'eshell-path-env)
             :calls (neomacs-exec-path-test-trace trace)))))
    (set-default 'eshell-path-env old-eshell-path)
    (when (file-exists-p root)
      (delete-directory root t))))
"##;
    let expected = expect![[
        r####"OK (:result (("DEPLOY_TOKEN" . "release \"blue\" $literal") ("MANPATH" . "/opt/neomacs/man:/usr/share/man") ("PATH" . "/workspace tools/bin:/usr/local/bin:/usr/bin")) :environment (("PATH" . "/workspace tools/bin:/usr/local/bin:/usr/bin") ("MANPATH" . "/opt/neomacs/man:/usr/share/man") ("DEPLOY_TOKEN" . "release \"blue\" $literal")) :exec-path ("/workspace tools/bin/" "/usr/local/bin/" "/usr/bin/") :exec-directory-tail t :eshell-path "/workspace tools/bin:/usr/local/bin:/usr/bin" :calls ("call"))"####
    ]];
    ParityBatchCase::value(
        "initialization_synchronizes_process_path_exec_path_eshell_and_custom_variables",
        elisp_form,
        expected,
    )
}

fn interactive_copy_replaces_a_stale_secret_and_missing_shell_values_remove_old_state()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-exec-path-test-root "interactive"))
       (trace (expand-file-name "calls.log" root))
       (shell (neomacs-exec-path-test-shell root "login-shell"))
       (process-environment (copy-sequence process-environment)))
  (unwind-protect
      (progn
        (setenv "EPFS_TRACE" trace)
        (setenv "DEPLOY_TOKEN" "stale-token")
        (setenv "ABSENT" "stale-value")
        (let ((exec-path-from-shell-shell-name shell)
              (exec-path-from-shell-arguments nil)
              (exec-path-from-shell-warn-duration-millis 10000))
          (with-temp-buffer
            (execute-kbd-macro
             (vconcat (kbd "C-c e") "DEPLOY_TOKEN" (kbd "RET"))))
          (let ((missing (exec-path-from-shell-copy-env "ABSENT")))
            (list :token (getenv "DEPLOY_TOKEN")
                  :missing-return missing
                  :missing-environment (getenv "ABSENT")
                  :calls (neomacs-exec-path-test-trace trace)))))
    (when (file-exists-p root)
      (delete-directory root t))))
"##;
    let expected = expect![[
        r####"OK (:token "release \"blue\" $literal" :missing-return nil :missing-environment nil :calls ("call" "call"))"####
    ]];
    ParityBatchCase::value(
        "interactive_copy_replaces_a_stale_secret_and_missing_shell_values_remove_old_state",
        elisp_form,
        expected,
    )
}

fn non_posix_shell_adapters_run_the_posix_print_protocol_through_real_wrappers() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root (neomacs-exec-path-test-root "shells"))
       (trace (expand-file-name "calls.log" root))
       (process-environment (copy-sequence process-environment))
       values)
  (unwind-protect
      (progn
        (setenv "EPFS_TRACE" trace)
        (dolist (name '("fish" "tcsh" "nu"))
          (neomacs-exec-path-test-write-program
           root name
           "#!/bin/sh\nname=$(basename \"$0\")\nprintf '%s:%s\\n' \"$name\" \"$1\" >> \"$EPFS_TRACE\"\nexport EPFS_FLAVOR=\"$name\"\ncase \"$name:$1\" in\n  fish:-l|tcsh:-d) shift ;;\nesac\nexec /bin/sh \"$@\"\n"))
        (dolist (spec '(("fish" ("-l"))
                        ("tcsh" ("-d"))
                        ("nu" nil)))
          (let ((exec-path-from-shell-shell-name
                 (expand-file-name (car spec) root))
                (exec-path-from-shell-arguments (cadr spec))
                (exec-path-from-shell-warn-duration-millis 10000))
            (push
             (list (car spec)
                   (exec-path-from-shell-printf "%s" '("$EPFS_FLAVOR"))
                   (and (exec-path-from-shell--standard-shell-p
                         exec-path-from-shell-shell-name)
                        t)
                   (and (exec-path-from-shell--nushell-p
                         exec-path-from-shell-shell-name)
                        t))
             values)))
        (list :shells (nreverse values)
              :trace (neomacs-exec-path-test-trace trace)))
    (when (file-exists-p root)
      (delete-directory root t))))
"##;
    let expected = expect![[
        r####"OK (:shells (("fish" "fish" nil nil) ("tcsh" "tcsh" nil nil) ("nu" "nu" nil t)) :trace ("fish:-l" "tcsh:-d" "nu:-c"))"####
    ]];
    ParityBatchCase::value(
        "non_posix_shell_adapters_run_the_posix_print_protocol_through_real_wrappers",
        elisp_form,
        expected,
    )
}

fn broken_shells_and_remote_buffers_report_actionable_failures_without_mutating_state()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-exec-path-test-root "failures"))
       (failed (neomacs-exec-path-test-write-program
                root "failed-shell"
                "#!/bin/sh\necho 'startup exploded'\nexit 17\n"))
       (silent (neomacs-exec-path-test-write-program
                root "silent-shell"
                "#!/bin/sh\necho 'startup omitted protocol markers'\nexit 0\n"))
       (_printf (neomacs-exec-path-test-write-program
                 root "printf"
                 "#!/bin/sh\nprintf \"$@\"\n"))
       (process-environment (copy-sequence process-environment)))
  (unwind-protect
      (let ((exec-path (cons root exec-path)))
        (list
       :nonzero
       (let ((exec-path-from-shell-shell-name failed)
             (exec-path-from-shell-arguments nil)
             (exec-path-from-shell-warn-duration-millis 10000))
         (neomacs-exec-path-test-error
          (lambda () (exec-path-from-shell-printf "%s" '("$PATH"))) root))
       :missing-markers
       (let ((exec-path-from-shell-shell-name silent)
             (exec-path-from-shell-arguments nil)
             (exec-path-from-shell-warn-duration-millis 10000))
         (neomacs-exec-path-test-error
          (lambda () (exec-path-from-shell-printf "%s" '("$PATH"))) root))
       :remote
       (let ((default-directory "/ssh:fixture-host:/srv/service/")
             (exec-path-from-shell-shell-name "/bin/sh")
             (exec-path-from-shell-arguments nil))
         (neomacs-exec-path-test-error
          (lambda () (exec-path-from-shell-getenvs '("PATH"))) root))))
    (when (file-exists-p root)
      (delete-directory root t))))
"##;
    let expected = expect![[
        r####"OK (:nonzero (:signal error :data ("Non-zero exit code from shell [FIXTURE]/failed-shell invoked with args (\"-c\" \"[FIXTURE]/printf '__RESULT\\\\000%s\\\\000__RESULT' \\\"$PATH\\\"\").  Output was:\n\"startup exploded\\n\"") :message "Non-zero exit code from shell [FIXTURE]/failed-shell invoked with args (\"-c\" \"[FIXTURE]/printf '__RESULT\\\\000%s\\\\000__RESULT' \\\"$PATH\\\"\").  Output was:\n\"startup exploded\\n\"") :missing-markers (:signal error :data ("Expected printf output from shell, but got: \"startup omitted protocol markers\\n\"") :message "Expected printf output from shell, but got: \"startup omitted protocol markers\\n\"") :remote (:signal error :data ("You cannot run exec-path-from-shell from a remote buffer (Tramp, etc.)") :message "You cannot run exec-path-from-shell from a remote buffer (Tramp, etc.)"))"####
    ]];
    ParityBatchCase::value(
        "broken_shells_and_remote_buffers_report_actionable_failures_without_mutating_state",
        elisp_form,
        expected,
    )
}

#[test]
fn exec_path_from_shell_package_batch() {
    assert_oracle_batch_cases(
        exec_path_from_shell_oracle(),
        "exec-path-from-shell-package-batch",
        "Exec Path From Shell",
        &[
            one_shell_session_imports_complex_empty_and_missing_environment_values(),
            initialization_synchronizes_process_path_exec_path_eshell_and_custom_variables(),
            interactive_copy_replaces_a_stale_secret_and_missing_shell_values_remove_old_state(),
            non_posix_shell_adapters_run_the_posix_print_protocol_through_real_wrappers(),
            broken_shells_and_remote_buffers_report_actionable_failures_without_mutating_state(),
        ],
    );
}
