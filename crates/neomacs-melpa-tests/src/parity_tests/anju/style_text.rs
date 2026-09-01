use expect_test::expect;

use super::ParityBatchCase;

fn anju_org_region_styling_performs_every_supported_real_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_org_region_styling_performs_every_supported_real_edit",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (org-mode)
             (insert "alpha beta")
             (goto-char (point-min))
             (set-mark 6)
             (activate-mark)
             (funcall (car case))
             (list
              (cadr case)
              (buffer-string)
              (point)
              (mark)
              (use-region-p))))
         '((anju-style-bold bold)
           (anju-style-italic italic)
           (anju-style-code code)
           (anju-style-underline underline)
           (anju-style-verbatim verbatim)
           (anju-style-strike-through strike)))"##,
        expect![[
            r#"OK ((bold "*alpha* beta" 8 1 t) (italic "/alpha/ beta" 8 1 t) (code "~alpha~ beta" 8 1 t) (underline "_alpha_ beta" 8 1 t) (verbatim "=alpha= beta" 8 1 t) (strike "+alpha+ beta" 8 1 t))"#
        ]],
    )
}

fn anju_markdown_region_styling_uses_markdown_mode_commands_in_practical_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_markdown_region_styling_uses_markdown_mode_commands_in_practical_text",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (markdown-mode)
             (insert "Deploy alpha-service today.")
             (goto-char 8)
             (set-mark 21)
             (activate-mark)
             (funcall (car case))
             (list
              (cadr case)
              (buffer-string)
              (point)
              (mark)
              (use-region-p))))
         '((anju-style-bold bold)
           (anju-style-italic italic)
           (anju-style-code code)
           (anju-style-strike-through strike)))"##,
        expect![[
            r#"OK ((bold "Deploy **alpha-service** today." 10 23 t) (italic "Deploy *alpha-service* today." 9 22 t) (code "Deploy `alpha-service` today." 9 22 t) (strike "Deploy ~~alpha-service~~ today." 10 23 t))"#
        ]],
    )
}

fn anju_style_commands_without_a_region_select_the_balanced_expression_at_point() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_style_commands_without_a_region_select_the_balanced_expression_at_point",
        r##"(list
         (with-temp-buffer
           (org-mode)
           (insert "Ship (alpha beta) after review")
           (goto-char 10)
           (anju-style-bold)
           (buffer-string))
         (with-temp-buffer
           (markdown-mode)
           (insert "Call deploy(alpha, beta) now")
           (search-backward "alpha")
           (anju-style-code)
           (buffer-string)))"##,
        expect![[r#"OK ("Ship (** alpha beta) after review" "Call deploy(`alpha`, beta) now")"#]],
    )
}

fn anju_style_remove_strips_org_markup_but_leaves_markdown_unchanged() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_style_remove_strips_org_markup_but_leaves_markdown_unchanged",
        r##"(list
         (with-temp-buffer
           (org-mode)
           (insert "*important* and /urgent/")
           (goto-char 1)
           (set-mark 12)
           (activate-mark)
           (anju-style-remove)
           (buffer-string))
         (with-temp-buffer
           (markdown-mode)
           (insert "**important**")
           (goto-char 1)
           (set-mark (point-max))
           (activate-mark)
           (anju-style-remove)
           (buffer-string)))"##,
        expect![[r#"OK ("important and /urgent/" "**important**")"#]],
    )
}

fn anju_style_dwim_drives_real_org_and_markdown_workflows_from_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_style_dwim_drives_real_org_and_markdown_workflows_from_completion",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (funcall (nth 0 case))
             (insert (nth 1 case))
             (goto-char (nth 2 case))
             (let (prompts)
               (cl-letf
                   (((symbol-function 'completing-read-multiple)
                     (lambda (prompt collection &rest arguments)
                       (setq prompts
                             (list prompt collection arguments))
                       (list (nth 3 case)))))
                 (anju-style-dwim)
                 (list prompts (buffer-string))))))
         '((org-mode "alpha beta" 3 "italic")
           (markdown-mode "ship alpha now" 8 "bold")
           (org-mode "remove-me" 4 "verbatim")
           (markdown-mode "unsupported" 4 "remove")))"##,
        expect![[
            r#"OK ((("Style: " ("bold" "italic" "code" "underline" "verbatim" "strike" "remove") nil) "// alpha beta") (("Style: " ("bold" "italic" "code" "underline" "verbatim" "strike" "remove") nil) "ship **alpha** now") (("Style: " ("bold" "italic" "code" "underline" "verbatim" "strike" "remove") nil) "== remove-me") (("Style: " ("bold" "italic" "code" "underline" "verbatim" "strike" "remove") nil) "unsupported"))"#
        ]],
    )
}

