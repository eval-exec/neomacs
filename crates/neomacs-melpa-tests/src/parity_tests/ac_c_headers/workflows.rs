use expect_test::expect;

use super::ParityBatchCase;

fn ac_c_headers_completes_an_angle_bracket_include_and_closes_the_directive() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_c_headers_completes_an_angle_bracket_include_and_closes_the_directive",
        r##"(let ((directory (ac-c-headers-test-include-tree))
      (ac-c-headers--files-cache nil)
      (ac-c-headers--symbols-cache nil))
  (ac-c-headers-test-in-buffer
   (let ((cc-search-directories (list directory)))
     (insert "#include <std")
     (let ((candidates (ac-c-headers-test-candidates)))
       (ac-complete)
       (list candidates
             (buffer-string)
             (point)
             (line-number-at-pos)
             (mapcar #'car ac-c-headers--symbols-cache)
             (cdr (assq 'symbol ac-source-c-headers))
             (cdr (assq 'requires ac-source-c-headers)))))))"##,
        expect![[
            r##"OK (("stdio.h") "#include <stdio.h>\n" 20 2 (#("stdio.h" 0 7 (action (lambda nil (when (string-match "\\.h$" candidate) (ac-c-headers--symbols-update candidate) (cond ((looking-at "[>\"]") (forward-char 1) (newline-and-indent)) ((looking-back "#include *<\\([^<]*\\)") (insert ">\n")) (t (insert "\"\n"))))) symbol "h"))) "h" 0)"##
        ]],
    )
}

fn ac_c_headers_completes_a_quoted_include_with_a_closing_quote() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_c_headers_completes_a_quoted_include_with_a_closing_quote",
        r##"(let ((directory (ac-c-headers-test-include-tree))
      (ac-c-headers--files-cache nil)
      (ac-c-headers--symbols-cache nil))
  (ac-c-headers-test-in-buffer
   (let ((cc-search-directories (list directory)))
     (insert "#include \"str")
     (let ((candidates (ac-c-headers-test-candidates)))
       (ac-complete)
       (list candidates
             (buffer-string)
             (point)
             (mapcar #'car ac-c-headers--symbols-cache))))))"##,
        expect![[
            r##"OK (("string.h") "#include \"string.h\"\n" 21 (#("string.h" 0 8 (action (lambda nil (when (string-match "\\.h$" candidate) (ac-c-headers--symbols-update candidate) (cond ((looking-at "[>\"]") (forward-char 1) (newline-and-indent)) ((looking-back "#include *<\\([^<]*\\)") (insert ">\n")) (t (insert "\"\n"))))) symbol "h"))))"##
        ]],
    )
}

fn ac_c_headers_offers_directories_and_descends_into_a_nested_include_path() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_c_headers_offers_directories_and_descends_into_a_nested_include_path",
        r##"(let ((directory (ac-c-headers-test-include-tree))
      (ac-c-headers--files-cache nil)
      (ac-c-headers--symbols-cache nil))
  (ac-c-headers-test-in-buffer
   (let ((cc-search-directories (list directory)))
     (insert "#include <s")
     (let ((top (ac-c-headers-test-candidates)))
       (delete-region (point-min) (point-max))
       (insert "#include <sys/")
       (let ((nested (ac-c-headers-test-candidates)))
         (delete-region (point-min) (point-max))
         (insert "#include <sys/ty")
         (let ((filtered (ac-c-headers-test-candidates)))
           (ac-complete)
           (list top
                 nested
                 filtered
                 (buffer-string)
                 (mapcar #'car ac-c-headers--files-cache))))))))"##,
        expect![[
            r##"OK (("sys/" "stdio.h" "string.h") ("./" "../" "stat.h" "types.h") ("types.h") "#include <sys/types.h>\n" ("sys/" ""))"##
        ]],
    )
}

fn ac_c_headers_completes_symbols_declared_by_the_headers_the_buffer_includes() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_c_headers_completes_symbols_declared_by_the_headers_the_buffer_includes",
        r##"(let ((directory (ac-c-headers-test-include-tree))
      (ac-c-headers--files-cache nil)
      (ac-c-headers--symbols-cache nil))
  (ac-c-headers-test-in-buffer
   (let ((cc-search-directories (list directory)))
     (setq ac-sources '(ac-source-c-header-symbols))
     (insert "#include <stdio.h>\n"
             "#include <string.h>\n"
             "#include <missing.h>\n"
             "int main(void) { pr")
     (let ((candidates (ac-c-headers-test-candidates))
           (everything (sort (copy-sequence (ac-c-headers--symbols-list)) #'string<)))
       (ac-complete)
       (list candidates
             everything
             (sort (mapcar #'car ac-c-headers--symbols-cache) #'string<)
             (buffer-substring-no-properties (line-beginning-position) (point-max))
             (point))))))"##,
        expect![[
            r#"OK (("printf") ("c" "char" "char" "const" "const" "int" "int" "printf" "puts" "size_t" "strlen") ("stdio.h" "string.h") "int main(void) { printf" 84)"#
        ]],
    )
}

fn ac_c_headers_serves_a_prefix_from_its_cache_until_a_new_prefix_is_typed() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_c_headers_serves_a_prefix_from_its_cache_until_a_new_prefix_is_typed",
        r##"(let ((directory (ac-c-headers-test-include-tree))
      (ac-c-headers--files-cache nil)
      (ac-c-headers--symbols-cache nil))
  (ac-c-headers-test-in-buffer
   (let ((cc-search-directories (list directory)))
     (insert "#include <s")
     (let ((first (ac-c-headers-test-candidates)))
       (ac-c-headers-test-write "include/stdlib.h" "void *malloc(unsigned long size);\n")
       (let ((cached (ac-c-headers-test-candidates)))
         (delete-region (point-min) (point-max))
         (insert "#include <st")
         (let ((fresh (ac-c-headers-test-candidates)))
           (list first
                 cached
                 fresh
                 (mapcar #'car ac-c-headers--files-cache))))))))"##,
        expect![[
            r#"OK (("sys/" "stdio.h" "string.h") ("sys/" "stdio.h" "string.h") ("stdio.h" "string.h") (""))"#
        ]],
    )
}

fn ac_c_headers_ignores_non_header_files_and_unknown_prefixes() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_c_headers_ignores_non_header_files_and_unknown_prefixes",
        r##"(let ((directory (ac-c-headers-test-include-tree))
      (ac-c-headers--files-cache nil)
      (ac-c-headers--symbols-cache nil))
  (ac-c-headers-test-in-buffer
   (let ((cc-search-directories (list directory)))
     (insert "#include <nota")
     (let ((no-match (ac-c-headers-test-candidates))
           (buffer-after-no-match (buffer-string)))
       (delete-region (point-min) (point-max))
       (insert "#include <zzz")
       (let ((unknown (ac-c-headers-test-candidates)))
         (delete-region (point-min) (point-max))
         (insert "int main(void) { ret")
         (let ((outside (ac-c-headers-test-candidates)))
           (list no-match
                 buffer-after-no-match
                 unknown
                 outside
                 (buffer-string)
                 (mapcar #'car ac-c-headers--files-cache))))))))"##,
        expect![[
            r##"OK (nil "#include <nota\n\n\n\n\n\n\n\n\n\n\n" nil nil "int main(void) { ret" (""))"##
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_c_headers_completes_an_angle_bracket_include_and_closes_the_directive(),
        ac_c_headers_completes_a_quoted_include_with_a_closing_quote(),
        ac_c_headers_offers_directories_and_descends_into_a_nested_include_path(),
        ac_c_headers_completes_symbols_declared_by_the_headers_the_buffer_includes(),
        ac_c_headers_serves_a_prefix_from_its_cache_until_a_new_prefix_is_typed(),
        ac_c_headers_ignores_non_header_files_and_unknown_prefixes(),
    ]
}
