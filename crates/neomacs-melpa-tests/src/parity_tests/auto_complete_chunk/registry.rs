use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_chunk_exact_descriptor_and_archive_payload_bytes_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_exact_descriptor_and_archive_payload_bytes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-chunk
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-summary descriptor)
                            (package-desc-reqs descriptor)
                            (package-desc-kind descriptor)
                            (package-desc-extras descriptor)
                            (mapcar
                             (lambda (name)
                               (let ((file
                                      (expand-file-name
                                       name
                                       directory)))
                                 (list
                                  name
                                  (file-attribute-size
                                   (file-attributes file))
                                  (with-temp-buffer
                                    (set-buffer-multibyte nil)
                                    (insert-file-contents-literally
                                     file)
                                    (secure-hash
                                     'sha256
                                     (current-buffer))))))
                             '("auto-complete-chunk-pkg.el"
                               "auto-complete-chunk.el"))))"##,
        expect![[
            r#"OK (auto-complete-chunk "20140225.946" "Auto-completion for dot.separated.words." ((auto-complete (1 4))) nil ((:revdesc . "a9aa77ffb84a") (:commit . "a9aa77ffb84a1037984a7ce4dda25074272f13fe") (:url . "https://github.com/tkf/auto-complete-chunk")) (("auto-complete-chunk-pkg.el" 309 "b32c5927e058f368121f2f301555d5042436f26eb7cebef8d351c9e149519d19") ("auto-complete-chunk.el" 3732 "f8b3b7e01a171677690a05314304618f6a55da5a54c98333019ea6e1397dae22")))"#
        ]],
    )
}

fn auto_complete_chunk_complete_public_and_internal_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_complete_public_and_internal_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name
                                     (symbol-name symbol)))
                                (when
                                    (and
                                     (string-prefix-p
                                      "ac-chunk-"
                                      name)
                                     (not
                                      (string-prefix-p
                                       "auto-complete-chunk-test-"
                                       name)))
                                  (push
                                   (list
                                    symbol
                                    (fboundp symbol)
                                    (boundp symbol)
                                    (and
                                     (commandp symbol)
                                     t)
                                    (and
                                     (macrop symbol)
                                     t)
                                    (local-variable-if-set-p
                                     symbol)
                                    (file-name-nondirectory
                                     (or
                                      (symbol-file symbol)
                                      "")))
                                   symbols)))))
                           (sort
                            symbols
                            (lambda (left right)
                              (string<
                               (symbol-name
                                (car left))
                               (symbol-name
                                (car right))))))"##,
        expect![[
            r#"OK ((ac-chunk-beginning t nil nil nil nil "auto-complete-chunk.el") (ac-chunk-candidates-from-list t nil nil nil nil "auto-complete-chunk.el") (ac-chunk-list t t nil nil t "auto-complete-chunk.el") (ac-chunk-list-candidates t nil nil nil nil "auto-complete-chunk.el") (ac-chunk-regex nil t nil nil nil "auto-complete-chunk.el"))"#
        ]],
    )
}

fn auto_complete_chunk_all_callable_signatures_docs_interactivity_and_origins_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_all_callable_signatures_docs_interactivity_and_origins_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (help-function-arglist
                               symbol
                               t)
                              (and
                               (interactive-form symbol)
                               t)
                              (and
                               (commandp symbol)
                               t)
                              (documentation
                               symbol
                               t)
                              (file-name-nondirectory
                               (or
                                (symbol-file
                                 symbol
                                 'defun)
                                ""))))
                           '(ac-chunk-beginning
                             ac-chunk-candidates-from-list
                             ac-chunk-list
                             ac-chunk-list-candidates
                             ac-dictionary-chunk-candidates
                             ac-use-dictionary-chunk
                             ac-complete-chunk-list
                             ac-complete-dictionary-chunk))"##,
        expect![[
            r#"OK ((ac-chunk-beginning nil nil nil "Return the position where the chunk begins." "auto-complete-chunk.el") (ac-chunk-candidates-from-list (chunk-list) nil nil "Return matched candidates in CHUNK-LIST." "auto-complete-chunk.el") (ac-chunk-list nil nil nil "Util function to access the variable `ac-chunk-list'." "auto-complete-chunk.el") (ac-chunk-list-candidates nil nil nil "Create candidates from a buffer local variable `ac-chunk-list'." "auto-complete-chunk.el") (ac-dictionary-chunk-candidates nil nil nil "Create candidates from dictionary (variable `ac-buffer-dictionary')." "auto-complete-chunk.el") (ac-use-dictionary-chunk nil nil nil "Swap `ac-source-dictionary' with `ac-source-dictionary-chunk'." "auto-complete-chunk.el") (ac-complete-chunk-list nil t t nil "auto-complete-chunk.el") (ac-complete-dictionary-chunk nil t t nil "auto-complete-chunk.el"))"#
        ]],
    )
}

