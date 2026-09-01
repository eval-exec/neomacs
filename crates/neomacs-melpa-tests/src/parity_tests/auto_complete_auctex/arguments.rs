use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_auctex_expand_argument_info_preserves_literal_and_optional_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_argument_info_preserves_literal_and_optional_arguments",
        r##"(ac-auctex-expand-arg-info
          '("Required"
            ["Optional"]
            ""
            [""]))"##,
        expect![[r#"OK ("Required" ["Optional"] "" [""])"#]],
    )
}

fn auto_complete_auctex_expand_argument_info_resolves_direct_auctex_argument_functions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_argument_info_resolves_direct_auctex_argument_functions",
        r##"(ac-auctex-expand-arg-info
          '(TeX-arg-file
            TeX-arg-ref
            LaTeX-arg-usepackage
            LaTeX-env-tabular*
            LaTeX-env-item))"##,
        expect![[
            r#"OK ("Filename" "Name" ["opt1,..."] "Package" "Width" ["htbp!"] "lcrpmb|><" "")"#
        ]],
    )
}

fn auto_complete_auctex_expand_argument_info_resolves_nested_function_specs_by_head()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_argument_info_resolves_nested_function_specs_by_head",
        r##"(ac-auctex-expand-arg-info
          '((TeX-arg-file :extensions ("png" "pdf"))
            (TeX-arg-ref :prompt "Cross reference")
            (LaTeX-env-minipage :placement)
            (unknown-handler :metadata)))"##,
        expect![[r#"OK ("Filename" "Name" ["htbp!"] "Width" "")"#]],
    )
}

fn auto_complete_auctex_expand_argument_info_turns_vector_handlers_into_optional_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_argument_info_turns_vector_handlers_into_optional_arguments",
        r##"(ac-auctex-expand-arg-info
          '([TeX-arg-file]
            [(TeX-arg-ref :prompt "Reference")]
            [LaTeX-env-array]
            [unknown-handler]))"##,
        expect!["OK (#1=[item-2] #1# #1# #1# #1#)"],
    )
}

fn auto_complete_auctex_expand_argument_info_supports_numeric_auctex_arities() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_argument_info_supports_numeric_auctex_arities",
        r##"(mapcar
          (lambda (arity)
            (list
             arity
             (ac-auctex-expand-arg-info
              (list arity))))
          '(2 5 9))"##,
        expect![[r#"OK ((2 ("" "")) (5 ("" "" "" "" "")) (9 ("" "" "" "" "" "" "" "" "")))"#]],
    )
}

fn auto_complete_auctex_expand_argument_info_defaults_unknown_and_empty_specs() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_argument_info_defaults_unknown_and_empty_specs",
        r##"(list
          (ac-auctex-expand-arg-info
           '(unknown-handler))
          (ac-auctex-expand-arg-info
           '((unknown-handler :anything)))
          (ac-auctex-expand-arg-info nil))"##,
        expect![[r#"OK (("") ("") nil)"#]],
    )
}

fn auto_complete_auctex_snippet_argument_numbers_required_and_optional_fields_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_snippet_argument_numbers_required_and_optional_fields_exactly",
        r##"(mapcar
          (lambda (fixture)
            (apply
             #'ac-auctex-snippet-arg
             fixture))
          '((1 "Title")
            (2 ["Short title"])
            (7 "")
            (9 [""])))"##,
        expect![[r#"OK ((2 "{${Title}}") (4 "${[${Short title}]}") (8 "{${}}") (11 "${[${}]}"))"#]],
    )
}

fn auto_complete_auctex_macro_snippet_builds_practical_section_graphics_and_tabular_fields()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_macro_snippet_builds_practical_section_graphics_and_tabular_fields",
        r##"(mapcar
          (lambda (fixture)
            (list
             (car fixture)
             (ac-auctex-macro-snippet
              (cdr fixture))))
          '(("section"
             ["Short title"]
             "Title")
            ("includegraphics"
             ["width=0.8\\textwidth"]
             TeX-arg-file)
            ("tabular*"
             LaTeX-env-tabular*)
            ("empty")))"##,
        expect![[
            r#"OK (("section" "${[${Short title}]}{${Title}}") ("includegraphics" "${[${width=0.8\\textwidth}]}{${Filename}}") ("tabular*" "{${Width}}${[${htbp!}]}{${lcrpmb|><}}") ("empty" ""))"#
        ]],
    )
}

fn auto_complete_auctex_macro_snippet_mixes_expanded_arity_handlers_and_literal_fields()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_macro_snippet_mixes_expanded_arity_handlers_and_literal_fields",
        r##"(ac-auctex-macro-snippet
          '(3
            ["Options"]
            TeX-arg-ref
            (TeX-arg-file :extensions ("tex"))
            "Tail"))"##,
        expect![[r#"OK "{${}}{${}}{${}}${[${Options}]}{${Name}}{${Filename}}{${Tail}}""#]],
    )
}

fn auto_complete_auctex_expand_args_finds_environment_entry_and_forwards_exact_snippet()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_expand_args_finds_environment_entry_and_forwards_exact_snippet",
        r##"(let ((environment
                                '(("figure"
                                   ["htbp!"])
                                  ("table"
                                   ["tbp"]
                                   "Columns")))
                               captured)
          (fset
           'yas/expand-snippet
           (lambda (snippet &rest arguments)
             (setq captured
                   (list
                    snippet
                    arguments))
             :expanded))
          (list
           (ac-auctex-expand-args
            "table"
            environment)
           captured
           (ac-auctex-expand-args
            "missing"
            environment)
           captured))"##,
        expect![[r#"OK (:expanded ("${[${tbp}]}{${Columns}}" nil) :expanded ("" nil))"#]],
    )
}

pub(super) fn arguments_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_auctex_expand_argument_info_preserves_literal_and_optional_arguments(),
        auto_complete_auctex_expand_argument_info_resolves_direct_auctex_argument_functions(),
        auto_complete_auctex_expand_argument_info_resolves_nested_function_specs_by_head(),
        auto_complete_auctex_expand_argument_info_turns_vector_handlers_into_optional_arguments(),
        auto_complete_auctex_expand_argument_info_supports_numeric_auctex_arities(),
        auto_complete_auctex_expand_argument_info_defaults_unknown_and_empty_specs(),
        auto_complete_auctex_snippet_argument_numbers_required_and_optional_fields_exactly(),
        auto_complete_auctex_macro_snippet_builds_practical_section_graphics_and_tabular_fields(),
        auto_complete_auctex_macro_snippet_mixes_expanded_arity_handlers_and_literal_fields(),
        auto_complete_auctex_expand_args_finds_environment_entry_and_forwards_exact_snippet(),
    ]
}
