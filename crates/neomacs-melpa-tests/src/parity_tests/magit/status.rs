use expect_test::expect;

use super::ParityBatchCase;

fn magit_status_sections_track_unicode_spaced_and_plain_files_across_states() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_status_sections_track_unicode_spaced_and_plain_files_across_states",
        r##"(let* ((root (make-temp-file "magit-status-files-" t))
                    (default-directory (file-name-as-directory root))
                    status-buffer)
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (cl-labels
                         ((modify
                           (file content)
                           (with-temp-file file (insert content)))
                          (section-has-file
                           (kind file)
                           (setq status-buffer
                                 (magit-status-setup-buffer
                                  default-directory))
                           (with-current-buffer status-buffer
                             (magit-section-show-level-4-all)
                             (and
                              (seq-find
                               (lambda (section)
                                 (equal (oref section value) file))
                               (oref
                                (magit-get-section
                                 `((,kind) (status)))
                                children))
                              t))))
                       (dolist
                           (file
                            '("plain"
                              "file with space"
                              "file with äöüéλ"))
                         (modify file "untracked\n"))
                       (let ((untracked
                              (mapcar
                               (lambda (file)
                                 (section-has-file
                                  'untracked file))
                               '("plain"
                                 "file with space"
                                 "file with äöüéλ"))))
                         (magit-git "add" ".")
                         (let ((staged
                                (mapcar
                                 (lambda (file)
                                   (section-has-file
                                    'staged file))
                                 '("plain"
                                   "file with space"
                                   "file with äöüéλ"))))
                           (dolist
                               (file
                                '("plain"
                                  "file with space"
                                  "file with äöüéλ"))
                             (modify file "modified\n"))
                           (list
                            untracked
                            staged
                            (mapcar
                             (lambda (file)
                               (section-has-file
                                'unstaged file))
                             '("plain"
                               "file with space"
                               "file with äöüéλ")))))))
                 (when (buffer-live-p status-buffer)
                   (kill-buffer status-buffer))
                 (delete-directory root t)))"##,
        expect![[r#"OK ((t t t) (t t t) (t t t))"#]],
    )
}

fn magit_status_section_visibility_commands_preserve_exact_visible_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_status_section_visibility_commands_preserve_exact_visible_text",
        r##"(let* ((root (make-temp-file "magit-status-text-" t))
                    (default-directory (file-name-as-directory root))
                    status-buffer)
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (magit-git
                      "commit" "-m" "dummy" "--allow-empty")
                     (setq status-buffer
                           (magit-status-setup-buffer
                            default-directory))
                     (with-current-buffer status-buffer
                       (cl-labels
                           ((visible-text
                             ()
                             (save-excursion
                               (let (chunks)
                                 (goto-char (point-min))
                                 (while
                                     (let
                                         ((to
                                           (next-single-char-property-change
                                            (point)
                                            'invisible)))
                                       (unless (invisible-p (point))
                                         (push
                                          (buffer-substring-no-properties
                                           (point) to)
                                          chunks))
                                       (goto-char to)
                                       (< (point) (point-max))))
                                 (replace-regexp-in-string
                                  "\\b[[:xdigit:]]\\{7,\\}\\b"
                                  "<HASH>"
                                  (string-trim
                                   (string-join
                                    (nreverse chunks))))))))
                         (magit-section-show-level-1-all)
                         (let ((level-one (visible-text)))
                           (magit-section-show-level-2-all)
                           (let ((level-two (visible-text)))
                             (goto-char (point-min))
                             (search-forward "Recent")
                             (magit-section-show-level-1)
                             (list
                              level-one
                              level-two
                              (visible-text)))))))
                 (when (buffer-live-p status-buffer)
                   (kill-buffer status-buffer))
                 (delete-directory root t)))"##,
        expect![[
            r#"OK ("Head:     master dummy\n\nRecent commits" "Head:     master dummy\n\nRecent commits\n<HASH> master dummy" "Head:     master dummy\n\nRecent commits")"#
        ]],
    )
}

pub(super) fn status_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        magit_status_sections_track_unicode_spaced_and_plain_files_across_states(),
        magit_status_section_visibility_commands_preserve_exact_visible_text(),
    ]
}
