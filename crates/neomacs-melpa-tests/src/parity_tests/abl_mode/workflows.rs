use expect_test::expect;

use super::ParityBatchCase;

/// A developer opens a test module of a git-controlled Python project and turns
/// abl-mode on.  The mode has to find the project base from `pyproject.toml',
/// read the current git branch, derive the per-branch shell buffer name and the
/// virtualenv name, keep all of that buffer local, and install its key map; a
/// file that belongs to no Python project must be refused with the documented
/// message and leave the mode off.
fn enabling_abl_mode_derives_project_shell_and_virtualenv_names_from_git() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_abl_mode_derives_project_shell_and_virtualenv_names_from_git",
        r##"(let* ((base (abl-test-project))
       (file (expand-file-name "tests/ünïcode_tests.py" base))
       (observed nil))
  (find-file file)
  (abl-mode 1)
  (push (list :enabled abl-mode
              :lighter (assq 'abl-mode minor-mode-alist)
              :base (abl-test-relative abl-package-base)
              :branch abl-mode-branch
              :project abl-mode-project-name
              :shell abl-mode-shell-name
              :virtualenv abl-ve-name
              :buffer-local (list (local-variable-p 'abl-package-base)
                                  (local-variable-p 'abl-mode-branch)
                                  (local-variable-p 'abl-ve-name))
              :keys (list (key-binding (kbd "C-c t"))
                          (key-binding (kbd "C-c u"))
                          (key-binding (kbd "C-c f"))))
        observed)
  (abl-mode -1)
  (push (list :disabled abl-mode :keys (key-binding (kbd "C-c t"))) observed)
  (find-file (abl-test-loose-file))
  (let ((mark (abl-test-message-mark)))
    (abl-mode 1)
    (push (list :outside abl-mode
                :base abl-package-base
                :messages (abl-test-messages-since mark))
          observed))
  (nreverse observed))"##,
        expect![[
            r#"OK ((:enabled t :lighter (abl-mode " abl-mode") :base "ünïcode-projekt/" :branch "feature/ünïcode-tests" :project "ünïcode-projekt" :shell "ABL-SHELL:ünïcode-projekt_feature/ünïcode-tests" :virtualenv "ünïcode-projekt_feature-ünïcode-tests" :buffer-local (t t t) :keys (abl-mode-run-test-at-point abl-mode-rerun-last-test abl-mode-format-file)) (:disabled nil :keys nil) (:outside nil :base "" :messages ("Could not find project base. Please make sure there is a setup.py or requirements.txt in a higher directory.")))"#
        ]],
    )
}

fn running_the_test_at_point_sends_one_unittest_command_to_the_project_shell() -> ParityBatchCase {
    ParityBatchCase::value(
        "running_the_test_at_point_sends_one_unittest_command_to_the_project_shell",
        r##"(let* ((base (abl-test-project))
       (file (expand-file-name "tests/ünïcode_tests.py" base)))
  (abl-test-setup "python")
  (find-file file)
  (abl-mode 1)
  (setq abl-mode-check-and-activate-ve nil)
  (goto-char (point-min))
  (search-forward "self.assertEqual")
  (let ((shell abl-mode-shell-name)
        (code (current-buffer))
        (mark (abl-test-message-mark)))
    (execute-kbd-macro (kbd "C-c t"))
    (list :ready (abl-test-wait-for-shell shell 1)
          :sent (abl-test-shell-inputs)
          :argv (abl-test-commands)
          :directories (abl-test-directories)
          :shell-text (abl-test-shell-text shell)
          :shell-mode (with-current-buffer shell major-mode)
          :messages (abl-test-messages-since mark)
          :code-point (with-current-buffer code
                        (list (line-number-at-pos) (current-column)))
          :current (buffer-name)
          :windows (length (window-list)))))"##,
        expect![[
            r#"OK (:ready 1 :sent ("cd [ORACLE-SANDBOX]/ünïcode-projekt/ && python -m unittest tests/ünïcode_tests.py::ÜnicodeTests::test_encodes_a_name") :argv ("python|-m|unittest|tests/ünïcode_tests.py::ÜnicodeTests::test_encodes_a_name") :directories ("ünïcode-projekt") :shell-text "cd [ORACLE-SANDBOX]/ünïcode-projekt/ && python -m unittest tests/ünïcode_tests.py::ÜnicodeTests::test_encodes_a_name\nabl-ready\n" :shell-mode shell-mode :messages ("Running test(s) tests/ünïcode_tests.py::ÜnicodeTests::test_encodes_a_name on ABL-SHELL:ünïcode-projekt_feature/ünïcode-tests" "[ORACLE-SANDBOX]/ünïcode-projekt ") :code-point (7 24) :current "ABL-SHELL:ünïcode-projekt_feature/ünïcode-tests" :windows 1)"#
        ]],
    )
    .fresh_process()
}

