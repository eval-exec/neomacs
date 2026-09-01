use expect_test::expect;

use super::ParityBatchCase;

fn aider_file_paths_are_repo_relative_local_and_shell_space_aware() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_file_paths_are_repo_relative_local_and_shell_space_aware",
        r##"(list
         (cl-letf (((symbol-function 'magit-toplevel)
                    (lambda () "/repo/root/")))
           (mapcar
            (lambda (path)
              (aider--format-file-path
               (aider--get-file-path path)))
            '("/repo/root/src/main.py"
              "/repo/root/docs/user guide.md"
              "/outside/data.txt")))
         (cl-letf (((symbol-function 'magit-toplevel)
                    (lambda () nil)))
           (mapcar #'aider--get-file-path
                   '("/outside/file.py" "./relative.el")))
         (mapcar #'aider--format-file-path
                 '("plain.el" "two words.el" "tabs\tstay.el")))"##,
        expect![[
            r#"OK (("src/main.py" "\"docs/user guide.md\"" "../../outside/data.txt") ("/outside/file.py" "[ORACLE-SANDBOX]/relative.el") ("plain.el" "\"two words.el\"" "tabs\11stay.el"))"#
        ]],
    )
}

fn aider_real_directory_suffix_and_content_filter_pipeline_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_real_directory_suffix_and_content_filter_pipeline_matches",
        r##"(let* ((root (expand-file-name "module"
                                           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (nested (expand-file-name "nested" root)))
         (make-directory nested t)
         (dolist (entry
                  '(("main.py" . "import helper\nFEATURE = 1\n")
                    ("helper.py" . "VALUE = 2\n")
                    ("test_main.py" . "from main import FEATURE\n")
                    ("notes.txt" . "FEATURE docs\n")
                    ("nested/view.py" . "FEATURE = 3\n")
                    ("nested/view.el" . "(message \"FEATURE\")\n")))
           (with-temp-file (expand-file-name (car entry) root)
             (insert (cdr entry))))
         (let* ((python (sort (aider--get-files-in-directory root '("py"))
                              #'string-lessp))
                (feature (sort
                          (aider--filter-files-by-content-regex
                           python "FEATURE")
                          #'string-lessp)))
           (list
            (mapcar #'file-name-nondirectory python)
            (mapcar (lambda (file) (file-relative-name file root)) feature)
            (aider--filter-files-by-content-regex python "NO_MATCH")
            (length
             (aider--filter-files-by-content-regex python nil)))))"##,
        expect![[
            r#"OK (("helper.py" "main.py" "view.py" "test_main.py") ("main.py" "nested/view.py" "test_main.py") nil 4)"#
        ]],
    )
}

fn aider_dependency_scanner_ignores_comments_strings_tests_and_flycheck_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_dependency_scanner_ignores_comments_strings_tests_and_flycheck_files",
        r##"(let* ((root (expand-file-name "context"
                                           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (main (expand-file-name "main.py" root)))
         (make-directory root t)
         (dolist (entry
                  '(("main.py" . "from helper import run\n# import ignored\ntext = \"client\"\n")
                    ("helper.py" . "def run(): pass\n")
                    ("client.py" . "from main import value\n")
                    ("test_main.py" . "from main import value\n")
                    ("flycheck_main.py" . "from main import value\n")
                    ("ignored.py" . "# main only in comment\n")
                    ("string_only.py" . "x = \"main\"\n")))
           (with-temp-file (expand-file-name (car entry) root)
             (insert (cdr entry))))
         (let ((dependencies
                (sort (aider--find-file-dependencies main root)
                      #'string-lessp))
               (dependents
                (sort (aider--find-file-dependents main root)
                      #'string-lessp)))
           (list
            (mapcar #'file-name-nondirectory dependencies)
            (mapcar #'file-name-nondirectory dependents)
            (mapcar #'file-name-nondirectory
                    (aider--filter-test-files dependents nil))
            (mapcar #'file-name-nondirectory
                    (aider--filter-test-files dependents t)))))"##,
        expect![[
            r#"OK (("helper.py" "ignored.py") ("client.py" "test_main.py") ("client.py" "test_main.py") ("client.py" "test_main.py"))"#
        ]],
    )
}

fn aider_context_processing_deduplicates_and_sends_one_formatted_command_per_file()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aider_context_processing_deduplicates_and_sends_one_formatted_command_per_file",
        r##"(let (sent switched)
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _) "/read-only"))
                   ((symbol-function 'aider--get-file-path)
                    (lambda (file) (file-name-nondirectory file)))
                   ((symbol-function 'aider--send-command)
                    (lambda (command &optional switch _log)
                      (push (list command switch) sent)
                      t))
                   ((symbol-function 'aider-switch-to-buffer)
                    (lambda () (setq switched t))))
           (aider--process-context-files
            "/repo/main.py"
            '("/repo/helper.py" "/repo/shared file.py")
            '("/repo/client.py" "/repo/helper.py"))
           (list (nreverse sent) switched)))"##,
        expect![[
            r#"OK ((("/read-only main.py" nil) ("/read-only helper.py" nil) ("/read-only \"shared file.py\"" nil) ("/read-only client.py" nil)) t)"#
        ]],
    )
}

fn aider_current_file_and_dired_command_workflows_build_exact_session_messages() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aider_current_file_and_dired_command_workflows_build_exact_session_messages",
        r##"(let* ((root (expand-file-name "files"
                                           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (one (expand-file-name "one.el" root))
                (two (expand-file-name "two words.el" root))
                sent)
         (make-directory root t)
         (dolist (file (list one two))
           (with-temp-file file (insert "x")))
         (cl-letf (((symbol-function 'magit-toplevel)
                    (lambda () (file-name-as-directory root)))
                   ((symbol-function 'aider--send-command)
                    (lambda (command &optional switch _log)
                      (push (list command switch) sent)
                      t)))
           (with-temp-buffer
             (setq buffer-file-name one)
             (aider-action-current-file "/add")
             (aider-action-current-file "/drop"))
           (cl-letf (((symbol-function 'dired-get-marked-files)
                      (lambda () (list one two))))
             (aider--batch-add-dired-marked-files-with-command
              "/read-only"))
           (nreverse sent)))"##,
        expect![[
            r#"OK (("/add one.el" nil) ("/drop one.el" nil) ("/read-only one.el \"two words.el\"" t))"#
        ]],
    )
}

fn aider_source_pattern_file_discovery_and_import_keyword_detection_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_source_pattern_file_discovery_and_import_keyword_detection_match",
        r##"(let* ((root (expand-file-name "patterns"
                                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (make-directory root t)
         (dolist (entry
                  '(("a.rs" . "use crate::b;")
                    ("b.rs" . "pub fn b() {}")
                    ("flycheck_shadow.rs" . "use crate::a;")
                    ("c.py" . "import os")
                    ("README" . "require module")))
           (with-temp-file (expand-file-name (car entry) root)
             (insert (cdr entry))))
         (list
          (aider--get-source-file-patterns "rs")
          (mapcar
           #'file-name-nondirectory
           (sort
            (aider--find-files-by-patterns root '("*.rs"))
            #'string-lessp))
          (mapcar #'aider--line-has-import-keyword-p
                  '("use crate::foo;"
                    "from x import y"
                    "  require('x')"
                    "ordinary prose"
                    "// import only comment"))))"##,
        expect![[r#"OK (("*.rs") ("a.rs" "b.rs") (nil 7 2 nil 3))"#]],
    )
}

fn aider_file_at_point_resolution_and_drop_command_use_real_workspace_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_file_at_point_resolution_and_drop_command_use_real_workspace_file",
        r##"(let* ((root (expand-file-name "cursor-file"
                                           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (file (expand-file-name "src/demo file.el" root))
                sent)
         (make-directory (file-name-directory file) t)
         (with-temp-file file (insert "(message \"ok\")"))
         (with-temp-buffer
           (setq default-directory (file-name-as-directory root))
           (insert "src/demo file.el")
           (goto-char (+ (point-min) 5))
           (cl-letf (((symbol-function 'magit-toplevel)
                      (lambda () (file-name-as-directory root)))
                     ((symbol-function 'aider--send-command)
                      (lambda (command &rest _)
                        (setq sent command)
                        'sent)))
             (list
              (file-relative-name
               (aider--get-full-expanded-file-path-at-point)
               root)
              (aider--file-path-under-cursor-is-file)
              (aider--drop-file-under-cursor)
              sent))))"##,
        expect![[r#"OK ("src/demo file.el" t sent "/drop \"src/demo file.el\"")"#]],
    )
}

pub(super) fn files_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aider_file_paths_are_repo_relative_local_and_shell_space_aware(),
        aider_real_directory_suffix_and_content_filter_pipeline_matches(),
        aider_dependency_scanner_ignores_comments_strings_tests_and_flycheck_files(),
        aider_context_processing_deduplicates_and_sends_one_formatted_command_per_file(),
        aider_current_file_and_dired_command_workflows_build_exact_session_messages(),
        aider_source_pattern_file_discovery_and_import_keyword_detection_match(),
        aider_file_at_point_resolution_and_drop_command_use_real_workspace_file(),
    ]
}
