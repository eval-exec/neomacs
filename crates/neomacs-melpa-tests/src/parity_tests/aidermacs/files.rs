use expect_test::expect;

use super::ParityBatchCase;

fn aidermacs_file_command_builder_quotes_localizes_and_handles_empty_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_file_command_builder_quotes_localizes_and_handles_empty_inputs",
        r##"(list
                      (aidermacs--prepare-file-paths-for-command
                       "/add"
                       '("/work/src/main.el" "/work/docs/user guide.md" nil))
                      (aidermacs--prepare-file-paths-for-command "/drop" nil)
                      (aidermacs--localize-tramp-path
                       "/ssh:user@example.test:/srv/app/main.py")
                      (aidermacs--localize-tramp-path "/local/app.el"))"##,
        expect![[
            r#"OK ("/add \"/work/src/main.el\" \"/work/docs/user guide.md\"" "/drop" "/srv/app/main.py" "/local/app.el")"#
        ]],
    )
}

fn aidermacs_ls_parser_tracks_real_editable_and_read_only_files_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_ls_parser_tracks_real_editable_and_read_only_files_in_order",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (default-directory root)
                          (aidermacs--tracked-files nil))
                      (make-directory (expand-file-name ".git" root) t)
                      (make-directory (expand-file-name "src" root) t)
                      (make-directory (expand-file-name "docs" root) t)
                      (with-temp-file (expand-file-name "src/main.el" root)
                        (insert "(message \"hi\")\n"))
                      (with-temp-file (expand-file-name "docs/guide.md" root)
                        (insert "# Guide\n"))
                      (list
                       (aidermacs--parse-ls-output
                        (concat
                         "Read-only files:\n"
                         "  docs/guide.md tokens: 25\n"
                         "  missing.txt\n"
                         "\nFiles in chat:\n"
                         "  src/main.el tokens: 10\n"
                         "  docs/guide.md tokens: 25\n"
                         "  missing.el\n"
                         "\nTokens: 60\n"))
                       aidermacs--tracked-files))"##,
        expect![[r#"OK (("docs/guide.md (read-only)" "src/main.el" "docs/guide.md") nil)"#]],
    )
}

fn aidermacs_output_parser_applies_real_chat_file_state_transitions_and_udiffs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aidermacs_output_parser_applies_real_chat_file_state_transitions_and_udiffs",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (default-directory root)
                          (aidermacs--tracked-files nil))
                      (make-directory (expand-file-name "src/lib" root) t)
                      (dolist (name '("src/main.el" "src/lib/tool.el"
                                      "README.md" "notes.txt"))
                        (with-temp-file (expand-file-name name root)
                          (insert name "\n")))
                      (aidermacs--parse-output-for-files
                       (concat
                        "Added ./src/main.el to the chat.\n"
                        "Added README.md to read-only files.\n"
                        "Applied edit to src/lib/tool.el\n"
                        "--- a/notes.txt\n+++ b/notes.txt\n"
                        "Moved README.md from read-only to editable files in the chat\n"))
                      (let ((after-add (sort (copy-sequence aidermacs--tracked-files)
                                             #'string-lessp)))
                        (aidermacs--parse-output-for-files
                         (concat
                          "Removed src/main.el from the chat\n"
                          "Moved README.md from editable to read-only files in the chat\n"))
                        (list
                         after-add
                         (sort (copy-sequence aidermacs--tracked-files)
                               #'string-lessp)
                         (aidermacs--find-tracked-file
                          (expand-file-name "src/lib/tool.el" root))
                         (aidermacs--find-tracked-file "tool.el")
                         (aidermacs--find-tracked-file "missing.el"))))"##,
        expect![[r#"OK (("README.md") ("README.md (read-only)") nil nil nil)"#]],
    )
    .fresh_process()
}

fn aidermacs_file_tracking_requires_unambiguous_basename_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_file_tracking_requires_unambiguous_basename_matches",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (default-directory root)
                          (aidermacs--tracked-files
                           '("src/main.el" "test/main.el"
                             "docs/guide.md (read-only)"
                             "lib/tool.el")))
                      (make-directory (expand-file-name ".git" root) t)
                      (list
                       (aidermacs--find-tracked-file "src/main.el")
                       (aidermacs--find-tracked-file
                        (expand-file-name "docs/guide.md" root))
                       (aidermacs--find-tracked-file "tool.el")
                       (aidermacs--find-tracked-file "main.el")
                       (aidermacs--find-tracked-file "src/../src/main.el")))"##,
        expect![[
            r#"OK ("src/main.el" "docs/guide.md (read-only)" "lib/tool.el" "src/main.el" "src/main.el")"#
        ]],
    )
}

