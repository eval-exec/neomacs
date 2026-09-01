use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_auctex_macro_candidates_normalize_real_auctex_entries_and_filter_prefix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_macro_candidates_normalize_real_auctex_entries_and_filter_prefix",
        r##"(let ((TeX-symbol-list
                                '(("alpha" "Required")
                                  (("alphabet" 1) ["Optional"])
                                  ("beta")
                                  (("alpine" TeX-arg-file))))
                               (ac-prefix
                                "alp"))
          (ac-auctex-macro-candidates))"##,
        expect![[r#"OK ("alpha" "alphabet" "alpine")"#]],
    )
}

fn auto_complete_auctex_macro_candidates_preserve_source_order_duplicates_and_case_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_macro_candidates_preserve_source_order_duplicates_and_case_rules",
        r##"(let ((TeX-symbol-list
                                '(("alpha")
                                  ("Alpha")
                                  (("alphabet" 1))
                                  ("alpha")
                                  ("alpine")
                                  ("beta")))
                               (completion-ignore-case
                                nil))
          (mapcar
           (lambda (prefix)
             (let ((ac-prefix prefix))
               (list
                prefix
                (ac-auctex-macro-candidates))))
           '("al" "Al" "alpha" "z")))"##,
        expect![[
            r#"OK (("al" ("alpha" "alphabet" "alpha" "alpine")) ("Al" ("Alpha")) ("alpha" ("alpha" "alphabet" "alpha")) ("z" nil))"#
        ]],
    )
}

fn auto_complete_auctex_symbol_candidates_filter_real_math_command_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_symbol_candidates_filter_real_math_command_names",
        r##"(let ((LaTeX-math-default
                                '((?a "alpha" "Greek alpha" 945)
                                  (?b "beta" "Greek beta" 946)
                                  (?l "leq" ("AMS" "less or equal") 8804)
                                  (?L "Leftarrow" "double arrow" 8656)))
                               (completion-ignore-case
                                nil))
          (mapcar
           (lambda (prefix)
             (let ((ac-prefix prefix))
               (list
                prefix
                (ac-auctex-symbol-candidates))))
           '("" "al" "le" "L" "missing")))"##,
        expect![[
            r#"OK (("" ("alpha" "beta" "leq" "Leftarrow")) ("al" ("alpha")) ("le" ("leq")) ("L" ("Leftarrow")) ("missing" nil))"#
        ]],
    )
}

fn auto_complete_auctex_symbol_document_formats_strings_lists_unicode_and_missing_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_symbol_document_formats_strings_lists_unicode_and_missing_entries",
        r##"(let ((LaTeX-math-default
                                '((?a "alpha" "Greek alpha" 945)
                                  (?l "leq" ("AMS" "less or equal") 8804)
                                  (?n "nexists" "does not exist")
                                  (?e "empty" nil nil))))
          (mapcar
           (lambda (candidate)
             (list
              candidate
              (ac-auctex-symbol-document
               candidate)))
           '("alpha"
             "leq"
             "nexists"
             "empty"
             "unknown")))"##,
        expect![[
            r#"OK (("alpha" "Greek alpha == α") ("leq" "AMS less or equal == ≤") ("nexists" "does not exist == ") ("empty" " == ") ("unknown" " == "))"#
        ]],
    )
}

fn auto_complete_auctex_environment_candidates_add_beg_prefix_before_filtering() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_auctex_environment_candidates_add_beg_prefix_before_filtering",
        r##"(let ((LaTeX-environment-list
                                '(("document")
                                  ("description" LaTeX-env-item)
                                  ("figure" ["htbp!"])
                                  ("figure*" ["htbp!"])
                                  ("frame" ["fragile"])
                                  ("table")))
                               (completion-ignore-case
                                nil))
          (mapcar
           (lambda (prefix)
             (let ((ac-prefix prefix))
               (list
                prefix
                (ac-auctex-environment-candidates))))
           '("beg" "begf" "begfigure" "figure" "begz")))"##,
        expect![[
            r#"OK (("beg" ("begdocument" "begdescription" "begfigure" "begfigure*" "begframe" "begtable")) ("begf" ("begfigure" "begfigure*" "begframe")) ("begfigure" ("begfigure" "begfigure*")) ("figure" nil) ("begz" nil))"#
        ]],
    )
}

fn auto_complete_auctex_environment_candidates_honor_custom_prefix_and_duplicate_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_environment_candidates_honor_custom_prefix_and_duplicate_entries",
        r##"(let ((LaTeX-environment-list
                                '(("itemize")
                                  ("itemize")
                                  ("enumerate")
                                  ("equation")))
                               (ac-auctex-environment-prefix
                                "begin:")
                               (ac-prefix
                                "begin:i"))
          (ac-auctex-environment-candidates))"##,
        expect![[r#"OK ("begin:itemize" "begin:itemize")"#]],
    )
}

fn auto_complete_auctex_label_candidates_filter_real_cross_reference_records() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_label_candidates_filter_real_cross_reference_records",
        r##"(let ((LaTeX-label-list
                                '(("sec:introduction" "main.tex" 12)
                                  ("sec:implementation" "impl.tex" 48)
                                  ("fig:architecture" "figures.tex" 7)
                                  ("tab:results" "results.tex" 31)
                                  ("sec:introduction" "appendix.tex" 4)))
                               (ac-prefix
                                "sec:i"))
          (ac-auctex-label-candidates))"##,
        expect![[r#"OK ("sec:introduction" "sec:implementation" "sec:introduction")"#]],
    )
}

