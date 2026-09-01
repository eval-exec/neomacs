use expect_test::expect;

use super::ParityBatchCase;

fn atcoder_tools_open_problem_parses_realistic_nested_metadata_and_browses_exact_url()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_open_problem_parses_realistic_nested_metadata_and_browses_exact_url",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (metadata
                (atcoder-tools-test-write-file
                 root
                 "abc133/A/metadata.json"
                 (concat
                  "{\n"
                  "  \"code_filename\": \"main.cpp\",\n"
                  "  \"judge\": {\"judge_type\": \"normal\"},\n"
                  "  \"lang\": \"cpp\",\n"
                  "  \"problem\": {\n"
                  "    \"alphabet\": \"A\",\n"
                  "    \"contest\": {\"contest_id\": \"abc133\"},\n"
                  "    \"problem_id\": \"abc133_a\"\n"
                  "  },\n"
                  "  \"sample_in_pattern\": \"in_*.txt\",\n"
                  "  \"sample_out_pattern\": \"out_*.txt\"\n"
                  "}\n")))
               browsed)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (url &rest arguments)
                  (setq browsed
                        (list url arguments))
                  :opened)))
            (list
             (atcoder-tools--open-problem
              metadata)
             browsed
             (file-relative-name
              metadata
              root)
             (secure-hash
              'sha256
              (atcoder-tools-test-read-file
               metadata)))))"##,
        expect![[
            r#"OK (:opened ("https://atcoder.jp/contests/abc133/tasks/abc133_a" nil) "abc133/A/metadata.json" "c06bc8bc02204bf1743d3a4ad0fc93f6619b47c4e02a7f5076631ce56ae580a9")"#
        ]],
    )
}

fn atcoder_tools_open_problem_interpolates_unicode_spaces_and_reserved_ids_without_encoding()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_open_problem_interpolates_unicode_spaces_and_reserved_ids_without_encoding",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (metadata
                (atcoder-tools-test-write-file
                 root
                 "special/metadata.json"
                 (concat
                  "{\"problem\": {"
                  "\"contest\": {\"contest_id\": \"春 2026/x?y\"},"
                  "\"problem_id\": \"task #1/β\""
                  "}}")))
               urls)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (url &rest _)
                  (push url urls)
                  :opened)))
            (list
             (atcoder-tools--open-problem
              metadata)
             (nreverse urls))))"##,
        expect![[r#"OK (:opened ("https://atcoder.jp/contests/春 2026/x?y/tasks/task #1/β"))"#]],
    )
}

fn atcoder_tools_missing_unreadable_metadata_has_exact_early_error_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_missing_unreadable_metadata_has_exact_early_error_contract",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (missing
                (expand-file-name
                 "abc404/Z/metadata.json"
                 root))
               browse-calls)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (&rest arguments)
                  (push arguments browse-calls))))
            (list
             (file-readable-p missing)
             (atcoder-tools-test-error-data
              (lambda ()
                (atcoder-tools--open-problem
                 missing)))
             browse-calls)))"##,
        expect![[
            r#"OK (nil (:error error ("Could not retrieve information from metadata.json")) nil)"#
        ]],
    )
}

fn atcoder_tools_malformed_metadata_json_matrix_records_parser_outcomes_and_browser_calls()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_malformed_metadata_json_matrix_records_parser_outcomes_and_browser_calls",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (cases
                '(("empty" . "")
                  ("truncated" . "{\"problem\":")
                  ("trailing" . "{\"problem\": {}} garbage")
                  ("scalar" . "42")
                  ("array" . "[]")))
               browse-calls
               observations)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (&rest arguments)
                  (push arguments browse-calls)
                  :opened)))
            (dolist (case cases)
              (let ((file
                     (atcoder-tools-test-write-file
                      root
                      (concat
                       (car case)
                       "/metadata.json")
                      (cdr case))))
                (push
                 (list
                  (car case)
                  (atcoder-tools-test-error-data
                   (lambda ()
                     (atcoder-tools--open-problem
                      file))))
                 observations))))
          (list
           (nreverse observations)
           (nreverse browse-calls)))"##,
        expect![[
            r#"OK ((("empty" (:error json-end-of-file nil)) ("truncated" (:error json-end-of-file nil)) ("trailing" (:ok :opened)) ("scalar" (:error wrong-type-argument (listp 42))) ("array" (:error wrong-type-argument (listp [])))) (("https://atcoder.jp/contests/nil/tasks/nil")))"#
        ]],
    )
}

fn atcoder_tools_partial_metadata_interpolates_missing_fields_as_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_partial_metadata_interpolates_missing_fields_as_nil",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (documents
                '(("{}" . "empty-object")
                  ("{\"problem\": {}}"
                   . "empty-problem")
                  ("{\"problem\": {\"contest\": {\"contest_id\": \"abc\"}}}"
                   . "contest-only")
                  ("{\"problem\": {\"problem_id\": \"task\"}}"
                   . "problem-only")))
               observations)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (url &rest _)
                  url)))
            (dolist (document documents)
              (let ((file
                     (atcoder-tools-test-write-file
                      root
                      (concat
                       (cdr document)
                       "/metadata.json")
                      (car document))))
                (push
                 (list
                  (cdr document)
                  (atcoder-tools-test-error-data
                   (lambda ()
                     (atcoder-tools--open-problem
                      file))))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK (("empty-object" (:ok "https://atcoder.jp/contests/nil/tasks/nil")) ("empty-problem" (:ok "https://atcoder.jp/contests/nil/tasks/nil")) ("contest-only" (:ok "https://atcoder.jp/contests/abc/tasks/nil")) ("problem-only" (:ok "https://atcoder.jp/contests/nil/tasks/task")))"#
        ]],
    )
}

