use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_prefix_parsers_handle_symbols_files_and_c_family_members_in_real_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_prefix_parsers_handle_symbols_files_and_c_family_members_in_real_buffers",
        r##"(mapcar
                          (lambda (case)
                            (with-temp-buffer
                              (insert (car case))
                              (goto-char
                               (or (cdr case)
                                   (point-max)))
                              (list
                               (car case)
                               (point)
                               (ac-prefix-symbol)
                               (ac-prefix-file)
                               (ac-prefix-c-dot)
                               (ac-prefix-c-dot-ref)
                               (ac-prefix-cc-member))))
                          '(("alpha_beta")
                            ("obj.member")
                            ("ptr->member")
                            ("Type::member")
                            ("\"dir/file")
                            ("alpha-beta")
                            ("12345")
                            ("0xbeef")))"##,
        expect![[
            r#"OK (("alpha_beta" 11 1 nil nil nil nil) ("obj.member" 11 5 nil 5 nil nil) ("ptr->member" 12 1 6 nil nil nil) ("Type::member" 13 7 nil nil nil 7) ("\"dir/file" 10 2 2 nil nil nil) ("alpha-beta" 11 1 nil nil nil nil) ("12345" 6 1 nil nil nil nil) ("0xbeef" 7 1 nil nil nil nil))"#
        ]],
    )
}

fn auto_complete_default_prefix_rejects_numeric_literals_but_accepts_mixed_symbols()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_default_prefix_rejects_numeric_literals_but_accepts_mixed_symbols",
        r##"(mapcar
                          (lambda (text)
                            (with-temp-buffer
                              (insert text)
                              (list
                               text
                               (ac-prefix-symbol)
                               (ac-prefix-default)
                               (and
                                (ac-prefix-default)
                                (buffer-substring-no-properties
                                 (ac-prefix-default)
                                 (point))))))
                          '("42"
                            "007bond"
                            "0xff"
                            "0b101"
                            "0o755"
                            "alpha42"
                            "_42"
                            "λ-value"))"##,
        expect![[
            r#"OK (("42" 1 nil nil) ("007bond" 1 nil nil) ("0xff" 1 nil nil) ("0b101" 1 nil nil) ("0o755" 1 nil nil) ("alpha42" 1 1 "alpha42") ("_42" 1 1 "_42") ("λ-value" 1 1 "λ-value"))"#
        ]],
    )
}

fn auto_complete_source_compilation_expands_prefix_and_match_shorthands_and_filters_dependencies()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_source_compilation_expands_prefix_and_match_shorthands_and_filters_dependencies",
        r##"(let ((ac-prefix-definitions
                                '((word . ac-prefix-symbol))))
                           (setq
                            auto-complete-test-available-calls
                            0)
                           (fset
                            'auto-complete-test-available
                            (lambda ()
                              (setq
                               auto-complete-test-available-calls
                               (1+
                                auto-complete-test-available-calls))
                              t))
                           (list
                            (ac-compile-sources
                             '(((candidates list "alpha")
                                (prefix . word)
                                (match . substring))
                               ((candidates list "beta")
                                (available
                                 . auto-complete-test-available))
                               ((candidates list "gamma")
                                (available
                                 . (progn
                                     (setq
                                      auto-complete-test-available-calls
                                      (+
                                       auto-complete-test-available-calls
                                       10))
                                     nil)))
                               ((candidates list "delta")
                                (depends
                                 auto-complete-test-missing-feature))))
                            auto-complete-test-available-calls))"##,
        expect![[
            r#"OK ((((prefix . ac-prefix-symbol) (candidates list "alpha") (prefix . word) (match . ac-match-substring)) ((prefix . ac-prefix-default) (candidates list "beta") (available . auto-complete-test-available))) 11)"#
        ]],
    )
}