fn anju_style_support_and_menu_visibility_track_real_major_modes_and_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_style_support_and_menu_visibility_track_real_major_modes_and_state",
        r##"(mapcar
         (lambda (mode)
           (with-temp-buffer
             (funcall mode)
             (insert "alpha")
             (goto-char 1)
             (set-mark (point-max))
             (activate-mark)
             (list
              major-mode
              (anju-style-mode-supported-p)
              (anju-test-menu-entries anju-style-menu))))
         '(org-mode markdown-mode text-mode emacs-lisp-mode))"##,
        expect![[
            r#"OK ((org-mode org-mode ((Bold "Bold" anju-style-bold :enable nil :visible nil :style nil :selected nil :help "Bold selected region") (Italic "Italic" anju-style-italic :enable nil :visible nil :style nil :selected nil :help "Italic selected region") (Code "Code" anju-style-code :enable nil :visible nil :style nil :selected nil :help "Code selected region") (Underline "Underline" anju-style-underline :enable nil :visible #1=(derived-mode-p 'org-mode) :style nil :selected nil :help "Underline selected region") (Verbatim "Verbatim" anju-style-verbatim :enable nil :visible #2=(derived-mode-p 'org-mode) :style nil :selected nil :help "Verbatim selected region") (Strike\ Through "Strike Through" anju-style-strike-through :enable nil :visible nil :style nil :selected nil :help "Strike-through selected region") (Remove "Remove" anju-style-remove :enable nil :visible #3=(and (derived-mode-p 'org-mode) visible-mode) :style nil :selected nil :help "Remove markup from selected region"))) (markdown-mode markdown-mode ((Bold "Bold" anju-style-bold :enable nil :visible nil :style nil :selected nil :help "Bold selected region") (Italic "Italic" anju-style-italic :enable nil :visible nil :style nil :selected nil :help "Italic selected region") (Code "Code" anju-style-code :enable nil :visible nil :style nil :selected nil :help "Code selected region") (Underline "Underline" anju-style-underline :enable nil :visible #1# :style nil :selected nil :help "Underline selected region") (Verbatim "Verbatim" anju-style-verbatim :enable nil :visible #2# :style nil :selected nil :help "Verbatim selected region") (Strike\ Through "Strike Through" anju-style-strike-through :enable nil :visible nil :style nil :selected nil :help "Strike-through selected region") (Remove "Remove" anju-style-remove :enable nil :visible #3# :style nil :selected nil :help "Remove markup from selected region"))) (text-mode nil ((Bold "Bold" anju-style-bold :enable nil :visible nil :style nil :selected nil :help "Bold selected region") (Italic "Italic" anju-style-italic :enable nil :visible nil :style nil :selected nil :help "Italic selected region") (Code "Code" anju-style-code :enable nil :visible nil :style nil :selected nil :help "Code selected region") (Underline "Underline" anju-style-underline :enable nil :visible #1# :style nil :selected nil :help "Underline selected region") (Verbatim "Verbatim" anju-style-verbatim :enable nil :visible #2# :style nil :selected nil :help "Verbatim selected region") (Strike\ Through "Strike Through" anju-style-strike-through :enable nil :visible nil :style nil :selected nil :help "Strike-through selected region") (Remove "Remove" anju-style-remove :enable nil :visible #3# :style nil :selected nil :help "Remove markup from selected region"))) (emacs-lisp-mode nil ((Bold "Bold" anju-style-bold :enable nil :visible nil :style nil :selected nil :help "Bold selected region") (Italic "Italic" anju-style-italic :enable nil :visible nil :style nil :selected nil :help "Italic selected region") (Code "Code" anju-style-code :enable nil :visible nil :style nil :selected nil :help "Code selected region") (Underline "Underline" anju-style-underline :enable nil :visible #1# :style nil :selected nil :help "Underline selected region") (Verbatim "Verbatim" anju-style-verbatim :enable nil :visible #2# :style nil :selected nil :help "Verbatim selected region") (Strike\ Through "Strike Through" anju-style-strike-through :enable nil :visible nil :style nil :selected nil :help "Strike-through selected region") (Remove "Remove" anju-style-remove :enable nil :visible #3# :style nil :selected nil :help "Remove markup from selected region"))))"#
        ]],
    )
}

pub(super) fn style_text_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anju_org_region_styling_performs_every_supported_real_edit(),
        anju_markdown_region_styling_uses_markdown_mode_commands_in_practical_text(),
        anju_style_commands_without_a_region_select_the_balanced_expression_at_point(),
        anju_style_remove_strips_org_markup_but_leaves_markdown_unchanged(),
        anju_style_dwim_drives_real_org_and_markdown_workflows_from_completion(),
        anju_style_support_and_menu_visibility_track_real_major_modes_and_state(),
    ]
}
