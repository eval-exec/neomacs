use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_chunk_ports_all_upstream_candidate_match_no_match_and_after_word_cases()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_ports_all_upstream_candidate_match_no_match_and_after_word_cases",
        r##"(mapcar
                           (lambda (case)
                             (with-temp-buffer
                               (insert
                                (car case))
                               (list
                                (car case)
                                (cadr case)
                                (ac-chunk-candidates-from-list
                                 (cadr case)))))
                           '(("a."
                              ("a.x" "a.y" "b.x" "b.y"))
                             ("a.x"
                              ("a.xx" "a.xy" "b.xx" "b.xy"))
                             ("c."
                              ("a.x" "a.y" "b.x" "b.y"))))"##,
        expect![[
            r#"OK (("a." ("a.x" "a.y" "b.x" "b.y") ("a.x" "a.y")) ("a.x" ("a.xx" "a.xy" "b.xx" "b.xy") ("a.xx" "a.xy")) ("c." ("a.x" "a.y" "b.x" "b.y") nil))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_candidates_preserve_input_order_duplicates_and_exact_prefix_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_candidates_preserve_input_order_duplicates_and_exact_prefix_matches",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert "api.user")
                           (let ((dictionary
                                  '("api.users.list"
                                    "api.user"
                                    "api.users.get"
                                    "other.api.user"
                                    "api.users.list"
                                    "api.User")))
                             (list
                              dictionary
                              (ac-chunk-candidates-from-list
                               dictionary))))"##,
        expect![[
            r#"OK (("api.users.list" "api.user" "api.users.get" "other.api.user" "api.users.list" "api.User") ("api.users.list" "api.user" "api.users.get" "api.users.list"))"#
        ]],
    )
}

fn auto_complete_chunk_candidate_filtering_is_case_sensitive_across_mixed_case_prefixes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_candidate_filtering_is_case_sensitive_across_mixed_case_prefixes",
        r##"(mapcar
                           (lambda (prefix)
                             (with-temp-buffer
                               (emacs-lisp-mode)
                               (insert prefix)
                               (list
                                prefix
                                (ac-chunk-candidates-from-list
                                 '("Pkg.Module"
                                   "Pkg.module"
                                   "pkg.Module"
                                   "pkg.module")))))
                           '("Pkg."
                             "pkg."
                             "PKG."))"##,
        expect![[
            r#"OK (("Pkg." ("Pkg.Module" "Pkg.module")) ("pkg." ("pkg.Module" "pkg.module")) ("PKG." nil))"#
        ]],
    )
}

fn auto_complete_chunk_candidates_use_only_text_before_point_during_incremental_editing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_candidates_use_only_text_before_point_during_incremental_editing",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert
                            "service.client.request trailing")
                           (let ((dictionary
                                  '("service.cache.clear"
                                    "service.client.close"
                                    "service.client.request"
                                    "service.client.retry")))
                             (mapcar
                              (lambda (needle)
                                (goto-char
                                 (point-min))
                                (search-forward
                                 needle)
                                (let ((beginning
                                       (ac-chunk-beginning)))
                                  (list
                                   needle
                                   (point)
                                   beginning
                                   (buffer-substring-no-properties
                                    beginning
                                    (point))
                                   (ac-chunk-candidates-from-list
                                    dictionary))))
                              '("service."
                                "service.c"
                                "service.client."
                                "service.client.r"))))"##,
        expect![[
            r#"OK (("service." 9 1 "service." ("service.cache.clear" "service.client.close" "service.client.request" "service.client.retry")) ("service.c" 10 1 "service.c" ("service.cache.clear" "service.client.close" "service.client.request" "service.client.retry")) ("service.client." 16 1 "service.client." ("service.client.close" "service.client.request" "service.client.retry")) ("service.client.r" 17 1 "service.client.r" ("service.client.request" "service.client.retry")))"#
        ]],
    )
}

fn auto_complete_chunk_no_boundary_short_circuits_invalid_candidate_elements() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_no_boundary_short_circuits_invalid_candidate_elements",
        r##"(mapcar
                           (lambda (text)
                             (with-temp-buffer
                               (fundamental-mode)
                               (insert text)
                               (auto-complete-chunk-test-error
                                (lambda ()
                                  (ac-chunk-candidates-from-list
                                   '(42 fixture-symbol
                                     ("nested")))))))
                           '(""
                             "two words"
                             "a.."))"##,
        expect!["OK ((:value nil) (:signal wrong-type-argument (sequencep 42)) (:value nil))"],
    )
}