fn rerunning_the_last_test_repeats_the_class_entity_regardless_of_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "rerunning_the_last_test_repeats_the_class_entity_regardless_of_point",
        r##"(let* ((base (abl-test-project))
       (file (expand-file-name "tests/ünïcode_tests.py" base))
       (observed nil))
  (abl-test-setup "python")
  (find-file file)
  (abl-mode 1)
  (setq abl-mode-check-and-activate-ve nil)
  (let ((shell abl-mode-shell-name)
        (code (current-buffer))
        (mark (abl-test-message-mark)))
    (execute-kbd-macro (kbd "C-c u"))
    (push (list :nothing-run-yet (abl-test-messages-since mark)
                :argv (abl-test-commands)
                :shell-buffer (and (get-buffer shell) t))
          observed)
    (goto-char (point-min))
    (search-forward "class ÜnicodeTests")
    (execute-kbd-macro (kbd "C-c t"))
    (abl-test-wait-for-shell shell 1)
    (switch-to-buffer code)
    (goto-char (point-min))
    (setq mark (abl-test-message-mark))
    (push (list :entity-at-point (abl-mode-get-test-entity)) observed)
    (execute-kbd-macro (kbd "C-c u"))
    (push (list :ready (abl-test-wait-for-shell shell 2)
                :sent (abl-test-shell-inputs)
                :argv (abl-test-commands)
                :messages (abl-test-messages-since mark))
          observed))
  (nreverse observed))"##,
        expect![[
            r#"OK ((:nothing-run-yet ("You haven’t run any tests yet.") :argv nothing-recorded :shell-buffer nil) (:entity-at-point "tests/ünïcode_tests.py") (:ready 2 :sent ("cd [ORACLE-SANDBOX]/ünïcode-projekt/ && python -m unittest tests/ünïcode_tests.py::ÜnicodeTests" "cd [ORACLE-SANDBOX]/ünïcode-projekt/ && python -m unittest tests/ünïcode_tests.py::ÜnicodeTests") :argv ("python|-m|unittest|tests/ünïcode_tests.py::ÜnicodeTests" "python|-m|unittest|tests/ünïcode_tests.py::ÜnicodeTests") :messages ("Running test(s) tests/ünïcode_tests.py::ÜnicodeTests on ABL-SHELL:ünïcode-projekt_feature/ünïcode-tests" "[ORACLE-SANDBOX]/ünïcode-projekt ")))"#
        ]],
    )
    .fresh_process()
}

fn a_project_abl_file_switches_the_runner_to_pytest_with_module_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_project_abl_file_switches_the_runner_to_pytest_with_module_names",
        r##"(let* ((base (abl-test-project
              (concat "abl-mode-test-command \"pytest -q %s\"\n"
                      "abl-mode-check-and-activate-ve nil\n"
                      "abl-use-test-file-path nil\n")))
       (file (expand-file-name "tests/api layer/service_tests.py" base)))
  (abl-test-setup "pytest")
  (find-file file)
  (abl-mode 1)
  (goto-char (point-min))
  (search-forward "assert True")
  (let ((shell abl-mode-shell-name)
        (code (current-buffer))
        (mark (abl-test-message-mark)))
    (execute-kbd-macro (kbd "C-c t"))
    (list :ready (abl-test-wait-for-shell shell 1)
          :options (with-current-buffer code
                     (list abl-mode-test-command
                           abl-mode-check-and-activate-ve
                           abl-use-test-file-path
                           (local-variable-p 'abl-mode-test-command)
                           (local-variable-p 'abl-use-test-file-path)))
          :sent (abl-test-shell-inputs)
          :argv (abl-test-commands)
          :directories (abl-test-directories)
          :messages (abl-test-messages-since mark))))"##,
        expect![[
            r#"OK (:ready 1 :options ("pytest -q %s" nil nil t nil) :sent ("cd [ORACLE-SANDBOX]/ünïcode-projekt/ && pytest -q tests.api layer.service_tests::test_service_root") :argv ("pytest|-q|tests.api|layer.service_tests::test_service_root") :directories ("ünïcode-projekt") :messages ("Running test(s) tests.api layer.service_tests::test_service_root on ABL-SHELL:ünïcode-projekt_feature/ünïcode-tests" "[ORACLE-SANDBOX]/ünïcode-projekt "))"#
        ]],
    )
    .fresh_process()
}

fn an_existing_virtualenv_is_activated_before_the_test_command_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_existing_virtualenv_is_activated_before_the_test_command_runs",
        r##"(let* ((base (abl-test-project))
       (file (expand-file-name "tests/ünïcode_tests.py" base))
       (virtualenv (abl-test-virtualenv "ünïcode-projekt_feature-ünïcode-tests")))
  (abl-test-setup "python" "workon")
  (find-file file)
  (abl-mode 1)
  (goto-char (point-min))
  (search-forward "def test_rejects_empty_input")
  (let ((shell abl-mode-shell-name)
        (code (current-buffer)))
    (execute-kbd-macro (kbd "C-c t"))
    (list :ready (abl-test-wait-for-shell shell 1)
          :virtualenv (abl-test-relative virtualenv)
          :name (with-current-buffer code abl-ve-name)
          :activate (with-current-buffer code abl-mode-ve-activate-command)
          :sent (abl-test-shell-inputs)
          :argv (abl-test-commands)
          :directories (abl-test-directories))))"##,
        expect![[
            r#"OK (:ready 1 :virtualenv "home/.virtualenvs/ünïcode-projekt_feature-ünïcode-tests" :name "ünïcode-projekt_feature-ünïcode-tests" :activate "workon %s" :sent ("cd [ORACLE-SANDBOX]/ünïcode-projekt/ && workon ünïcode-projekt_feature-ünïcode-tests && python -m unittest tests/ünïcode_tests.py::ÜnicodeTests::test_rejects_empty_input") :argv ("workon|ünïcode-projekt_feature-ünïcode-tests" "python|-m|unittest|tests/ünïcode_tests.py::ÜnicodeTests::test_rejects_empty_input") :directories ("ünïcode-projekt" "ünïcode-projekt"))"#
        ]],
    )
    .fresh_process()
}

