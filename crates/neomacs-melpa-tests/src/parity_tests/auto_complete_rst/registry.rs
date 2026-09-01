use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_rst_exact_descriptor_and_archive_payload_bytes_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_exact_descriptor_and_archive_payload_bytes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-rst
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
                                    (insert-file-contents-literally file)
                                    (secure-hash
                                     'sha256
                                     (current-buffer))))))
                             '("auto-complete-rst-pkg.el"
                               "auto-complete-rst.el"
                               "genesource.py"))))"##,
        expect![[
            r####"OK (auto-complete-rst "20140225.944" "Auto-complete extension for ReST and Sphinx." ((auto-complete (1 4))) nil ((:revdesc . "4803ce41a962") (:commit . "4803ce41a96224e6fa54e6741a5b5f40ebed7351") (:url . "https://github.com/tkf/auto-complete-rst")) (("auto-complete-rst-pkg.el" 309 "e9f01f4813414c2b458f062f7f9feece539cd8087281ed622b3a24d6b0548580") ("auto-complete-rst.el" 6567 "a280e014e925c9cd301ad1c24d6f47fe823334210afde99babf02e54f9265b00") ("genesource.py" 7490 "95c1d9d3ac8d08ba6fcb74fd5390bb865f2e7a7a81d3441a0a6233807c9ab5b7")))"####
        ]],
    )
}

fn auto_complete_rst_complete_target_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_complete_target_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name (symbol-name symbol)))
                                (when
                                    (and
                                     (string-prefix-p
                                      "auto-complete-rst-"
                                      name)
                                     (not
                                      (string-prefix-p
                                       "auto-complete-rst-test-"
                                       name)))
                                  (push
                                   (list
                                    symbol
                                    (fboundp symbol)
                                    (boundp symbol)
                                    (and (commandp symbol) t)
                                    (local-variable-if-set-p symbol)
                                    (file-name-nondirectory
                                     (or (symbol-file symbol) "")))
                                   symbols)))))
                           (sort
                            symbols
                            (lambda (left right)
                              (string<
                               (symbol-name (car left))
                               (symbol-name (car right))))))"##,
        expect![[
            r####"OK ((auto-complete-rst-add-sources t nil nil nil "auto-complete-rst.el") (auto-complete-rst-autoloads nil nil nil nil "auto-complete-rst-autoloads.el") (auto-complete-rst-complete-colon t nil t nil "auto-complete-rst.el") (auto-complete-rst-complete-space t nil t nil "auto-complete-rst.el") (auto-complete-rst-directive-name-at-option t nil nil nil "auto-complete-rst.el") (auto-complete-rst-directive-options-map nil t nil nil "auto-complete-rst.el") (auto-complete-rst-directives-candidates nil nil nil nil "") (auto-complete-rst-genesource-command t nil nil nil "auto-complete-rst.el") (auto-complete-rst-genesource-eval t nil nil nil "auto-complete-rst.el") (auto-complete-rst-genesource-py nil t nil nil "auto-complete-rst.el") (auto-complete-rst-get-option t nil nil nil "auto-complete-rst.el") (auto-complete-rst-goto-directive-from-option t nil nil nil "auto-complete-rst.el") (auto-complete-rst-init t nil nil nil "auto-complete-rst.el") (auto-complete-rst-insert-two-backquotes t nil nil nil "auto-complete-rst.el") (auto-complete-rst-options-candidates t nil nil nil "auto-complete-rst.el") (auto-complete-rst-other-sources nil t nil nil "auto-complete-rst.el") (auto-complete-rst-regenerate-source t nil t nil "auto-complete-rst.el") (auto-complete-rst-roles-candidates nil nil nil nil "") (auto-complete-rst-sphinx-extensions nil t nil nil "auto-complete-rst.el"))"####
        ]],
    )
    .fresh_process()
}

