use expect_test::expect;

use super::ParityBatchCase;

fn auto_org_md_practical_save_hook_exports_org_buffer_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_practical_save_hook_exports_org_buffer_once",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "save-hook"))
         (org-file (expand-file-name "notes.org" root))
         calls)
         (with-temp-file org-file
           (insert "* Initial\n"))
         (let ((buffer (find-file-noselect org-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (cl-letf (((symbol-function
                             'org-md-export-to-markdown)
                            (lambda ()
                              (push
                               (list
                                (file-name-nondirectory
                                 buffer-file-name)
                                (buffer-string)
                                (buffer-modified-p))
                               calls)
                              "notes.md"))
                           ((symbol-function 'message)
                            (lambda (&rest _arguments)
                              nil)))
                   (auto-org-md-test-reset-state)
                   (auto-org-md-mode 1)
                   (goto-char (point-max))
                   (insert "Body\n")
                   (save-buffer)
                   (list
                    (nreverse calls)
                    (buffer-modified-p)
                    (memq 'auto-org-md-export
                          after-save-hook)
                    (auto-org-md-test-read-file
                     org-file))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((("notes.org" #("* Initial\nBody\n" 0 10 (fontified nil) 10 15 (fontified nil)) nil)) nil (auto-org-md-export t) "* Initial\nBody\n")"#
        ]],
    )
}

fn auto_org_md_save_hook_in_non_org_buffer_is_a_noop() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_save_hook_in_non_org_buffer_is_a_noop",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "text-save"))
         (text-file
          (expand-file-name "notes.txt" root))
         calls)
         (with-temp-file text-file
           (insert "Initial\n"))
         (let ((buffer (find-file-noselect text-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (text-mode)
                 (add-hook
                  'after-save-hook
                  #'auto-org-md-export
                  nil t)
                 (cl-letf (((symbol-function
                             'org-md-export-to-markdown)
                            (lambda ()
                              (push :export calls))))
                   (goto-char (point-max))
                   (insert "Body\n")
                   (save-buffer)
                   (list
                    calls
                    (buffer-modified-p)
                    (auto-org-md-test-read-file
                     text-file))))
             (kill-buffer buffer))))"##,
        expect![[r#"OK (nil nil "Initial\nBody\n")"#]],
    )
}

fn auto_org_md_disabling_mode_stops_future_save_exports() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_disabling_mode_stops_future_save_exports",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "disable-save"))
         (org-file (expand-file-name "notes.org" root))
         calls)
         (with-temp-file org-file
           (insert "* Initial\n"))
         (let ((buffer (find-file-noselect org-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (cl-letf (((symbol-function
                             'org-md-export-to-markdown)
                            (lambda ()
                              (push (buffer-string) calls)
                              "notes.md"))
                           ((symbol-function 'message)
                            (lambda (&rest _arguments)
                              nil)))
                   (auto-org-md-on)
                   (goto-char (point-max))
                   (insert "First\n")
                   (save-buffer)
                   (auto-org-md-off)
                   (goto-char (point-max))
                   (insert "Second\n")
                   (save-buffer)
                   (list
                    (nreverse calls)
                    (memq 'auto-org-md-export
                          after-save-hook)
                    (auto-org-md-test-read-file
                     org-file))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((#("* Initial\nFirst\n" 0 10 (fontified nil) 10 16 (fontified nil))) nil "* Initial\nFirst\nSecond\n")"#
        ]],
    )
}

fn auto_org_md_real_export_writes_simple_markdown_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_real_export_writes_simple_markdown_document",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "real-simple"))
         (org-file (expand-file-name "guide.org" root))
         (md-file (expand-file-name "guide.md" root)))
         (with-temp-file org-file
           (insert
            "#+TITLE: Practical Guide\n"
            "* Overview\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: overview\n"
            ":END:\n"
            "Plain body with *bold* and /italic/ text.\n"
            "** Tasks\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: tasks\n"
            ":END:\n"
            "- [X] Export the document\n"
            "- [ ] Review the result\n"))
         (let ((buffer (find-file-noselect org-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (list
                  (file-name-nondirectory
                   (auto-org-md-export))
                  (file-exists-p md-file)
                  (auto-org-md-test-read-file
                   md-file)
                  (buffer-modified-p)))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("guide.md" t "\n# Table of Contents\n\n1.  [Overview](#overview)\n    1.  [Tasks](#tasks)\n\n\n\n<a id=\"overview\"></a>\n\n# Overview\n\nPlain body with **bold** and *italic* text.\n\n\n<a id=\"tasks\"></a>\n\n## Tasks\n\n-   [X] Export the document\n-   [ ] Review the result\n\n" nil)"#
        ]],
    )
}

fn auto_org_md_real_export_handles_links_source_blocks_and_tables() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_real_export_handles_links_source_blocks_and_tables",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "real-complex"))
         (org-file (expand-file-name "project.org" root))
         (md-file (expand-file-name "project.md" root)))
         (with-temp-file org-file
           (insert
            "#+OPTIONS: toc:nil num:nil\n"
            "* Release\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: release\n"
            ":END:\n"
            "See [[https://example.invalid/spec][the specification]].\n\n"
            "| Item | State |\n"
            "|------+-------|\n"
            "| API  | ready |\n\n"
            "#+begin_src emacs-lisp\n"
            "(message \"ready\")\n"
            "#+end_src\n"))
         (let ((buffer (find-file-noselect org-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (auto-org-md-export)
                 (list
                  (file-exists-p md-file)
                  (auto-org-md-test-read-file
                   md-file)))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (t "\n\n# Release\n\nSee [the specification](https://example.invalid/spec).\n\n<table border=\"2\" cellspacing=\"0\" cellpadding=\"6\" rules=\"groups\" frame=\"hsides\">\n\n\n<colgroup>\n<col  class=\"org-left\" />\n\n<col  class=\"org-left\" />\n</colgroup>\n<thead>\n<tr>\n<th scope=\"col\" class=\"org-left\">Item</th>\n<th scope=\"col\" class=\"org-left\">State</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td class=\"org-left\">API</td>\n<td class=\"org-left\">ready</td>\n</tr>\n</tbody>\n</table>\n\n    (message \"ready\")\n\n")"#
        ]],
    )
}

fn auto_org_md_real_save_replaces_markdown_with_latest_org_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_real_save_replaces_markdown_with_latest_org_content",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "real-update"))
         (org-file (expand-file-name "status.org" root))
         (md-file (expand-file-name "status.md" root)))
         (with-temp-file org-file
           (insert
            "* Status\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: status\n"
            ":END:\n"
            "Version one.\n"))
         (let ((buffer (find-file-noselect org-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (cl-letf (((symbol-function 'message)
                            (lambda (&rest _arguments)
                              nil)))
                   (auto-org-md-on)
                   (goto-char (point-max))
                   (insert "\n** Detail\nFirst export.\n")
                   (goto-char (point-min))
                   (search-forward "** Detail\n")
                   (insert
                    ":PROPERTIES:\n"
                    ":CUSTOM_ID: detail\n"
                    ":END:\n")
                   (goto-char (point-max))
                   (save-buffer)
                   (let ((first
                          (auto-org-md-test-read-file
                           md-file)))
                     (erase-buffer)
                     (insert
                      "* Status\n"
                      ":PROPERTIES:\n"
                      ":CUSTOM_ID: status\n"
                      ":END:\n"
                      "Version two.\n"
                      "- stable\n"
                      "- published\n")
                     (save-buffer)
                     (list
                      first
                      (auto-org-md-test-read-file
                       md-file)
                      (buffer-modified-p)))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("\n# Table of Contents\n\n1.  [Status](#status)\n    1.  [Detail](#detail)\n\n\n<a id=\"status\"></a>\n\n# Status\n\nVersion one.\n\n\n<a id=\"detail\"></a>\n\n## Detail\n\nFirst export.\n\n" "\n# Table of Contents\n\n1.  [Status](#status)\n\n\n<a id=\"status\"></a>\n\n# Status\n\nVersion two.\n\n-   stable\n-   published\n\n" nil)"#
        ]],
    )
}

fn auto_org_md_two_local_hooks_export_independent_org_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_two_local_hooks_export_independent_org_files",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "two-files"))
         (first-org
          (expand-file-name "first.org" root))
         (second-org
          (expand-file-name "second.org" root))
         (first-md
          (expand-file-name "first.md" root))
         (second-md
          (expand-file-name "second.md" root)))
         (with-temp-file first-org
           (insert
            "* First\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: first\n"
            ":END:\n"
            "Alpha.\n"))
         (with-temp-file second-org
           (insert
            "* Second\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: second\n"
            ":END:\n"
            "Beta.\n"))
         (let ((first-buffer
                (find-file-noselect first-org))
               (second-buffer
                (find-file-noselect second-org)))
           (unwind-protect
               (progn
                 (dolist (buffer
                          (list first-buffer
                                second-buffer))
                   (with-current-buffer buffer
                     (org-mode)
                     (cl-letf (((symbol-function
                                 'message)
                                (lambda (&rest _arguments)
                                  nil)))
                       (auto-org-md-on))
                     (goto-char (point-max))
                     (insert "Saved.\n")
                     (save-buffer)))
                 (list
                  (auto-org-md-test-read-file
                   first-md)
                  (auto-org-md-test-read-file
                   second-md)
                  (with-current-buffer first-buffer
                    (memq 'auto-org-md-export
                          after-save-hook))
                  (with-current-buffer second-buffer
                    (memq 'auto-org-md-export
                          after-save-hook))))
             (kill-buffer first-buffer)
             (kill-buffer second-buffer))))"##,
        expect![[
            r#"OK ("\n# Table of Contents\n\n1.  [First](#first)\n\n\n<a id=\"first\"></a>\n\n# First\n\nAlpha.\nSaved.\n\n" "\n# Table of Contents\n\n1.  [Second](#second)\n\n\n<a id=\"second\"></a>\n\n# Second\n\nBeta.\nSaved.\n\n" (auto-org-md-export t) (auto-org-md-export t))"#
        ]],
    )
}

fn auto_org_md_export_follows_renamed_visited_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_follows_renamed_visited_file",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "rename"))
         (draft-org
          (expand-file-name "draft.org" root))
         (draft-md
          (expand-file-name "draft.md" root))
         (final-org
          (expand-file-name "final.org" root))
         (final-md
          (expand-file-name "final.md" root)))
         (with-temp-file draft-org
           (insert
            "* Draft\n"
            ":PROPERTIES:\n"
            ":CUSTOM_ID: draft\n"
            ":END:\n"
            "Original path.\n"))
         (let ((buffer (find-file-noselect draft-org)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (auto-org-md-export)
                 (set-visited-file-name final-org)
                 (goto-char (point-max))
                 (insert "Final path.\n")
                 (save-buffer)
                 (auto-org-md-export)
                 (list
                  (file-exists-p draft-md)
                  (file-exists-p final-org)
                  (file-exists-p final-md)
                  (auto-org-md-test-read-file
                   draft-md)
                  (auto-org-md-test-read-file
                   final-md)))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (t t t "\n# Table of Contents\n\n1.  [Draft](#draft)\n\n\n<a id=\"draft\"></a>\n\n# Draft\n\nOriginal path.\n\n" "\n# Table of Contents\n\n1.  [Draft](#draft)\n\n\n<a id=\"draft\"></a>\n\n# Draft\n\nOriginal path.\nFinal path.\n\n")"#
        ]],
    )
}

fn auto_org_md_unsaved_org_buffer_surfaces_export_filename_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_unsaved_org_buffer_surfaces_export_filename_contract",
        r##"(with-temp-buffer
         (org-mode)
         (insert "* Unsaved\nNo visited file.\n")
         (list
          buffer-file-name
          (auto-org-md-test-error
           #'auto-org-md-export)
          (buffer-string)
          (buffer-modified-p)))"##,
        expect![[
            r#"OK (nil (:signal end-of-file ("Error reading from stdin")) "* Unsaved\nNo visited file.\n" t)"#
        ]],
    )
}

