use expect_test::expect;

use super::ParityBatchCase;

/// The plain search: `M-x ag` over a real tree.  Pins the exact argument vector
/// the silver searcher was given, the results buffer's generated name and mode,
/// and the fully rendered buffer -- ag.el's filter has turned the colour escapes
/// into `File:` headers and stripped the rest, and the echoed command line shows
/// how the arguments were shell-quoted.
fn a_plain_search_builds_the_argv_and_renders_grouped_results() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_plain_search_builds_the_argv_and_renders_grouped_results",
        r##"(ag-test-with-project
 (ag "Grüße" ag-test-project)
 (ag-test-wait-for-search)
 (list :calls (ag-test-calls) :rendered (ag-test-rendered)))"##,
        expect![[
            r#"OK (:calls (("--literal" "--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "Grüße" ".")) :rendered (:name "*ag search text:Grüße dir:[ORACLE-SANDBOX]/project/*" :mode ag-mode :text "-*- mode: ag; default-directory: \"<ROOT>/project/\" -*-\n<STATUS>\n\n<ROOT>/bin/ag --literal --group --line-number --column --color --color-match 30\\;43 --color-path 1\\;32 --smart-case --stats -- Gr\\ü\\ße .\nFile: src/greeting.el\n1:4:;; Grüße an alle\n2:24:(defun greet () \"Grüße\")\n\nFile: docs/design notes.md\n3:9:We say Grüße in the greeting module.\n\nFile: README.md\n1:1:Grüße everyone.\n\n<STATUS>\n"))"#
        ]],
    )
}

fn visiting_each_match_lands_on_the_exact_file_line_and_column() -> ParityBatchCase {
    ParityBatchCase::value(
        "visiting_each_match_lands_on_the_exact_file_line_and_column",
        r##"(ag-test-with-project
 (ag "Grüße" ag-test-project)
 (ag-test-wait-for-search)
 (let ((results (ag-test-results-buffer)) hops)
   (with-current-buffer results (goto-char (point-min)))
   (dotimes (_ 4)
     (push (with-current-buffer results
             (compilation-next-error 1)
             (compile-goto-error)
             (with-current-buffer (window-buffer (selected-window))
               (list (if (buffer-file-name)
                         (file-relative-name (buffer-file-name) ag-test-project)
                       (buffer-name))
                     (line-number-at-pos) (current-column)
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))))
           hops))
   (list :hops (reverse hops)
         :results-mode (with-current-buffer results major-mode)
         :next-error-fn (with-current-buffer results next-error-function)
         :error-alist (with-current-buffer results compilation-error-regexp-alist))))"##,
        expect![[
            r#"OK (:hops (("src/greeting.el" 1 3 ";; Grüße an alle") ("src/greeting.el" 2 23 "(defun greet () \"Grüße\")") ("docs/design notes.md" 3 8 "We say Grüße in the greeting module.") ("README.md" 1 0 "Grüße everyone.")) :results-mode ag-mode :next-error-fn ag/next-error-function :error-alist (compilation-ag-nogroup compilation-ag-group))"#
        ]],
    )
}

