use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_practical_lisp_return_indents_nested_form() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_practical_lisp_return_indents_nested_form",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(let ((value 1))")
         (let ((auto-indent-engine 'keys)
               (auto-indent-newline-function
                'newline-and-indent))
           (auto-indent-mode 1)
           (call-interactively (key-binding (kbd "RET")))
           (insert "(+ value 2))")
           (list
            auto-indent-mode
            (buffer-string)
            (current-indentation)
            (syntax-ppss))))"##,
        expect![[
            r#"OK (t "(let ((value 1))\n  (+ value 2))" 2 (0 nil 1 nil nil nil 0 nil nil nil nil))"#
        ]],
    )
}

fn auto_indent_mode_practical_paste_runs_cleanup_hook_and_indents_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_practical_paste_runs_cleanup_hook_and_indents_code",
        r##"(let (hook-calls)
         (with-temp-buffer
           (emacs-lisp-mode)
           (insert "(progn\n\t(message \"one\")   \n(message \"two\"))")
           (set-mark (point-min))
           (goto-char (point-max))
           (let ((auto-indent-after-yank-hook
                  (list
                   (lambda (begin end)
                     (save-restriction
                       (narrow-to-region begin end)
                       (goto-char (point-min))
                       (while (search-forward "one" nil t)
                         (replace-match "ONE"))
                       (push (list begin end) hook-calls)))))
                 (auto-indent-on-yank-or-paste t)
                 (auto-indent-mode-untabify-on-yank-or-paste t))
             (auto-indent-yank-post-command)
             (list
              (buffer-string)
              (nreverse hook-calls)
              (mark t)
              (point)))))"##,
        expect![[r#"OK ("(progn\n  (message \"ONE\")   \n  (message \"two\"))" ((1 44)) 1 47)"#]],
    )
}

fn auto_indent_mode_practical_visit_then_save_applies_distinct_policies() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_practical_visit_then_save_applies_distinct_policies",
        r##"(let ((file
                                (expand-file-name
                                 "auto-indent-save-workflow.el"
                                 default-directory)))
         (when (file-exists-p file)
           (delete-file file))
         (unwind-protect
             (with-temp-buffer
               (emacs-lisp-mode)
               (setq buffer-file-name file)
               (insert "(progn\n\t(message \"x\")   \n(message \"y\"))\n")
               (let ((auto-indent-indent-style 'aggressive)
                     (auto-indent-disabled-modes-list nil)
                     (auto-indent-on-visit-file nil)
                     (auto-indent-untabify-on-visit-file nil)
                     (auto-indent-delete-trailing-whitespace-on-visit-file t)
                     (auto-indent-on-visit-pretend-nothing-changed t)
                     (auto-indent-on-save-file t)
                     (auto-indent-untabify-on-save-file t)
                     (auto-indent-delete-trailing-whitespace-on-save-file t)
                     (auto-indent-mode t))
                 (auto-indent-file-when-visit)
                 (let ((after-visit (buffer-string))
                       (modified-after-visit
                        (buffer-modified-p)))
                   (auto-indent-file-when-save)
                   (list
                    after-visit
                    modified-after-visit
                    (buffer-string)
                    (buffer-modified-p)))))
           (when (file-exists-p file)
             (delete-file file))))"##,
        expect![[
            r#"OK ("(progn\n\11(message \"x\")\n(message \"y\"))\n" nil "(progn\n  (message \"x\")\n  (message \"y\"))\n" t)"#
        ]],
    )
}

fn auto_indent_mode_practical_delete_and_kill_workflow_preserves_lisp_structure() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_practical_delete_and_kill_workflow_preserves_lisp_structure",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(list \"alpha\",\n      \"beta\",\n      \"gamma\")")
         (goto-char (point-min))
         (search-forward ",")
         (let ((auto-indent-mode t)
               (auto-indent-par-region-timer nil)
               (auto-indent-delete-line-char-remove-extra-spaces t)
               (auto-indent-delete-line-char-add-extra-spaces t)
               (auto-indent-delete-line-char-remove-last-space t)
               (kill-ring nil))
           (auto-indent-delete-char 1)
           (search-forward "\"beta\"")
           (beginning-of-line)
           (auto-indent-kill-line 1)
           (list
            (buffer-string)
            (point)
            kill-ring
            (condition-case error-data
                (progn
                  (check-parens)
                  :balanced)
              (error
               (list :unbalanced error-data))))))"##,
        expect![[
            r#"OK ("\"gamma\")" 1 ("(list \"alpha\", \"beta\",\n") (:unbalanced (user-error "Unmatched bracket or quote")))"#
        ]],
    )
}