fn auto_org_md_after_save_export_error_preserves_saved_org_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_after_save_export_error_preserves_saved_org_content",
        r##"(let* ((root
                                 (auto-org-md-test-root
                                  "save-error"))
         (org-file (expand-file-name "broken.org" root)))
         (with-temp-file org-file
           (insert "* Before\n"))
         (let ((buffer (find-file-noselect org-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (org-mode)
                 (add-hook
                  'after-save-hook
                  #'auto-org-md-export
                  nil t)
                 (goto-char (point-max))
                 (insert "Saved before export failure.\n")
                 (cl-letf (((symbol-function
                             'org-md-export-to-markdown)
                            (lambda ()
                              (signal
                               'error
                               '("fixture export failed")))))
                   (list
                    (auto-org-md-test-error
                     #'save-buffer)
                    (buffer-modified-p)
                    (auto-org-md-test-read-file
                     org-file))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((:signal error ("fixture export failed")) nil "* Before\nSaved before export failure.\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_org_md_practical_save_hook_exports_org_buffer_once(),
        auto_org_md_save_hook_in_non_org_buffer_is_a_noop(),
        auto_org_md_disabling_mode_stops_future_save_exports(),
        auto_org_md_real_export_writes_simple_markdown_document(),
        auto_org_md_real_export_handles_links_source_blocks_and_tables(),
        auto_org_md_real_save_replaces_markdown_with_latest_org_content(),
        auto_org_md_two_local_hooks_export_independent_org_files(),
        auto_org_md_export_follows_renamed_visited_file(),
        auto_org_md_unsaved_org_buffer_surfaces_export_filename_contract(),
        auto_org_md_after_save_export_error_preserves_saved_org_content(),
    ]
}