fn auto_complete_rst_callable_signatures_docs_interactivity_and_origins_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_callable_signatures_docs_interactivity_and_origins_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (help-function-arglist symbol t)
                              (and (interactive-form symbol) t)
                              (and (commandp symbol) t)
                              (documentation symbol t)
                              (file-name-nondirectory
                               (or
                                (symbol-file symbol 'defun)
                                ""))))
                           '(auto-complete-rst-genesource-command
                             auto-complete-rst-genesource-eval
                             auto-complete-rst-regenerate-source
                             auto-complete-rst-options-candidates
                             auto-complete-rst-get-option
                             auto-complete-rst-directive-name-at-option
                             auto-complete-rst-goto-directive-from-option
                             auto-complete-rst-insert-two-backquotes
                             auto-complete-rst-complete-space
                             auto-complete-rst-complete-colon
                             auto-complete-rst-add-sources
                             auto-complete-rst-init))"##,
        expect![[
            r####"OK ((auto-complete-rst-genesource-command nil nil nil nil "auto-complete-rst.el") (auto-complete-rst-genesource-eval nil nil nil nil "auto-complete-rst.el") (auto-complete-rst-regenerate-source nil t t "Recreate sources for auto-complete-rst.\nUseful, for example, to add new extension(s) after modifying\n`auto-complete-rst-sphinx-extensions'." "auto-complete-rst.el") (auto-complete-rst-options-candidates nil nil nil nil "auto-complete-rst.el") (auto-complete-rst-get-option (directive) nil nil "Get options (list of string) of given directive" "auto-complete-rst.el") (auto-complete-rst-directive-name-at-option (&optional bound) nil nil "Get the directive name when the cursor is at the option" "auto-complete-rst.el") (auto-complete-rst-goto-directive-from-option (&optional bound) nil nil "Go to the position right after the :: of directive (#) from option (@)\n\n.. DIRECTIVE::#\n   :OPTION:\n   :OPT@\n\n" "auto-complete-rst.el") (auto-complete-rst-insert-two-backquotes nil nil nil nil "auto-complete-rst.el") (auto-complete-rst-complete-space nil t t nil "auto-complete-rst.el") (auto-complete-rst-complete-colon nil t t nil "auto-complete-rst.el") (auto-complete-rst-add-sources nil nil nil nil "auto-complete-rst.el") (auto-complete-rst-init nil nil nil nil "auto-complete-rst.el"))"####
        ]],
    )
}

fn auto_complete_rst_variable_defaults_docs_locality_and_sources_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_variable_defaults_docs_locality_and_sources_match",
        r##"(list
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (if
                                   (eq
                                    symbol
                                    'auto-complete-rst-genesource-py)
                                   (file-name-nondirectory
                                    (default-value symbol))
                                 (default-value symbol))
                               (local-variable-if-set-p symbol)
                               (documentation-property
                                symbol
                                'variable-documentation
                                t)
                               (file-name-nondirectory
                                (or
                                 (symbol-file symbol 'defvar)
                                 ""))))
                            '(auto-complete-rst-sphinx-extensions
                              auto-complete-rst-genesource-py
                              auto-complete-rst-directive-options-map
                              auto-complete-rst-other-sources))
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (auto-complete-rst-test-source-shape
                                (symbol-value symbol))))
                            '(ac-source-rst-directives
                              ac-source-rst-roles
                              ac-source-rst-options)))"##,
        expect![[
            r####"OK (((auto-complete-rst-sphinx-extensions nil nil "Paths to Sphinx extensions." "auto-complete-rst.el") (auto-complete-rst-genesource-py "genesource.py" nil nil "auto-complete-rst.el") (auto-complete-rst-directive-options-map #s(hash-table test equal) nil "A map from directive name (string) to its options (list of string)" "auto-complete-rst.el") (auto-complete-rst-other-sources nil nil "Sources to use other than the sources defined in `auto-complete-rst'\n\nDefault `ac-sources' will be used if it is `nil' (default)." "auto-complete-rst.el")) ((ac-source-rst-directives ((candidates . auto-complete-rst-directives-candidates) (available fboundp 'auto-complete-rst-directives-candidates) (prefix . "[[:space:]]\\.\\. \\([[:alnum:]-:]*\\)") (symbol . "D") (requires . 0))) (ac-source-rst-roles ((candidates . auto-complete-rst-roles-candidates) (available fboundp 'auto-complete-rst-roles-candidates) (prefix . "[^[:alnum:]:]:\\([[:alnum:]-:]*\\)") (symbol . "R") (requires . 0) (action . :function))) (ac-source-rst-options ((candidates . :function) (prefix . "[[:space:]]\\{4,\\}:\\([^:]*\\)") (symbol . "O") (requires . 0)))))"####
        ]],
    )
    .fresh_process()
}

