use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_auctex_macro_action_expands_the_selected_macro_arguments_at_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_macro_action_expands_the_selected_macro_arguments_at_point",
        r##"(with-temp-buffer
          (insert "\\includegraphics")
          (goto-char (point-max))
          (let ((candidate
                 "includegraphics")
                (TeX-symbol-list
                 '(("includegraphics"
                    [TeX-arg-key-val
                     (("width")
                      ("height")
                      ("scale"))]
                    TeX-arg-file)))
                captured)
            (fset
             'yas/expand-snippet
             (lambda (snippet &rest arguments)
               (setq captured
                     (list
                      snippet
                      arguments
                      (point)
                      (buffer-string)))
               :expanded))
            (list
             (ac-auctex-macro-action)
             captured)))"##,
        expect![[r#"OK (:expanded ("${[${item-2}]}{${Filename}}" nil 17 "\\includegraphics"))"#]],
    )
}

fn auto_complete_auctex_symbol_action_inside_math_replaces_typed_command_and_expands_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_symbol_action_inside_math_replaces_typed_command_and_expands_arguments",
        r##"(with-temp-buffer
          (insert "\\operator")
          (goto-char (point-max))
          (let ((candidate
                 "operator")
                (TeX-symbol-list
                 '(("operator" "Name")))
                captured)
            (fset
             'texmathp
             (lambda ()
               t))
            (fset
             'yas/expand-snippet
             (lambda (snippet &rest arguments)
               (setq captured
                     (list
                      snippet
                      arguments
                      (point)
                      (buffer-string)))
               :expanded))
            (list
             (ac-auctex-symbol-action)
             captured
             (point)
             (buffer-string))))"##,
        expect![[r#"OK (:expanded ("{${Name}}" nil 10 "\\operator") 10 "\\operator")"#]],
    )
}

fn auto_complete_auctex_symbol_action_outside_math_wraps_command_and_leaves_point_before_closing_dollar()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_symbol_action_outside_math_wraps_command_and_leaves_point_before_closing_dollar",
        r##"(with-temp-buffer
          (insert "Euler wrote \\alpha")
          (goto-char (point-max))
          (let ((candidate
                 "alpha")
                (TeX-symbol-list
                 '(("alpha")))
                captured)
            (fset
             'texmathp
             (lambda ()
               nil))
            (fset
             'yas/expand-snippet
             (lambda (snippet &rest arguments)
               (setq captured
                     (list
                      snippet
                      arguments
                      (point)
                      (buffer-string)))
               :expanded))
            (list
             (ac-auctex-symbol-action)
             captured
             (point)
             (char-after)
             (buffer-string))))"##,
        expect![[
            r#"OK (:expanded ("" nil 20 "Euler wrote $\\alpha$") 20 36 "Euler wrote $\\alpha$")"#
        ]],
    )
}

fn auto_complete_auctex_environment_action_replaces_prefix_with_complete_nested_figure_snippet()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_environment_action_replaces_prefix_with_complete_nested_figure_snippet",
        r##"(with-temp-buffer
          (insert "Before\n\\begfigure")
          (goto-char (point-max))
          (let ((candidate
                 "begfigure")
                (LaTeX-environment-list
                 '(("figure"
                    ["htbp!"]
                    (TeX-arg-file :extensions ("png" "pdf")))))
                captured)
            (fset
             'yas/expand-snippet
             (lambda (snippet &rest arguments)
               (setq captured
                     (list
                      snippet
                      arguments
                      (point)
                      (buffer-string)))
               :expanded))
            (list
             (ac-auctex-environment-action)
             captured
             (buffer-string))))"##,
        expect![[
            r#"OK (:expanded ("\\begin{figure}${[${htbp!}]}{${Filename}}\n$0\n\\end{figure}" nil 8 "Before\n") "Before\n")"#
        ]],
    )
}

fn auto_complete_auctex_environment_action_honors_custom_candidate_prefix_and_tab_stops()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_environment_action_honors_custom_candidate_prefix_and_tab_stops",
        r##"(with-temp-buffer
          (insert "\\begin:itemize")
          (goto-char (point-max))
          (let ((candidate
                 "begin:itemize")
                (ac-auctex-environment-prefix
                 "begin:")
                (LaTeX-environment-list
                 '(("itemize"
                    ["compact"]
                    "Label")))
                captured)
            (fset
             'yas/expand-snippet
             (lambda (snippet &rest arguments)
               (setq captured
                     (list
                      snippet
                      arguments
                      (point)
                      (buffer-string)))
               :expanded))
            (list
             (ac-auctex-environment-action)
             captured)))"##,
        expect![[
            r#"OK (:expanded ("\\begin{itemize}${[${compact}]}{${Label}}\n$0\n\\end{itemize}" nil 1 ""))"#
        ]],
    )
}

fn auto_complete_auctex_macro_action_drives_current_yasnippet_through_its_legacy_alias()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_macro_action_drives_current_yasnippet_through_its_legacy_alias",
        r##"(with-temp-buffer
          (insert "\\section")
          (goto-char (point-max))
          (let ((candidate
                 "section")
                (TeX-symbol-list
                 '(("section"
                    ["Short title"]
                    "Title"))))
            (yas-minor-mode 1)
            (list
             (ac-auctex-macro-action)
             (buffer-string)
             (point)
             (buffer-substring-no-properties
              (line-beginning-position)
              (point-max)))))"##,
        expect![[r#"OK (t "\\section[Short title]{Title}" 9 "\\section[Short title]{Title}")"#]],
    )
    .fresh_process()
}

fn auto_complete_auctex_environment_action_builds_real_current_yasnippet_document_structure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_environment_action_builds_real_current_yasnippet_document_structure",
        r##"(with-temp-buffer
          (insert "\\begdocument")
          (goto-char (point-max))
          (let ((candidate
                 "begdocument")
                (LaTeX-environment-list
                 '(("document"))))
            (yas-minor-mode 1)
            (list
             (ac-auctex-environment-action)
             (buffer-string)
             (point)
             (line-number-at-pos)
             (current-column))))"##,
        expect![[r#"OK (t "\\begin{document}\n\n\\end{document}" 18 2 0)"#]],
    )
    .fresh_process()
}

pub(super) fn actions_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_auctex_macro_action_expands_the_selected_macro_arguments_at_point(),
        auto_complete_auctex_symbol_action_inside_math_replaces_typed_command_and_expands_arguments(),
        auto_complete_auctex_symbol_action_outside_math_wraps_command_and_leaves_point_before_closing_dollar(),
        auto_complete_auctex_environment_action_replaces_prefix_with_complete_nested_figure_snippet(),
        auto_complete_auctex_environment_action_honors_custom_candidate_prefix_and_tab_stops(),
        auto_complete_auctex_macro_action_drives_current_yasnippet_through_its_legacy_alias(),
        auto_complete_auctex_environment_action_builds_real_current_yasnippet_document_structure(),
    ]
}
