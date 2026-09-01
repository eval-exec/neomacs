use expect_test::expect;

use super::ParityBatchCase;

fn aider_markdown_safety_advice_contains_failures_only_inside_aider_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_markdown_safety_advice_contains_failures_only_inside_aider_mode",
        r##"(let* (calls
                (original-fn
                 (lambda (&rest args)
                   (push args calls)
                   (if (equal (car args) 'boom)
                       (error "boom")
                     "original"))))
           (list
            (let ((major-mode 'aider-comint-mode))
              (list
               (aider--safe-maybe-funcall-regexp
                original-fn (lambda () "dynamic"))
               (aider--safe-maybe-funcall-regexp
                original-fn "literal")
               (aider--safe-maybe-funcall-regexp
                original-fn nil)
               (aider--safe-maybe-funcall-regexp
                original-fn 42)
               (aider--safe-get-start-fence-regexp
                original-fn 'boom)))
            (let ((major-mode 'fundamental-mode))
              (list
               (aider--safe-maybe-funcall-regexp
                original-fn "outside")
               (aider--safe-get-start-fence-regexp
                original-fn 'outside)))
            (nreverse calls)))"##,
        expect![[
            r#"OK (("dynamic" "literal" "" "" "\\`never-match\\`") ("original" "original") ((boom) ("outside" nil) (outside)))"#
        ]],
    )
}

fn aider_highlight_refinement_creates_and_clears_real_diff_overlays() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_highlight_refinement_creates_and_clears_real_diff_overlays",
        r##"(with-temp-buffer
         (insert "<<<<<<< SEARCH\nalpha beta\n=======\nalpha gamma\n>>>>>>> REPLACE\n")
         (goto-char (point-min))
         (search-forward "beta")
         (let ((conflict (aider--find-conflict-at-point (point))))
           (aider--smerge-refine-conflict conflict)
           (let ((before
                  (mapcar
                   (lambda (overlay)
                     (list (overlay-start overlay)
                           (overlay-end overlay)
                           (overlay-get overlay 'face)
                           (overlay-get overlay 'priority)))
                   (overlays-in (point-min) (point-max)))))
             (aider--clear-diff-overlays)
             (list conflict
                   (sort before
                         (lambda (left right)
                           (< (car left) (car right))))
                   (overlays-in (point-min) (point-max))))))"##,
        expect![
            "OK ((15 27 34 47) ((15 27 nil 1000) (22 26 smerge-refined-removed 1000) (34 47 nil 1000) (41 46 smerge-refined-added 1000)) (#<overlay in no buffer> #<overlay in no buffer>))"
        ],
    )
}

fn aider_git_branch_resolution_and_diff_parameter_workflows_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_git_branch_resolution_and_diff_parameter_workflows_match",
        r##"(let (checks)
         (cl-letf (((symbol-function 'magit-branch-p)
                    (lambda (branch)
                      (push (list 'branch branch) checks)
                      (member branch '("main" "origin/topic"))))
                   ((symbol-function 'magit-rev-verify)
                    (lambda (revision)
                      (push (list 'rev revision) checks)
                      (equal revision "abc123"))))
           (list
            (mapcar #'aider--get-full-branch-ref
                    '("main" "topic" "origin/topic" "abc123" "missing"))
            (aider--resolve-diff-branches
             'commit "abc123^" "abc123")
            (aider--resolve-diff-branches
             'base-vs-head "main" "ignored")
            (aider--resolve-diff-branches
             'branch-range "main" "topic" 'local)
            (aider--resolve-diff-branches
             'branch-range "main" "topic" 'remote)
            (nreverse checks))))"##,
        expect![[
            r#"OK (("main" "origin/topic" "origin/topic" "abc123" "missing") ("abc123^" . "abc123") ("main" . "HEAD") ("main" . "topic") ("origin/main" . "origin/topic") ((branch "origin/main") (branch "main") (branch "origin/topic") (branch "origin/topic") (branch "origin/abc123") (branch "abc123") (rev "abc123") (branch "origin/missing") (branch "missing") (rev "missing") (branch "origin/main") (branch "main")))"#
        ]],
    )
}

fn aider_real_git_staged_diff_generation_writes_expected_patch() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_real_git_staged_diff_generation_writes_expected_patch",
        r##"(let* ((root (expand-file-name "git-diff"
                                           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (default-directory (file-name-as-directory root))
                (file (expand-file-name "demo.txt" root))
                (diff-file (expand-file-name "staged.diff" root)))
         (make-directory root t)
         (process-file "git" nil nil nil "init" "-q")
         (process-file "git" nil nil nil "config" "user.name" "Parity")
         (process-file "git" nil nil nil "config" "user.email" "parity@example.test")
         (with-temp-file file (insert "one\n"))
         (process-file "git" nil nil nil "add" "demo.txt")
         (process-file "git" nil nil nil "commit" "-qm" "base")
         (with-temp-file file (insert "one\ntwo\n"))
         (process-file "git" nil nil nil "add" "demo.txt")
         (aider--generate-staged-diff diff-file)
         (with-temp-buffer
           (insert-file-contents diff-file)
           (list
            (file-exists-p diff-file)
            (buffer-string)
            (process-lines "git" "status" "--short"))))"##,
        expect![[
            r#"OK (t "diff --git a/demo.txt b/demo.txt\nindex 5626abf..814f4a4 100644\n--- a/demo.txt\n+++ b/demo.txt\n@@ -1 +1,2 @@\n one\n+two\n" ("M  demo.txt" "?? staged.diff"))"#
        ]],
    )
}

fn aider_log_prompt_builders_cover_keyword_and_whole_repository_analysis() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_log_prompt_builders_cover_keyword_and_whole_repository_analysis",
        r##"(list
         (aider--default-log-analysis-instructions "")
         (aider--default-log-analysis-instructions "parser")
         (aider--build-log-prompt
          "neomacs"
          "Focus on correctness and regressions.")
         (string-match-p
          "ASCII art diagram"
          (let (sent)
            (cl-letf (((symbol-function 'aider-read-string)
                       (lambda (_prompt initial &rest _) initial))
                      ((symbol-function 'aider--send-command)
                       (lambda (command &rest _)
                         (setq sent command)
                         t)))
              (aider--plot-module-architecture)
              sent))))"##,
        expect![[
            r#"OK ("Please analyze the following Git log for the entire repository. Provide insights on:\n1. Overall project evolution and major development phases, with author name in each phase.\n2. Identification of key features, refactorings, or architectural changes and their timeline, with author name for each one.\n3. Patterns in development activity (e.g., periods of rapid development, bug fixing, etc.), with author name.\n4. Significant contributors or shifts in contribution patterns (if discernible from commit messages).\n5. Potential areas of technical debt or architectural concerns suggested by the commit history.\n6. General trends in the project's direction or focus over time." "Analyze the commits filtered by keyword 'parser'. Provide insights on:\n1. Overall 'parser' related feature evolution and major development phases, with author name in each phase.\n2. Frequency and patterns of 'parser' related commits.\n3. Files or areas most impacted by 'parser' changes.\n4. Main contributors and their roles in 'parser' work.\n5. Trends or hotspots in 'parser' related development.\n6. Suggestions for improving or refactoring 'parser' implementation.\n" "Analyze the Git commit history for the entire repository 'neomacs'.\n\nRepository: neomacs\n\nThe detailed Git log content is in the 'git.log' file (which has been added to the chat).\nPlease use its content for your analysis, following these instructions:\nFocus on correctness and regressions." 135)"#
        ]],
    )
}

fn aider_question_and_code_read_workflows_build_contextual_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_question_and_code_read_workflows_build_contextual_commands",
        r##"(let (calls)
         (cl-letf (((symbol-function 'aider-read-string)
                    (lambda (prompt &optional initial candidates)
                      (push (list 'read prompt initial (length candidates)) calls)
                      "What are the invariants?"))
                   ((symbol-function 'aider-current-file-command-and-switch)
                    (lambda (prefix command)
                      (push (list 'current prefix command) calls)
                      t))
                   ((symbol-function 'aider-add-current-file)
                    (lambda () (push '(add) calls)))
                   ((symbol-function 'aider--send-command)
                    (lambda (command &optional switch _log)
                      (push (list 'send command switch) calls)
                      t))
                   ((symbol-function 'which-function)
                    (lambda () "calculate")))
           (with-temp-buffer
             (insert "first line\nsecond line")
             (goto-char (point-min))
             (set-mark (point))
             (goto-char (point-max))
             (activate-mark)
             (aider--ask-about-region "calculate")
             (deactivate-mark))
           (aider--analyze-code-unit)
           (aider--analyze-for-maintainability)
           (nreverse calls)))"##,
        expect![[
            r#"OK ((read "Question for the selected region in function 'calculate': " nil 11) (current "/ask " "Question for the selected region in function 'calculate': What are the invariants?: first line\nsecond line") (read "Enter analysis instructions: " "In the current file, analyze function 'calculate' using bottom-up reading approach.\nExplain its basic operations, data structures, and control flow." 0) (send "/ask What are the invariants?" t) (read "Enter maintainability analysis instructions: " "Please analyze the code in the current file for maintainability and code quality:\n1. Readability: Is the code clear, well-formatted, and easy to understand? Are variable/function names meaningful?\n2. Complexity: Are functions/methods too long or complex (high cyclomatic complexity)? Are classes too large (violating SRP)?\n3. Duplication: Is there significant duplicated code (potential for DRY principle violation)?\n4. Code Smells: Are there common code smells (e.g., magic numbers, feature envy, inappropriate intimacy)?\n5. Comments/Documentation: Is the code adequately commented? Is documentation (e.g., docstrings) present and accurate?\n6. Testability: Is the code structured in a way that makes it easy to write unit tests (e.g., low coupling, dependency injection)?\n7. Consistency: Is the code style consistent throughout the file?\n8. Modularity: Is the code well-modularized with clear responsibilities?" 0) (send "/ask What are the invariants?" t))"#
        ]],
    )
}

fn aider_model_selection_sends_model_then_reasoning_effort_for_openai_models_only()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aider_model_selection_sends_model_then_reasoning_effort_for_openai_models_only",
        r##"(let (sent messages answers)
         (setq answers '("/model" "o4-mini" "high"
                         "/editor-model" "sonnet"))
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _)
                      (prog1 (car answers)
                        (setq answers (cdr answers)))))
                   ((symbol-function 'aider--send-command)
                    (lambda (command &optional switch _log)
                      (push (list command switch) sent)
                      t))
                   ((symbol-function 'message)
                    (lambda (format-string &rest args)
                      (push (apply #'format format-string args) messages))))
           (aider-change-model nil)
           (aider-change-model nil)
           (list (nreverse sent) (nreverse messages))))"##,
        expect![[
            r#"OK ((("/model o4-mini" t) ("/reasoning-effort high" t) ("/editor-model sonnet" t)) ("model changed to o4-mini, customize aider-popular-models for the model candidates" "Reasoning effort set to high for model o4-mini" "editor-model changed to sonnet, customize aider-popular-models for the model candidates"))"#
        ]],
    )
}

fn aider_helm_history_merges_cli_candidates_and_persists_latest_input() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_helm_history_merges_cli_candidates_and_persists_latest_input",
        r##"(let* ((user-emacs-directory
                  (file-name-as-directory
                   (expand-file-name "helm-home"
                                     (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                (helm-file
                 (expand-file-name
                  "aider-helm-read-string-history.el"
                  user-emacs-directory))
                (cli-file
                 (expand-file-name ".aider.input.history"
                                   user-emacs-directory))
                captured)
         (make-directory user-emacs-directory t)
         (with-temp-file helm-file
           (prin1 '("helm-new" "shared" "helm-old") (current-buffer)))
         (with-temp-file cli-file
           (insert "+ cli-old\n+ shared\n+ cli-new\n")
           (insert "+ {aider\n+ multi\n+ aider}\n"))
         (cl-letf (((symbol-function 'aider--generate-history-file-name)
                    (lambda () cli-file))
                   ((symbol-function 'helm-comp-read)
                    (lambda (prompt collection &rest arguments)
                      (setq captured
                            (list prompt collection arguments))
                      "new answer")))
           (let ((result
                  (aider-helm-read-string-with-history
                   "Prompt: "
                   "aider-helm-read-string-history.el"
                   "seed"
                   '("candidate" "shared"))))
             (list
              result
              captured
              (with-temp-buffer
                (insert-file-contents helm-file)
                (read (buffer-string)))))))"##,
        expect![[
            r#"OK ("new answer" ("Prompt: " ("helm-new" "candidate" "shared" "==================== HISTORY ========================================" "shared" "helm-old" "cli-new" "cli-old") (:must-match nil :name "Helm Read String, Use C-c C-y to edit selected command. C-b and C-f to move cursor during editing" :fuzzy t :initial-input "seed")) ("new answer" "helm-new" "shared" "helm-old" "cli-new" "cli-old"))"#
        ]],
    )
}

pub(super) fn workflows_aider_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aider_markdown_safety_advice_contains_failures_only_inside_aider_mode(),
        aider_highlight_refinement_creates_and_clears_real_diff_overlays(),
        aider_git_branch_resolution_and_diff_parameter_workflows_match(),
        aider_real_git_staged_diff_generation_writes_expected_patch(),
        aider_log_prompt_builders_cover_keyword_and_whole_repository_analysis(),
        aider_question_and_code_read_workflows_build_contextual_commands(),
        aider_model_selection_sends_model_then_reasoning_effort_for_openai_models_only(),
    ]
}

pub(super) fn workflows_aider_helm_batch_cases() -> Vec<ParityBatchCase> {
    vec![aider_helm_history_merges_cli_candidates_and_persists_latest_input()]
}
