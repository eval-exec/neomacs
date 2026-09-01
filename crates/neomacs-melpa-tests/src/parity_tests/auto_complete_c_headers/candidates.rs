use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_c_headers_root_scan_returns_real_headers_suffix_free_files_and_directories()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_root_scan_returns_real_headers_suffix_free_files_and_directories",
        r##"(let* ((root
                 (expand-file-name
                  "achead-root-scan"
                  default-directory))
                (achead:include-directories
                 (list root))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (dolist
                   (entry
                    '(("api.h" . "api")
                      ("engine.hpp" . "engine")
                      ("legacy.hh" . "legacy")
                      ("vector" . "vector")
                      ("README" . "readme")
                      ("upper.H" . "upper")
                      (".hidden.h" . "hidden")))
                 (achead-test-write-file
                  root (car entry)
                  (cdr entry)))
               (make-directory
                (expand-file-name
                 "project" root))
               (make-directory
                (expand-file-name
                 "dir.h" root))
               (make-directory
                (expand-file-name
                 ".private" root))
               (achead-test-relative-results
                (achead:get-include-file-candidates)
                root))
           (delete-directory root t)))"##,
        expect![[
            r#"OK (("README" . "README") ("api.h" . "api.h") ("dir.h/" . "dir.h") ("engine.hpp" . "engine.hpp") ("legacy.hh" . "legacy.hh") ("project/" . "project") ("upper.H" . "upper.H") ("vector" . "vector"))"#
        ]],
    )
}

fn auto_complete_c_headers_nested_prefix_scans_only_that_directory_and_preserves_candidate_prefix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_nested_prefix_scans_only_that_directory_and_preserves_candidate_prefix",
        r##"(let* ((root
                 (expand-file-name
                  "achead-nested-scan"
                  default-directory))
                (achead:include-directories
                 (list root))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "top.h" "top")
               (achead-test-write-file
                root "pkg/detail.h" "detail")
               (achead-test-write-file
                root "pkg/vector" "vector")
               (achead-test-write-file
                root "pkg/skip.txt" "skip")
               (make-directory
                (expand-file-name
                 "pkg/deeper" root))
               (achead-test-relative-results
                (achead:get-include-file-candidates
                 "pkg/")
                root))
           (delete-directory root t)))"##,
        expect![[
            r#"OK (("pkg/deeper/" . "pkg/deeper") ("pkg/detail.h" . "pkg/detail.h") ("pkg/vector" . "pkg/vector"))"#
        ]],
    )
}

fn auto_complete_c_headers_exact_duplicate_include_directories_are_scanned_once_in_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_exact_duplicate_include_directories_are_scanned_once_in_order",
        r##"(let* ((first-root
                 (expand-file-name
                  "achead-dedupe-first"
                  default-directory))
                (second-root
                 (expand-file-name
                  "achead-dedupe-second"
                  default-directory))
                (achead:include-directories
                 (list first-root
                       first-root
                       second-root
                       first-root))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                first-root)
               (achead-test-reset-directory
                second-root)
               (achead-test-write-file
                first-root "first.h" "first")
               (achead-test-write-file
                second-root "second.h" "second")
               (let ((results
                      (achead:get-include-file-candidates)))
                 (list
                  (mapcar #'car results)
                  (mapcar
                   (lambda (entry)
                     (cond
                      ((string-prefix-p
                        first-root
                        (cdr entry))
                       'first-root)
                      ((string-prefix-p
                        second-root
                        (cdr entry))
                       'second-root)
                      (t 'unknown)))
                   results)
                  (length
                   achead:include-cache))))
           (delete-directory first-root t)
           (delete-directory second-root t)))"##,
        expect![[r#"OK (("second.h" "first.h") (second-root first-root) 2)"#]],
    )
}

fn auto_complete_c_headers_same_candidate_from_multiple_roots_preserves_both_paths_and_first_lookup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_same_candidate_from_multiple_roots_preserves_both_paths_and_first_lookup",
        r##"(let* ((first-root
                 (expand-file-name
                  "achead-shadow-first"
                  default-directory))
                (second-root
                 (expand-file-name
                  "achead-shadow-second"
                  default-directory))
                (achead:include-directories
                 (list first-root second-root))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                first-root)
               (achead-test-reset-directory
                second-root)
               (achead-test-write-file
                first-root "shared.h" "first")
               (achead-test-write-file
                second-root "shared.h" "second")
               (let ((results
                      (achead:get-include-file-candidates)))
                 (setq
                  achead:ac-latest-results-alist
                  results)
                 (list
                  (mapcar #'car results)
                  (mapcar
                   (lambda (entry)
                     (with-temp-buffer
                       (insert-file-contents
                        (cdr entry))
                       (buffer-string)))
                   results)
                  (achead:documentation-for-candidate
                   "shared.h"))))
           (delete-directory first-root t)
           (delete-directory second-root t)))"##,
        expect![[
            r#"OK (("shared.h" "shared.h") ("first" "second") "[ORACLE-SANDBOX]/achead-shadow-first/shared.h\n--------------------------\nfirst")"#
        ]],
    )
}

fn auto_complete_c_headers_nil_patterns_still_offer_directories_but_no_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_nil_patterns_still_offer_directories_but_no_files",
        r##"(let* ((root
                 (expand-file-name
                  "achead-nil-patterns"
                  default-directory))
                (achead:include-directories
                 (list root))
                (achead:include-patterns nil)
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "api.h" "api")
               (achead-test-write-file
                root "vector" "vector")
               (make-directory
                (expand-file-name
                 "nested" root))
               (achead-test-relative-results
                (achead:get-include-file-candidates)
                root))
           (delete-directory root t)))"##,
        expect![[r#"OK (("nested/" . "nested"))"#]],
    )
}

