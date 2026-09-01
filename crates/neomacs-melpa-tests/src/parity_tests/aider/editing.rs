use expect_test::expect;

use super::ParityBatchCase;

fn aider_comment_detection_extraction_location_and_instruction_generation_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aider_comment_detection_extraction_location_and_instruction_generation_match",
        r##"(let ((comment-start ";; ")
               (comment-end ""))
         (list
          (mapcar #'aider--is-comment-line
                  '(";; one" "  ;;; two" "code ;; tail" "" "plain"))
          (aider--is-all-comment-lines
           ";; first\n  ;; second\n;;; third")
          (aider--is-all-comment-lines
           ";; first\ncode\n;; third")
          (aider--extract-comment-content
           ";; first requirement\n  ;;; second detail")
          (with-temp-buffer
            (insert "one\ntwo\nthree\nfour\n")
            (list (aider--region-location-info 1 3)
                  (aider--region-location-info 1 12)))
          (cl-letf (((symbol-function 'aider-read-string)
                     (lambda (_prompt initial &rest _) initial)))
            (list
             (aider--get-comment-instruction "ship it" "demo")
             (aider--get-comment-instruction "ship it" nil)))))"##,
        expect![[
            r#"OK ((0 0 nil nil nil) t nil "first requirement second detail" ("Selected region on line 1" "Selected region from line 1 to 3") ("In function demo, change code according to requirement: ship it" "Change code according to requirement: ship it"))"#
        ]],
    )
}

fn aider_region_change_commands_quote_function_and_free_region_contexts_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aider_region_change_commands_quote_function_and_free_region_contexts_exactly",
        r##"(list
         (aider-region-change-generate-command
          "x = 1;\ny = 2;" "calculate" "remove mutation")
         (aider-region-change-generate-command
          "x = 1;" nil "use a constant")
         (cl-letf (((symbol-function 'aider-read-string)
                    (lambda (prompt initial candidates)
                      (list prompt initial
                            (length candidates)
                            (car candidates)))))
           (with-temp-buffer
             (setq buffer-file-name "/repo/test_demo.py")
             (aider--get-standard-instruction t "demo")))
         (cl-letf (((symbol-function 'aider-read-string)
                    (lambda (prompt initial candidates)
                      (list prompt initial
                            (length candidates)
                            (car candidates)))))
           (with-temp-buffer
             (setq buffer-file-name "/repo/README.org")
             (aider--get-standard-instruction nil nil))))"##,
        expect![[
            r#"OK ("/architect \"in function calculate, for the following code block, remove mutation: x = 1;\ny = 2;\"" "/architect \"for the following code block, use a constant: x = 1;\"" ("Code change instruction for selected region in function 'demo': " nil 5 "Write a new unit test function based on the given description.") ("Change instruction: " nil 15 "Improve English grammar and clarity of the text."))"#
        ]],
    )
}