fn auto_complete_chunk_variable_defaults_docs_locality_and_definition_origins_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_variable_defaults_docs_locality_and_definition_origins_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (default-value symbol)
                              (local-variable-if-set-p
                               symbol)
                              (documentation-property
                               symbol
                               'variable-documentation
                               t)
                              (file-name-nondirectory
                               (or
                                (symbol-file
                                 symbol
                                 'defvar)
                                ""))))
                           '(ac-chunk-regex
                             ac-chunk-list
                             ac-source-chunk-list
                             ac-source-dictionary-chunk))"##,
        expect![[
            r#"OK ((ac-chunk-regex "\\(\\s-\\|\\s(\\|\\s)\\|^\\)\\(?:\\(?:\\w\\|\\s_\\)+\\s.\\)*\\(?:\\w\\|\\s_\\)+\\s.?\\=" nil "A regexp that matches to a \"chunk\" containing words and dots." "auto-complete-chunk.el") (ac-chunk-list nil t "Dictionary used from `ac-source-chunk-list'.  List of strings." "auto-complete-chunk.el") (ac-source-chunk-list ((candidates . ac-chunk-list-candidates) (prefix . ac-chunk-beginning) (symbol . "c")) nil nil "") (ac-source-dictionary-chunk ((candidates . ac-dictionary-chunk-candidates) (prefix . ac-chunk-beginning) (symbol . "c")) nil nil ""))"#
        ]],
    )
}

fn auto_complete_chunk_exact_source_definitions_and_commands_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_exact_source_definitions_and_commands_match",
        r##"(list
                           ac-source-chunk-list
                           ac-source-dictionary-chunk
                           (mapcar
                            (lambda (command)
                              (list
                               command
                               (commandp command)
                               (interactive-form command)
                               (help-function-arglist
                                command
                                t)))
                            '(ac-complete-chunk-list
                              ac-complete-dictionary-chunk))
                           (featurep
                            'auto-complete-chunk)
                           (featurep
                            'auto-complete)
                           (featurep 'popup))"##,
        expect![[
            r#"OK (((candidates . ac-chunk-list-candidates) (prefix . ac-chunk-beginning) (symbol . "c")) ((candidates . ac-dictionary-chunk-candidates) (prefix . ac-chunk-beginning) (symbol . "c")) ((ac-complete-chunk-list t (interactive nil) nil) (ac-complete-dictionary-chunk t (interactive nil) nil)) t t t)"#
        ]],
    )
}

fn auto_complete_chunk_source_load_history_records_complete_definition_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_source_load_history_records_complete_definition_order",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-complete-chunk.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(require
                                       defun
                                       provide)))
                                  (cdr history))))
                           (list
                            (file-name-nondirectory
                             (car history))
                            events
                            (featurep
                             'auto-complete-chunk)
                            (featurep
                             'auto-complete)))"##,
        expect![[
            r#"OK ("auto-complete-chunk.el" ((require . cl) (require . auto-complete) (defun . ac-chunk-beginning) (defun . ac-chunk-candidates-from-list) (defun . ac-chunk-list) (defun . ac-chunk-list-candidates) (defun . ac-complete-chunk-list) (defun . ac-dictionary-chunk-candidates) (defun . ac-complete-dictionary-chunk) (defun . ac-use-dictionary-chunk) (provide . auto-complete-chunk)) t t)"#
        ]],
    )
}

fn auto_complete_chunk_exact_auto_complete_and_popup_dependency_versions_are_loaded()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_exact_auto_complete_and_popup_dependency_versions_are_loaded",
        r##"(mapcar
                           (lambda (package)
                             (let ((descriptor
                                    (cadr
                                     (assq
                                      package
                                      package-alist))))
                               (list
                                package
                                (package-version-join
                                 (package-desc-version descriptor))
                                (package-desc-reqs descriptor)
                                (featurep package)
                                (file-name-nondirectory
                                 (or
                                  (locate-library
                                   (symbol-name package))
                                  "")))))
                           '(auto-complete-chunk
                             auto-complete
                             popup))"##,
        expect![[
            r#"OK ((auto-complete-chunk "20140225.946" ((auto-complete (1 4))) t "auto-complete-chunk.el") (auto-complete "20251231.1622" ((emacs (25 1)) (popup (0 5 8))) t "auto-complete.el") (popup "20251231.1622" ((emacs (24 3))) t "popup.el"))"#
        ]],
    )
}

fn auto_complete_chunk_generated_autoload_contains_only_feature_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_generated_autoload_contains_only_feature_contract",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-complete-chunk-autoloads.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(defun
                                       provide)))
                                  (cdr history))))
                           (list
                            (featurep
                             'auto-complete-chunk-autoloads)
                            (featurep
                             'auto-complete-chunk)
                            (fboundp
                             'ac-chunk-beginning)
                            (boundp
                             'ac-chunk-regex)
                            events))"##,
        expect!["OK (t nil nil nil ((provide . auto-complete-chunk-autoloads)))"],
    )
}

pub(super) fn registry_auto_complete_chunk_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_chunk_exact_descriptor_and_archive_payload_bytes_match(),
        auto_complete_chunk_complete_public_and_internal_symbol_inventory_matches(),
        auto_complete_chunk_all_callable_signatures_docs_interactivity_and_origins_match(),
        auto_complete_chunk_variable_defaults_docs_locality_and_definition_origins_match(),
        auto_complete_chunk_exact_source_definitions_and_commands_match(),
        auto_complete_chunk_source_load_history_records_complete_definition_order(),
        auto_complete_chunk_exact_auto_complete_and_popup_dependency_versions_are_loaded(),
    ]
}

pub(super) fn registry_auto_complete_chunk_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_chunk_generated_autoload_contains_only_feature_contract()]
}
