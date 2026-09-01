use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_exuberant_ctags_get_line_preserves_practical_tag_records() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_line_preserves_practical_tag_records",
        r##"(with-temp-buffer
                           (insert
                            "render_frame\tui.rs\t/^fn render_frame/;\"\tkind:f\tlanguage:Rust")
                           (list
                            (ac-exuberant-ctags-get-line
                             (point-min)
                             (point-max))
                            (buffer-string)
                            (point-min)
                            (point-max)))"##,
        expect![[
            r#"OK ("render_frame\11ui.rs\11/^fn render_frame/;\"\11kind:f\11language:Rust" "render_frame\11ui.rs\11/^fn render_frame/;\"\11kind:f\11language:Rust" 1 61)"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_get_line_filters_headers_but_not_embedded_markers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_line_filters_headers_but_not_embedded_markers",
        r##"(mapcar
                           (lambda (line)
                             (with-temp-buffer
                               (insert line)
                               (ac-exuberant-ctags-get-line
                                (point-min)
                                (point-max))))
                           '("!_TAG_FILE_FORMAT\t2"
                             "!_TAG_PROGRAM_NAME\tUniversal Ctags"
                             "alpha!_beta\tfile\tkind:v\tlanguage:C"
                             " !_\tfile\tkind:v\tlanguage:C"))"##,
        expect![[
            r#"OK ("" "" "alpha!_beta\11file\11kind:v\11language:C" " !_\11file\11kind:v\11language:C")"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_get_line_observes_exact_length_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_line_observes_exact_length_boundary",
        r##"(let ((ac-exuberant-ctags-line-length-limit 5))
                           (mapcar
                            (lambda (line)
                              (with-temp-buffer
                                (insert line)
                                (list
                                 (length line)
                                 (ac-exuberant-ctags-get-line
                                  (point-min)
                                  (point-max)))))
                            '("" "abcd" "abcde" "abcdef")))"##,
        expect![[r#"OK ((0 "") (4 "abcd") (5 "abcde") (6 ""))"#]],
    )
}

fn auto_complete_exuberant_ctags_get_line_counts_multibyte_characters() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_line_counts_multibyte_characters",
        r##"(let ((ac-exuberant-ctags-line-length-limit 3))
                           (mapcar
                            (lambda (line)
                              (with-temp-buffer
                                (insert line)
                                (list
                                 (length line)
                                 (string-bytes line)
                                 (ac-exuberant-ctags-get-line
                                  (point-min)
                                  (point-max)))))
                            '("λ界x" "λ界xy" "ééé" "éééé")))"##,
        expect![[r#"OK ((3 6 "λ界x") (4 7 "") (3 6 "ééé") (4 8 ""))"#]],
    )
}

fn auto_complete_exuberant_ctags_get_line_respects_arbitrary_buffer_spans() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_line_respects_arbitrary_buffer_spans",
        r##"(with-temp-buffer
                           (insert "prefix|actual-tag-record|suffix")
                           (let ((ac-exuberant-ctags-line-length-limit
                                  100))
                             (list
                              (ac-exuberant-ctags-get-line 8 25)
                              (ac-exuberant-ctags-get-line 1 7)
                              (ac-exuberant-ctags-get-line 25 32))))"##,
        expect![[r#"OK ("actual-tag-record" "prefix" "|suffix")"#]],
    )
}

fn auto_complete_exuberant_ctags_get_line_invalid_ranges_signal_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_line_invalid_ranges_signal_exactly",
        r##"(with-temp-buffer
                           (insert "abc")
                           (mapcar
                            (lambda (bounds)
                              (auto-complete-exuberant-ctags-test-error
                               (lambda ()
                                 (ac-exuberant-ctags-get-line
                                  (car bounds)
                                  (cadr bounds)))))
                            '((0 2) (1 9) (3 2))))"##,
        expect![[
            r#"OK ((:signal args-out-of-range ((:buffer nil) 0 2)) (:signal args-out-of-range ((:buffer nil) 1 9)) (:value "b"))"#
        ]],
    )
}

pub(super) fn lines_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_exuberant_ctags_get_line_preserves_practical_tag_records(),
        auto_complete_exuberant_ctags_get_line_filters_headers_but_not_embedded_markers(),
        auto_complete_exuberant_ctags_get_line_observes_exact_length_boundary(),
        auto_complete_exuberant_ctags_get_line_counts_multibyte_characters(),
        auto_complete_exuberant_ctags_get_line_respects_arbitrary_buffer_spans(),
        auto_complete_exuberant_ctags_get_line_invalid_ranges_signal_exactly(),
    ]
}
