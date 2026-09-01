use expect_test::expect;

use super::ParityBatchCase;

fn aidermacs_fake_cli_version_boundary_is_cached_per_workspace_and_clearable() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_fake_cli_version_boundary_is_cached_per_workspace_and_clearable",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "project" sandbox)))
                          (bin (expand-file-name "bin/aider-fixture" sandbox))
                          (count-file (expand-file-name "calls" sandbox)))
                      (make-directory (expand-file-name ".git" root) t)
                      (make-directory (file-name-directory bin) t)
                      (with-temp-file bin
                        (insert
                         "#!/bin/sh\n"
                         "printf x >> \"$(dirname \"$0\")/../calls\"\n"
                         "printf 'aider 1.2.3-dev\\n'\n"))
                      (set-file-modes bin #o755)
                      (let ((default-directory root)
                            (aidermacs-program bin)
                            (aidermacs--resolved-programs
                             (make-hash-table :test 'equal))
                            (aidermacs--cached-versions
                             (make-hash-table :test 'equal)))
                        (list
                         (aidermacs-aider-version)
                         (aidermacs-aider-version)
                         (with-temp-buffer
                           (when (file-exists-p count-file)
                             (insert-file-contents count-file))
                           (buffer-string))
                         (progn
                           (aidermacs-clear-aider-version-cache)
                           (aidermacs-aider-version))
                         (with-temp-buffer
                           (insert-file-contents count-file)
                           (buffer-string)))))"##,
        expect![[r#"OK ("1.2.3" "1.2.3" "x" "1.2.3" "xx")"#]],
    )
}

fn aidermacs_program_resolution_uses_ordered_fallbacks_and_workspace_cache() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_program_resolution_uses_ordered_fallbacks_and_workspace_cache",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (bin-dir (file-name-as-directory
                                    (expand-file-name "bin" sandbox)))
                          (fallback (expand-file-name "aider-fallback" bin-dir))
                          (default-directory root)
                          (exec-path (cons bin-dir exec-path))
                          (aidermacs-program
                           '("missing-aider" "aider-fallback"))
                          (aidermacs--resolved-programs
                           (make-hash-table :test 'equal)))
                      (make-directory (expand-file-name ".git" root) t)
                      (make-directory bin-dir t)
                      (with-temp-file fallback
                        (insert "#!/bin/sh\nexit 0\n"))
                      (set-file-modes fallback #o755)
                      (let ((first (aidermacs-get-program)))
                        (delete-file fallback)
                        (list
                         (file-name-nondirectory first)
                         (equal first (aidermacs-get-program))
                         (aidermacs--get-cache-key)
                         (hash-table-count
                          aidermacs--resolved-programs))))"##,
        expect![[r#"OK ("aider-fallback" t "local::[ORACLE-SANDBOX]/repo/" 1)"#]],
    )
}

fn aidermacs_run_builds_code_architect_config_and_subtree_cli_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_run_builds_code_architect_config_and_subtree_cli_contracts",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (bin (expand-file-name "bin/aider-ce" sandbox))
                          (global-read (expand-file-name "rules.md" sandbox))
                          (project-read "docs/guide.md")
                          (default-directory root)
                          (aidermacs-program bin)
                          (aidermacs--resolved-programs
                           (make-hash-table :test 'equal))
                          (aidermacs--cached-versions
                           (make-hash-table :test 'equal))
                          calls)
                      (make-directory (expand-file-name ".git" root) t)
                      (make-directory (expand-file-name "docs" root) t)
                      (make-directory (file-name-directory bin) t)
                      (with-temp-file global-read (insert "rules\n"))
                      (with-temp-file
                          (expand-file-name project-read root)
                        (insert "guide\n"))
                      (with-temp-file bin
                        (insert
                         "#!/bin/sh\n"
                         "printf 'aider 0.80.1\\n'\n"))
                      (set-file-modes bin #o755)
                      (cl-letf
                          (((symbol-function 'aidermacs-run-backend)
                            (lambda (program args buffer-name)
                              (push
                               (list
                                (file-name-nondirectory program)
                                args buffer-name)
                               calls)
                              (get-buffer-create buffer-name)))
                           ((symbol-function 'aidermacs-switch-to-buffer)
                            (lambda (&rest _) nil))
                           ((symbol-function
                             'aidermacs--setup-ediff-cleanup-hooks)
                            #'ignore)
                           ((symbol-function 'aidermacs--setup-cleanup-hooks)
                            #'ignore)
                           ((symbol-function 'aidermacs-setup-minor-mode)
                            #'ignore))
                        (let ((aidermacs-default-chat-mode nil)
                              (aidermacs-default-model "code-model")
                              (aidermacs-auto-commits nil)
                              (aidermacs-watch-files t)
                              (aidermacs-weak-model "weak-model")
                              (aidermacs-global-read-only-files
                               (list global-read))
                              (aidermacs-project-read-only-files
                               (list project-read))
                              (aidermacs-extra-args
                               '("--verbose" "--thinking-tokens 8k")))
                          (aidermacs-run))
                        (mapc #'kill-buffer
                              (match-buffers
                               (lambda (buffer)
                                 (string-prefix-p
                                  "*aidermacs:"
                                  (buffer-name buffer)))))
                        (let ((aidermacs-default-chat-mode 'architect)
                              (aidermacs-default-model "default")
                              (aidermacs-architect-model "reasoner")
                              (aidermacs-editor-model "editor")
                              (aidermacs-auto-accept-architect t)
                              (aidermacs-auto-commits t)
                              (aidermacs-subtree-only t)
                              (aidermacs-extra-args nil))
                          (aidermacs-run))
                        (mapc #'kill-buffer
                              (match-buffers
                               (lambda (buffer)
                                 (string-prefix-p
                                  "*aidermacs:"
                                  (buffer-name buffer)))))
                        (let ((config
                               (expand-file-name "aider.yml" sandbox))
                              (aidermacs-default-chat-mode 'ask)
                              (aidermacs-config-file
                               (expand-file-name "aider.yml" sandbox))
                              (aidermacs-subtree-only t)
                              (aidermacs-extra-args '("--debug")))
                          (with-temp-file config (insert "model: x\n"))
                          (aidermacs-run))
                        (prog1 (nreverse calls)
                          (mapc #'kill-buffer
                                (match-buffers
                                 (lambda (buffer)
                                   (string-prefix-p
                                    "*aidermacs:"
                                    (buffer-name buffer))))))))"##,
        expect![[
            r#"OK (("aider-ce" ("--model" "code-model" "--no-auto-commits" "--no-auto-accept-architect" "--watch-files" "--weak-model" "weak-model" "--linear-output" "--read" "[ORACLE-SANDBOX]/rules.md" "--read" "[ORACLE-SANDBOX]/repo/docs/guide.md" "--verbose" "--thinking-tokens 8k") "*aidermacs:[ORACLE-SANDBOX]/repo/*") ("aider-ce" ("--chat-mode" "architect" "--model" "reasoner" "--editor-model" "editor" "--linear-output" "--subtree-only") "*aidermacs:[ORACLE-SANDBOX]/repo/*") ("aider-ce" ("--config" "[ORACLE-SANDBOX]/aider.yml" "--subtree-only" "--debug") "*aidermacs:[ORACLE-SANDBOX]/repo/*"))"#
        ]],
    )
}

fn aidermacs_system_notification_dispatches_linux_windows_and_fallback_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_system_notification_dispatches_linux_windows_and_fallback_boundaries",
        r##"(let ((aidermacs-enable-notifications t)
                          (original-featurep
                           (symbol-function 'featurep))
                          calls)
                      (cl-letf
                          (((symbol-function 'call-process)
                            (lambda (program &rest arguments)
                              (push (cons program arguments) calls)
                              0))
                           ((symbol-function 'featurep)
                            (lambda (feature)
                              (and (not (eq feature 'notifications))
                                   (funcall original-featurep feature)))))
                        (let ((system-type 'gnu/linux))
                          (aidermacs--send-notification
                           "Build" "Tests finished"))
                        (let ((system-type 'windows-nt))
                          (aidermacs--send-notification
                           "Review" "Needs attention"))
                        (let ((system-type 'berkeley-unix))
                          (aidermacs--send-notification
                           "Ignored" "No backend"))
                        (nreverse calls)))"##,
        expect![[
            r#"OK (("notify-send" nil nil nil "Build" "Tests finished" "-t" "0") ("powershell" nil nil nil "-Command" "[System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms'); [System.Windows.Forms.MessageBox]::Show('Needs attention', 'Review', [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Information)"))"#
        ]],
    )
}

pub(super) fn session_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidermacs_fake_cli_version_boundary_is_cached_per_workspace_and_clearable(),
        aidermacs_program_resolution_uses_ordered_fallbacks_and_workspace_cache(),
        aidermacs_run_builds_code_architect_config_and_subtree_cli_contracts(),
        aidermacs_system_notification_dispatches_linux_windows_and_fallback_boundaries(),
    ]
}
