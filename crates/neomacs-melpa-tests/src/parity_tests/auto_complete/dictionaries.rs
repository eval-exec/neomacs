use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_file_dictionary_caches_contents_until_explicit_clear() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_file_dictionary_caches_contents_until_explicit_clear",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-dictionary-cache"
                                  (getenv "TMPDIR")))
                                (file
                                 (expand-file-name
                                  "words"
                                  root)))
                           (make-directory root t)
                           (with-temp-file file
                             (insert
                              "alpha\n"
                              "beta\n"
                              "alpha\n"))
                           (clrhash ac-file-dictionary)
                           (let ((first
                                  (ac-file-dictionary file)))
                             (with-temp-file file
                               (insert
                                "gamma\n"
                                "delta\n"))
                             (let ((cached
                                    (ac-file-dictionary file)))
                               (ac-clear-dictionary-cache)
                               (list
                                first
                                cached
                                (ac-file-dictionary file)
                                (hash-table-count
                                 ac-file-dictionary)))))"##,
        expect![[r#"OK (#1=("alpha" "beta" "alpha") #1# ("gamma" "delta") 1)"#]],
    )
}

fn auto_complete_buffer_dictionary_combines_user_mode_extension_and_explicit_files_in_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_buffer_dictionary_combines_user_mode_extension_and_explicit_files_in_order",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-dictionary-combine"
                                  (getenv "TMPDIR")))
                                (dictionary-directory
                                 (expand-file-name
                                  "mode-dicts"
                                  root))
                                (explicit-file
                                 (expand-file-name
                                  "explicit.dict"
                                  root)))
                           (make-directory
                            dictionary-directory
                            t)
                           (with-temp-file
                               (expand-file-name
                                "text-mode"
                                dictionary-directory)
                             (insert
                              "mode-one\n"
                              "shared\n"))
                           (with-temp-file
                               (expand-file-name
                                "notes"
                                dictionary-directory)
                             (insert
                              "extension-one\n"
                              "shared\n"))
                           (with-temp-file explicit-file
                             (insert
                              "file-one\n"
                              "shared\n"))
                           (clrhash ac-file-dictionary)
                           (with-temp-buffer
                             (text-mode)
                             (setq
                              buffer-file-name
                              (expand-file-name
                               "project.notes"
                               root))
                             (let ((ac-user-dictionary
                                    '("user-one"
                                      "shared"))
                                   (ac-dictionary-directories
                                    (list
                                     dictionary-directory))
                                   (ac-dictionary-files
                                    (list explicit-file)))
                               (list
                                (ac-mode-dictionary
                                 major-mode)
                                (ac-buffer-dictionary)
                                (local-variable-p
                                 'ac-buffer-dictionary)
                                (ac-buffer-dictionary)))))"##,
        expect![[
            r#"OK (("mode-one" "shared" "extension-one" "shared") #1=("user-one" "shared" "mode-one" "shared" "extension-one" "shared" "file-one" "shared") t #1#)"#
        ]],
    )
}