fn aider_blank_line_todo_workflow_inserts_language_appropriate_requirement() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_blank_line_todo_workflow_inserts_language_appropriate_requirement",
        r##"(let (results)
         (dolist (mode '(emacs-lisp-mode python-mode))
           (with-temp-buffer
             (setq buffer-file-name
                   (expand-file-name
                    (if (eq mode 'emacs-lisp-mode)
                        "blank-todo.el"
                      "blank-todo.py")
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
             (funcall mode)
             (insert
              (if (eq mode 'emacs-lisp-mode)
                  "   \n(defun existing () nil)\n"
                "   \ndef existing():\n    pass\n"))
             (goto-char (point-min))
             (cl-letf (((symbol-function 'aider--validate-buffer-file)
                        (lambda () "/sandbox/example"))
                       ((symbol-function 'aider-read-string)
                        (lambda (&rest _) "Validate empty input")))
               (aider-implement-todo)
               (push
                (list mode
                      (buffer-substring-no-properties
                       (point-min) (point-max))
                      (point)
                      comment-start
                      comment-end)
                results))))
         (nreverse results))"##,
        expect![[
            r##"OK ((emacs-lisp-mode ";; TODO: Validate empty input\n(defun existing () nil)\n" 30 ";" "") (python-mode "# TODO: Validate empty input\ndef existing():\n    pass\n" 29 "# " ""))"##
        ]],
    )
}

fn aider_comment_requirement_workflow_deletes_line_and_routes_architect_command() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aider_comment_requirement_workflow_deletes_line_and_routes_architect_command",
        r##"(let (calls)
         (with-temp-buffer
           (emacs-lisp-mode)
           (insert ";; Replace mutation with a fold\n(defun demo (xs) xs)\n")
           (goto-char (point-min))
           (cl-letf (((symbol-function 'which-function)
                      (lambda () "demo"))
                     ((symbol-function 'aider-read-string)
                      (lambda (prompt initial &rest _)
                        (push (list 'read prompt initial) calls)
                        initial))
                     ((symbol-function 'aider-add-current-file)
                      (lambda () (push '(add-file) calls)))
                     ((symbol-function 'aider--send-command)
                      (lambda (command &optional switch _log)
                        (push (list 'send command switch) calls)
                        t)))
             (aider-function-or-region-change)
             (list (buffer-string) (nreverse calls)))))"##,
        expect![[
            r#"OK ("(defun demo (xs) xs)\n" ((read "Code change instruction: " "In function demo, change code according to requirement: Replace mutation with a fold") (add-file) (send "/architect In function demo, change code according to requirement: Replace mutation with a fold" t)))"#
        ]],
    )
}

fn aider_prompt_mode_cycles_file_and_question_commands_in_real_org_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_prompt_mode_cycles_file_and_question_commands_in_real_org_buffer",
        r##"(with-temp-buffer
         (let ((aider-enable-markdown-highlighting nil))
           (aider-prompt-mode))
         (insert "src/main.py\n/read-only src/lib.py\n/drop docs/a.md\n/ask explain\n/architect change\n")
         (goto-char (point-min))
         (dotimes (_ 5)
           (aider-prompt-cycle-file-command)
           (forward-line 1))
         (let ((first-pass (buffer-string)))
           (goto-char (point-min))
           (aider-prompt-cycle-file-command)
           (forward-line 1)
           (aider-prompt-cycle-file-command)
           (list
            major-mode
            (derived-mode-p 'org-mode)
            comment-start
            first-pass
            (buffer-string)
            (memq #'aider-core--command-completion
                  completion-at-point-functions))))"##,
        expect![[
            r##"OK (aider-prompt-mode org-mode "# " "/add src/main.py\n/drop src/lib.py\n/add docs/a.md\n/architect explain\n/ask change\n" "/read-only src/main.py\n/add src/lib.py\n/add docs/a.md\n/architect explain\n/ask change\n" (aider-core--command-completion pcomplete-completions-at-point t ispell-completion-at-point))"##
        ]],
    )
}

fn aider_prompt_font_lock_marks_safe_and_mutating_commands_in_real_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_prompt_font_lock_marks_safe_and_mutating_commands_in_real_document",
        r##"(with-temp-buffer
         (aider-prompt-mode)
         (insert "/ask explain\n/code change\n/commit now\n/read-only src/a.el\ngo ahead\n")
         (font-lock-ensure)
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (list needle
                  (get-text-property (1- (point)) 'face)))
          '("/ask" "/code" "/commit" "/read-only" "go ahead")))"##,
        expect![[
            r#"OK (("/ask" font-lock-type-face) ("/code" font-lock-warning-face) ("/commit" font-lock-warning-face) ("/read-only" font-lock-type-face) ("go ahead" font-lock-type-face))"#
        ]],
    )
}

fn aider_class_detection_walks_python_java_rust_and_non_class_contexts() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_class_detection_walks_python_java_rust_and_non_class_contexts",
        r##"(mapcar
         (lambda (source)
           (with-temp-buffer
             (insert source)
             (goto-char (point-max))
             (aider--get-class-at-point)))
         '("class UserService:\n    pass\n"
           "interface Gateway {\n  void send();\n}\n"
           "trait Render {\n}\n"
           "struct Point {\n int x;\n};\n"
           "def ordinary():\n    pass\n"
           "class Outer:\n  pass\nclass Inner:\n  pass\n"))"##,
        expect![[r#"OK ("UserService" "Gateway" "Render" "Point" nil "Inner")"#]],
    )
}

fn aider_search_replace_block_parser_tracks_boundaries_content_and_outside_points()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aider_search_replace_block_parser_tracks_boundaries_content_and_outside_points",
        r##"(with-temp-buffer
         (insert "before\n<<<<<<< SEARCH\nold one\nold two\n=======\nnew one\nnew three\n>>>>>>> REPLACE\nafter\n")
         (let ((inside
                (progn
                  (goto-char (point-min))
                  (search-forward "old two")
                  (point)))
               (outside (point-min)))
           (list
            (aider--find-search-replace-block-at-point inside)
            (progn
              (goto-char inside)
              (aider--extract-search-replace-blocks))
            (aider--find-search-replace-block-at-point outside)
            (aider--find-conflict-at-point inside))))"##,
        expect![[
            r#"OK ((8 22 39 46 65 80) ("\nold one\nold two\n" "\nnew one\nnew three\n") nil (22 39 46 65))"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aider_comment_detection_extraction_location_and_instruction_generation_match(),
        aider_region_change_commands_quote_function_and_free_region_contexts_exactly(),
        aider_blank_line_todo_workflow_inserts_language_appropriate_requirement(),
        aider_comment_requirement_workflow_deletes_line_and_routes_architect_command(),
        aider_prompt_mode_cycles_file_and_question_commands_in_real_org_buffer(),
        aider_prompt_font_lock_marks_safe_and_mutating_commands_in_real_document(),
        aider_class_detection_walks_python_java_rust_and_non_class_contexts(),
        aider_search_replace_block_parser_tracks_boundaries_content_and_outside_points(),
    ]
}
