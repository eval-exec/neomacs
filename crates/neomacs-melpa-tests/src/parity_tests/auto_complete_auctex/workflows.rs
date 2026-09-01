use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_auctex_setup_prepends_all_sources_to_an_existing_completion_configuration()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_setup_prepends_all_sources_to_an_existing_completion_configuration",
        r##"(let ((ac-sources
                                '(ac-source-words-in-buffer
                                  ac-source-files)))
          (list
           (ac-auctex-setup)
           ac-sources))"##,
        expect![
            "OK (#1=(ac-source-auctex-symbols ac-source-auctex-macros ac-source-auctex-environments ac-source-auctex-labels ac-source-auctex-bibs ac-source-words-in-buffer ac-source-files) #1#)"
        ],
    )
}

fn auto_complete_auctex_repeated_setup_preserves_the_packages_exact_prepend_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_repeated_setup_preserves_the_packages_exact_prepend_semantics",
        r##"(let ((ac-sources
                                '(ac-source-dictionary)))
          (ac-auctex-setup)
          (let ((once
                 ac-sources))
            (ac-auctex-setup)
            (list
             once
             ac-sources
             (length ac-sources))))"##,
        expect![
            "OK (#1=(ac-source-auctex-symbols ac-source-auctex-macros ac-source-auctex-environments ac-source-auctex-labels ac-source-auctex-bibs ac-source-dictionary) (ac-source-auctex-symbols ac-source-auctex-macros ac-source-auctex-environments ac-source-auctex-labels ac-source-auctex-bibs . #1#) 11)"
        ],
    )
}

fn auto_complete_auctex_real_latex_mode_hook_installs_sources_in_an_authoring_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_real_latex_mode_hook_installs_sources_in_an_authoring_buffer",
        r##"(with-temp-buffer
          (let ((ac-sources
                 '(ac-source-words-in-same-mode-buffers)))
            (LaTeX-mode)
            (list
             major-mode
             mode-name
             (bound-and-true-p TeX-mode-p)
             ac-sources
             (seq-every-p
              #'boundp
              '(TeX-symbol-list
                LaTeX-math-default
                LaTeX-environment-list
                LaTeX-label-list
                LaTeX-bibitem-list)))))"##,
        expect![[
            r#"OK (LaTeX-mode "LaTeX/P" t (ac-source-auctex-symbols ac-source-auctex-macros ac-source-auctex-environments ac-source-auctex-labels ac-source-auctex-bibs ac-source-words-in-same-mode-buffers) t)"#
        ]],
    )
}

fn auto_complete_auctex_sources_are_data_only_and_do_not_generate_interactive_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_sources_are_data_only_and_do_not_generate_interactive_commands",
        r##"(mapcar
          (lambda (source)
            (let ((command
                   (intern
                    (substring
                     (symbol-name source)
                     (length "ac-source-")))))
              (list
               source
               (boundp source)
               command
               (fboundp command)
               (commandp command))))
          '(ac-source-auctex-macros
            ac-source-auctex-symbols
            ac-source-auctex-environments
            ac-source-auctex-labels
            ac-source-auctex-bibs))"##,
        expect![
            "OK ((ac-source-auctex-macros t auctex-macros nil nil) (ac-source-auctex-symbols t auctex-symbols nil nil) (ac-source-auctex-environments t auctex-environments nil nil) (ac-source-auctex-labels t auctex-labels nil nil) (ac-source-auctex-bibs t auctex-bibs nil nil))"
        ],
    )
}

fn auto_complete_auctex_practical_document_workflow_finds_macro_environment_label_and_citation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_practical_document_workflow_finds_macro_environment_label_and_citation",
        r##"(with-temp-buffer
          (LaTeX-mode)
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "\\section{Introduction}\\label{sec:intro}\n"
           "See \\ref{sec:in} and \\cite[p. 42]{knu}\n"
           "\\begfig\n"
           "\\incl\n"
           "\\end{document}\n")
          (let ((TeX-symbol-list
                 '(("includegraphics" TeX-arg-file)
                   ("include" TeX-arg-file)
                   ("input" TeX-arg-file)
                   ("section" "Title")))
                (LaTeX-environment-list
                 '(("figure" ["htbp!"])
                   ("figure*" ["htbp!"])
                   ("document")))
                (LaTeX-label-list
                 '(("sec:intro" "paper.tex" 3)
                   ("sec:implementation" "paper.tex" 14)))
                (LaTeX-bibitem-list
                 '(("knuth1984" "The TeXbook")
                   ("knuth1992" "Literate Programming")
                   ("lamport1994" "LaTeX"))))
            (list
             (let ((ac-prefix "incl"))
               (ac-auctex-macro-candidates))
             (let ((ac-prefix "begfig"))
               (ac-auctex-environment-candidates))
             (let ((ac-prefix "sec:in"))
               (ac-auctex-label-candidates))
             (let ((ac-prefix "knu"))
               (ac-auctex-bib-candidates))
             (buffer-string))))"##,
        expect![[
            r#"OK (("includegraphics" "include") ("begfigure" "begfigure*") ("sec:intro") ("knuth1984" "knuth1992") "\\documentclass{article}\n\\begin{document}\n\\section{Introduction}\\label{sec:intro}\nSee \\ref{sec:in} and \\cite[p. 42]{knu}\n\\begfig\n\\incl\n\\end{document}\n")"#
        ]],
    )
}