fn auto_complete_dictionary_cache_is_buffer_local_and_clear_invalidates_every_live_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_dictionary_cache_is_buffer_local_and_clear_invalidates_every_live_buffer",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-dictionary-buffers"
                                  (getenv "TMPDIR")))
                                (file
                                 (expand-file-name
                                  "shared.dict"
                                  root))
                                (first
                                 (generate-new-buffer
                                  " *ac-dict-first*"))
                                (second
                                 (generate-new-buffer
                                  " *ac-dict-second*")))
                           (make-directory root t)
                           (with-temp-file file
                             (insert "before\n"))
                           (unwind-protect
                               (let ((ac-user-dictionary nil)
                                     (ac-dictionary-directories nil)
                                     (ac-dictionary-files
                                      (list file)))
                                 (with-current-buffer first
                                   (ac-buffer-dictionary))
                                 (with-current-buffer second
                                   (ac-buffer-dictionary))
                                 (with-temp-file file
                                   (insert "after\n"))
                                 (let ((before
                                        (mapcar
                                         (lambda (buffer)
                                           (with-current-buffer
                                               buffer
                                             (list
                                              (local-variable-p
                                               'ac-buffer-dictionary)
                                              (ac-buffer-dictionary))))
                                         (list first second))))
                                   (ac-clear-dictionary-cache)
                                   (list
                                    before
                                    (mapcar
                                     (lambda (buffer)
                                       (with-current-buffer
                                           buffer
                                         (list
                                          (local-variable-p
                                           'ac-buffer-dictionary)
                                          (ac-buffer-dictionary))))
                                     (list first second)))))
                             (kill-buffer first)
                             (kill-buffer second)))"##,
        expect![[r#"OK (((t #1=("before")) (t #1#)) ((nil #2=("after")) (nil #2#)))"#]],
    )
}

fn auto_complete_missing_dictionary_file_is_cached_as_nil_until_cache_clear() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_missing_dictionary_file_is_cached_as_nil_until_cache_clear",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-dictionary-missing"
                                  (getenv "TMPDIR")))
                                (file
                                 (expand-file-name
                                  "later.dict"
                                  root)))
                           (make-directory root t)
                           (when (file-exists-p file)
                             (delete-file file))
                           (clrhash ac-file-dictionary)
                           (let ((missing
                                  (ac-file-dictionary file)))
                             (with-temp-file file
                               (insert "appeared\n"))
                             (let ((still-cached
                                    (ac-file-dictionary file)))
                               (ac-clear-dictionary-cache)
                               (list
                                missing
                                still-cached
                                (ac-file-dictionary file)))))"##,
        expect![[r#"OK (nil ("appeared") ("appeared"))"#]],
    )
}

fn auto_complete_builtin_c_and_python_dictionaries_support_practical_prefix_queries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_builtin_c_and_python_dictionaries_support_practical_prefix_queries",
        r##"(let ((ac-dictionary-files nil)
                               (ac-user-dictionary nil))
                           (list
                            (with-temp-buffer
                              (c-mode)
                              (let ((dictionary
                                     (ac-buffer-dictionary)))
                                (list
                                 (length dictionary)
                                 (all-completions
                                  "str"
                                  dictionary)
                                 (all-completions
                                  "volatile"
                                  dictionary)
                                 (and
                                  (member
                                   "while"
                                   dictionary)
                                  t))))
                            (with-temp-buffer
                              (python-mode)
                              (let ((dictionary
                                     (ac-buffer-dictionary)))
                                (list
                                 (length dictionary)
                                 (all-completions
                                  "Import"
                                  dictionary)
                                 (all-completions
                                  "Unicode"
                                  dictionary)
                                 (all-completions
                                  "zip"
                                  dictionary)
                                 (and
                                  (member
                                   "ArithmeticError"
                                   dictionary)
                                  t))))))"##,
        expect![[
            r#"OK ((55 ("struct") ("volatile") t) (379 ("ImportError" "ImportWarning") ("UnicodeDecodeError" "UnicodeEncodeError" "UnicodeError" "UnicodeTranslateError" "UnicodeWarning") ("zip" "zipfile" "zipimport") t))"#
        ]],
    )
}

fn auto_complete_dictionary_source_completes_real_user_address_and_retains_source_symbol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_dictionary_source_completes_real_user_address_and_retains_source_symbol",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-user-dictionary
                                   '("alice@example.com"
                                     "alice@engineering.example"
                                     "bob@example.com"))
                                  (ac-dictionary-files nil)
                                  (ac-dictionary-directories nil)
                                  (ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-expand-on-auto-complete nil)
                                  (ac-sources
                                   '(ac-source-dictionary)))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "alice")
                                    (auto-complete)
                                    (let ((candidates
                                           (mapcar
                                            (lambda (candidate)
                                              (list
                                               (substring-no-properties
                                                candidate)
                                               (popup-item-symbol
                                                candidate)))
                                            ac-candidates))
                                          (prefix ac-prefix)
                                          (selected
                                           (substring-no-properties
                                            (ac-selected-candidate))))
                                      (ac-next)
                                      (let ((next
                                             (substring-no-properties
                                              (ac-selected-candidate)))
                                            (completed
                                             (ac-complete)))
                                        (list
                                         candidates
                                         prefix
                                         selected
                                         next
                                         (substring-no-properties
                                          completed)
                                         (buffer-string)
                                         ac-menu
                                         ac-completing
                                         (and
                                          ac-last-completion
                                          (substring-no-properties
                                           (cdr
                                            ac-last-completion)))))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK ((("alice@example.com" "d") ("alice@engineering.example" "d")) "alice" "alice@example.com" "alice@engineering.example" "alice@engineering.example" "alice@engineering.example" nil nil "alice@engineering.example")"#
        ]],
    )
}

