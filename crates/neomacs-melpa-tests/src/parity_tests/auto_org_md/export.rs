use expect_test::expect;

use super::ParityBatchCase;

fn auto_org_md_export_skips_fundamental_mode_without_touching_exporter() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_skips_fundamental_mode_without_touching_exporter",
        r##"(let (calls)
         (cl-letf (((symbol-function 'org-md-export-to-markdown)
                    (lambda (&rest arguments)
                      (push arguments calls)
                      :exported)))
           (with-temp-buffer
             (fundamental-mode)
             (list
              (auto-org-md-export)
              calls
              major-mode
              (buffer-string)))))"##,
        expect![[r#"OK (nil nil fundamental-mode "")"#]],
    )
}

fn auto_org_md_export_skips_text_and_markdown_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_skips_text_and_markdown_modes",
        r##"(progn
         (define-derived-mode
           auto-org-md-test-markdown-mode
           text-mode
           "AutoOrgMarkdownFixture")
         (let (calls)
         (cl-letf (((symbol-function 'org-md-export-to-markdown)
                    (lambda (&rest arguments)
                      (push arguments calls)
                      :exported)))
           (mapcar
            (lambda (mode)
              (with-temp-buffer
                (funcall mode)
                (list mode
                      (auto-org-md-export)
                      calls)))
            '(text-mode
              auto-org-md-test-markdown-mode)))))"##,
        expect!["OK ((text-mode nil nil) (auto-org-md-test-markdown-mode nil nil))"],
    )
}

fn auto_org_md_export_invokes_markdown_exporter_once_in_org_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_invokes_markdown_exporter_once_in_org_mode",
        r##"(let (calls)
         (cl-letf (((symbol-function 'org-md-export-to-markdown)
                    (lambda (&rest arguments)
                      (push arguments calls)
                      :exported)))
           (with-temp-buffer
             (org-mode)
             (insert "* Heading\nBody")
             (list
              (auto-org-md-export)
              (nreverse calls)
              (buffer-string)
              major-mode))))"##,
        expect![[r#"OK (:exported (nil) "* Heading\nBody" org-mode)"#]],
    )
}

fn auto_org_md_export_recognizes_modes_derived_from_org_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_recognizes_modes_derived_from_org_mode",
        r##"(progn
         (define-derived-mode
           auto-org-md-test-derived-mode
           org-mode
           "AutoOrgFixture")
         (let (calls)
           (cl-letf (((symbol-function 'org-md-export-to-markdown)
                      (lambda (&rest arguments)
                        (push arguments calls)
                        :derived-export)))
             (with-temp-buffer
               (auto-org-md-test-derived-mode)
               (list
                (derived-mode-p 'org-mode)
                (auto-org-md-export)
                (nreverse calls)
                major-mode)))))"##,
        expect!["OK (org-mode :derived-export (nil) auto-org-md-test-derived-mode)"],
    )
}

fn auto_org_md_export_forwards_exporter_return_values_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_forwards_exporter_return_values_exactly",
        r##"(mapcar
         (lambda (value)
           (cl-letf (((symbol-function
                       'org-md-export-to-markdown)
                      (lambda () value)))
             (with-temp-buffer
               (org-mode)
               (list value
                     (auto-org-md-export)))))
         '(nil t :file "notes.md" 42 (nested value)))"##,
        expect![[
            r#"OK ((nil nil) (t t) (:file :file) ("notes.md" "notes.md") (42 42) (#1=(nested value) #1#))"#
        ]],
    )
}

fn auto_org_md_export_propagates_exporter_error_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_propagates_exporter_error_data",
        r##"(with-temp-buffer
         (org-mode)
         (cl-letf (((symbol-function
                     'org-md-export-to-markdown)
                    (lambda ()
                      (signal
                       'file-error
                       '("fixture export failed"
                         "notes.org")))))
           (auto-org-md-test-error
            #'auto-org-md-export)))"##,
        expect![[r#"OK (:signal file-error ("fixture export failed" "notes.org"))"#]],
    )
}

fn auto_org_md_export_calls_exporter_without_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_calls_exporter_without_arguments",
        r##"(with-temp-buffer
         (org-mode)
         (let (observed)
           (cl-letf (((symbol-function
                       'org-md-export-to-markdown)
                      (lambda (&rest arguments)
                        (setq observed arguments)
                        :done)))
             (list
              (auto-org-md-export)
              observed))))"##,
        expect!["OK (:done nil)"],
    )
}

fn auto_org_md_export_preserves_current_buffer_context_for_exporter() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_preserves_current_buffer_context_for_exporter",
        r##"(with-temp-buffer
         (org-mode)
         (insert "* First\nBody\n* Second\nTail\n")
         (setq buffer-file-name
               (expand-file-name
                "context.org"
                (auto-org-md-test-root "export-context")))
         (goto-char 10)
         (narrow-to-region 9 18)
         (set-buffer-modified-p t)
         (cl-letf (((symbol-function
                     'org-md-export-to-markdown)
                    (lambda ()
                      (list
                       major-mode
                       (point)
                       (point-min)
                       (point-max)
                       (buffer-modified-p)
                       (file-name-nondirectory
                        buffer-file-name)))))
           (list
            (auto-org-md-export)
            (point)
            (point-min)
            (point-max)
            (buffer-modified-p))))"##,
        expect![[r#"OK ((org-mode 10 9 18 t "context.org") 10 9 18 t)"#]],
    )
}

fn auto_org_md_export_uses_runtime_derived_mode_predicate() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_export_uses_runtime_derived_mode_predicate",
        r##"(let (calls)
         (cl-letf (((symbol-function 'derived-mode-p)
                    (lambda (&rest parents)
                      (push parents calls)
                      :fixture-derived))
                   ((symbol-function 'org-md-export-to-markdown)
                    (lambda ()
                      :exported)))
           (with-temp-buffer
             (fundamental-mode)
             (list
              (auto-org-md-export)
              (nreverse calls)))))"##,
        expect!["OK (:exported ((org-mode)))"],
    )
}

fn auto_org_md_export_is_noninteractive_but_callable_via_funcall_interactively() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_org_md_export_is_noninteractive_but_callable_via_funcall_interactively",
        r##"(with-temp-buffer
         (org-mode)
         (cl-letf (((symbol-function
                     'org-md-export-to-markdown)
                    (lambda ()
                      (list
                       (called-interactively-p 'any)
                       this-command
                       real-this-command))))
           (list
            (commandp 'auto-org-md-export)
            (funcall-interactively
             #'auto-org-md-export))))"##,
        expect!["OK (nil (nil nil nil))"],
    )
}

pub(super) fn export_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_org_md_export_skips_fundamental_mode_without_touching_exporter(),
        auto_org_md_export_skips_text_and_markdown_modes(),
        auto_org_md_export_invokes_markdown_exporter_once_in_org_mode(),
        auto_org_md_export_recognizes_modes_derived_from_org_mode(),
        auto_org_md_export_forwards_exporter_return_values_exactly(),
        auto_org_md_export_propagates_exporter_error_data(),
        auto_org_md_export_calls_exporter_without_arguments(),
        auto_org_md_export_preserves_current_buffer_context_for_exporter(),
        auto_org_md_export_uses_runtime_derived_mode_predicate(),
        auto_org_md_export_is_noninteractive_but_callable_via_funcall_interactively(),
    ]
}