fn auto_complete_auctex_bibliography_candidates_filter_keys_and_preserve_metadata_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_bibliography_candidates_filter_keys_and_preserve_metadata_order",
        r##"(let ((LaTeX-bibitem-list
                                '(("knuth1984" "The TeXbook")
                                  ("knuth1992" "Literate Programming")
                                  ("lamport1994" "LaTeX")
                                  ("knuth1984" "duplicate database")))
                               (ac-prefix
                                "knuth"))
          (ac-auctex-bib-candidates))"##,
        expect![[r#"OK ("knuth1984" "knuth1992" "knuth1984")"#]],
    )
}

fn auto_complete_auctex_source_prefix_regexps_extract_practical_macro_ref_and_cite_prefixes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_source_prefix_regexps_extract_practical_macro_ref_and_cite_prefixes",
        r##"(with-temp-buffer
          (mapcar
           (lambda (fixture)
             (erase-buffer)
             (insert
              (cadr fixture))
             (let* ((source-symbol
                     (car fixture))
                    (source
                     (symbol-value source-symbol))
                    (ac-sources
                     (list source))
                    (ac-compiled-sources nil)
                    (resolved
                     (ac-prefix 0 nil))
                    (start
                     (and
                      resolved
                      (nth 1 resolved))))
               (list
                source-symbol
                (buffer-string)
                (cdr
                 (assq 'prefix source))
                resolved
                (and
                 start
                 (buffer-substring-no-properties
                  start
                  (point))))))
           '((ac-source-auctex-macros
              "\\includegr")
             (ac-source-auctex-symbols
              "formula: \\alp")
             (ac-source-auctex-environments
              "\\begfig")
             (ac-source-auctex-labels
              "See \\ref{sec:impl")
             (ac-source-auctex-bibs
              "\\cite[p. 42]{knuth19")
             (ac-source-auctex-bibs
              "\\cite{first,knuth19"))))"##,
        expect![[
            r#"OK ((ac-source-auctex-macros "\\includegr" "\\\\\\([a-zA-Z]*\\)\\=" ("\\\\\\([a-zA-Z]*\\)\\=" 2 (((init . TeX-symbol-list) (candidates . ac-auctex-macro-candidates) (action . ac-auctex-macro-action) (requires . 0) (symbol . "m") (prefix . "\\\\\\([a-zA-Z]*\\)\\=")))) "includegr") (ac-source-auctex-symbols "formula: \\alp" "\\\\\\([a-zA-Z]*\\)\\=" ("\\\\\\([a-zA-Z]*\\)\\=" 11 (((init . LaTeX-math-mode) (candidates . ac-auctex-symbol-candidates) (document . ac-auctex-symbol-document) (action . ac-auctex-symbol-action) (requires . 0) (symbol . "s") (prefix . "\\\\\\([a-zA-Z]*\\)\\=")))) "alp") (ac-source-auctex-environments "\\begfig" "\\\\\\([a-zA-Z]*\\)\\=" ("\\\\\\([a-zA-Z]*\\)\\=" 2 (((init . LaTeX-environment-list) (candidates . ac-auctex-environment-candidates) (action . ac-auctex-environment-action) (requires . 0) (symbol . "e") (prefix . "\\\\\\([a-zA-Z]*\\)\\=")))) "begfig") (ac-source-auctex-labels "See \\ref{sec:impl" "\\\\ref{\\([^}]*\\)\\=" ("\\\\ref{\\([^}]*\\)\\=" 10 (((init . LaTeX-label-list) (candidates . ac-auctex-label-candidates) (requires . 0) (symbol . "r") (prefix . "\\\\ref{\\([^}]*\\)\\=")))) "sec:impl") (ac-source-auctex-bibs "\\cite[p. 42]{knuth19" "\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=" ("\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=" 14 (((init . LaTeX-bibitem-list) (candidates . ac-auctex-bib-candidates) (requires . 0) (symbol . "b") (prefix . "\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=")))) "knuth19") (ac-source-auctex-bibs "\\cite{first,knuth19" "\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=" nil nil))"#
        ]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_auctex_macro_candidates_normalize_real_auctex_entries_and_filter_prefix(),
        auto_complete_auctex_macro_candidates_preserve_source_order_duplicates_and_case_rules(),
        auto_complete_auctex_symbol_candidates_filter_real_math_command_names(),
        auto_complete_auctex_symbol_document_formats_strings_lists_unicode_and_missing_entries(),
        auto_complete_auctex_environment_candidates_add_beg_prefix_before_filtering(),
        auto_complete_auctex_environment_candidates_honor_custom_prefix_and_duplicate_entries(),
        auto_complete_auctex_label_candidates_filter_real_cross_reference_records(),
        auto_complete_auctex_bibliography_candidates_filter_keys_and_preserve_metadata_order(),
        auto_complete_auctex_source_prefix_regexps_extract_practical_macro_ref_and_cite_prefixes(),
    ]
}