fn auto_complete_filename_source_lists_files_directories_and_respects_comment_and_regular_file_guards()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_filename_source_lists_files_directories_and_respects_comment_and_regular_file_guards",
        r##"(let* ((root
                                 (file-name-as-directory
                                  (expand-file-name
                                   "auto-complete-filename"
                                   (getenv "TMPDIR"))))
                                (nested
                                 (expand-file-name
                                  "nested"
                                  root))
                                (regular
                                 (expand-file-name
                                  "alpha.txt"
                                  root)))
                           (make-directory nested t)
                           (with-temp-file regular
                             (insert "alpha"))
                           (with-temp-file
                               (expand-file-name
                                "amber.log"
                                root)
                             (insert "amber"))
                           (let ((ac-filename-cache nil)
                                 (comment-start-skip nil))
                             (list
                              (let ((ac-prefix root))
                                (sort
                                 (mapcar
                                  #'file-name-nondirectory
                                  (ac-filename-candidate))
                                 #'string<))
                              (let ((ac-prefix
                                     (concat root "a")))
                                (sort
                                 (mapcar
                                  #'file-name-nondirectory
                                  (all-completions
                                   ac-prefix
                                   (ac-filename-candidate)))
                                 #'string<))
                              (let ((ac-prefix regular))
                                (ac-filename-candidate))
                              (with-temp-buffer
                                (insert "# " root)
                                (setq
                                 ac-prefix
                                 (buffer-substring-no-properties
                                  (point-min)
                                  (point-max))
                                 comment-start-skip
                                 "#[ \t]*")
                                (ac-filename-candidate))
                              (length ac-filename-cache))))"##,
        expect![[r#"OK (("" "alpha.txt" "amber.log") ("alpha.txt" "amber.log") nil nil 1)"#]],
    )
}

fn auto_complete_mode_dictionary_uses_both_major_mode_and_filename_extension_with_duplicate_data_intact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_mode_dictionary_uses_both_major_mode_and_filename_extension_with_duplicate_data_intact",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-mode-extension"
                                  (getenv "TMPDIR")))
                                (directory
                                 (expand-file-name
                                  "dict"
                                  root)))
                           (make-directory directory t)
                           (with-temp-file
                               (expand-file-name
                                "text-mode"
                                directory)
                             (insert
                              "mode-only\n"
                              "duplicate\n"))
                           (with-temp-file
                               (expand-file-name
                                "journal"
                                directory)
                             (insert
                              "extension-only\n"
                              "duplicate\n"))
                           (clrhash ac-file-dictionary)
                           (with-temp-buffer
                             (text-mode)
                             (setq
                              buffer-file-name
                              (expand-file-name
                               "daily.journal"
                               root))
                             (let ((ac-dictionary-directories
                                    (list directory)))
                               (list
                                (ac-mode-dictionary
                                 major-mode)
                                (ac-mode-dictionary
                                 'fundamental-mode)))))"##,
        expect![[
            r#"OK (("mode-only" "duplicate" "extension-only" "duplicate") ("extension-only" "duplicate"))"#
        ]],
    )
}

pub(super) fn dictionaries_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_file_dictionary_caches_contents_until_explicit_clear(),
        auto_complete_buffer_dictionary_combines_user_mode_extension_and_explicit_files_in_order(),
        auto_complete_dictionary_cache_is_buffer_local_and_clear_invalidates_every_live_buffer(),
        auto_complete_missing_dictionary_file_is_cached_as_nil_until_cache_clear(),
        auto_complete_builtin_c_and_python_dictionaries_support_practical_prefix_queries(),
        auto_complete_dictionary_source_completes_real_user_address_and_retains_source_symbol(),
        auto_complete_filename_source_lists_files_directories_and_respects_comment_and_regular_file_guards(),
        auto_complete_mode_dictionary_uses_both_major_mode_and_filename_extension_with_duplicate_data_intact(),
    ]
}
