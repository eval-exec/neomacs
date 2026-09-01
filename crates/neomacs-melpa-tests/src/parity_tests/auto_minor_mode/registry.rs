use expect_test::expect;

use super::ParityBatchCase;

fn auto_minor_mode_exact_descriptor_and_archive_payload_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_exact_descriptor_and_archive_payload_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-minor-mode
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
                             '("auto-minor-mode-pkg.el"
                               "auto-minor-mode.el"))))"##,
        expect![[
            r#"OK (auto-minor-mode "20180527.1123" "Enable minor modes by file name and contents." ((emacs (24 4))) nil ((:maintainers ("Joe Wreschnig" . "joe.wreschnig@gmail.com")) (:authors ("Joe Wreschnig" . "joe.wreschnig@gmail.com")) (:keywords "convenience") (:revdesc . "c62f4e04c7b7") (:commit . "c62f4e04c7b73835c399f0348bea0ade2720bcbb") (:url . "https://github.com/joewreschnig/auto-minor-mode")) (("auto-minor-mode-pkg.el" 462 "7cafe33ead1d09c2ec49f58221053ecc9984bea5f5d0385751a2b0a995388a29") ("auto-minor-mode.el" 6707 "386e995842b2b193d5785d3b64b38f1a2addeb98529204336039d2bd12432c58")))"#
        ]],
    )
}

fn auto_minor_mode_complete_target_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_complete_target_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name
                                     (symbol-name symbol)))
                                (when
                                    (and
                                     (string-prefix-p
                                      "auto-minor-mode"
                                      name)
                                     (not
                                      (string-prefix-p
                                       "auto-minor-mode-test-"
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
            r#"OK ((auto-minor-mode nil nil nil nil nil "auto-minor-mode") (auto-minor-mode--plain-filename t nil nil nil nil "auto-minor-mode") (auto-minor-mode--run-auto t nil nil nil nil "auto-minor-mode") (auto-minor-mode--run-magic t nil nil nil nil "auto-minor-mode") (auto-minor-mode-alist nil t nil nil nil "auto-minor-mode") (auto-minor-mode-autoloads nil nil nil nil nil "auto-minor-mode-autoloads") (auto-minor-mode-enabled-p t nil nil nil nil "auto-minor-mode") (auto-minor-mode-magic-alist nil t nil nil nil "auto-minor-mode") (auto-minor-mode-set t nil nil nil nil "auto-minor-mode"))"#
        ]],
    )
}

fn auto_minor_mode_callable_signatures_docs_interactivity_and_origins_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_callable_signatures_docs_interactivity_and_origins_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (help-function-arglist symbol t)
                              (and
                               (interactive-form symbol)
                               t)
                              (and (commandp symbol) t)
                              (documentation symbol t)
                              (let ((origin
                                     (symbol-file
                                      symbol
                                      'defun)))
                                (and origin
                                     (file-name-base origin)))))
                           '(auto-minor-mode-enabled-p
                             auto-minor-mode--plain-filename
                             auto-minor-mode--run-auto
                             auto-minor-mode--run-magic
                             auto-minor-mode-set))"##,
        expect![[
            r#"OK ((auto-minor-mode-enabled-p (minor-mode) nil nil "Return non-nil if MINOR-MODE is enabled in the current buffer." "auto-minor-mode") (auto-minor-mode--plain-filename (file-name) nil nil "Remove remote connections and backup version from FILE-NAME." "auto-minor-mode") (auto-minor-mode--run-auto (alist keep-mode-if-same) nil nil "Run through an auto ALIST and enable all matching minor modes.\n\nA auto alist contains pairs of regexps or functions to match the\nbuffer’s contents, and functions to call when matched.  For more\ninformation, see ‘auto-mode-alist’.\n\nIf the optional argument KEEP-MODE-IF-SAME is non-nil, then we\ndon’t re-activate minor modes already enabled in the buffer." "auto-minor-mode") (auto-minor-mode--run-magic (alist keep-mode-if-same) nil nil "Run through a magic ALIST and enable all matching minor modes.\n\nA magic alist contains pairs of regexps or functions to match the\nbuffer’s contents, and functions to call when matched.  For more\ninformation, see ‘magic-mode-alist’.\n\nIf the optional argument KEEP-MODE-IF-SAME is non-nil, then we\ndon’t re-activate minor modes already enabled in the buffer." "auto-minor-mode") (auto-minor-mode-set (&optional keep-mode-if-same) nil nil "Enable all minor modes appropriate for the current buffer.\n\nIf the optional argument KEEP-MODE-IF-SAME is non-nil, then we\ndon’t re-activate minor modes already enabled in the buffer." "auto-minor-mode"))"#
        ]],
    )
}