fn auto_complete_chunk_valid_prefix_surfaces_exact_invalid_candidate_type_signals()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_valid_prefix_surfaces_exact_invalid_candidate_type_signals",
        r##"(mapcar
                           (lambda (dictionary)
                             (with-temp-buffer
                               (emacs-lisp-mode)
                               (insert "a.")
                               (list
                                dictionary
                                (auto-complete-chunk-test-error
                                 (lambda ()
                                   (ac-chunk-candidates-from-list
                                    dictionary))))))
                           '((42)
                             (fixture-symbol)
                             (("a.x"))
                             ("a.x" 42)
                             nil))"##,
        expect![[
            r#"OK (((42) (:signal wrong-type-argument (sequencep 42))) ((fixture-symbol) (:signal wrong-type-argument (sequencep fixture-symbol))) ((("a.x")) (:value nil)) (("a.x" 42) (:signal wrong-type-argument (sequencep 42))) (nil (:value nil)))"#
        ]],
    )
}

fn auto_complete_chunk_candidate_objects_keep_text_properties_and_object_identity()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_candidate_objects_keep_text_properties_and_object_identity",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert "api.")
                           (let* ((first
                                   (propertize
                                    "api.first"
                                    'kind
                                    :method))
                                  (second
                                   (propertize
                                    "api.second"
                                    'kind
                                    :field))
                                  (dictionary
                                   (list
                                    first
                                    "other"
                                    second))
                                  (result
                                   (ac-chunk-candidates-from-list
                                    dictionary)))
                             (list
                              result
                              (mapcar
                               (lambda (candidate)
                                 (list
                                  (get-text-property
                                   0
                                   'kind
                                   candidate)
                                  (or
                                   (eq candidate first)
                                   (eq candidate second))))
                               result)
                              dictionary)))"##,
        expect![[
            r#"OK ((#("api.first" 0 9 (kind :method)) #("api.second" 0 10 (kind :field))) ((:method t) (:field t)) (#("api.first" 0 9 (kind :method)) "other" #("api.second" 0 10 (kind :field))))"#
        ]],
    )
}

fn auto_complete_chunk_filter_does_not_mutate_dictionary_and_returns_fresh_result_spine()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_filter_does_not_mutate_dictionary_and_returns_fresh_result_spine",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert "a.")
                           (let* ((dictionary
                                   (list
                                    "a.x"
                                    "b.x"
                                    "a.y"))
                                  (snapshot
                                   (copy-tree
                                    dictionary))
                                  (result
                                   (ac-chunk-candidates-from-list
                                    dictionary)))
                             (list
                              result
                              dictionary
                              (equal dictionary snapshot)
                              (eq result dictionary)
                              (eq
                               (car result)
                               (car dictionary))
                              (eq
                               (cadr result)
                               (caddr dictionary)))))"##,
        expect![[r#"OK (("a.x" "a.y") ("a.x" "b.x" "a.y") t nil t t)"#]],
    )
}

fn auto_complete_chunk_dynamic_boundary_regex_drives_candidate_prefix_contract() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_chunk_dynamic_boundary_regex_drives_candidate_prefix_contract",
        r##"(with-temp-buffer
                           (insert
                            "prefix::member")
                           (let ((dictionary
                                  '("prefix::member"
                                    "prefix::method"
                                    "member"
                                    "method")))
                             (mapcar
                              (lambda (regex)
                                (let ((ac-chunk-regex
                                       regex))
                                  (list
                                   regex
                                   (ac-chunk-beginning)
                                   (ac-chunk-candidates-from-list
                                    dictionary))))
                              (list
                               ac-chunk-regex
                               (rx
                                (group "::")
                                (+ word)
                                point)
                               (rx
                                (group bol)
                                (+ any)
                                point)))))"##,
        expect![[
            r#"OK (("\\(\\s-\\|\\s(\\|\\s)\\|^\\)\\(?:\\(?:\\w\\|\\s_\\)+\\s.\\)*\\(?:\\w\\|\\s_\\)+\\s.?\\=" nil nil) ("\\(::\\)[[:word:]]+\\=" 9 ("member")) ("\\(^\\).+\\=" 1 ("prefix::member")))"#
        ]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_chunk_ports_all_upstream_candidate_match_no_match_and_after_word_cases(),
        auto_complete_chunk_candidates_preserve_input_order_duplicates_and_exact_prefix_matches(),
        auto_complete_chunk_candidate_filtering_is_case_sensitive_across_mixed_case_prefixes(),
        auto_complete_chunk_candidates_use_only_text_before_point_during_incremental_editing(),
        auto_complete_chunk_no_boundary_short_circuits_invalid_candidate_elements(),
        auto_complete_chunk_valid_prefix_surfaces_exact_invalid_candidate_type_signals(),
        auto_complete_chunk_candidate_objects_keep_text_properties_and_object_identity(),
        auto_complete_chunk_filter_does_not_mutate_dictionary_and_returns_fresh_result_spine(),
        auto_complete_chunk_dynamic_boundary_regex_drives_candidate_prefix_contract(),
    ]
}
