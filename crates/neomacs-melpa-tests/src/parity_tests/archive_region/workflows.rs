use expect_test::expect;

use super::ParityBatchCase;

fn archive_region_prefix_workflow_moves_repeated_selections_and_opens_the_companion_file()
-> ParityBatchCase {
    ParityBatchCase::value(
        "archive_region_prefix_workflow_moves_repeated_selections_and_opens_the_companion_file",
        r##"(save-window-excursion
  (let* ((source
          (archive-region-test-path
           "project.el"))
         (archive
          (concat source
                  archive-region-filename-suffix))
         (archive-region-date-format
          "[%Y/%m/%d]")
         (dates
          '("[2026/07/27]"
            "[2026/07/28]"))
         received-formats)
    (archive-region-test-cleanup
     source archive)
    (with-temp-file source
      (insert
       "(setq project-name \"Neomacs\")\n"
       "\n"
       ";; (setq obsolete-cache t)\n"
       "(setq current-cache nil)\n"
       ";; (message \"old deploy\")\n"
       "(message \"current deploy\")\n"
       "(message \"draft deploy\")\n"))
    (unwind-protect
        (let ((kill-ring nil)
              (kill-ring-yank-pointer nil)
              normal-cut)
          (find-file source)
          (emacs-lisp-mode)
          (goto-char (point-min))
          (search-forward
           "(message \"draft deploy\")")
          (kill-region-or-archive-region
           1
           (line-beginning-position)
           (progn
             (forward-line 1)
             (point)))
          (setq normal-cut
                (substring-no-properties
                 (current-kill 0 t)))
          (cl-letf
              (((symbol-function 'format-time-string)
                (lambda (format-string &rest _)
                  (unless
                      (equal format-string
                             archive-region-date-format)
                    (error "unexpected date format: %S"
                           format-string))
                  (push format-string
                        received-formats)
                  (or (pop dates)
                      (error "unexpected extra clock read")))))
            (goto-char (point-min))
            (search-forward
             ";; (setq obsolete-cache t)")
            (kill-region-or-archive-region
             4
             (line-beginning-position)
             (progn
               (forward-line 1)
               (point)))
            (goto-char (point-min))
            (search-forward
             ";; (message \"old deploy\")")
            (kill-region-or-archive-region
             4
             (line-beginning-position)
             (progn
               (forward-line 1)
               (point))))
          (save-buffer)
          (let ((source-buffer-content
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max)))
                (source-disk-content
                 (archive-region-test-read-file
                  source))
                (archive-content
                 (archive-region-test-read-file
                  archive))
                (copied-selections
                 (mapcar
                  #'substring-no-properties
                  kill-ring)))
            (kill-region-or-archive-region
             16
             (point-min)
             (point-min))
            (list
             source-buffer-content
             source-disk-content
             archive-content
             normal-cut
             copied-selections
             (file-name-nondirectory
              buffer-file-name)
             (buffer-substring-no-properties
              (point-min)
              (point-max))
             (nreverse received-formats)
             dates)))
      (archive-region-test-cleanup
       source archive))))"##,
        expect![[
            r##"OK ("(setq project-name \"Neomacs\")\n\n(setq current-cache nil)\n(message \"current deploy\")\n" "(setq project-name \"Neomacs\")\n\n(setq current-cache nil)\n(message \"current deploy\")\n" ";; [2026/07/27]\n;; (archive-region-pos \"(setq project-name \\\"Neomacs\\\")\")\n(setq obsolete-cache t)\n\n;; [2026/07/28]\n;; (archive-region-pos \"(setq current-cache nil)\")\n(message \"old deploy\")\n\n" "(message \"draft deploy\")\n" (";; (message \"old deploy\")\n" ";; (setq obsolete-cache t)\n" "(message \"draft deploy\")\n") "project.el_archive" ";; [2026/07/27]\n;; (archive-region-pos \"(setq project-name \\\"Neomacs\\\")\")\n(setq obsolete-cache t)\n\n;; [2026/07/28]\n;; (archive-region-pos \"(setq current-cache nil)\")\n(message \"old deploy\")\n\n" ("[%Y/%m/%d]" "[%Y/%m/%d]") nil)"##
        ]],
    )
}