fn auto_complete_c_headers_custom_pattern_can_include_arbitrary_files_without_hiding_directories()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_custom_pattern_can_include_arbitrary_files_without_hiding_directories",
        r##"(let* ((root
                 (expand-file-name
                  "achead-custom-pattern"
                  default-directory))
                (achead:include-directories
                 (list root))
                (achead:include-patterns
                 '("\\.inc\\'"))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "config.inc" "config")
               (achead-test-write-file
                root "api.h" "api")
               (achead-test-write-file
                root "plain" "plain")
               (make-directory
                (expand-file-name
                 "directory.txt" root))
               (achead-test-relative-results
                (achead:get-include-file-candidates)
                root))
           (delete-directory root t)))"##,
        expect![[r#"OK (("config.inc" . "config.inc") ("directory.txt/" . "directory.txt"))"#]],
    )
}

fn auto_complete_c_headers_remote_inspection_prefixes_only_directory_listing_probe()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_remote_inspection_prefixes_only_directory_listing_probe",
        r##"(let ((default-directory
                "/ssh:host:/project/")
               (achead:inspect-remote-directories
                t)
               (achead:include-directories
                '("/usr/include"))
               (achead:include-cache nil)
               (directory-probes nil)
               (directory-checks nil))
         (cl-letf
             (((symbol-function
                'file-remote-p)
               (lambda (_path)
                 "/ssh:host:"))
              ((symbol-function
                'directory-files)
               (lambda (directory
                        &optional _full match
                        &rest _arguments)
                 (push
                  (list directory match)
                  directory-probes)
                 '("api.h" "subdir")))
              ((symbol-function
                'file-directory-p)
               (lambda (path)
                 (push path directory-checks)
                 (string-suffix-p
                  "subdir" path))))
           (list
            (achead:get-include-file-candidates)
            (nreverse directory-probes)
            (nreverse directory-checks))))"##,
        expect![[
            r#"OK ((("api.h" . "/usr/include/api.h") ("subdir/" . "/usr/include/subdir")) (("/ssh:host:/usr/include/" "^[^.]")) ("/usr/include/api.h" "/usr/include/api.h" "/usr/include/subdir" "/usr/include/subdir"))"#
        ]],
    )
}

fn auto_complete_c_headers_disabled_remote_inspection_uses_unprefixed_directory() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_c_headers_disabled_remote_inspection_uses_unprefixed_directory",
        r##"(let ((default-directory
                "/ssh:host:/project/")
               (achead:inspect-remote-directories
                nil)
               (achead:include-directories
                '("/usr/include"))
               (achead:include-cache nil)
               (probes nil))
         (cl-letf
             (((symbol-function
                'file-remote-p)
               (lambda (_path)
                 (error
                  "must not inspect remote")))
              ((symbol-function
                'directory-files)
               (lambda (directory
                        &rest _arguments)
                 (push directory probes)
                 '("api.h")))
              ((symbol-function
                'file-directory-p)
               (lambda (_path) nil)))
           (list
            (achead:get-include-file-candidates)
            probes)))"##,
        expect![[r#"OK ((("api.h" . "/usr/include/api.h")) ("/usr/include/"))"#]],
    )
}

fn auto_complete_c_headers_relative_include_roots_resolve_file_checks_against_default_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_relative_include_roots_resolve_file_checks_against_default_directory",
        r##"(let* ((root
                 (expand-file-name
                  "achead-relative-root"
                  default-directory))
                (default-directory
                 (file-name-as-directory
                  root))
                (achead:include-directories
                 '("include"))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "include/local.h"
                "local")
               (make-directory
                (expand-file-name
                 "include/sub" root))
               (list
                (achead:get-include-file-candidates)
                (mapcar
                 (lambda (entry)
                   (file-exists-p
                    (cdr entry)))
                 (achead:get-include-file-candidates))))
           (delete-directory root t)))"##,
        expect![[r#"OK ((("local.h" . "include/local.h") ("sub/" . "include/sub")) (t t))"#]],
    )
}

fn auto_complete_c_headers_non_directory_basedir_uses_parent_scan_but_concatenates_raw_suffix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_non_directory_basedir_uses_parent_scan_but_concatenates_raw_suffix",
        r##"(let* ((root
                 (expand-file-name
                  "achead-partial-basedir"
                  default-directory))
                (achead:include-directories
                 (list root))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "pkg/api.h" "api")
               (achead-test-write-file
                root "pkg/vector" "vector")
               (achead-test-relative-results
                (achead:get-include-file-candidates
                 "pkg/ap")
                root))
           (delete-directory root t)))"##,
        expect![[r#"OK (("pkg/apapi.h" . "pkg/api.h") ("pkg/apvector" . "pkg/vector"))"#]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_c_headers_root_scan_returns_real_headers_suffix_free_files_and_directories(),
        auto_complete_c_headers_nested_prefix_scans_only_that_directory_and_preserves_candidate_prefix(),
        auto_complete_c_headers_exact_duplicate_include_directories_are_scanned_once_in_order(),
        auto_complete_c_headers_same_candidate_from_multiple_roots_preserves_both_paths_and_first_lookup(),
        auto_complete_c_headers_nil_patterns_still_offer_directories_but_no_files(),
        auto_complete_c_headers_custom_pattern_can_include_arbitrary_files_without_hiding_directories(),
        auto_complete_c_headers_remote_inspection_prefixes_only_directory_listing_probe(),
        auto_complete_c_headers_disabled_remote_inspection_uses_unprefixed_directory(),
        auto_complete_c_headers_relative_include_roots_resolve_file_checks_against_default_directory(),
        auto_complete_c_headers_non_directory_basedir_uses_parent_scan_but_concatenates_raw_suffix(),
    ]
}
