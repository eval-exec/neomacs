use expect_test::expect;

use super::ParityBatchCase;

fn icon_formatter_preserves_the_real_dependency_glyph_and_rebuilds_color_and_geometry_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "icon_formatter_preserves_the_real_dependency_glyph_and_rebuilds_color_and_geometry_properties",
        r##"(let* ((icon
                     (all-the-icons-faicon
                      "cog"
                      :face 'all-the-icons-blue
                      :height 0.95
                      :v-adjust -0.05))
                    (all-the-icons-ivy-rich-icon-size 1.25)
                    (all-the-icons-ivy-rich-color-icon t)
                    (colored
                     (all-the-icons-ivy-rich--format-icon icon))
                    (all-the-icons-ivy-rich-color-icon nil)
                    (plain
                     (all-the-icons-ivy-rich--format-icon icon)))
               (list
                (list
                 (substring-no-properties icon)
                 (string-to-list
                  (substring-no-properties icon))
                 (get-text-property 0 'face icon)
                 (get-text-property 0 'display icon))
                (list
                 (substring-no-properties colored)
                 (get-text-property 0 'display colored)
                 (get-text-property 1 'face colored)
                 (get-text-property 1 'display colored))
                (list
                 (substring-no-properties plain)
                 (get-text-property 0 'display plain)
                 (get-text-property 1 'face plain)
                 (get-text-property 1 'display plain))))"##,
        expect![[
            r#"OK (("" (61459) (:family "FontAwesome" :height 1.14 :inherit all-the-icons-blue) #1=(raise -0.06)) (" " #2=((space :relative-width 0.1)) (:inherit all-the-icons-blue :family "FontAwesome" :height 1.25) #1#) (" " #2# (:inherit all-the-icons-ivy-rich-icon-face :family "FontAwesome" :height 1.25) #1#))"#
        ]],
    )
}

fn real_all_the_icons_file_lookup_renders_directory_source_document_and_fallback_candidates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "real_all_the_icons_file_lookup_renders_directory_source_document_and_fallback_candidates",
        r##"(progn
               (require 'cl-lib)
               (cl-letf
                   (((symbol-function 'display-graphic-p)
                     (lambda (&optional _frame) t)))
                 (mapcar
                  (lambda (candidate)
                    (let ((icon
                           (all-the-icons-ivy-rich-file-icon
                            candidate)))
                      (list
                       candidate
                       (substring-no-properties icon)
                       (string-to-list
                        (substring-no-properties icon))
                       (get-text-property 1 'face icon)
                       (get-text-property 1 'display icon))))
                  '("src/"
                    "main.rs"
                    "init.el"
                    "README.md"
                    "archive.unknown-extension"
                    ""))))"##,
        expect![[
            r#"OK (("src/" " " (32 61462) (:inherit all-the-icons-ivy-rich-dir-face :family "github-octicons" :height 1.0) #1=(raise 0.0)) ("main.rs" " " (32 59692) (:inherit all-the-icons-maroon :family "all-the-icons" :height 1.0) #1#) ("init.el" " " (32 59686) (:inherit all-the-icons-purple :family "file-icons" :height 1.0) #1#) ("README.md" " " (32 61447) (:inherit all-the-icons-lcyan :family "github-octicons" :height 1.0) #1#) ("archive.unknown-extension" " " (32 61462) (:inherit all-the-icons-dsilver :family "FontAwesome" :height 1.0) #1#) ("" " " (32 61462) (:inherit all-the-icons-dsilver :family "FontAwesome" :height 1.0) (raise 0.0)))"#
        ]],
    )
}

fn installed_symbol_transformer_renders_real_command_and_custom_variable_candidates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "installed_symbol_transformer_renders_real_command_and_custom_variable_candidates",
        r##"(progn
               (require 'cl-lib)
               (defun all-the-icons-ivy-rich-workflow-command
                   (path &optional force)
                 "Open PATH and optionally FORCE the operation."
                 (interactive "fPath: \nP"))
               (defcustom all-the-icons-ivy-rich-workflow-option 17
                 "Number of entries rendered by the workflow."
                 :type 'integer)
               (let (rendered)
                 (unwind-protect
                     (cl-letf
                       (((symbol-function 'display-graphic-p)
                           (lambda (&optional _frame) t)))
                       (all-the-icons-ivy-rich-mode 1)
                       (let* ((configuration
                               (cadr
                                (memq
                                 'counsel-describe-symbol
                                 ivy-rich-display-transformers-list)))
                              (transformer
                               (ivy-rich-build-transformer
                                'counsel-describe-symbol
                                configuration)))
                         (setq
                          rendered
                          (mapcar
                           (lambda (candidate)
                             (let* ((line
                                     (funcall transformer candidate))
                                    (fields
                                     (split-string line "\t")))
                               (mapcar
                                (lambda (field)
                                  (list
                                   (string-trim-right
                                    (substring-no-properties field))
                                   (get-text-property
                                    (if (> (length field) 1) 1 0)
                                    'face
                                    field)))
                                fields)))
                           '("all-the-icons-ivy-rich-workflow-command"
                             "all-the-icons-ivy-rich-workflow-option")))))
                   (when all-the-icons-ivy-rich-mode
                     (all-the-icons-ivy-rich-mode -1)))
                 rendered))"##,
        expect![[
            r#"OK (((" " (:inherit all-the-icons-blue :family "FontAwesome" :height 1.0)) ("all-the-icons-ivy-rich-…" nil) ("c" all-the-icons-ivy-rich-type-face) ("Open PATH and optionally FORCE the operation." all-the-icons-ivy-rich-doc-face)) ((" " (:inherit all-the-icons-lblue :family "FontAwesome" :height 1.0)) ("all-the-icons-ivy-rich-…" nil) ("u" all-the-icons-ivy-rich-type-face) ("Number of entries rendered by the workflow." all-the-icons-ivy-rich-doc-face)))"#
        ]],
    )
}

fn dynamic_function_variable_symbol_and_imenu_icons_follow_real_candidate_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "dynamic_function_variable_symbol_and_imenu_icons_follow_real_candidate_semantics",
        r##"(progn
               (require 'cl-lib)
               (defun all-the-icons-ivy-rich-icon-command ()
                 (interactive))
               (defun all-the-icons-ivy-rich-icon-function ())
               (defcustom all-the-icons-ivy-rich-icon-custom t
                 "Fixture."
                 :type 'boolean)
               (defvar all-the-icons-ivy-rich-icon-variable t)
               (defface all-the-icons-ivy-rich-icon-face-fixture
                 '((t :inherit default))
                 "Fixture.")
               (cl-letf
                   (((symbol-function 'display-graphic-p)
                     (lambda (&optional _frame) t)))
                 (mapcar
                  (lambda (entry)
                    (let ((icon
                           (funcall
                            (car entry)
                            (cdr entry))))
                      (list
                       (car entry)
                       (cdr entry)
                       (substring-no-properties icon)
                       (get-text-property 1 'face icon))))
                  '((all-the-icons-ivy-rich-function-icon
                     . "all-the-icons-ivy-rich-icon-command")
                    (all-the-icons-ivy-rich-function-icon
                     . "all-the-icons-ivy-rich-icon-function")
                    (all-the-icons-ivy-rich-variable-icon
                     . "all-the-icons-ivy-rich-icon-custom")
                    (all-the-icons-ivy-rich-variable-icon
                     . "all-the-icons-ivy-rich-icon-variable")
                    (all-the-icons-ivy-rich-symbol-icon
                     . "all-the-icons-ivy-rich-icon-face-fixture")
                    (all-the-icons-ivy-rich-symbol-icon
                     . "Packages: fixture")
                    (all-the-icons-ivy-rich-imenu-icon
                     . "Functions: all-the-icons-ivy-rich-icon-function")
                    (all-the-icons-ivy-rich-imenu-icon
                     . "Variables: all-the-icons-ivy-rich-icon-variable")))))"##,
        expect![[
            r#"OK ((all-the-icons-ivy-rich-function-icon "all-the-icons-ivy-rich-icon-command" " " (:inherit all-the-icons-blue :family "FontAwesome" :height 1.0)) (all-the-icons-ivy-rich-function-icon "all-the-icons-ivy-rich-icon-function" " " (:inherit all-the-icons-purple :family "FontAwesome" :height 1.0)) (all-the-icons-ivy-rich-variable-icon "all-the-icons-ivy-rich-icon-custom" " " (:inherit all-the-icons-lblue :family "FontAwesome" :height 1.0)) (all-the-icons-ivy-rich-variable-icon "all-the-icons-ivy-rich-icon-variable" " " (:inherit all-the-icons-lblue :family "github-octicons" :height 1.0)) (all-the-icons-ivy-rich-symbol-icon "all-the-icons-ivy-rich-icon-face-fixture" " " (:inherit all-the-icons-blue :family "Material Icons" :height 1.0)) (all-the-icons-ivy-rich-symbol-icon "Packages: fixture" " " (:inherit all-the-icons-silver :family "FontAwesome" :height 1.0)) (all-the-icons-ivy-rich-imenu-icon "Functions: all-the-icons-ivy-rich-icon-function" " " (:inherit all-the-icons-purple :family "FontAwesome" :height 1.0)) (all-the-icons-ivy-rich-imenu-icon "Variables: all-the-icons-ivy-rich-icon-variable" " " (:inherit all-the-icons-lblue :family "github-octicons" :height 1.0)))"#
        ]],
    )
}

fn bookmark_icons_distinguish_real_file_directory_and_missing_targets() -> ParityBatchCase {
    ParityBatchCase::value(
        "bookmark_icons_distinguish_real_file_directory_and_missing_targets",
        r##"(progn
               (require 'cl-lib)
               (let* ((root
                       (file-name-as-directory
                        (expand-file-name
                         "all-the-icons-ivy-rich-icon-bookmarks"
                         (getenv "TMPDIR"))))
                      (file
                       (expand-file-name "notes.md" root))
                      (bookmark-alist
                       `(("file" (filename . ,file))
                         ("directory" (filename . ,root))
                         ("missing"
                          (filename
                           . ,(expand-file-name
                              "missing.el" root))))))
                 (unwind-protect
                     (progn
                       (when (file-exists-p root)
                         (delete-directory root t))
                       (make-directory root t)
                       (with-temp-file file
                         (insert "# Notes\n"))
                       (cl-letf
                           (((symbol-function 'display-graphic-p)
                             (lambda (&optional _frame) t)))
                         (mapcar
                          (lambda (candidate)
                            (let ((icon
                                   (all-the-icons-ivy-rich-bookmark-icon
                                    candidate)))
                              (list
                               candidate
                               (substring-no-properties icon)
                               (string-to-list
                                (substring-no-properties icon))
                               (get-text-property 1 'face icon)
                               (all-the-icons-ivy-rich-bookmark-filename
                                candidate))))
                          '("file"
                            "directory"
                            "missing"))))
                   (when (file-exists-p root)
                     (delete-directory root t)))))"##,
        expect![[
            r#"OK (("file" " " (32 61641) (:inherit all-the-icons-lblue :family "github-octicons" :height 1.0) "[ORACLE-TMPDIR]/all-the-icons-ivy-rich-icon-bookmarks/notes.md") ("directory" " " (32 61462) (:inherit (:family "github-octicons" :height 1.08) :family "github-octicons" :height 1.0) "[ORACLE-TMPDIR]/all-the-icons-ivy-rich-icon-bookmarks/") ("missing" " " (32 57675) (:inherit all-the-icons-ivy-rich-error-face :family "Material Icons" :height 1.0) "[ORACLE-TMPDIR]/all-the-icons-ivy-rich-icon-bookmarks/missing.el"))"#
        ]],
    )
}

fn grep_icons_parse_real_line_error_and_non_result_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "grep_icons_parse_real_line_error_and_non_result_candidates",
        r##"(progn
               (require 'cl-lib)
               (cl-letf
                   (((symbol-function 'display-graphic-p)
                     (lambda (&optional _frame) t)))
                 (mapcar
                  (lambda (candidate)
                    (let ((icon
                           (all-the-icons-ivy-rich-grep-file-icon
                            candidate)))
                      (list
                       candidate
                       (and icon
                            (substring-no-properties icon)))))
                  '("notes.md:12:heading"
                    "notes.md:error(failed)"
                    "not-a-result"))))"##,
        expect![[
            r#"OK (("notes.md:12:heading" " ") ("notes.md:error(failed)" " ") ("not-a-result" nil))"#
        ]],
    )
}

fn markdown_link_icons_distinguish_anchor_and_external_link_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "markdown_link_icons_distinguish_anchor_and_external_link_candidates",
        r##"(progn
               (require 'cl-lib)
               (cl-letf
                   (((symbol-function 'display-graphic-p)
                     (lambda (&optional _frame) t)))
                 (mapcar
                  (lambda (candidate)
                    (let ((icon
                           (all-the-icons-ivy-rich-link-icon
                            candidate)))
                      (list
                       candidate
                       (substring-no-properties icon)
                       (get-text-property 1 'face icon))))
                  '("#section"
                    "https://example.invalid/page"))))"##,
        expect![[
            r##"OK (("#section" " " (:inherit all-the-icons-green :family "FontAwesome" :height 1.0)) ("https://example.invalid/page" " " (:inherit all-the-icons-blue :family "Material Icons" :height 1.0)))"##
        ]],
    )
}

fn nongraphical_file_transformer_keeps_useful_metadata_while_suppressing_only_the_icon()
-> ParityBatchCase {
    ParityBatchCase::value(
        "nongraphical_file_transformer_keeps_useful_metadata_while_suppressing_only_the_icon",
        r##"(let* ((root
                     (file-name-as-directory
                      (expand-file-name
                       "all-the-icons-ivy-rich-nongraphical"
                       (getenv "TMPDIR"))))
                    (file (expand-file-name "report.md" root))
                    (ivy--directory root)
                    (ivy-last
                     (make-ivy-state :caller 'counsel-find-file))
                    rendered)
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory root t)
                     (with-temp-file file
                       (insert "release notes\n"))
                     (set-file-modes file #o640)
                     (set-file-times file
                                     (encode-time 0 34 12 2 1 2024 t))
                     (all-the-icons-ivy-rich-mode 1)
                     (let* ((configuration
                             (cadr
                              (memq
                               'counsel-find-file
                               ivy-rich-display-transformers-list)))
                            (transformer
                             (ivy-rich-build-transformer
                              'counsel-find-file
                              configuration))
                            (line
                             (funcall transformer "report.md")))
                       (setq
                        rendered
                        (list
                         (display-graphic-p)
                         (mapcar
                          (lambda (field)
                            (string-trim-right
                             (substring-no-properties field)))
                          (split-string line "\t"))
                         (mapcar
                          (lambda (field)
                            (and (> (length field) 0)
                                 (get-text-property 0 'face field)))
                          (split-string line "\t"))))))
                 (when all-the-icons-ivy-rich-mode
                   (all-the-icons-ivy-rich-mode -1))
                 (when (file-exists-p root)
                   (delete-directory root t)))
               rendered)"##,
        expect![[
            r#"OK (nil ("" "report.md" "" "-rw-r-----" "14" "Jan 02 12:34") (nil nil all-the-icons-ivy-rich-file-owner-face all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-size-face all-the-icons-ivy-rich-time-face))"#
        ]],
    )
}

pub(super) fn icons_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        icon_formatter_preserves_the_real_dependency_glyph_and_rebuilds_color_and_geometry_properties(),
        real_all_the_icons_file_lookup_renders_directory_source_document_and_fallback_candidates(),
        installed_symbol_transformer_renders_real_command_and_custom_variable_candidates(),
        dynamic_function_variable_symbol_and_imenu_icons_follow_real_candidate_semantics(),
        bookmark_icons_distinguish_real_file_directory_and_missing_targets(),
        grep_icons_parse_real_line_error_and_non_result_candidates(),
        markdown_link_icons_distinguish_anchor_and_external_link_candidates(),
        nongraphical_file_transformer_keeps_useful_metadata_while_suppressing_only_the_icon(),
    ]
}
