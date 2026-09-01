use expect_test::expect;

use super::ParityBatchCase;

fn command_format_composes_working_directory_runner_and_tests() -> ParityBatchCase {
    ParityBatchCase::value(
        "command_format_composes_working_directory_runner_and_tests",
        r####"
(list :default-flags pytest-cmd-flags
      :default-runner pytest-global-name
      :formatted
      (pytest-cmd-format
       pytest-cmd-format-string
       "/tmp/proj"
       "pytest"
       "-x -s"
       "test_demo.py")
      :custom-template
      (pytest-cmd-format
       "%s :: %s :: %s :: %s"
       "/work"
       "python -m pytest"
       "-k smoke"
       "tests/"))
"####,
        expect![[
            r#"OK (:default-flags "-x -s" :default-runner "pytest" :formatted "cd '/tmp/proj' && pytest -x -s 'test_demo.py'" :custom-template "/work :: python -m pytest :: -k smoke :: tests/")"#
        ]],
    )
}

fn project_root_walks_up_to_marker_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_root_walks_up_to_marker_files",
        r####"
(neomacs-pytest-test-with-project
 (lambda (root)
   (let* ((nested (expand-file-name "pkg/nested" root))
          (file (expand-file-name "test_demo.py" root)))
     (make-directory nested t)
     (list :from-file
           (equal (directory-file-name
                   (file-truename
                    (pytest-find-project-root (file-name-directory file))))
                  (directory-file-name (file-truename root)))
           :from-nested
           (equal (directory-file-name
                   (file-truename (pytest-find-project-root nested)))
                  (directory-file-name (file-truename root)))
           :is-root (and (pytest-project-root root) t)
           :nested-not-root (not (pytest-project-root nested))))))
"####,
        expect!["OK (:from-file t :from-nested t :is-root t :nested-not-root t)"],
    )
}

fn py_testable_builds_class_and_function_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "py_testable_builds_class_and_function_paths",
        r####"
(neomacs-pytest-test-with-project
 (lambda (root)
   (let ((file (expand-file-name "test_demo.py" root)))
     (with-current-buffer (find-file-noselect file)
       (python-mode)
       (goto-char (point-min))
       (search-forward "assert 1 + 1")
       (let ((method (pytest-py-testable))
             (inner (pytest-inner-testable))
             (outer (pytest-outer-testable)))
         (goto-char (point-min))
         (search-forward "assert True")
         (let ((top (pytest-py-testable)))
           (list :inner inner
                 :outer outer
                 :method-ends-with
                 (and (string-suffix-p "::TestMath::test_add" method) t)
                 :top-ends-with
                 (and (string-suffix-p "::test_top_level" top) t)
                 :file-prefix
                 (and (string-prefix-p file method) t)
                 :method-has-file
                 (and (string-match-p "test_demo\\.py::" method) t))))))))
"####,
        expect![[
            r#"OK (:inner "test_add" :outer ("class" . "TestMath") :method-ends-with t :top-ends-with t :file-prefix t :method-has-file t)"#
        ]],
    )
}

fn get_command_composes_cd_and_quoted_test_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "get_command_composes_cd_and_quoted_test_names",
        r####"
(neomacs-pytest-test-with-project
 (lambda (root)
   (let* ((file (expand-file-name "test_demo.py" root))
          (default-directory root)
          (pytest-global-name "pytest")
          (where (or (pytest-find-project-root (file-name-directory file))
                     root))
          (cmd (pytest-cmd-format
                pytest-cmd-format-string where "pytest" "-q"
                (format "'%s'" (file-name-nondirectory file))))
          (cmd-all (pytest-cmd-format
                    pytest-cmd-format-string where "pytest" "-x" "'.'")))
     (list :where-ok
           (equal (directory-file-name (file-truename where))
                  (directory-file-name (file-truename root)))
           :has-cd (and (string-match-p "\\`cd '" cmd) t)
           :has-pytest (and (string-match-p "&& pytest -q" cmd) t)
           :has-file (and (string-match-p "test_demo\\.py" cmd) t)
           :all-has-dot (and (string-match-p "'\\.'" cmd-all) t)
           :all-has-x (and (string-match-p "pytest -x" cmd-all) t)))))
"####,
        expect!["OK (:where-ok t :has-cd t :has-pytest t :has-file t :all-has-dot t :all-has-x t)"],
    )
}

fn missing_test_file_signals_and_again_without_history_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_test_file_signals_and_again_without_history_errors",
        r####"
(list :missing
      (condition-case err
          (progn (pytest-check-test-file "/no/such/pytest-file.py") :ok)
        (error (error-message-string err)))
      :buffer-name (pytest-get-temp-buffer-name)
      :again-without-history
      (condition-case err
          (progn (pytest-again) :ok)
        (error (error-message-string err))))
"####,
        expect![[
            r#"OK (:missing "’/no/such/pytest-file.py’ is not an extant file." :buffer-name "*pytest*" :again-without-history "Pytest has not run before")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        command_format_composes_working_directory_runner_and_tests(),
        project_root_walks_up_to_marker_files(),
        py_testable_builds_class_and_function_paths(),
        get_command_composes_cd_and_quoted_test_names(),
        missing_test_file_signals_and_again_without_history_errors(),
    ]
}
