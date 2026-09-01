use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_c_headers_directory_listing_excludes_dot_entries_and_caches_visible_names()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_directory_listing_excludes_dot_entries_and_caches_visible_names",
        r##"(let* ((root
                 (expand-file-name
                  "achead-listing"
                  default-directory))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "zeta.hpp" "z")
               (achead-test-write-file
                root "alpha.h" "a")
               (achead-test-write-file
                root ".hidden.h" "hidden")
               (make-directory
                (expand-file-name
                 "nested" root))
               (list
                (achead:file-list-for-directory
                 root)
                (mapcar
                 (lambda (entry)
                   (cons
                    (file-name-nondirectory
                     (directory-file-name
                      (car entry)))
                    (cdr entry)))
                 achead:include-cache)))
           (delete-directory root t)))"##,
        expect![[r#"OK (#1=("alpha.h" "nested" "zeta.hpp") (("achead-listing" . #1#)))"#]],
    )
}

fn auto_complete_c_headers_nonempty_cache_is_reused_after_filesystem_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_nonempty_cache_is_reused_after_filesystem_changes",
        r##"(let* ((root
                 (expand-file-name
                  "achead-cache-hit"
                  default-directory))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "first.h" "one")
               (let ((first
                      (achead:file-list-for-directory
                       root)))
                 (achead-test-write-file
                  root "second.h" "two")
                 (delete-file
                  (expand-file-name
                   "first.h" root))
                 (list
                  first
                  (achead:file-list-for-directory
                   root)
                  (directory-files
                   root nil "^[^.]")
                  (length
                   achead:include-cache))))
           (delete-directory root t)))"##,
        expect![[r#"OK (#1=("first.h") #1# ("second.h") 1)"#]],
    )
}

fn auto_complete_c_headers_cache_keys_distinguish_exact_directory_spellings() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_cache_keys_distinguish_exact_directory_spellings",
        r##"(let* ((root
                 (expand-file-name
                  "achead-cache-keys"
                  default-directory))
                (with-slash
                 (file-name-as-directory root))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "api.hh" "api")
               (list
                (achead:file-list-for-directory
                 root)
                (achead:file-list-for-directory
                 with-slash)
                (mapcar
                 (lambda (entry)
                   (list
                    (string-suffix-p
                     "/" (car entry))
                    (cdr entry)))
                 achead:include-cache)))
           (delete-directory root t)))"##,
        expect![[r#"OK (#2=("api.hh") #1=("api.hh") ((t #1#) (nil #2#)))"#]],
    )
}

fn auto_complete_c_headers_missing_directory_error_is_suppressed_and_not_cached() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_c_headers_missing_directory_error_is_suppressed_and_not_cached",
        r##"(let* ((root
                 (expand-file-name
                  "achead-missing"
                  default-directory))
                (achead:include-cache nil))
         (when (file-exists-p root)
           (delete-directory root t))
         (let ((missing
                (achead:file-list-for-directory
                 root)))
           (make-directory root t)
           (achead-test-write-file
            root "later.h" "later")
           (unwind-protect
               (list
                missing
                achead:include-cache
                (achead:file-list-for-directory
                 root)
                (length
                 achead:include-cache))
             (delete-directory root t))))"##,
        expect![[r#"OK (nil nil ("later.h") 1)"#]],
    )
}

fn auto_complete_c_headers_empty_directory_nil_result_is_relisted_and_duplicate_cached()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_empty_directory_nil_result_is_relisted_and_duplicate_cached",
        r##"(let ((achead:include-cache nil)
               (calls 0))
         (cl-letf
             (((symbol-function
                'directory-files)
               (lambda (&rest _arguments)
                 (setq calls (1+ calls))
                 nil)))
           (list
            (achead:file-list-for-directory
             "/virtual/empty")
            (achead:file-list-for-directory
             "/virtual/empty")
            calls
            achead:include-cache)))"##,
        expect![[r#"OK (nil nil 2 (("/virtual/empty") ("/virtual/empty")))"#]],
    )
}

fn auto_complete_c_headers_false_cached_value_also_forces_refresh_and_prepends_new_entry()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_false_cached_value_also_forces_refresh_and_prepends_new_entry",
        r##"(let ((achead:include-cache
                '(("/virtual" . nil)
                  ("/other" "old.h")))
               (calls 0))
         (cl-letf
             (((symbol-function
                'directory-files)
               (lambda (&rest _arguments)
                 (setq calls (1+ calls))
                 '("fresh.h"))))
           (list
            (achead:file-list-for-directory
             "/virtual")
            calls
            achead:include-cache)))"##,
        expect![[r#"OK (#1=("fresh.h") 1 (("/virtual" . #1#) ("/virtual") ("/other" "old.h")))"#]],
    )
}

fn auto_complete_c_headers_cached_names_are_shared_objects_not_copied() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_cached_names_are_shared_objects_not_copied",
        r##"(let* ((root
                 (expand-file-name
                  "achead-cache-identity"
                  default-directory))
                (achead:include-cache nil))
         (unwind-protect
             (progn
               (achead-test-reset-directory
                root)
               (achead-test-write-file
                root "one.h" "one")
               (achead-test-write-file
                root "two.h" "two")
               (let* ((first
                       (achead:file-list-for-directory
                        root))
                      (second
                       (achead:file-list-for-directory
                        root)))
                 (setcar first "mutated.h")
                 (list
                  (eq first second)
                  second
                  (assoc-default
                   root
                   achead:include-cache))))
           (delete-directory root t)))"##,
        expect![[r#"OK (t #1=("mutated.h" "two.h") #1#)"#]],
    )
}

fn auto_complete_c_headers_cache_can_be_preseeded_to_avoid_any_filesystem_probe() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_c_headers_cache_can_be_preseeded_to_avoid_any_filesystem_probe",
        r##"(let ((achead:include-cache
                '(("/sdk/include"
                   "vector" "api.h")))
               (calls 0))
         (cl-letf
             (((symbol-function
                'directory-files)
               (lambda (&rest _arguments)
                 (setq calls (1+ calls))
                 (error
                  "filesystem must not run"))))
           (list
            (achead:file-list-for-directory
             "/sdk/include")
            calls
            achead:include-cache)))"##,
        expect![[r#"OK (#1=("vector" "api.h") 0 (("/sdk/include" . #1#)))"#]],
    )
}

pub(super) fn filesystem_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_c_headers_directory_listing_excludes_dot_entries_and_caches_visible_names(),
        auto_complete_c_headers_nonempty_cache_is_reused_after_filesystem_changes(),
        auto_complete_c_headers_cache_keys_distinguish_exact_directory_spellings(),
        auto_complete_c_headers_missing_directory_error_is_suppressed_and_not_cached(),
        auto_complete_c_headers_empty_directory_nil_result_is_relisted_and_duplicate_cached(),
        auto_complete_c_headers_false_cached_value_also_forces_refresh_and_prepends_new_entry(),
        auto_complete_c_headers_cached_names_are_shared_objects_not_copied(),
        auto_complete_c_headers_cache_can_be_preseeded_to_avoid_any_filesystem_probe(),
    ]
}