fn each_search_command_produces_its_own_argv_and_buffer_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "each_search_command_produces_its_own_argv_and_buffer_name",
        r##"(ag-test-with-project
 (ag-regexp "Gr[uü][ßs]" ag-test-project)
 (ag-test-wait-for-search)
 (ag-files "Grüße" (list :file-type "elisp") ag-test-project)
 (ag-test-wait-for-search)
 (ag-files "Grüße" (list :file-regex "\\.md$") ag-test-project)
 (ag-test-wait-for-search)
 (let ((default-directory (file-name-as-directory
                           (expand-file-name "src" ag-test-project))))
   (ag-project "Grüße")
   (ag-test-wait-for-search)
   (list :calls (ag-test-calls)
         :project-root (ag/project-root default-directory)
         :buffers (sort (delq nil (mapcar (lambda (b)
                                            (and (string-prefix-p "*ag search" (buffer-name b))
                                                 (buffer-name b)))
                                          (buffer-list)))
                        #'string<))))"##,
        expect![[
            r#"OK (:calls (("--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "Gr[uü][ßs]" ".") ("--elisp" "--literal" "--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "Grüße" ".") ("--file-search-regex" "\\.md$" "--literal" "--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "Grüße" ".") ("--literal" "--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "Grüße" ".")) :project-root "[ORACLE-SANDBOX]/project/" :buffers ("*ag search regexp:Gr[uü][ßs] dir:[ORACLE-SANDBOX]/project/*" "*ag search text:Grüße dir:[ORACLE-SANDBOX]/project/*"))"#
        ]],
    )
    .fresh_process()
}

fn customizations_change_both_the_argv_and_the_rendering() -> ParityBatchCase {
    ParityBatchCase::value(
        "customizations_change_both_the_argv_and_the_rendering",
        r##"(ag-test-with-project
 (let ((ag-group-matches nil)
       (ag-context-lines 2)
       (ag-ignore-list '("vendor" "node_modules"))
       (ag-highlight-search t))
   (ag "Grüße" ag-test-project)
   (ag-test-wait-for-search)
   (let ((rendered (ag-test-rendered))
         (faces (with-current-buffer (ag-test-results-buffer)
                  (goto-char (point-min))
                  (when (search-forward "Grüße" nil t)
                    (list (get-text-property (match-beginning 0) 'font-lock-face)
                          (buffer-substring-no-properties
                           (line-beginning-position) (line-end-position)))))))
     (list :calls (ag-test-calls) :rendered rendered :match-face faces))))"##,
        expect![[
            r#"OK (:calls (("--ignore" "vendor" "--ignore" "node_modules" "--context=2" "--literal" "--nogroup" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "Grüße" ".")) :rendered (:name "*ag search text:Grüße dir:[ORACLE-SANDBOX]/project/*" :mode ag-mode :text "-*- mode: ag; default-directory: \"<ROOT>/project/\" -*-\n<STATUS>\n\n<ROOT>/bin/ag --ignore vendor --ignore node_modules --context\\=2 --literal --nogroup --line-number --column --color --color-match 30\\;43 --color-path 1\\;32 --smart-case --stats -- Gr\\ü\\ße .\nsrc/greeting.el\n1:4:;; Grüße an alle\n2:24:(defun greet () \"Grüße\")\n\ndocs/design notes.md\n3:9:We say Grüße in the greeting module.\n\nREADME.md\n1:1:Grüße everyone.\n\n<STATUS>\n") :match-face (ag-match-face "1:4:;; Grüße an alle"))"#
        ]],
    )
    .fresh_process()
}

fn a_search_with_no_matches_and_a_failing_search_are_both_rendered() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_search_with_no_matches_and_a_failing_search_are_both_rendered",
        r##"(ag-test-with-project
 (let ((none (progn (ag "NOTHING" ag-test-project)
                    (ag-test-wait-for-search)
                    (ag-test-rendered))))
   (dolist (b (buffer-list))
     (when (string-prefix-p "*ag search" (buffer-name b))
       (let ((kill-buffer-query-functions nil)) (kill-buffer b))))
   (ag "EXPLODE" ag-test-project)
   (ag-test-wait-for-search)
   (list :none none :failed (ag-test-rendered) :calls (ag-test-calls))))"##,
        expect![[
            r#"OK (:none (:name "*ag search text:NOTHING dir:[ORACLE-SANDBOX]/project/*" :mode ag-mode :text "-*- mode: ag; default-directory: \"<ROOT>/project/\" -*-\n<STATUS>\n\n<ROOT>/bin/ag --literal --group --line-number --column --color --color-match 30\\;43 --color-path 1\\;32 --smart-case --stats -- NOTHING .\n\n<STATUS>\n") :failed (:name "*ag search text:EXPLODE dir:[ORACLE-SANDBOX]/project/*" :mode ag-mode :text "-*- mode: ag; default-directory: \"<ROOT>/project/\" -*-\n<STATUS>\n\n<ROOT>/bin/ag --literal --group --line-number --column --color --color-match 30\\;43 --color-path 1\\;32 --smart-case --stats -- EXPLODE .\nag: unknown option\n\n<STATUS>\n") :calls (("--literal" "--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "NOTHING" ".") ("--literal" "--group" "--line-number" "--column" "--color" "--color-match" "30;43" "--color-path" "1;32" "--smart-case" "--stats" "--" "EXPLODE" ".")))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_plain_search_builds_the_argv_and_renders_grouped_results(),
        visiting_each_match_lands_on_the_exact_file_line_and_column(),
        each_search_command_produces_its_own_argv_and_buffer_name(),
        customizations_change_both_the_argv_and_the_rendering(),
        a_search_with_no_matches_and_a_failing_search_are_both_rendered(),
    ]
}
