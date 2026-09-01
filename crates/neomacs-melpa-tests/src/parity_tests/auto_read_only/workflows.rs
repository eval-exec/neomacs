use expect_test::expect;

use super::ParityBatchCase;

fn auto_read_only_global_find_file_workflow_protects_selected_compiled_buffer_with_view_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_global_find_file_workflow_protects_selected_compiled_buffer_with_view_mode",
        r##"(save-window-excursion
         (let ((buffer
                (generate-new-buffer
                 " *auto-read-only-compiled-workflow*"))
               (find-file-hook nil))
           (unwind-protect
               (progn
                 (delete-other-windows)
                 (set-window-buffer
                  (selected-window)
                  buffer)
                 (with-current-buffer buffer
                   (insert "compiled library")
                   (set-buffer-modified-p nil)
                   (setq buffer-file-name
                         "/workspace/build/library.elc"))
                 (auto-read-only-mode 1)
                 (cl-letf
                     (((symbol-function
                        'project-current)
                       (lambda (&rest _arguments)
                         nil)))
                   (with-current-buffer buffer
                     (let ((before
                            (auto-read-only-test-buffer-state)))
                       (run-hooks 'find-file-hook)
                       (list
                        before
                        (auto-read-only-test-buffer-state)
                        auto-read-only-mode
                        (auto-read-only-test-hook-count
                         'auto-read-only--hook-find-file
                         'find-file-hook))))))
             (auto-read-only-mode -1)
             (when (buffer-live-p buffer)
               (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((" *auto-read-only-compiled-workflow*" "/workspace/build/library.elc" "compiled library" 17 nil nil nil) (" *auto-read-only-compiled-workflow*" "/workspace/build/library.elc" "compiled library" 17 t t nil) t 1)"#
        ]],
    )
}

fn auto_read_only_global_find_file_workflow_leaves_unmatched_source_editable() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_global_find_file_workflow_leaves_unmatched_source_editable",
        r##"(save-window-excursion
         (let ((buffer
                (generate-new-buffer
                 " *auto-read-only-source-workflow*"))
               (find-file-hook nil))
           (unwind-protect
               (progn
                 (delete-other-windows)
                 (set-window-buffer
                  (selected-window)
                  buffer)
                 (with-current-buffer buffer
                   (insert "editable source")
                   (set-buffer-modified-p nil)
                   (setq buffer-file-name
                         "/workspace/src/library.el"))
                 (auto-read-only-mode 1)
                 (cl-letf
                     (((symbol-function
                        'project-current)
                       (lambda (&rest _arguments)
                         nil)))
                   (with-current-buffer buffer
                     (run-hooks 'find-file-hook)
                     (goto-char (point-max))
                     (insert "!")
                     (auto-read-only-test-buffer-state))))
             (auto-read-only-mode -1)
             (when (buffer-live-p buffer)
               (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (" *auto-read-only-source-workflow*" "/workspace/src/library.el" "editable source!" 17 nil nil t)"#
        ]],
    )
}

fn auto_read_only_default_user_emacs_directory_pattern_protects_installed_package_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_default_user_emacs_directory_pattern_protects_installed_package_source",
        r##"(let* ((source
                 (locate-library "auto-read-only"))
                (installed-source
                 (expand-file-name
                  "elpa/auto-read-only-20260521.1659/auto-read-only.el"
                  user-emacs-directory)))
         (with-temp-buffer
           (insert-file-contents source)
           (set-buffer-modified-p nil)
           (setq buffer-file-name
                 installed-source)
           (list
            (file-name-nondirectory
             buffer-file-name)
            (file-in-directory-p
             buffer-file-name
             user-emacs-directory)
            (mapcar
             (lambda (regexp)
               (and
                (string-match-p
                 regexp
                 buffer-file-name)
                t))
             auto-read-only-file-regexps)
            (auto-read-only)
            buffer-read-only
            (bound-and-true-p view-mode)
            (buffer-modified-p))))"##,
        expect![[r#"OK ("auto-read-only.el" t (nil nil t nil) t t t nil)"#]],
    )
}

fn auto_read_only_project_suppression_defers_protection_until_buffer_is_outside_project()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_project_suppression_defers_protection_until_buffer_is_outside_project",
        r##"(save-window-excursion
         (let ((buffer
                (generate-new-buffer
                 " *auto-read-only-project-workflow*"))
               (project
                '(vc . "/workspace/project/"))
               events)
           (unwind-protect
               (progn
                 (delete-other-windows)
                 (set-window-buffer
                  (selected-window)
                  buffer)
                 (with-current-buffer buffer
                   (setq buffer-file-name
                         "/workspace/project/vendor/pkg.el")
                   (setq-local
                    auto-read-only-file-regexps
                    '("/vendor/"))
                   (setq-local
                    auto-read-only-function
                    (lambda ()
                      (push
                       (list
                        :protected
                        buffer-file-name)
                       events)
                      (read-only-mode 1))))
                 (cl-letf
                     (((symbol-function
                        'project-current)
                       (lambda (&rest _arguments)
                         project)))
                   (let ((inside
                          (with-current-buffer buffer
                            (auto-read-only--hook-find-file))))
                     (setq project nil)
                     (let ((outside
                            (with-current-buffer buffer
                              (auto-read-only--hook-find-file))))
                       (with-current-buffer buffer
                         (list
                          inside
                          outside
                          buffer-read-only
                          (nreverse events)))))))
             (when (buffer-live-p buffer)
               (kill-buffer buffer)))))"##,
        expect![[r#"OK (nil t t ((:protected "/workspace/project/vendor/pkg.el")))"#]],
    )
}

fn auto_read_only_custom_read_only_mode_blocks_edits_then_allows_them_after_manual_unlock()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_custom_read_only_mode_blocks_edits_then_allows_them_after_manual_unlock",
        r##"(with-temp-buffer
         (insert "vendor payload")
         (set-buffer-modified-p nil)
         (setq buffer-file-name
               "/workspace/vendor/pkg.el")
         (let ((auto-read-only-file-regexps
                '("/vendor/"))
               (auto-read-only-function
                (lambda ()
                  (read-only-mode 1))))
           (let ((result
                  (auto-read-only))
                 (blocked
                  (auto-read-only-test-error-data
                   (lambda ()
                     (goto-char (point-max))
                     (insert "!")))))
             (read-only-mode -1)
             (goto-char (point-max))
             (insert "?")
             (list
              result
              blocked
              (auto-read-only-test-buffer-state)))))"##,
        expect![[
            r#"OK (t (:error buffer-read-only ((:buffer nil))) (" *temp*" "/workspace/vendor/pkg.el" "vendor payload?" 16 nil nil t))"#
        ]],
    )
}

fn auto_read_only_disabling_global_mode_stops_subsequent_find_file_hook_protection()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_disabling_global_mode_stops_subsequent_find_file_hook_protection",
        r##"(save-window-excursion
         (let ((buffer
                (generate-new-buffer
                 " *auto-read-only-disabled-workflow*"))
               (find-file-hook nil)
               calls)
           (unwind-protect
               (progn
                 (delete-other-windows)
                 (set-window-buffer
                  (selected-window)
                  buffer)
                 (with-current-buffer buffer
                   (setq buffer-file-name
                         "/workspace/vendor/pkg.el")
                   (setq-local
                    auto-read-only-file-regexps
                    '("/vendor/"))
                   (setq-local
                    auto-read-only-function
                    (lambda ()
                      (push :protected calls))))
                 (auto-read-only-mode 1)
                 (auto-read-only-mode -1)
                 (cl-letf
                     (((symbol-function
                        'project-current)
                       (lambda (&rest _arguments)
                         nil)))
                   (with-current-buffer buffer
                     (run-hooks 'find-file-hook)
                     (list
                      auto-read-only-mode
                      find-file-hook
                      calls
                      buffer-read-only))))
             (auto-read-only-mode -1)
             (when (buffer-live-p buffer)
               (kill-buffer buffer)))))"##,
        expect!["OK (nil nil nil nil)"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_read_only_global_find_file_workflow_protects_selected_compiled_buffer_with_view_mode(),
        auto_read_only_global_find_file_workflow_leaves_unmatched_source_editable(),
        auto_read_only_default_user_emacs_directory_pattern_protects_installed_package_source(),
        auto_read_only_project_suppression_defers_protection_until_buffer_is_outside_project(),
        auto_read_only_custom_read_only_mode_blocks_edits_then_allows_them_after_manual_unlock(),
        auto_read_only_disabling_global_mode_stops_subsequent_find_file_hook_protection(),
    ]
}
