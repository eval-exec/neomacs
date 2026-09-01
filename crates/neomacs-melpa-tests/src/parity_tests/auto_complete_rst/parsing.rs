use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_rst_directive_parser_finds_directive_above_partial_single_option()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_directive_parser_finds_directive_above_partial_single_option",
        r##"(with-temp-buffer
                           (insert
                            "Heading\n=======\n\n"
                            ".. note:: Keep backups\n"
                            "   :cla")
                           (goto-char (point-max))
                           (let
                               ((original-point (point))
                                (directive
                                 (auto-complete-rst-directive-name-at-option)))
                             (list
                              directive
                              (= (point) original-point)
                              (line-number-at-pos)
                              (current-column))))"##,
        expect![[r####"OK ("note" t 5 7)"####]],
    )
}

fn auto_complete_rst_directive_parser_walks_across_multiple_preceding_options() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_directive_parser_walks_across_multiple_preceding_options",
        r##"(with-temp-buffer
                           (insert
                            ".. code_block:: rust\n"
                            "   :class: highlighted\n"
                            "   :linenos:\n"
                            "   :caption")
                           (goto-char (point-max))
                           (list
                            (auto-complete-rst-directive-name-at-option)
                            (save-excursion
                              (auto-complete-rst-goto-directive-from-option)
                              (list
                               (line-number-at-pos)
                               (current-column)
                               (buffer-substring
                                (line-beginning-position)
                                (point))))))"##,
        expect![[r####"OK ("code_block" (1 15 ".. code_block::"))"####]],
    )
}

fn auto_complete_rst_directive_parser_stops_at_intervening_body_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_directive_parser_stops_at_intervening_body_content",
        r##"(mapcar
                           (lambda (text)
                             (with-temp-buffer
                               (insert text)
                               (goto-char (point-max))
                               (list
                                (save-excursion
                                  (auto-complete-rst-goto-directive-from-option))
                                (save-excursion
                                  (auto-complete-rst-directive-name-at-option))
                                (point))))
                           '(".. note::\n   body text\n   :class"
                             ".. note::\n\n   :class"
                             "paragraph\n   :class"))"##,
        expect![[r####"OK ((nil nil 33) (nil nil 21) (nil nil 20))"####]],
    )
}

fn auto_complete_rst_directive_parser_respects_explicit_search_bound() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_directive_parser_respects_explicit_search_bound",
        r##"(with-temp-buffer
                           (insert
                            ".. note:: First\n"
                            "   :class: old\n"
                            "\n"
                            ".. image:: cover.png\n"
                            "   :alt")
                           (goto-char (point-max))
                           (let
                               ((second-directive
                                 (save-excursion
                                   (goto-char (point-min))
                                   (forward-line 3)
                                   (point)))
                                (option-point (point)))
                             (list
                              (auto-complete-rst-directive-name-at-option)
                              (progn
                                (goto-char option-point)
                                (auto-complete-rst-directive-name-at-option
                                 second-directive))
                              (progn
                                (goto-char option-point)
                                (auto-complete-rst-directive-name-at-option
                                 (line-beginning-position))))))"##,
        expect![[r####"OK ("image" "image" nil)"####]],
    )
}

fn auto_complete_rst_directive_parser_exposes_domain_and_hyphen_name_boundaries() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_directive_parser_exposes_domain_and_hyphen_name_boundaries",
        r##"(mapcar
                           (lambda (directive)
                             (with-temp-buffer
                               (insert
                                ".. "
                                directive
                                ":: target\n"
                                "   :option")
                               (goto-char (point-max))
                               (list
                                directive
                                (auto-complete-rst-directive-name-at-option))))
                           '("note"
                             "code-block"
                             "py:function"
                             "custom_directive"
                             "δοκιμή"))"##,
        expect![[
            r####"OK (("note" "note") ("code-block" "code-block") ("py:function" nil) ("custom_directive" "custom_directive") ("δοκιμή" "δοκιμή"))"####
        ]],
    )
}

fn auto_complete_rst_option_parser_handles_indentation_and_partial_option_names() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_option_parser_handles_indentation_and_partial_option_names",
        r##"(mapcar
                           (lambda (option-line)
                             (with-temp-buffer
                               (insert
                                "  .. image:: diagram.svg\n"
                                option-line)
                               (goto-char (point-max))
                               (list
                                option-line
                                (auto-complete-rst-directive-name-at-option))))
                           '("     :"
                             "     :alt"
                             "     :height:"
                             "     :width: 640"
                             "\t:target"))"##,
        expect![[
            r####"OK (("     :" "image") ("     :alt" "image") ("     :height:" nil) ("     :width: 640" nil) ("\11:target" "image"))"####
        ]],
    )
}

fn auto_complete_rst_goto_parser_reports_nil_and_exposes_search_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_goto_parser_reports_nil_and_exposes_search_side_effects",
        r##"(mapcar
                           (lambda (text)
                             (with-temp-buffer
                               (insert text)
                               (goto-char (point-max))
                               (let
                                   ((original (point))
                                    (result
                                     (auto-complete-rst-goto-directive-from-option)))
                                 (list
                                  text
                                  result
                                  original
                                  (point)
                                  (line-number-at-pos)))))
                           '("plain paragraph"
                             ".. note:: no option"
                             ":class"
                             "   class"
                             ""))"##,
        expect![[
            r####"OK (("plain paragraph" nil 16 6 1) (".. note:: no option" nil 20 13 1) (":class" nil 7 7 1) ("   class" nil 9 1 1) ("" nil 1 1 1))"####
        ]],
    )
}

fn auto_complete_rst_directive_name_lookup_preserves_point_and_buffer_contents() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_directive_name_lookup_preserves_point_and_buffer_contents",
        r##"(with-temp-buffer
                           (insert
                            "Intro\n\n"
                            ".. custom_directive:: value\n"
                            "   :first: one\n"
                            "   :second")
                           (goto-char (point-max))
                           (let
                               ((original-point (point))
                                (original-text (buffer-string))
                                first
                                second)
                             (setq
                              first
                              (auto-complete-rst-directive-name-at-option))
                             (setq
                              second
                              (auto-complete-rst-directive-name-at-option))
                             (list
                              first
                              second
                              (= original-point (point))
                              (equal original-text (buffer-string))
                              (buffer-modified-p))))"##,
        expect![[r####"OK ("custom_directive" "custom_directive" t t t)"####]],
    )
}

pub(super) fn parsing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_rst_directive_parser_finds_directive_above_partial_single_option(),
        auto_complete_rst_directive_parser_walks_across_multiple_preceding_options(),
        auto_complete_rst_directive_parser_stops_at_intervening_body_content(),
        auto_complete_rst_directive_parser_respects_explicit_search_bound(),
        auto_complete_rst_directive_parser_exposes_domain_and_hyphen_name_boundaries(),
        auto_complete_rst_option_parser_handles_indentation_and_partial_option_names(),
        auto_complete_rst_goto_parser_reports_nil_and_exposes_search_side_effects(),
        auto_complete_rst_directive_name_lookup_preserves_point_and_buffer_contents(),
    ]
}