fn aidermacs_pre_edit_capture_detects_real_changes_and_cleanup_is_idempotent() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_pre_edit_capture_detects_real_changes_and_cleanup_is_idempotent",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (default-directory root)
                          (session (get-buffer-create "*aidermacs:capture*"))
                          (one (expand-file-name "src/one.el" root))
                          (two (expand-file-name "src/two.el" root)))
                      (make-directory (expand-file-name ".git" root) t)
                      (make-directory (file-name-directory one) t)
                      (with-temp-file one (insert "before one\n"))
                      (with-temp-file two (insert "before two\n"))
                      (unwind-protect
                          (cl-letf
                              (((symbol-function 'aidermacs-get-buffer-name)
                                (lambda (&rest _) (buffer-name session))))
                            (with-current-buffer session
                              (setq-local default-directory root)
                              (setq-local aidermacs--tracked-files
                                          '("src/one.el" "src/two.el"
                                            "missing.el" "   "))
                              (setq-local aidermacs--pre-edit-file-buffers nil)
                              (setq-local aidermacs--pre-edit-prepared nil)
                              (aidermacs--prepare-for-code-edit)
                              (let ((captured
                                     (mapcar
                                      (lambda (entry)
                                        (list
                                         (file-relative-name (car entry) root)
                                         (with-current-buffer (cdr entry)
                                           (buffer-string))
                                         (buffer-local-value
                                          'buffer-read-only (cdr entry))))
                                      aidermacs--pre-edit-file-buffers)))
                                (with-temp-file one (insert "after one\n"))
                                (setq-local aidermacs--current-output
                                            "Applied edit to src/one.el\nApplied edit to src/two.el\n")
                                (let ((edited (aidermacs--detect-edited-files)))
                                  (aidermacs--cleanup-temp-buffers)
                                  (list
                                   captured
                                   edited
                                   aidermacs--pre-edit-file-buffers
                                   aidermacs--pre-edit-prepared)))))
                        (when (buffer-live-p session)
                          (kill-buffer session))))"##,
        expect![[
            r#"OK ((("src/one.el" "before one\n" t) ("src/two.el" "before two\n" t)) ("src/one.el" "src/two.el") nil nil)"#
        ]],
    )
}

fn aidermacs_add_and_drop_helpers_build_batched_commands_and_user_messages() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_add_and_drop_helpers_build_batched_commands_and_user_messages",
        r##"(let (commands messages)
                      (cl-letf
                          (((symbol-function 'aidermacs--send-command)
                            (lambda (command &rest _)
                              (push command commands)))
                           ((symbol-function 'message)
                            (lambda (format-string &rest arguments)
                              (push
                               (apply #'format format-string arguments)
                               messages))))
                        (aidermacs--add-files-helper
                         '("/repo/src/a.el" "/repo/src/b b.el") nil)
                        (aidermacs--add-files-helper
                         '("/repo/README.md") t "Reference added")
                        (aidermacs--add-files-helper '(nil) nil)
                        (aidermacs--drop-files-helper
                         '("/repo/src/a.el" "/repo/src/b b.el"))
                        (aidermacs--drop-files-helper nil)
                        (list (nreverse commands) (nreverse messages))))"##,
        expect![[
            r#"OK (("/add \"/repo/src/a.el\" \"/repo/src/b b.el\"" "/read-only \"/repo/README.md\"" "/drop \"/repo/src/a.el\" \"/repo/src/b b.el\"") ("Added 2 files as editable" "Reference added" "No files to add." "Dropped 2 files" "No files to drop."))"#
        ]],
    )
}

pub(super) fn files_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidermacs_file_command_builder_quotes_localizes_and_handles_empty_inputs(),
        aidermacs_ls_parser_tracks_real_editable_and_read_only_files_in_order(),
        aidermacs_output_parser_applies_real_chat_file_state_transitions_and_udiffs(),
        aidermacs_file_tracking_requires_unambiguous_basename_matches(),
        aidermacs_pre_edit_capture_detects_real_changes_and_cleanup_is_idempotent(),
        aidermacs_add_and_drop_helpers_build_batched_commands_and_user_messages(),
    ]
}