fn archive_region_navigation_link_supports_a_real_copy_and_restore_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "archive_region_navigation_link_supports_a_real_copy_and_restore_workflow",
        r##"(save-window-excursion
  (let* ((source
          (archive-region-test-path
           "restore.el"))
         (archive
          (concat source
                  archive-region-filename-suffix))
         (archive-region-date-format
          "[%Y/%m/%d]"))
    (archive-region-test-cleanup
     source archive)
    (with-temp-file source
      (insert
       "(setq project 'neomacs)\n"
       "(setq checkpoint 'stable)\n"
       ";; (message \"restore λ\")\n"
       "(setq next-step 'ship)\n"))
    (unwind-protect
        (let ((kill-ring nil)
              (kill-ring-yank-pointer nil))
          (find-file source)
          (emacs-lisp-mode)
          (goto-char (point-min))
          (search-forward
           ";; (message \"restore λ\")")
          (cl-letf
              (((symbol-function 'format-time-string)
                (lambda (format-string &rest _)
                  (unless
                      (equal format-string
                             archive-region-date-format)
                    (error "unexpected date format: %S"
                           format-string))
                  "[2026/07/28]")))
            (kill-region-or-archive-region
             4
             (line-beginning-position)
             (progn
               (forward-line 1)
               (point))))
          (save-buffer)
          (kill-region-or-archive-region
           16
           (point-min)
           (point-min))
          (let ((archive-content
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max)))
                navigation-form
                restored-text)
            (goto-char (point-min))
            (forward-line 1)
            (setq navigation-form
                  (read
                   (substring
                    (buffer-substring-no-properties
                     (line-beginning-position)
                     (line-end-position))
                    3)))
            (forward-line 1)
            (let ((start
                   (line-beginning-position))
                  (end
                   (progn
                     (forward-line 1)
                     (point))))
              (setq restored-text
                    (buffer-substring-no-properties
                     start
                     end))
              (kill-ring-save start end))
            (erase-buffer)
            (save-buffer)
            (eval navigation-form t)
            (let ((navigation-position
                   (list
                    (file-name-nondirectory
                     buffer-file-name)
                    (line-number-at-pos)
                    (buffer-substring-no-properties
                     (line-beginning-position)
                     (line-end-position)))))
              (forward-line 1)
              (yank)
              (save-buffer)
              (list
               archive-content
               navigation-form
               navigation-position
               restored-text
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               (archive-region-test-read-file
                source)
               (archive-region-test-read-file
                archive)))))
      (archive-region-test-cleanup
       source archive))))"##,
        expect![[
            r##"OK (";; [2026/07/28]\n;; (archive-region-pos \"(setq checkpoint 'stable)\")\n(message \"restore λ\")\n\n" (archive-region-pos "(setq checkpoint 'stable)") ("restore.el" 2 "(setq checkpoint 'stable)") "(message \"restore λ\")\n" "(setq project 'neomacs)\n(setq checkpoint 'stable)\n(message \"restore λ\")\n(setq next-step 'ship)\n" "(setq project 'neomacs)\n(setq checkpoint 'stable)\n(message \"restore \316\273\")\n(setq next-step 'ship)\n" "")"##
        ]],
    )
}

fn archive_region_custom_markdown_history_keeps_comment_syntax_suffix_and_date_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "archive_region_custom_markdown_history_keeps_comment_syntax_suffix_and_date_contract",
        r##"(save-window-excursion
  (let* ((source
          (archive-region-test-path
           "release-notes.md"))
         (archive-region-filename-suffix
          ".history.md")
         (archive-region-date-format
          "%Y-%m-%dT%H:%M")
         (archive
          (concat source
                  archive-region-filename-suffix))
         received-format)
    (archive-region-test-cleanup
     source archive)
    (with-temp-file source
      (insert
       "# Release notes\n"
       "\n"
       "<!-- old rollout procedure -->\n"
       "Current rollout procedure\n"))
    (unwind-protect
        (progn
          (find-file source)
          (html-mode)
          (goto-char (point-min))
          (search-forward
           "<!-- old rollout procedure -->")
          (cl-letf
              (((symbol-function 'format-time-string)
                (lambda (format-string &rest _)
                  (setq received-format
                        format-string)
                  (unless
                      (equal format-string
                             archive-region-date-format)
                    (error "unexpected date format: %S"
                           format-string))
                  "2026-07-28T09:30")))
            (kill-region-or-archive-region
             4
             (line-beginning-position)
             (progn
               (forward-line 1)
               (point))))
          (save-buffer)
          (let ((source-content
                 (archive-region-test-read-file
                  source))
                (archive-content
                 (archive-region-test-read-file
                  archive)))
            (kill-region-or-archive-region
             16
             (point-min)
             (point-min))
            (list
             received-format
             source-content
             (file-name-nondirectory
              buffer-file-name)
             archive-content
             (buffer-substring-no-properties
              (point-min)
              (point-max)))))
      (archive-region-test-cleanup
       source archive))))"##,
        expect![[
            r##"OK ("%Y-%m-%dT%H:%M" "# Release notes\n\nCurrent rollout procedure\n" "release-notes.md.history.md" "<!-- 2026-07-28T09:30 -->\n<!-- (archive-region-pos \"# Release notes\") -->\nold rollout procedure\n\n" "<!-- 2026-07-28T09:30 -->\n<!-- (archive-region-pos \"# Release notes\") -->\nold rollout procedure\n\n")"##
        ]],
    )
}