fn formatting_the_current_file_and_then_the_whole_project_reuses_one_shell() -> ParityBatchCase {
    ParityBatchCase::value(
        "formatting_the_current_file_and_then_the_whole_project_reuses_one_shell",
        r##"(let* ((base (abl-test-project))
       (file (expand-file-name "tests/api layer/service_tests.py" base)))
  (abl-test-setup "black" "isort")
  (find-file file)
  (abl-mode 1)
  (setq abl-mode-check-and-activate-ve nil)
  (let ((shell abl-mode-shell-name)
        (code (current-buffer)))
    (execute-kbd-macro (kbd "C-c f"))
    (abl-test-wait-for-shell shell 1)
    (switch-to-buffer code)
    (execute-kbd-macro (kbd "C-u C-c f"))
    (list :ready (abl-test-wait-for-shell shell 2)
          :sent (abl-test-shell-inputs)
          :argv (abl-test-commands)
          :directories (abl-test-directories)
          :modified (with-current-buffer code (buffer-modified-p)))))"##,
        expect![[
            r#"OK (:ready 2 :sent ("cd [ORACLE-SANDBOX]/ünïcode-projekt/ && black [ORACLE-SANDBOX]/ünïcode-projekt/tests/api layer/service_tests.py && isort --profile black [ORACLE-SANDBOX]/ünïcode-projekt/tests/api layer/service_tests.py" "cd [ORACLE-SANDBOX]/ünïcode-projekt/ && black . && isort --profile black .") :argv ("black|[ORACLE-SANDBOX]/ünïcode-projekt/tests/api|layer/service_tests.py" "isort|--profile|black|[ORACLE-SANDBOX]/ünïcode-projekt/tests/api|layer/service_tests.py" "black|." "isort|--profile|black|.") :directories ("ünïcode-projekt" "ünïcode-projekt" "ünïcode-projekt" "ünïcode-projekt") :modified nil)"#
        ]],
    )
    .fresh_process()
}

fn running_a_test_outside_any_test_entity_signals_and_starts_no_shell() -> ParityBatchCase {
    ParityBatchCase::value(
        "running_a_test_outside_any_test_entity_signals_and_starts_no_shell",
        r##"(let* ((base (abl-test-project))
       (file (expand-file-name "conftest.py" base)))
  (abl-test-setup "python")
  (find-file file)
  (abl-mode 1)
  (setq abl-mode-check-and-activate-ve nil)
  (goto-char (point-min))
  (search-forward "SETTINGS")
  (let ((shell abl-mode-shell-name)
        (mark (abl-test-message-mark)))
    (list :signal (condition-case failure
                      (execute-kbd-macro (kbd "C-c t"))
                    (error failure))
          :point (list (line-number-at-pos) (current-column))
          :current (buffer-name)
          :argv (abl-test-commands)
          :shell-buffer (and (get-buffer shell) t)
          :messages (abl-test-messages-since mark))))"##,
        expect![[
            r#"OK (:signal (error "You do not appear to be in a recognized test entity") :point (4 8) :current "conftest.py" :argv nothing-recorded :shell-buffer nil :messages nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_abl_mode_derives_project_shell_and_virtualenv_names_from_git(),
        running_the_test_at_point_sends_one_unittest_command_to_the_project_shell(),
        rerunning_the_last_test_repeats_the_class_entity_regardless_of_point(),
        a_project_abl_file_switches_the_runner_to_pytest_with_module_names(),
        an_existing_virtualenv_is_activated_before_the_test_command_runs(),
        formatting_the_current_file_and_then_the_whole_project_reuses_one_shell(),
        running_a_test_outside_any_test_entity_signals_and_starts_no_shell(),
    ]
}