fn auto_complete_auctex_practical_macro_selection_inserts_real_current_yasnippet_fields()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_practical_macro_selection_inserts_real_current_yasnippet_fields",
        r##"(with-temp-buffer
          (LaTeX-mode)
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "\\includegraphics")
          (goto-char (point-max))
          (let ((TeX-symbol-list
                 '(("includegraphics"
                    ["width=0.8\\textwidth"]
                    TeX-arg-file)
                   ("include" TeX-arg-file)))
                (ac-prefix
                 "include")
                candidate)
            (setq candidate
                  (car
                   (ac-auctex-macro-candidates)))
            (yas-minor-mode 1)
            (let ((action-result
                   (ac-auctex-macro-action)))
              (list
               candidate
               action-result
               (buffer-string)
               (point)
               (line-number-at-pos)
               (current-column)))))"##,
        expect![[
            r#"OK ("includegraphics" t "\\documentclass{article}\n\\begin{document}\n\\includegraphics[width=0.8\\textwidth]{Filename}" 58 3 16)"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_auctex_source_reload_deduplicates_global_registration_but_hook_setup_repeats_sources()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_source_reload_deduplicates_global_registration_but_hook_setup_repeats_sources",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE"))
                               (ac-sources
                                '(ac-source-files)))
          (load source nil t t)
          (load source nil t t)
          (run-hooks 'LaTeX-mode-hook)
          (run-hooks 'LaTeX-mode-hook)
          (list
           (length
            (seq-filter
             (lambda (mode)
               (eq mode 'latex-mode))
             ac-modes))
           (length
            (seq-filter
             (lambda (function)
               (eq function 'ac-auctex-setup))
             LaTeX-mode-hook))
           (length ac-sources)
           ac-sources))"##,
        expect![
            "OK (1 1 11 (ac-source-auctex-symbols ac-source-auctex-macros ac-source-auctex-environments ac-source-auctex-labels ac-source-auctex-bibs ac-source-auctex-symbols ac-source-auctex-macros ac-source-auctex-environments ac-source-auctex-labels ac-source-auctex-bibs ac-source-files))"
        ],
    )
}

fn auto_complete_auctex_label_and_bibliography_prefixes_follow_cursor_position_in_real_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_label_and_bibliography_prefixes_follow_cursor_position_in_real_text",
        r##"(with-temp-buffer
          (insert
           "See \\ref{sec:implementation} and "
           "\\cite[chapter 2]{knuth")
          (let ((LaTeX-label-list
                 '(("sec:introduction")
                   ("sec:implementation")
                   ("fig:architecture")))
                (LaTeX-bibitem-list
                 '(("knuth1984")
                   ("lamport1994")
                   ("lammel2009"))))
            (mapcar
             (lambda (fixture)
               (goto-char (point-min))
               (search-forward
                (cadr fixture))
               (let* ((source-symbol
                       (car fixture))
                      (source
                       (symbol-value
                        source-symbol))
                      (ac-sources
                       (list source))
                      (ac-compiled-sources nil)
                      (resolved
                       (ac-prefix 0 nil))
                      (start
                       (and
                        resolved
                        (nth 1 resolved)))
                      (prefix
                       (and
                        start
                        (buffer-substring-no-properties
                         start
                         (point)))))
                 (list
                  source-symbol
                  (cadr fixture)
                  resolved
                  prefix
                  (and
                   prefix
                   (let ((ac-prefix prefix))
                     (funcall
                      (nth 2 fixture)))))))
             '((ac-source-auctex-labels
                "sec:imple"
                ac-auctex-label-candidates)
               (ac-source-auctex-bibs
                "\\cite[chapter 2]{knuth"
                ac-auctex-bib-candidates)))))"##,
        expect![[
            r#"OK ((ac-source-auctex-labels "sec:imple" ("\\\\ref{\\([^}]*\\)\\=" 10 (((init . LaTeX-label-list) (candidates . ac-auctex-label-candidates) (requires . 0) (symbol . "r") (prefix . "\\\\ref{\\([^}]*\\)\\=")))) "sec:imple" ("sec:implementation")) (ac-source-auctex-bibs "\\cite[chapter 2]{knuth" ("\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=" 51 (((init . LaTeX-bibitem-list) (candidates . ac-auctex-bib-candidates) (requires . 0) (symbol . "b") (prefix . "\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=")))) "knuth" ("knuth1984")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_auctex_setup_prepends_all_sources_to_an_existing_completion_configuration(),
        auto_complete_auctex_repeated_setup_preserves_the_packages_exact_prepend_semantics(),
        auto_complete_auctex_real_latex_mode_hook_installs_sources_in_an_authoring_buffer(),
        auto_complete_auctex_sources_are_data_only_and_do_not_generate_interactive_commands(),
        auto_complete_auctex_practical_document_workflow_finds_macro_environment_label_and_citation(),
        auto_complete_auctex_practical_macro_selection_inserts_real_current_yasnippet_fields(),
        auto_complete_auctex_source_reload_deduplicates_global_registration_but_hook_setup_repeats_sources(),
        auto_complete_auctex_label_and_bibliography_prefixes_follow_cursor_position_in_real_text(),
    ]
}