fn auto_minor_mode_variable_defaults_docs_locality_and_origins_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_variable_defaults_docs_locality_and_origins_match",
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
                              (let ((origin
                                     (symbol-file
                                      symbol
                                      'defvar)))
                                (and origin
                                     (file-name-base origin)))))
                           '(auto-minor-mode-alist
                             auto-minor-mode-magic-alist))"##,
        expect![[
            r#"OK ((auto-minor-mode-alist nil nil "Alist of filename patterns vs corresponding minor mode functions.\n\nThis is an equivalent of ‘auto-mode-alist’, for minor modes.\n\nUnlike ‘auto-mode-alist’, matching is always case-folded." "auto-minor-mode") (auto-minor-mode-magic-alist nil nil "Alist of buffer beginnings vs corresponding minor mode functions.\n\nThis is an equivalent of ‘magic-mode-alist’, for minor modes.\n\nMagic minor modes are applied after ‘set-auto-mode’ enables any\nmajor mode, so it’s possible to check for expected major modes in\nmatch functions.\n\nUnlike ‘magic-mode-alist’, matching is always case-folded." "auto-minor-mode"))"#
        ]],
    )
    .fresh_process()
}

fn auto_minor_mode_feature_advice_and_load_history_contract_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_feature_advice_and_load_history_contract_matches",
        r##"(let ((history
                                (seq-find
                                 (lambda (entry)
                                   (and
                                    (stringp (car entry))
                                    (string=
                                     (file-name-base
                                      (car entry))
                                     "auto-minor-mode")))
                                 load-history)))
                           (list
                            (featurep 'auto-minor-mode)
                            (featurep 'use-package)
                            (and
                             (advice-member-p
                              #'auto-minor-mode-set
                              #'set-auto-mode)
                             t)
                            (mapcar
                             (lambda (entry)
                               (if (stringp entry)
                                   (file-name-base entry)
                                 entry))
                             history)))"##,
        expect![[
            r#"OK (t nil t ("auto-minor-mode" (require . cl-lib) auto-minor-mode-alist auto-minor-mode-magic-alist (defun . auto-minor-mode-enabled-p) (defun . auto-minor-mode--plain-filename) (defun . auto-minor-mode--run-auto) (defun . auto-minor-mode--run-magic) (defun . auto-minor-mode-set) (provide . auto-minor-mode)))"#
        ]],
    )
}

fn auto_minor_mode_enabled_predicate_distinguishes_registered_state_and_errors() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_enabled_predicate_distinguishes_registered_state_and_errors",
        r##"(let ((auto-minor-mode-test-alpha-mode nil)
                                (auto-minor-mode-test-unregistered-mode t))
                           (list
                            (auto-minor-mode-enabled-p
                             'auto-minor-mode-test-alpha-mode)
                            (progn
                              (setq
                               auto-minor-mode-test-alpha-mode
                               t)
                              (auto-minor-mode-enabled-p
                               'auto-minor-mode-test-alpha-mode))
                            (auto-minor-mode-enabled-p
                             'auto-minor-mode-test-unregistered-mode)
                            (auto-minor-mode-enabled-p
                             'auto-minor-mode-test-missing-mode)
                            (let ((minor-mode-list
                                   (cons
                                    'auto-minor-mode-test-missing-mode
                                    minor-mode-list)))
                              (auto-minor-mode-test-error
                               (lambda ()
                                 (auto-minor-mode-enabled-p
                                  'auto-minor-mode-test-missing-mode))))))"##,
        expect!["OK (nil t nil nil (:signal void-variable (auto-minor-mode-test-missing-mode)))"],
    )
}

fn auto_minor_mode_generated_autoload_contract_installs_variables_function_and_advice()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_generated_autoload_contract_installs_variables_function_and_advice",
        r##"(list
                           (featurep 'auto-minor-mode)
                           (boundp
                            'auto-minor-mode-alist)
                           auto-minor-mode-alist
                           (boundp
                            'auto-minor-mode-magic-alist)
                           auto-minor-mode-magic-alist
                           (fboundp
                            'auto-minor-mode-set)
                           (autoloadp
                            (symbol-function
                             'auto-minor-mode-set))
                           (and
                            (advice-member-p
                             #'auto-minor-mode-set
                             #'set-auto-mode)
                            t)
                           (let ((origin
                                  (symbol-file
                                   'auto-minor-mode-set
                                   'defun)))
                             (and origin
                                  (file-name-base origin))))"##,
        expect![[r#"OK (nil t nil t nil t t t "auto-minor-mode")"#]],
    )
}

pub(super) fn registry_auto_minor_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_minor_mode_exact_descriptor_and_archive_payload_match(),
        auto_minor_mode_complete_target_symbol_inventory_matches(),
        auto_minor_mode_callable_signatures_docs_interactivity_and_origins_match(),
        auto_minor_mode_variable_defaults_docs_locality_and_origins_match(),
        auto_minor_mode_feature_advice_and_load_history_contract_matches(),
        auto_minor_mode_enabled_predicate_distinguishes_registered_state_and_errors(),
    ]
}

pub(super) fn registry_auto_minor_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_minor_mode_generated_autoload_contract_installs_variables_function_and_advice()]
}
