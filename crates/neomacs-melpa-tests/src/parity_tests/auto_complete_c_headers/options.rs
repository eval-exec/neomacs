use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_c_headers_default_directory_provider_returns_configured_value_by_identity()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_default_directory_provider_returns_configured_value_by_identity",
        r##"(let ((achead:include-directories
                '("project" "/sdk/include"
                  "/opt/vendor/include")))
         (let ((result
                (achead:get-include-directories)))
           (list
            result
            (eq result
                achead:include-directories)
            (progn
              (setcar result "changed")
              achead:include-directories))))"##,
        expect![[r#"OK (#1=("changed" "/sdk/include" "/opt/vendor/include") t #1#)"#]],
    )
}

fn auto_complete_c_headers_custom_directory_provider_is_called_once_per_candidate_scan()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_custom_directory_provider_is_called_once_per_candidate_scan",
        r##"(let* ((calls 0)
               (achead:include-patterns
                '("\\.h\\'"))
               (achead:get-include-directories-function
                (lambda ()
                  (setq calls (1+ calls))
                  nil)))
         (list
          (achead:get-include-file-candidates)
          calls
          (achead:get-include-file-candidates
           "nested/")
          calls))"##,
        expect!["OK (nil 1 nil 2)"],
    )
}

fn auto_complete_c_headers_extracts_include_options_in_order_with_empty_and_duplicate_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_extracts_include_options_in_order_with_empty_and_duplicate_values",
        r##"(achead:get-include-directories-from-options
         '("-Wall"
           "-I/usr/include"
           "-I"
           "-isystem"
           "/ignored"
           "-I../relative"
           "-I/usr/include"
           "-DNAME=-Iinside"
           "-Ipath with spaces"))"##,
        expect![[r#"OK ("/usr/include" "" "../relative" "/usr/include" "path with spaces")"#]],
    )
}

fn auto_complete_c_headers_include_option_matching_is_strictly_case_sensitive() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_include_option_matching_is_strictly_case_sensitive",
        r##"(let ((case-fold-search t))
         (list
          (achead:get-include-directories-from-options
           '("-i/lower"
             "-I/upper"
             "-IÜnicode"
             "-include"
             "-I./local"))
          case-fold-search))"##,
        expect![[r#"OK (("/upper" "Ünicode" "./local") t)"#]],
    )
}

fn auto_complete_c_headers_option_parser_signals_on_non_string_members_after_prior_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_option_parser_signals_on_non_string_members_after_prior_matches",
        r##"(list
         (achead-test-error
          (lambda ()
            (achead:get-include-directories-from-options
             '("-Iok" 17 "-Ilater"))))
         (achead-test-error
          (lambda ()
            (achead:get-include-directories-from-options
             nil))))"##,
        expect!["OK ((:signal wrong-type-argument (stringp 17)) (:value nil))"],
    )
}

fn auto_complete_c_headers_default_patterns_cover_supported_extensions_and_suffix_free_cpp_names()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_default_patterns_cover_supported_extensions_and_suffix_free_cpp_names",
        r##"(mapcar
         (lambda (path)
           (list
            path
            (achead:path-should-be-displayed
             path)))
         '("/sdk/vector"
           "/sdk/unordered_map"
           "/sdk/x86-vector"
           "/sdk/foo.h"
           "/sdk/foo.hpp"
           "/sdk/foo.hh"
           "/sdk/foo.H"
           "/sdk/foo.hxx"
           "/sdk/foo.h.in"
           "vector"
           "/sdk/vector2"
           "/sdk/.hidden.h"
           "/sdk/space name"))"##,
        expect![[
            r#"OK (("/sdk/vector" t) ("/sdk/unordered_map" t) ("/sdk/x86-vector" nil) ("/sdk/foo.h" t) ("/sdk/foo.hpp" t) ("/sdk/foo.hh" t) ("/sdk/foo.H" t) ("/sdk/foo.hxx" nil) ("/sdk/foo.h.in" nil) ("vector" nil) ("/sdk/vector2" nil) ("/sdk/.hidden.h" t) ("/sdk/space name" nil))"#
        ]],
    )
}

fn auto_complete_c_headers_custom_pattern_order_nil_and_empty_patterns_have_exact_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_custom_pattern_order_nil_and_empty_patterns_have_exact_semantics",
        r##"(list
         (let ((achead:include-patterns
                '("\\.inc\\'" "^special/")))
           (mapcar
            #'achead:path-should-be-displayed
            '("x.inc" "special/x"
              "x.h" "other")))
         (let ((achead:include-patterns nil))
           (achead:path-should-be-displayed
            "anything.h"))
         (let ((achead:include-patterns
                '("")))
           (achead:path-should-be-displayed
            "anything"))
         (let ((achead:include-patterns
                '("never" "\\.h\\'")))
           (achead:path-should-be-displayed
            "yes.h")))"##,
        expect!["OK ((t t nil nil) nil t t)"],
    )
}

fn auto_complete_c_headers_prefix_regexp_extracts_real_include_and_import_fragments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_prefix_regexp_extracts_real_include_and_import_fragments",
        r##"(mapcar
         (lambda (line)
           (with-temp-buffer
             (insert line)
             (goto-char (point-max))
             (when
                 (re-search-backward
                  achead:ac-prefix nil t)
               (list
                (match-string 0)
                (match-string 1)
                (match-beginning 1)
                (match-end 1)))))
         '("#include <vector"
           "#include \"project/api"
           "# import   < Foundation/NSObject.h"
           "  #include <ignored"
           "#include <>"
           "#include <two words"
           "#include 'single"
           "#include <nested/file.hpp>"))"##,
        expect![[
            r##"OK (("#include <vector" "vector" 11 17) ("#include \"project/api" "project/api" 11 22) nil ("#include <ignored" "ignored" 13 20) nil ("#include <two" "two" 11 14) nil ("#include <nested/file.hpp" "nested/file.hpp" 11 26))"##
        ]],
    )
}

pub(super) fn options_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_c_headers_default_directory_provider_returns_configured_value_by_identity(),
        auto_complete_c_headers_custom_directory_provider_is_called_once_per_candidate_scan(),
        auto_complete_c_headers_extracts_include_options_in_order_with_empty_and_duplicate_values(),
        auto_complete_c_headers_include_option_matching_is_strictly_case_sensitive(),
        auto_complete_c_headers_option_parser_signals_on_non_string_members_after_prior_matches(),
        auto_complete_c_headers_default_patterns_cover_supported_extensions_and_suffix_free_cpp_names(),
        auto_complete_c_headers_custom_pattern_order_nil_and_empty_patterns_have_exact_semantics(),
        auto_complete_c_headers_prefix_regexp_extracts_real_include_and_import_fragments(),
    ]
}