fn auto_complete_prefix_resolution_groups_only_sources_at_the_winning_start_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_prefix_resolution_groups_only_sources_at_the_winning_start_point",
        r##"(with-temp-buffer
                          (insert "obj.member")
                          (let ((ac-sources
                                 '(((candidates list "one")
                                    (prefix . ac-prefix-symbol)
                                    (requires . 1))
                                   ((candidates list "two")
                                    (prefix . ac-prefix-c-dot)
                                    (requires . 1))
                                   ((candidates list "three")
                                    (prefix . ac-prefix-c-dot-ref)
                                    (requires . 20))
                                   ((candidates list "four")
                                    (prefix . "obj\\.\\(.*\\)")
                                    (requires . 0)))))
                            (mapcar
                             (lambda (ignored)
                               (let ((info
                                      (ac-prefix
                                       2
                                       ignored)))
                                 (list
                                  ignored
                                  (nth 0 info)
                                  (nth 1 info)
                                  (mapcar
                                   (lambda (source)
                                     (assoc-default
                                      'candidates
                                      source))
                                   (nth 2 info)))))
                             '(nil
                               (ac-prefix-symbol)
                               (ac-prefix-c-dot)
                               (ac-prefix-c-dot
                                ac-prefix-symbol)))))"##,
        expect![[
            r#"OK ((nil ac-prefix-symbol 5 (#3=(list "one") #1=(list "two") #2=(list "four"))) ((ac-prefix-symbol) ac-prefix-c-dot 5 (#1# #2#)) ((ac-prefix-c-dot) ac-prefix-symbol 5 (#3# #2#)) ((ac-prefix-c-dot ac-prefix-symbol) "obj\\.\\(.*\\)" 5 (#2#)))"#
        ]],
    )
}

fn auto_complete_candidate_pipeline_preserves_values_actions_docs_faces_cache_and_limits()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_candidate_pipeline_preserves_values_actions_docs_faces_cache_and_limits",
        r##"(let ((ac-prefix "a")
                               (ac-limit 3)
                               (ac-candidates-cache nil)
                               (source
                                '((candidates
                                   . (lambda ()
                                       (setq
                                        auto-complete-test-candidate-calls
                                        (1+
                                         auto-complete-test-candidate-calls))
                                       '(("alpha"
                                          . payload-alpha)
                                         "alpine"
                                         "amber"
                                         "azure")))
                                  (action
                                   . auto-complete-test-action)
                                  (document
                                   . auto-complete-test-document)
                                  (symbol . "x")
                                  (candidate-face
                                   . auto-complete-test-face)
                                  (selection-face
                                   . auto-complete-test-selection)
                                  (cache))))
                           (setq
                            auto-complete-test-candidate-calls
                            0)
                           (let ((first
                                  (ac-candidates-1 source))
                                 (second
                                  (ac-candidates-1 source)))
                             (list
                              auto-complete-test-candidate-calls
                              (mapcar
                               (lambda (candidate)
                                 (list
                                  (substring-no-properties
                                   candidate)
                                  (popup-item-value candidate)
                                  (popup-item-property
                                   candidate
                                   'action)
                                  (popup-item-property
                                   candidate
                                   'document)
                                  (popup-item-symbol candidate)
                                  (popup-item-face candidate)
                                  (popup-item-selection-face
                                   candidate)))
                               first)
                              (mapcar
                               #'substring-no-properties
                               second)
                              (length ac-candidates-cache))))"##,
        expect![[
            r#"OK (1 (("alpha" payload-alpha auto-complete-test-action auto-complete-test-document "x" auto-complete-test-face auto-complete-test-selection) ("alpine" nil auto-complete-test-action auto-complete-test-document "x" auto-complete-test-face auto-complete-test-selection) ("amber" nil auto-complete-test-action auto-complete-test-document "x" auto-complete-test-face auto-complete-test-selection)) ("alpha" "alpine" "amber") 1)"#
        ]],
    )
}

fn auto_complete_case_policy_changes_real_candidate_filtering_for_lower_and_upper_prefixes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_case_policy_changes_real_candidate_filtering_for_lower_and_upper_prefixes",
        r##"(mapcar
                          (lambda (case)
                            (let ((ac-ignore-case (car case))
                                  (ac-prefix (cadr case))
                                  (ac-use-comphist nil)
                                  (ac-show-menu t)
                                  (ac-current-sources
                                   '(((candidates
                                      list
                                      "alpha"
                                      "Alpha"
                                      "ALPINE"
                                      "almanac")))))
                              (list
                               case
                               (mapcar
                                #'substring-no-properties
                                (ac-candidates))
                               ac-common-part
                               ac-whole-common-part)))
                          '((smart "al")
                            (smart "Al")
                            (smart "AL")
                            (t "Al")
                            (nil "al")))"##,
        expect![[
            r#"OK (((smart "al") ("alpha" "Alpha" "ALPINE" "almanac") "al" "al") ((smart "Al") ("Alpha") "Alpha" "Alpha") ((smart "AL") ("ALPINE") "ALPINE" "ALPINE") ((t "Al") ("alpha" "Alpha" "ALPINE" "almanac") "Al" "Al") ((nil "al") ("alpha" "almanac") "al" "al"))"#
        ]],
    )
}