fn auto_indent_mode_repository_moderate_style_avoids_whole_buffer_reformat() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_repository_moderate_style_avoids_whole_buffer_reformat",
        r##"(let ((root
                                (expand-file-name
                                 "auto-indent-workflow-repo"
                                 default-directory)))
         (when (file-exists-p root)
           (delete-directory root t))
         (unwind-protect
             (progn
               (make-directory (expand-file-name ".git" root) t)
               (with-temp-buffer
                 (emacs-lisp-mode)
                 (setq buffer-file-name
                       (expand-file-name "code.el" root)
                       auto-indent-is-repository nil)
                 (insert "(progn\n(message \"x\")   )")
                 (let ((before (buffer-string))
                       (auto-indent-indent-style 'moderate)
                       (auto-indent-on-save-file t)
                       (auto-indent-untabify-on-save-file t)
                       (auto-indent-delete-trailing-whitespace-on-save-file t)
                       (auto-indent-disabled-modes-list nil))
                   (list
                    (auto-indent-is-repository-p)
                    (auto-indent-whole-buffer t)
                    (equal before (buffer-string))
                    (buffer-string)
                    (auto-indent-test-relative-or-value
                     auto-indent-is-repository root)))))
           (when (file-exists-p root)
             (delete-directory root t))))"##,
        expect![[r#"OK (t nil t "(progn\n(message \"x\")   )" "./")"#]],
    )
}

fn auto_indent_mode_textmate_commands_build_statement_endings_and_new_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_textmate_commands_build_statement_endings_and_new_lines",
        r##"(let (calls)
         (fset
          'auto-indent-test-return
          (lambda ()
            (interactive)
            (push (point) calls)
            (insert "\n")))
         (with-temp-buffer
           (insert "first\nsecond\nthird")
           (goto-char 3)
           (let ((auto-indent-eol-char ";")
                 (auto-indent-alternate-return-function-for-end-of-line-then-newline
                  'auto-indent-test-return))
             (auto-indent-eol-char-newline)
             (insert "inserted")
             (auto-indent-eol-newline)
             (list
              (buffer-string)
              (point)
              (nreverse calls)))))"##,
        expect![[r#"OK ("first;\ninserted\n\nsecond\nthird" 17 (7 16))"#]],
    )
}

fn auto_indent_mode_mode_toggle_changes_electric_and_hook_lifecycle_together() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_mode_toggle_changes_electric_and_hook_lifecycle_together",
        r##"(let (electric-calls)
         (cl-letf (((symbol-function 'electric-indent-local-mode)
                    (lambda (argument)
                      (push argument electric-calls))))
           (with-temp-buffer
             (emacs-lisp-mode)
             (let ((auto-indent-engine nil)
                   (auto-indent-on-save-file nil)
                   (auto-indent-untabify-on-save-file nil))
               (auto-indent-mode 1)
               (auto-indent-disable-electric)
               (let ((enabled
                      (list
                       auto-indent-mode
                       electric-indent-inhibit
                       (memq 'auto-indent-mode-pre-command-hook
                             pre-command-hook)
                       (memq 'auto-indent-mode-post-command-hook
                             post-command-hook))))
                 (auto-indent-mode -1)
                 (auto-indent-disable-electric)
                 (list
                  enabled
                  auto-indent-mode
                  electric-indent-inhibit
                  (memq 'auto-indent-mode-pre-command-hook
                        pre-command-hook)
                  (memq 'auto-indent-mode-post-command-hook
                        post-command-hook)
                  (nreverse electric-calls)))))))"##,
        expect![
            "OK ((t t (auto-indent-mode-pre-command-hook eldoc-pre-command-refresh-echo-area t) (auto-indent-mode-post-command-hook eldoc-schedule-timer t auto-indent-mode-post-command-hook-last)) nil nil nil nil (0))"
        ],
    )
}

fn auto_indent_mode_real_pair_tracking_follows_edit_inside_nested_form() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_real_pair_tracking_follows_edit_inside_nested_form",
        r##"(let (scheduled)
         (cl-letf (((symbol-function 'run-with-timer)
                    (lambda (delay repeat function &rest arguments)
                      (setq scheduled
                            (list delay repeat function arguments))
                      :pair-timer))
                   ((symbol-function 'cancel-timer)
                    (lambda (_timer) nil)))
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert "(outer\n  (inner value)\n  tail)")
             (search-backward "value")
             (setq auto-indent-mode t
                   auto-indent-current-pairs t
                   auto-indent-next-pair nil
                   auto-indent-indent-style 'aggressive
                   auto-indent-pairs-begin nil
                   auto-indent-pairs-end nil
                   auto-indent-par-region-timer nil
                   auto-indent-last-pre-command-hook-minibufferp nil
                   auto-indent-next-pair-timer-geo-mean
                   '((emacs-lisp-mode 0.001 1)))
             (auto-indent-mode-pre-command-hook)
             (insert "!")
             (auto-indent-mode-post-command-hook-last)
             (list
              auto-indent-pairs-begin
              auto-indent-pairs-end
              (buffer-substring
               auto-indent-pairs-begin
               auto-indent-pairs-end)
              auto-indent-par-region-timer
              scheduled))))"##,
        expect![[r#"OK (10 23 "(inner !value" :pair-timer (0.0 nil auto-indent-par-region nil))"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_practical_lisp_return_indents_nested_form(),
        auto_indent_mode_practical_paste_runs_cleanup_hook_and_indents_code(),
        auto_indent_mode_practical_visit_then_save_applies_distinct_policies(),
        auto_indent_mode_practical_delete_and_kill_workflow_preserves_lisp_structure(),
        auto_indent_mode_repository_moderate_style_avoids_whole_buffer_reformat(),
        auto_indent_mode_textmate_commands_build_statement_endings_and_new_lines(),
        auto_indent_mode_mode_toggle_changes_electric_and_hook_lifecycle_together(),
        auto_indent_mode_real_pair_tracking_follows_edit_inside_nested_form(),
    ]
}