fn atcoder_tools_metadata_value_types_follow_format_string_coercion_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_metadata_value_types_follow_format_string_coercion_exactly",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (documents
                '(("{\"problem\":{\"contest\":{\"contest_id\":123},\"problem_id\":false}}"
                   . "number-bool")
                  ("{\"problem\":{\"contest\":{\"contest_id\":[\"a\",\"b\"]},\"problem_id\":{\"x\":1}}}"
                   . "array-object")
                  ("{\"problem\":{\"contest\":{\"contest_id\":null},\"problem_id\":0}}"
                   . "null-number")))
               observations)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (url &rest _)
                  url)))
            (dolist (document documents)
              (let ((file
                     (atcoder-tools-test-write-file
                      root
                      (concat
                       (cdr document)
                       "/metadata.json")
                      (car document))))
                (push
                 (list
                  (cdr document)
                  (atcoder-tools-test-error-data
                   (lambda ()
                     (atcoder-tools--open-problem
                      file))))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK (("number-bool" (:ok "https://atcoder.jp/contests/123/tasks/:json-false")) ("array-object" (:ok "https://atcoder.jp/contests/[a b]/tasks/((x . 1))")) ("null-number" (:ok "https://atcoder.jp/contests/nil/tasks/0")))"#
        ]],
    )
}

fn atcoder_tools_public_open_problem_uses_metadata_sibling_of_buffer_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_public_open_problem_uses_metadata_sibling_of_buffer_file",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "contest 100/A/main solution.cpp"
                 "source"))
               observed)
          (cl-letf
              (((symbol-function
                 'atcoder-tools--open-problem)
                (lambda (metadata)
                  (setq observed
                        (atcoder-tools-test-normalize
                         metadata root))
                  :delegated)))
            (with-temp-buffer
              (setq buffer-file-name source)
              (list
               (atcoder-tools-open-problem)
               (call-interactively
                #'atcoder-tools-open-problem)
               observed))))"##,
        expect![[r#"OK (:delegated :delegated "[ROOT]/contest 100/A/metadata.json")"#]],
    )
}

fn atcoder_tools_public_open_problem_unsaved_buffer_preserves_exact_path_error() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atcoder_tools_public_open_problem_unsaved_buffer_preserves_exact_path_error",
        r##"(with-temp-buffer
          (setq buffer-file-name nil)
          (list
           (atcoder-tools-test-error-data
            (lambda ()
              (atcoder-tools-open-problem)))
           (atcoder-tools-test-error-data
            (lambda ()
              (call-interactively
               #'atcoder-tools-open-problem)))))"##,
        expect![
            "OK ((:error wrong-type-argument (stringp nil)) (:error wrong-type-argument (stringp nil)))"
        ],
    )
}

fn atcoder_tools_browser_failure_propagates_after_successful_metadata_parse() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_browser_failure_propagates_after_successful_metadata_parse",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (metadata
                (atcoder-tools-test-write-file
                 root
                 "abc500/C/metadata.json"
                 "{\"problem\":{\"contest\":{\"contest_id\":\"abc500\"},\"problem_id\":\"abc500_c\"}}"))
               calls)
          (cl-letf
              (((symbol-function 'browse-url)
                (lambda (url &rest _)
                  (push url calls)
                  (error "browser unavailable"))))
            (list
             (atcoder-tools-test-error-data
              (lambda ()
                (atcoder-tools--open-problem
                 metadata)))
             (nreverse calls)
             (file-readable-p metadata))))"##,
        expect![[
            r#"OK ((:error error ("browser unavailable")) ("https://atcoder.jp/contests/abc500/tasks/abc500_c") t)"#
        ]],
    )
}

pub(super) fn metadata_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atcoder_tools_open_problem_parses_realistic_nested_metadata_and_browses_exact_url(),
        atcoder_tools_open_problem_interpolates_unicode_spaces_and_reserved_ids_without_encoding(),
        atcoder_tools_missing_unreadable_metadata_has_exact_early_error_contract(),
        atcoder_tools_malformed_metadata_json_matrix_records_parser_outcomes_and_browser_calls(),
        atcoder_tools_partial_metadata_interpolates_missing_fields_as_nil(),
        atcoder_tools_metadata_value_types_follow_format_string_coercion_exactly(),
        atcoder_tools_public_open_problem_uses_metadata_sibling_of_buffer_file(),
        atcoder_tools_public_open_problem_unsaved_buffer_preserves_exact_path_error(),
        atcoder_tools_browser_failure_propagates_after_successful_metadata_parse(),
    ]
}