fn auto_complete_duplicate_reduction_distinguishes_actions_and_limits_work_to_front_window()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_duplicate_reduction_distinguishes_actions_and_limits_work_to_front_window",
        r##"(let* ((a1
                                 (propertize
                                  "same"
                                  'action
                                  'first))
                                (a2
                                 (propertize
                                  "same"
                                  'action
                                  'second))
                                (a3
                                 (propertize
                                  "same"
                                  'action
                                  'first))
                                (tail
                                 (append
                                  (number-sequence 1 25)
                                  '(22 23)))
                                (front
                                 (append
                                  (list a1 a2 a3 "other" "other")
                                  tail))
                                (result
                                 (ac-reduce-candidates
                                  front)))
                           (list
                            (mapcar
                             (lambda (item)
                               (if (stringp item)
                                   (list
                                    (substring-no-properties item)
                                    (popup-item-property
                                     item
                                     'action))
                                 item))
                             result)
                            (length result)
                            (cl-count 22 result)
                            (cl-count 23 result)))"##,
        expect![[
            r#"OK ((("same" first) ("same" second) ("other" nil) 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 22 23) 30 2 2)"#
        ]],
    )
}

fn auto_complete_completion_history_learns_prefix_positions_sorts_and_round_trips()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_completion_history_learns_prefix_positions_sorts_and_round_trips",
        r##"(let ((db (ac-comphist-make)))
                           (dolist
                               (event
                                '(("format" 1)
                                  ("format" 1)
                                  ("format" 3)
                                  ("forward-char" 3)
                                  ("forward-char" 3)
                                  ("function" 1)))
                             (ac-comphist-add
                              db
                              (car event)
                              (cadr event)))
                           (let* ((scores
                                  (mapcar
                                   (lambda (candidate)
                                     (list
                                      candidate
                                      (format
                                       "%.8f"
                                       (ac-comphist-score
                                        db
                                        candidate
                                        1))
                                      (format
                                       "%.8f"
                                       (ac-comphist-score
                                        db
                                        candidate
                                        3))))
                                   '("format"
                                     "forward-char"
                                     "function"
                                     "fresh")))
                                  (sorted
                                   (ac-comphist-sort
                                    db
                                    '("fresh"
                                      "function"
                                      "forward-char"
                                      "format")
                                    1))
                                  (serialized
                                   (ac-comphist-serialize db))
                                  (restored
                                   (ac-comphist-deserialize
                                    serialized)))
                             (list
                              scores
                              sorted
                              (sort
                               (mapcar
                                (lambda (entry)
                                  (list
                                   (car entry)
                                   (append
                                    (cdr entry)
                                    nil)))
                                (car serialized))
                               (lambda (a b)
                                 (string<
                                  (car a)
                                  (car b))))
                              (mapcar
                               (lambda (candidate)
                                 (format
                                  "%.8f"
                                  (ac-comphist-score
                                   restored
                                   candidate
                                   1)))
                               '("format"
                                 "forward-char"
                                 "function")))))"##,
        expect![[
            r#"OK ((("format" "2.26304096" "1.30349980") ("forward-char" "0.22597242" "2.20505475") ("function" "1.22752738" "0.26798621") ("fresh" "0.26000000" "0.28000000")) ("format" "function" "fresh" "forward-char") (("format" (0 2 0 1 0 0)) ("forward-char" (0 0 0 2 0 0 0 0 0 0 0 0)) ("function" (0 1 0 0 0 0 0 0))) ("2.26304096" "0.22597242" "1.22752738"))"#
        ]],
    )
}

fn auto_complete_word_scanner_returns_unique_nearest_candidates_across_both_directions_and_limits()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_word_scanner_returns_unique_nearest_candidates_across_both_directions_and_limits",
        r##"(with-temp-buffer
                          (insert
                           "alpha alpine alphabet\n"
                           "zero alpha amber\n"
                           "al")
                          (let ((point (point))
                                (prefix "al"))
                            (list
                             (ac-candidate-words-in-buffer
                              point
                              prefix
                              nil)
                             (ac-candidate-words-in-buffer
                              point
                              prefix
                              2)
                             (progn
                               (goto-char (point-min))
                               (search-forward
                                "zero ")
                               (ac-candidate-words-in-buffer
                                (point)
                                "a"
                                4)))))"##,
        expect![[
            r#"OK (("alpha" "alphabet" "alpine") ("alpha" "alphabet") ("alphabet" "alpine" "alpha" "amber"))"#
        ]],
    )
}

