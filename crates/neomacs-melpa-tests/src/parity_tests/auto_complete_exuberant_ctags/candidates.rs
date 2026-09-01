use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_exuberant_ctags_candidates_match_practical_prefix_and_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidates_match_practical_prefix_and_order",
        r##"(with-temp-buffer
                           (insert "    ren")
                           (let ((ac-point (point))
                                 (ac-target "ren")
                                 (candidates '("already-present"))
                                 (ac-exuberant-ctags-index
                                  '("render f C"
                                    "rename m Rust"
                                    "renew v Python"
                                    "other f C")))
                             (auto-complete-exuberant-ctags-test-candidate
                              candidates)))"##,
        expect![[r#"OK ("renew" "render" "rename")"#]],
    )
}

fn auto_complete_exuberant_ctags_candidates_are_case_sensitive() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidates_are_case_sensitive",
        r##"(with-temp-buffer
                           (insert "    Fo")
                           (let ((ac-point (point))
                                 (ac-target "Fo")
                                 (candidates nil)
                                 (ac-exuberant-ctags-index
                                  '("Foo c C++"
                                    "foo f C"
                                    "Foobar m Ruby"
                                    "FOO v Go")))
                             (auto-complete-exuberant-ctags-test-candidate
                              candidates)))"##,
        expect![[r#"OK ("Foobar" "Foo")"#]],
    )
}

fn auto_complete_exuberant_ctags_candidates_preserve_duplicate_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidates_preserve_duplicate_names",
        r##"(with-temp-buffer
                           (insert "    sa")
                           (let ((ac-point (point))
                                 (ac-target "sa")
                                 (candidates nil)
                                 (ac-exuberant-ctags-index
                                  '("save f C"
                                    "save m Ruby"
                                    "save p C++"
                                    "safe f Rust")))
                             (auto-complete-exuberant-ctags-test-candidate
                              candidates)))"##,
        expect![[r#"OK ("save" "save" "save" "safe")"#]],
    )
}

fn auto_complete_exuberant_ctags_candidates_ignore_malformed_index_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidates_ignore_malformed_index_entries",
        r##"(with-temp-buffer
                           (insert "    al")
                           (let ((ac-point (point))
                                 (ac-target "al")
                                 (candidates nil)
                                 (ac-exuberant-ctags-index
                                  '("alpha f C"
                                    "alone"
                                    "almost f"
                                    "al tabs not-enough extra"
                                    "beta f C")))
                             (auto-complete-exuberant-ctags-test-candidate
                              candidates)))"##,
        expect![[r#"OK ("alpha" "al")"#]],
    )
}

fn auto_complete_exuberant_ctags_candidate_limit_is_count_with_off_by_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidate_limit_is_count_with_off_by_one",
        r##"(mapcar
                           (lambda (limit)
                             (with-temp-buffer
                               (insert "    a")
                               (let ((ac-point (point))
                                     (ac-target "a")
                                     (candidates nil)
                                     (ac-exuberant-ctags-line-length-limit
                                      limit)
                                     (ac-exuberant-ctags-index
                                      '("a1 f C"
                                        "a2 f C"
                                        "a3 f C"
                                        "a4 f C")))
                                 (list
                                  limit
                                  (auto-complete-exuberant-ctags-test-candidate
                                   candidates)))))
                           '(0 1 2 3 10))"##,
        expect![[
            r#"OK ((0 ("a1")) (1 ("a2" "a1")) (2 ("a3" "a2" "a1")) (3 ("a4" "a3" "a2" "a1")) (10 ("a4" "a3" "a2" "a1")))"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_long_names_are_not_length_filtered() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_long_names_are_not_length_filtered",
        r##"(with-temp-buffer
                           (insert "    super")
                           (let ((ac-point (point))
                                 (ac-target "super")
                                 (candidates nil)
                                 (ac-exuberant-ctags-line-length-limit
                                  1)
                                 (ac-exuberant-ctags-index
                                  '("supercalifragilistic f C"
                                    "supervisor m Rust")))
                             (auto-complete-exuberant-ctags-test-candidate
                              candidates)))"##,
        expect![[r#"OK ("supervisor" "supercalifragilistic")"#]],
    )
}

fn auto_complete_exuberant_ctags_candidate_near_buffer_start_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidate_near_buffer_start_signals",
        r##"(with-temp-buffer
                           (insert "ab")
                           (let ((ac-point 2)
                                 (ac-target "a")
                                 (candidates nil)
                                 (ac-exuberant-ctags-index
                                  '("alpha f C")))
                             (auto-complete-exuberant-ctags-test-error
                              (lambda ()
                                (auto-complete-exuberant-ctags-test-candidate
                                 candidates)))))"##,
        expect!["OK (:signal args-out-of-range ((:buffer nil) -1 2))"],
    )
}

fn auto_complete_exuberant_ctags_candidate_reads_dynamic_caller_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_candidate_reads_dynamic_caller_candidates",
        r##"(with-temp-buffer
                           (insert "    al")
                           (let ((ac-point (point))
                                 (ac-target "al")
                                 (ac-exuberant-ctags-index
                                  '("alpha f C")))
                             (list
                              (let ((candidates '(one two)))
                                (auto-complete-exuberant-ctags-test-candidate
                                 candidates))
                              (let ((candidates nil))
                                (auto-complete-exuberant-ctags-test-candidate
                                 candidates))
                              (let ((candidates 17))
                                (auto-complete-exuberant-ctags-test-error
                                 (lambda ()
                                   (auto-complete-exuberant-ctags-test-candidate
                                    candidates)))))))"##,
        expect![[r#"OK (("alpha") ("alpha") (:signal wrong-type-argument (sequencep 17)))"#]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_exuberant_ctags_candidates_match_practical_prefix_and_order(),
        auto_complete_exuberant_ctags_candidates_are_case_sensitive(),
        auto_complete_exuberant_ctags_candidates_preserve_duplicate_names(),
        auto_complete_exuberant_ctags_candidates_ignore_malformed_index_entries(),
        auto_complete_exuberant_ctags_candidate_limit_is_count_with_off_by_one(),
        auto_complete_exuberant_ctags_long_names_are_not_length_filtered(),
        auto_complete_exuberant_ctags_candidate_near_buffer_start_signals(),
        auto_complete_exuberant_ctags_candidate_reads_dynamic_caller_candidates(),
    ]
}
