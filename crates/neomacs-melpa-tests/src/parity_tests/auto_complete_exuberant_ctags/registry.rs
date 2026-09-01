use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_exuberant_ctags_exact_descriptor_and_archive_payload_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_exact_descriptor_and_archive_payload_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-exuberant-ctags
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
                             '("auto-complete-exuberant-ctags-pkg.el"
                               "auto-complete-exuberant-ctags.el"))))"##,
        expect![[
            r#"OK (auto-complete-exuberant-ctags "20140320.724" "Exuberant ctags auto-complete.el source." ((auto-complete (1 4 0))) nil ((:maintainers ("Kenichirou Oyama" . "k1lowxb@gmail.com")) (:authors ("Kenichirou Oyama" . "k1lowxb@gmail.com")) (:keywords "anto-complete" "exuberant ctags") (:revdesc . "ff6121ff8b71") (:commit . "ff6121ff8b71beb5aa606d28fd389c484ed49765") (:url . "http://code.101000lab.org")) (("auto-complete-exuberant-ctags-pkg.el" 471 "9de48879cdc5ac388f3fbbc880ea338228efcb9255ee2f78a3280ba457fcd13a") ("auto-complete-exuberant-ctags.el" 7855 "fcb978dcfaab6f16f0157d1bb29d4101325eb57a68c2fae1c6ab2a910097188b")))"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_complete_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_complete_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name
                                     (symbol-name symbol)))
                                (when
                                    (and
                                     (string-prefix-p
                                      "ac-exuberant-ctags"
                                      name)
                                     (not
                                      (string-prefix-p
                                       "auto-complete-exuberant-ctags-test-"
                                       name)))
                                  (push
                                   (list
                                    symbol
                                    (fboundp symbol)
                                    (boundp symbol)
                                    (and (commandp symbol) t)
                                    (and (macrop symbol) t)
                                    (local-variable-if-set-p symbol)
                                    (let ((origin
                                           (symbol-file symbol)))
                                      (and origin
                                           (file-name-base
                                            origin))))
                                   symbols)))))
                           (sort
                            symbols
                            (lambda (left right)
                              (string<
                               (symbol-name (car left))
                               (symbol-name (car right))))))"##,
        expect![[
            r#"OK ((ac-exuberant-ctags nil nil nil nil nil nil) (ac-exuberant-ctags-build-index t nil nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-candidate t nil nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-find-tag-file t nil nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-get-line t nil nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-get-tag-file t nil nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-index nil t nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-line-length-limit nil t nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-setup t nil nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-tag-file-dir nil t nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-tag-file-name nil t nil nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-tag-file-search-limit nil t nil nil nil "auto-complete-exuberant-ctags"))"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_callable_contracts_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_callable_contracts_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (help-function-arglist symbol t)
                              (and (interactive-form symbol) t)
                              (documentation symbol t)
                              (let ((origin
                                     (symbol-file symbol 'defun)))
                                (and origin
                                     (file-name-base origin)))))
                           '(ac-exuberant-ctags-setup
                             ac-exuberant-ctags-build-index
                             ac-exuberant-ctags-get-line
                             ac-exuberant-ctags-get-tag-file
                             ac-exuberant-ctags-find-tag-file
                             ac-exuberant-ctags-candidate))"##,
        expect![[
            r#"OK ((ac-exuberant-ctags-setup nil nil "Setup ac-exuberant-ctags-setup." "auto-complete-exuberant-ctags") (ac-exuberant-ctags-build-index nil nil "Build index." "auto-complete-exuberant-ctags") (ac-exuberant-ctags-get-line (s e) nil nil "auto-complete-exuberant-ctags") (ac-exuberant-ctags-get-tag-file nil nil "Get Exuberant ctags tag file." "auto-complete-exuberant-ctags") (ac-exuberant-ctags-find-tag-file (current-dir) nil "Find tag file.\nTry to find tag file in upper directory if haven't found in CURRENT-DIR." "auto-complete-exuberant-ctags") (ac-exuberant-ctags-candidate nil nil nil "auto-complete-exuberant-ctags"))"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_customization_contract_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_customization_contract_matches",
        r##"(list
                           (get 'ac-exuberant-ctags 'group-documentation)
                           (get 'ac-exuberant-ctags 'custom-prefix)
                           (get 'ac-exuberant-ctags 'custom-group)
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (default-value symbol)
                               (get symbol 'custom-type)
                               (get symbol 'custom-group)
                               (documentation-property
                                symbol
                                'variable-documentation
                                t)))
                            '(ac-exuberant-ctags-tag-file-name
                              ac-exuberant-ctags-tag-file-search-limit
                              ac-exuberant-ctags-line-length-limit)))"##,
        expect![[
            r#"OK ("Exuberant ctags auto-complete.el source" "ac-exuberant-ctags-" ((ac-exuberant-ctags-tag-file-name custom-variable) (ac-exuberant-ctags-tag-file-search-limit custom-variable) (ac-exuberant-ctags-line-length-limit custom-variable)) ((ac-exuberant-ctags-tag-file-name "tags" string nil "Exuberant ctags tag file name.") (ac-exuberant-ctags-tag-file-search-limit 10 number nil "The limit level of directory that search tag file.\nDon't search tag file deeply if outside this value.\nThis value only use when option\n`ac-exuberant-ctags-tag-file-dir-cache' is nil.") (ac-exuberant-ctags-line-length-limit 400 number nil "The limit level of line length.\nDon't search line longer if outside this value.")))"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_source_and_feature_contract_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_source_and_feature_contract_matches",
        r##"(list
                           ac-source-exuberant-ctags
                           ac-exuberant-ctags-index
                           ac-exuberant-ctags-tag-file-dir
                           (featurep
                            'auto-complete-exuberant-ctags)
                           (featurep 'auto-complete)
                           (featurep 'popup)
                           (mapcar
                            (lambda (entry)
                              (if (stringp entry)
                                  (file-name-base entry)
                                entry))
                            (seq-find
                             (lambda (entry)
                               (and
                                (stringp (car entry))
                                (string=
                                 (file-name-base (car entry))
                                 "auto-complete-exuberant-ctags")))
                             load-history)))"##,
        expect![[
            r#"OK (((init lambda nil (unless ac-exuberant-ctags-index (ac-exuberant-ctags-build-index))) (candidates . ac-exuberant-ctags-candidate) (requires . 3) (symbol . "s")) nil nil t t t ("auto-complete-exuberant-ctags" (require . auto-complete) ac-exuberant-ctags-tag-file-name ac-exuberant-ctags-tag-file-search-limit ac-exuberant-ctags-line-length-limit (defun . ac-exuberant-ctags-setup) ac-exuberant-ctags-index ac-exuberant-ctags-tag-file-dir (defun . ac-exuberant-ctags-build-index) (defun . ac-exuberant-ctags-get-line) (defun . ac-exuberant-ctags-get-tag-file) (defun . ac-exuberant-ctags-find-tag-file) (defun . ac-exuberant-ctags-candidate) (defun . ac-complete-exuberant-ctags) (provide . auto-complete-exuberant-ctags)))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_exuberant_ctags_generated_autoload_contract_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_generated_autoload_contract_matches",
        r##"(list
                           (featurep
                            'auto-complete-exuberant-ctags)
                           (fboundp
                            'ac-exuberant-ctags-setup)
                           (autoloadp
                            (symbol-function
                             'ac-exuberant-ctags-setup))
                           (let ((origin
                                  (symbol-file
                                   'ac-exuberant-ctags-setup
                                   'defun)))
                             (and origin
                                  (file-name-base origin)))
                           (seq-some
                            (lambda (entry)
                              (and
                               (stringp (car entry))
                               (string=
                                (file-name-base (car entry))
                                "auto-complete-exuberant-ctags-autoloads")))
                            load-history))"##,
        expect!["OK (nil nil nil nil t)"],
    )
}

pub(super) fn registry_auto_complete_exuberant_ctags_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_exuberant_ctags_exact_descriptor_and_archive_payload_match(),
        auto_complete_exuberant_ctags_complete_symbol_inventory_matches(),
        auto_complete_exuberant_ctags_callable_contracts_match(),
        auto_complete_exuberant_ctags_customization_contract_matches(),
        auto_complete_exuberant_ctags_source_and_feature_contract_matches(),
    ]
}

pub(super) fn registry_auto_complete_exuberant_ctags_autoload_batch_cases() -> Vec<ParityBatchCase>
{
    vec![auto_complete_exuberant_ctags_generated_autoload_contract_matches()]
}
