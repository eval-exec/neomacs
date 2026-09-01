use expect_test::expect;

use super::ParityBatchCase;

fn versions_list_includes_system_and_installed_roots() -> ParityBatchCase {
    ParityBatchCase::value(
        "versions_list_includes_system_and_installed_roots",
        r####"
(neomacs-pyenv-mode-test-with-fixture
 (lambda (_activations)
   (list :versions (pyenv-mode-versions)
         :root (file-name-nondirectory
                (directory-file-name (pyenv-mode-root)))
         :full-3.12 (file-relative-name
                     (pyenv-mode-full-path "3.12.1")
                     neomacs-pyenv-mode-test-root)
         :system-full (pyenv-mode-full-path "system"))))
"####,
        expect![[
            r#"OK (:versions ("system" "3.11.0" "3.12.1") :root "pyenv-mode" :full-3.12 "versions/3.12.1" :system-full nil)"#
        ]],
    )
}

fn set_and_unset_update_env_and_pythonic_activation() -> ParityBatchCase {
    ParityBatchCase::value(
        "set_and_unset_update_env_and_pythonic_activation",
        r####"
(neomacs-pyenv-mode-test-with-fixture
 (lambda (activations-fn)
   (pyenv-mode-set "3.11.0")
   (let ((after-set
          (list :version (pyenv-mode-version)
                :env (getenv "PYENV_VERSION"))))
     (pyenv-mode-unset)
     (list :after-set after-set
           :after-unset
           (list :version (pyenv-mode-version)
                 :env (getenv "PYENV_VERSION"))
           :activations (funcall activations-fn)))))
"####,
        expect![[
            r#"OK (:after-set (:version "3.11.0" :env "3.11.0") :after-unset (:version nil :env nil) :activations ((:activate "[ORACLE-SANDBOX]/pyenv-mode//versions/3.11.0") (:deactivate)))"#
        ]],
    )
}

fn mode_enables_keymap_and_mode_line_when_pyenv_exists() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_enables_keymap_and_mode_line_when_pyenv_exists",
        r####"
(neomacs-pyenv-mode-test-with-fixture
 (lambda (_activations)
   (pyenv-mode 1)
   (let ((on
          (list :mode (and pyenv-mode t)
                :set-key (lookup-key pyenv-mode-map (kbd "C-c C-s"))
                :unset-key (lookup-key pyenv-mode-map (kbd "C-c C-u"))
                :mode-line (and (member pyenv-mode-mode-line-format
                                        mode-line-misc-info)
                                t))))
     (pyenv-mode -1)
     (list :on on
           :off (and pyenv-mode t)
           :mode-line-off (and (member pyenv-mode-mode-line-format
                                       mode-line-misc-info)
                               t)))))
"####,
        expect![
            "OK (:on (:mode t :set-key pyenv-mode-set :unset-key pyenv-mode-unset :mode-line t) :off nil :mode-line-off nil)"
        ],
    )
}

fn mode_errors_when_pyenv_executable_is_missing() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_errors_when_pyenv_executable_is_missing",
        r####"
(let ((pyenv-mode nil))
  (cl-letf (((symbol-function 'executable-find)
             (lambda (_command) nil)))
    (condition-case err
        (list :value (pyenv-mode 1))
      (error (list :signal (car err)
                   :message (error-message-string err)
                   :mode (and pyenv-mode t))))))
"####,
        expect![[
            r#"OK (:signal error :message "pyenv-mode: pyenv executable not found." :mode t)"#
        ]],
    )
}

fn interactive_set_uses_completing_read_over_versions() -> ParityBatchCase {
    ParityBatchCase::value(
        "interactive_set_uses_completing_read_over_versions",
        r####"
(neomacs-pyenv-mode-test-with-fixture
 (lambda (activations-fn)
   (let (prompt choices)
     (cl-letf (((symbol-function 'completing-read)
                (lambda (p c &rest _)
                  (setq prompt p choices (copy-sequence c))
                  "3.12.1")))
       (call-interactively #'pyenv-mode-set)
       (list :prompt prompt
             :choices choices
             :version (pyenv-mode-version)
             :activations (funcall activations-fn))))))
"####,
        expect![[
            r#"OK (:prompt "Pyenv: " :choices ("system" "3.11.0" "3.12.1") :version "3.12.1" :activations ((:activate "[ORACLE-SANDBOX]/pyenv-mode//versions/3.12.1")))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        versions_list_includes_system_and_installed_roots(),
        set_and_unset_update_env_and_pythonic_activation(),
        mode_enables_keymap_and_mode_line_when_pyenv_exists(),
        mode_errors_when_pyenv_executable_is_missing(),
        interactive_set_uses_completing_read_over_versions(),
    ]
}