fn archive_region_failed_append_can_be_reverted_and_retried_without_losing_source_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "archive_region_failed_append_can_be_reverted_and_retried_without_losing_source_text",
        r##"(let* ((source
         (archive-region-test-path
          "recovery.el"))
        (archive
         (concat source
                 archive-region-filename-suffix))
        (archive-region-date-format
         "[%Y/%m/%d]")
        (clock-reads 0))
  (archive-region-test-cleanup
   source archive)
  (with-temp-file source
    (insert
     "(setq retained t)\n"
     ";; (message \"archive after recovery\")\n"
     "(setq tail t)\n"))
  (make-directory archive)
  (unwind-protect
      (progn
        (find-file source)
        (emacs-lisp-mode)
        (goto-char (point-min))
        (search-forward
         ";; (message \"archive after recovery\")")
        (let ((start
               (line-beginning-position))
              (end
               (progn
                 (forward-line 1)
                 (point)))
              failure)
          (cl-letf
              (((symbol-function 'format-time-string)
                (lambda (format-string &rest _)
                  (unless
                      (equal format-string
                             archive-region-date-format)
                    (error "unexpected date format: %S"
                           format-string))
                  (setq clock-reads
                        (1+ clock-reads))
                  "[2026/07/28]")))
            (setq failure
                  (condition-case error-data
                      (list
                       :unexpected-success
                       (archive-region start end))
                    (error
                     (list
                      :error
                      (car error-data)
                      (cdr error-data)))))
            (let ((failed-buffer
                   (buffer-substring-no-properties
                    (point-min)
                    (point-max))))
              (revert-buffer :ignore-auto :noconfirm)
              (let ((reverted-buffer
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max))))
                (delete-directory archive)
                (goto-char (point-min))
                (search-forward
                 ";; (message \"archive after recovery\")")
                (archive-region
                 (line-beginning-position)
                 (progn
                   (forward-line 1)
                   (point)))
                (save-buffer)
                (list
                 failure
                 failed-buffer
                 reverted-buffer
                 (archive-region-test-read-file
                  source)
                 (archive-region-test-read-file
                  archive)
                 clock-reads))))))
    (archive-region-test-cleanup
     source archive)))"##,
        expect![[
            r##"OK ((:error file-error ("Opening output file" "Is a directory" "[ORACLE-SANDBOX]/recovery.el_archive")) "(setq retained t)\n;; [2026/07/28]\n;; (archive-region-pos \"(setq retained t)\")\n(message \"archive after recovery\")\n\n(setq tail t)\n" "(setq retained t)\n;; (message \"archive after recovery\")\n(setq tail t)\n" "(setq retained t)\n(setq tail t)\n" ";; [2026/07/28]\n;; (archive-region-pos \"(setq retained t)\")\n(message \"archive after recovery\")\n\n" 2)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        archive_region_prefix_workflow_moves_repeated_selections_and_opens_the_companion_file(),
        archive_region_navigation_link_supports_a_real_copy_and_restore_workflow(),
        archive_region_custom_markdown_history_keeps_comment_syntax_suffix_and_date_contract(),
        archive_region_failed_append_can_be_reverted_and_retried_without_losing_source_text(),
    ]
}