fn auto_complete_rst_load_history_records_definition_order_and_requirements() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_load_history_records_definition_order_and_requirements",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp (car entry))
                                     (string-suffix-p
                                      "auto-complete-rst.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(require defun provide)))
                                  (cdr history))))
                           (list
                            (file-name-nondirectory (car history))
                            events
                            (featurep 'auto-complete-rst)
                            (featurep 'auto-complete)
                            (featurep 'popup)))"##,
        expect![[
            r####"OK ("auto-complete-rst.el" ((require . cl) (require . auto-complete) (defun . auto-complete-rst-genesource-command) (defun . auto-complete-rst-genesource-eval) (defun . auto-complete-rst-regenerate-source) (defun . auto-complete-rst-options-candidates) (defun . auto-complete-rst-get-option) (defun . auto-complete-rst-directive-name-at-option) (defun . auto-complete-rst-goto-directive-from-option) (defun . auto-complete-rst-insert-two-backquotes) (defun . auto-complete-rst-complete-space) (defun . auto-complete-rst-complete-colon) (defun . auto-complete-rst-add-sources) (defun . auto-complete-rst-init) (provide . auto-complete-rst)) t t t)"####
        ]],
    )
}

fn auto_complete_rst_exact_dependency_versions_and_loaded_features_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_exact_dependency_versions_and_loaded_features_match",
        r##"(mapcar
                           (lambda (name)
                             (let ((descriptor
                                    (cadr
                                     (assq name package-alist))))
                               (list
                                name
                                (package-version-join
                                 (package-desc-version descriptor))
                                (package-desc-reqs descriptor)
                                (featurep name))))
                           '(auto-complete-rst
                             auto-complete
                             popup))"##,
        expect![[
            r####"OK ((auto-complete-rst "20140225.944" ((auto-complete (1 4))) t) (auto-complete "20251231.1622" ((emacs (25 1)) (popup (0 5 8))) t) (popup "20251231.1622" ((emacs (24 3))) t))"####
        ]],
    )
}

fn auto_complete_rst_generated_autoload_contains_only_feature_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_generated_autoload_contains_only_feature_contract",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp (car entry))
                                     (string-suffix-p
                                      "auto-complete-rst-autoloads.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(defun provide)))
                                  (cdr history))))
                           (list
                            (featurep
                             'auto-complete-rst-autoloads)
                            (featurep 'auto-complete-rst)
                            (fboundp
                             'auto-complete-rst-init)
                            (boundp
                             'auto-complete-rst-sphinx-extensions)
                            events))"##,
        expect![[r####"OK (t nil nil nil ((provide . auto-complete-rst-autoloads)))"####]],
    )
}

pub(super) fn registry_auto_complete_rst_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_rst_exact_descriptor_and_archive_payload_bytes_match(),
        auto_complete_rst_complete_target_symbol_inventory_matches(),
        auto_complete_rst_callable_signatures_docs_interactivity_and_origins_match(),
        auto_complete_rst_variable_defaults_docs_locality_and_sources_match(),
        auto_complete_rst_load_history_records_definition_order_and_requirements(),
        auto_complete_rst_exact_dependency_versions_and_loaded_features_match(),
    ]
}

pub(super) fn registry_auto_complete_rst_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_rst_generated_autoload_contains_only_feature_contract()]
}
