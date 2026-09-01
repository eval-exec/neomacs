use expect_test::expect;

use super::ParityBatchCase;

fn git_commit_mode_toggles_keymap_state_and_remains_permanent_local() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_mode_toggles_keymap_state_and_remains_permanent_local",
        r##"(with-temp-buffer
               (let ((initial-major-mode major-mode))
                 (git-commit-mode 1)
                 (let ((enabled
                        (list
                         git-commit-mode
                         (key-binding (kbd "C-c C-i"))
                         (key-binding (kbd "M-p"))
                         (get 'git-commit-mode 'permanent-local)
                         (eq major-mode initial-major-mode))))
                   (git-commit-mode -1)
                   (list enabled git-commit-mode))))"##,
        expect![[r#"OK ((t git-commit-insert-trailer git-commit-prev-message t t) nil)"#]],
    )
}

fn git_commit_setup_changelog_support_sets_buffer_local_fill_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_setup_changelog_support_sets_buffer_local_fill_contract",
        r##"(with-temp-buffer
               (let ((global-fill-paragraph-function
                      (default-value 'fill-paragraph-function)))
                 (git-commit-setup-changelog-support)
                 (list
                  (eq fill-paragraph-function
                      #'log-edit-fill-entry)
                  fill-indent-according-to-mode
                  (string-match-p "\\\\|\\\\\\*" paragraph-start)
                  (local-variable-p 'fill-paragraph-function)
                  (equal
                   (default-value 'fill-paragraph-function)
                   global-fill-paragraph-function))))"##,
        expect![[r#"OK (t t 9 t t)"#]],
    )
}

fn git_commit_auto_fill_skips_summary_but_wraps_body_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_auto_fill_skips_summary_but_wraps_body_lines",
        r##"(with-temp-buffer
               (text-mode)
               (setq fill-column 12
                     git-commit-need-summary-line t)
               (git-commit-setup-auto-fill)
               (insert "summary words stay together")
               (funcall auto-fill-function)
               (insert "\n\nbody words should wrap here")
               (funcall auto-fill-function)
               (list
                auto-fill-function
                (buffer-string)
                (local-variable-p 'auto-fill-function)))"##,
        expect![[
            r#"OK (git-commit--auto-fill-except-summary "summary words stay together\n\nbody words\nshould wrap\nhere" t)"#
        ]],
    )
}

fn git_commit_collapse_diff_installs_a_toggle_button_and_invisible_overlay() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_collapse_diff_installs_a_toggle_button_and_invisible_overlay",
        r##"(with-temp-buffer
               (setq-local comment-start "#")
               (setq-local buffer-invisibility-spec nil)
               (insert
                "Summary\n\n"
                "# ------------------------ >8 ------------------------\n"
                "diff --git a/a b/a\n-old\n+new\n")
               (git-commit-collapse-diff)
               (goto-char (point-min))
               (re-search-forward ">8")
               (let* ((button (button-at (1- (point))))
                      (overlay
                       (car
                        (overlays-at
                         (save-excursion
                           (forward-line 1)
                           (point)))))
                      (before
                       (list
                        (and button t)
                        (copy-tree buffer-invisibility-spec)
                        (and overlay
                             (overlay-get overlay 'invisible)))))
                 (button-activate button)
                 (list
                  before
                  buffer-invisibility-spec
                  (and overlay
                       (overlay-get overlay 'invisible)))))"##,
        expect![[r#"OK ((t ((git-commit-diff t)) git-commit-diff) t git-commit-diff)"#]],
    )
}

fn git_commit_setup_font_lock_configures_comment_syntax_and_exact_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_setup_font_lock_configures_comment_syntax_and_exact_faces",
        r##"(let* ((root (make-temp-file "git-commit-font-" t))
                    (default-directory (file-name-as-directory root)))
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (with-temp-buffer
                       (text-mode)
                       (insert
                        "A summary that is too long\n"
                        "nonempty second line\n"
                        "# Changes to be committed:\n"
                        "#\tmodified: tracked.el\n"
                        "Signed-off-by: A U Thor <author@example.test>\n")
                       (setq git-commit-summary-max-length 10)
                       (git-commit-setup-font-lock)
                       (font-lock-ensure)
                       (list
                        comment-start
                        comment-start-skip
                        font-lock-multiline
                        (and
                         (memq
                          #'git-commit-extend-region-summary-line
                          font-lock-extend-region-functions)
                         t)
                        (get-text-property 2 'face)
                        (get-text-property 12 'face)
                        (progn
                          (goto-char (point-min))
                          (forward-line 1)
                          (get-text-property (point) 'face))
                        (progn
                          (re-search-forward "Signed-off-by")
                          (get-text-property (match-beginning 0)
                                             'face)))))
                 (delete-directory root t)))"##,
        expect![[
            r##"OK ("#" "^#+[ \11]*" t t git-commit-summary git-commit-overlong-summary git-commit-nonempty-second-line git-commit-trailer-token)"##
        ]],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        git_commit_mode_toggles_keymap_state_and_remains_permanent_local(),
        git_commit_setup_changelog_support_sets_buffer_local_fill_contract(),
        git_commit_auto_fill_skips_summary_but_wraps_body_lines(),
        git_commit_collapse_diff_installs_a_toggle_button_and_invisible_overlay(),
        git_commit_setup_font_lock_configures_comment_syntax_and_exact_faces(),
    ]
}