fn auto_complete_word_index_shares_only_compatible_major_mode_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_word_index_shares_only_compatible_major_mode_buffers",
        r##"(let ((same
                                (generate-new-buffer
                                 " *ac-same*"))
                               (other
                                (generate-new-buffer
                                 " *ac-other*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer same
                                   (emacs-lisp-mode)
                                   (insert
                                    "sharedAlpha sharedBeta")
                                   (ac-update-word-index-1))
                                 (with-current-buffer other
                                   (text-mode)
                                   (insert
                                    "foreignAlpha foreignBeta")
                                   (ac-update-word-index-1))
                                 (with-temp-buffer
                                   (emacs-lisp-mode)
                                   (insert "sha")
                                   (setq
                                    ac-point (point-min)
                                    ac-prefix "sha"
                                    ac-limit nil
                                    ac-match-function
                                    'all-completions
                                    ac-fuzzy-enable nil)
                                   (list
                                    (sort
                                     (ac-word-candidates)
                                     #'string<)
                                    (sort
                                     (ac-word-candidates
                                      (lambda (buffer)
                                        (derived-mode-p
                                         (buffer-local-value
                                          'major-mode
                                          buffer))))
                                     #'string<))))
                             (kill-buffer same)
                             (kill-buffer other)))"##,
        expect![[r#"OK (("sharedAlpha" "sharedBeta") ("sharedAlpha" "sharedBeta"))"#]],
    )
}

fn auto_complete_symbol_function_and_variable_sources_classify_injected_real_lisp_objects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_symbol_function_and_variable_sources_classify_injected_real_lisp_objects",
        r##"(progn
                          (fset
                           'auto-complete-fixture-function
                           (lambda (value)
                             (+ value 7)))
                          (set
                           'auto-complete-fixture-variable
                           'fixture-value)
                          (put
                           'auto-complete-fixture-property-only
                           'fixture-property
                           42)
                          (setq
                           ac-symbols-cache nil
                           ac-functions-cache nil
                           ac-variables-cache nil)
                          (let ((symbols
                                 (ac-symbol-candidates))
                                (functions
                                 (ac-function-candidates))
                                (variables
                                 (ac-variable-candidates)))
                            (list
                             (mapcar
                              (lambda (name)
                                (list
                                 name
                                 (and
                                  (member name symbols)
                                  t)
                                 (and
                                  (member name functions)
                                  t)
                                 (and
                                  (member name variables)
                                  t)))
                              '("auto-complete-fixture-function"
                                "auto-complete-fixture-variable"
                                "auto-complete-fixture-property-only"))
                             (funcall
                              'auto-complete-fixture-function
                              5)
                             auto-complete-fixture-variable)))"##,
        expect![[
            r#"OK ((("auto-complete-fixture-function" t t nil) ("auto-complete-fixture-variable" t nil t) ("auto-complete-fixture-property-only" t nil nil)) 12 fixture-value)"#
        ]],
    )
}

pub(super) fn matching_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_prefix_parsers_handle_symbols_files_and_c_family_members_in_real_buffers(),
        auto_complete_default_prefix_rejects_numeric_literals_but_accepts_mixed_symbols(),
        auto_complete_source_compilation_expands_prefix_and_match_shorthands_and_filters_dependencies(),
        auto_complete_prefix_resolution_groups_only_sources_at_the_winning_start_point(),
        auto_complete_candidate_pipeline_preserves_values_actions_docs_faces_cache_and_limits(),
        auto_complete_case_policy_changes_real_candidate_filtering_for_lower_and_upper_prefixes(),
        auto_complete_duplicate_reduction_distinguishes_actions_and_limits_work_to_front_window(),
        auto_complete_completion_history_learns_prefix_positions_sorts_and_round_trips(),
        auto_complete_word_scanner_returns_unique_nearest_candidates_across_both_directions_and_limits(),
        auto_complete_word_index_shares_only_compatible_major_mode_buffers(),
        auto_complete_symbol_function_and_variable_sources_classify_injected_real_lisp_objects(),
    ]
}
