use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_kwallet_exact_package_descriptor_origin_and_dependency_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_exact_package_descriptor_origin_and_dependency_contract_match",
        r##"(let* ((description
                                 (cadr
                                  (assq
                                   'auth-source-kwallet
                                   package-alist)))
                                (extras
                                 (package-desc-extras description))
                                ;; Mask the installed package's own
                                ;; directory.  Spelling it out pinned the
                                ;; harness's acquisition layout, so this
                                ;; expectation broke when the cache moved
                                ;; from package-cache/ to the
                                ;; revision-pinned source-install-cache/ --
                                ;; a harness change wearing the shape of a
                                ;; package regression.
                                (installed
                                 (directory-file-name
                                  (file-name-directory
                                   (getenv
                                    "NEOMACS_PACKAGE_SOURCE")))))
                           (list
                            (package-desc-name description)
                            (package-version-join
                             (package-desc-version description))
                            (package-desc-summary description)
                            (package-desc-reqs description)
                            (package-desc-kind description)
                            (package-desc-archive description)
                            (replace-regexp-in-string
                             (regexp-quote installed)
                             "[PACKAGE]"
                             (package-desc-dir description)
                             t t)
                            (alist-get :commit extras)
                            (alist-get :revdesc extras)
                            (alist-get :url extras)
                            (alist-get :authors extras)
                            (alist-get :maintainers extras)))"##,
        expect![[
            r#"OK (auth-source-kwallet "20250419.1330" "KWallet integration for auth-source." ((emacs (24 4))) nil nil "[PACKAGE]" "1e1bff2403966c3a0683ee65fb28cb8d8ff2c389" "1e1bff240396" "https://github.com/vaartis/auth-source-kwallet" (("Ekaterina Vaartis" . "vaartis@kotobank.ch")) (("Ekaterina Vaartis" . "vaartis@kotobank.ch")))"#
        ]],
    )
}

fn auth_source_kwallet_installed_payload_inventory_and_exact_archive_hashes_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_installed_payload_inventory_and_exact_archive_hashes_match",
        r##"(let* ((description
                                 (cadr
                                  (assq
                                   'auth-source-kwallet
                                   package-alist)))
                                (directory
                                 (package-desc-dir description)))
                           (mapcar
                            (lambda (name)
                              (let ((file
                                     (expand-file-name
                                      name
                                      directory)))
                                (cond
                                 ((member
                                   name
                                   '("auth-source-kwallet.el"
                                     "auth-source-kwallet-pkg.el"))
                                  (list
                                   name
                                   :archive
                                   (file-attribute-size
                                    (file-attributes file))
                                   (with-temp-buffer
                                     (set-buffer-multibyte
                                      nil)
                                     (insert-file-contents-literally
                                      file)
                                     (secure-hash
                                      'sha256
                                      (current-buffer)))))
                                 (t
                                  (list name :generated t)))))
                            (sort
                             (directory-files
                              directory
                              nil
                              "\\`[^.]")
                             #'string<)))"##,
        expect![[
            r#"OK (("auth-source-kwallet-autoloads.el" :generated t) ("auth-source-kwallet-pkg.el" :archive 427 "e3805f16efde58f38c2b12eb4a6b6ed24c4d1ca0c8e3b76199608ba194258101") ("auth-source-kwallet.el" :archive 3443 "b950902e0dad963d2e85bcffe0d55c6ee7b61e58b64b9047443308a4c2ef4241") ("auth-source-kwallet.elc" :generated t))"#
        ]],
    )
}

fn auth_source_kwallet_complete_customization_defaults_and_metadata_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_complete_customization_defaults_and_metadata_match",
        r##"(mapcar
                          (lambda (symbol)
                            (list
                             symbol
                             (symbol-value symbol)
                             (get symbol 'custom-type)
                             (get symbol 'standard-value)
                             (get symbol 'custom-group)
                             (get symbol 'safe-local-variable)
                             (get symbol 'risky-local-variable)
                             (documentation-property
                              symbol
                              'variable-documentation
                              t)))
                          '(auth-source-kwallet-wallet
                            auth-source-kwallet-folder
                            auth-source-kwallet-key-separator
                            auth-source-kwallet-executable))"##,
        expect![[
            r#"OK ((auth-source-kwallet-wallet "Passwords" string ((funcall #'#[nil ("Passwords") #1=(t)])) nil nil nil "KWallet wallet to use.") (auth-source-kwallet-folder "Passwords" string ((funcall #'#[nil ("Passwords") #1#])) nil nil nil "KWallet folder to use.") (auth-source-kwallet-key-separator "@" string ((funcall #'#[nil ("@") #1#])) nil nil nil "Separator to use between the user and the host for KWallet.") (auth-source-kwallet-executable "kwallet-query" string ((funcall #'#[nil ("kwallet-query") #1#])) nil nil nil "Executable used to query kwallet."))"#
        ]],
    )
}

fn auth_source_kwallet_complete_function_surface_arglists_docs_and_origins_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_kwallet_complete_function_surface_arglists_docs_and_origins_match",
        r##"(mapcar
                          (lambda (symbol)
                            (list
                             symbol
                             (help-function-arglist symbol t)
                             (documentation symbol t)
                             (interactive-form symbol)
                             (autoloadp
                              (symbol-function symbol))
                             (file-name-nondirectory
                              (symbol-file symbol 'defun))))
                          '(auth-source-kwallet--kwallet-search
                            auth-source-kwallet--kwallet-backend-parse
                            auth-source-kwallet-enable))"##,
        expect![[
            r#"OK ((auth-source-kwallet--kwallet-search (&rest spec) "Searche KWallet for the specified user and host.\nSPEC, BACKEND, TYPE, HOST, USER and PORT are as required by auth-source.\n\n(fn &rest SPEC &key BACKEND TYPE HOST USER PORT &allow-other-keys)" nil nil "auth-source-kwallet.el") (auth-source-kwallet--kwallet-backend-parse (entry) "Parse the entry to check if this is a kwallet entry.\nENTRY is as required by auth-source." nil nil "auth-source-kwallet.el") (auth-source-kwallet-enable nil "Enable the kwallet auth source." nil nil "auth-source-kwallet.el"))"#
        ]],
    )
}

fn auth_source_kwallet_source_load_provides_feature_without_enabling_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_source_load_provides_feature_without_enabling_backend",
        r##"(list
                          (featurep 'auth-source-kwallet)
                          auth-sources
                          (and
                           (advice-member-p
                            #'auth-source-kwallet--kwallet-backend-parse
                            'auth-source-backend-parse)
                           t)
                          auth-source-kwallet-test-executable-calls
                          auth-source-kwallet-test-process-calls)"##,
        expect![[r#"OK (t ("~/.authinfo" "~/.authinfo.gpg" "~/.netrc") nil nil nil)"#]],
    )
    .fresh_process()
}

fn auth_source_kwallet_source_reload_preserves_every_user_assignment_and_backend_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_source_reload_preserves_every_user_assignment_and_backend_state",
        r##"(progn
                          (setq
                           auth-source-kwallet-wallet
                           "Engineering Wallet"
                           auth-source-kwallet-folder
                           "Production Tokens"
                           auth-source-kwallet-key-separator
                           "::"
                           auth-source-kwallet-executable
                           "kwallet-query-custom"
                           auth-sources
                           '(kwallet "secondary.authinfo"))
                          (auth-source-kwallet-enable)
                          (let ((source
                                 (symbol-file
                                  'auth-source-kwallet-enable
                                  'defun)))
                            (load source nil t t))
                          (list
                           auth-source-kwallet-wallet
                           auth-source-kwallet-folder
                           auth-source-kwallet-key-separator
                           auth-source-kwallet-executable
                           auth-sources
                           (and
                            (advice-member-p
                             #'auth-source-kwallet--kwallet-backend-parse
                             'auth-source-backend-parse)
                            t)
                           (featurep 'auth-source-kwallet)))"##,
        expect![[
            r#"OK ("Engineering Wallet" "Production Tokens" "::" "kwallet-query-custom" (kwallet "secondary.authinfo") t t)"#
        ]],
    )
}

fn auth_source_kwallet_custom_setters_accept_runtime_configuration_and_restore_defaults()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_custom_setters_accept_runtime_configuration_and_restore_defaults",
        r##"(let ((symbols
                                '(auth-source-kwallet-wallet
                                  auth-source-kwallet-folder
                                  auth-source-kwallet-key-separator
                                  auth-source-kwallet-executable)))
                           (let ((before
                                  (mapcar
                                   #'symbol-value
                                   symbols)))
                             (customize-set-variable
                              'auth-source-kwallet-wallet
                              "Team Vault")
                             (customize-set-variable
                              'auth-source-kwallet-folder
                              "CI Credentials")
                             (customize-set-variable
                              'auth-source-kwallet-key-separator
                              "|")
                             (customize-set-variable
                              'auth-source-kwallet-executable
                              "kwallet-query-v2")
                             (let ((changed
                                    (mapcar
                                     #'symbol-value
                                     symbols)))
                               (mapc
                                #'custom-reevaluate-setting
                                symbols)
                               (list
                                before
                                changed
                                (mapcar
                                 #'symbol-value
                                 symbols)
                                (mapcar
                                 (lambda (symbol)
                                   (get symbol 'saved-value))
                                 symbols)))))"##,
        expect![[
            r#"OK (("Passwords" "Passwords" "@" "kwallet-query") ("Team Vault" "CI Credentials" "|" "kwallet-query-v2") ("Passwords" "Passwords" "@" "kwallet-query") (nil nil nil nil))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_generated_autoload_exposes_only_enable_before_activation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_kwallet_generated_autoload_exposes_only_enable_before_activation",
        r##"(list
                          (featurep 'auth-source-kwallet)
                          (boundp 'auth-source-kwallet-wallet)
                          (boundp 'auth-source-kwallet-folder)
                          (boundp
                           'auth-source-kwallet-key-separator)
                          (boundp
                           'auth-source-kwallet-executable)
                          (mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (fboundp symbol)
                              (and
                               (fboundp symbol)
                               (symbol-function symbol))))
                           '(auth-source-kwallet--kwallet-search
                             auth-source-kwallet--kwallet-backend-parse
                             auth-source-kwallet-enable))
                          (get
                           'auth-source-kwallet-enable
                           'function-documentation)
                          (get
                           'auth-source-kwallet-enable
                           'definition-name))"##,
        expect![[
            r#"OK (nil nil nil nil nil ((auth-source-kwallet--kwallet-search nil nil) (auth-source-kwallet--kwallet-backend-parse nil nil) (auth-source-kwallet-enable t (autoload "auth-source-kwallet" "Enable the kwallet auth source." nil nil))) nil nil)"#
        ]],
    )
}

fn auth_source_kwallet_generated_autoload_performs_real_package_activation_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_generated_autoload_performs_real_package_activation_workflow",
        r##"(progn
                          (setq auth-sources nil)
                          (auth-source-kwallet-enable)
                          (list
                           (featurep 'auth-source-kwallet)
                           auth-sources
                           auth-source-kwallet-wallet
                           auth-source-kwallet-folder
                           auth-source-kwallet-key-separator
                           auth-source-kwallet-executable
                           (autoloadp
                            (symbol-function
                             'auth-source-kwallet-enable))
                           (and
                            (advice-member-p
                             #'auth-source-kwallet--kwallet-backend-parse
                             'auth-source-backend-parse)
                            t)))"##,
        expect![[r#"OK (t (kwallet) "Passwords" "Passwords" "@" "kwallet-query" nil t)"#]],
    )
}

pub(super) fn registry_auth_source_kwallet_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_kwallet_exact_package_descriptor_origin_and_dependency_contract_match(),
        auth_source_kwallet_installed_payload_inventory_and_exact_archive_hashes_match(),
        auth_source_kwallet_complete_customization_defaults_and_metadata_match(),
        auth_source_kwallet_complete_function_surface_arglists_docs_and_origins_match(),
        auth_source_kwallet_source_load_provides_feature_without_enabling_backend(),
        auth_source_kwallet_source_reload_preserves_every_user_assignment_and_backend_state(),
        auth_source_kwallet_custom_setters_accept_runtime_configuration_and_restore_defaults(),
    ]
}

pub(super) fn registry_auth_source_kwallet_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_kwallet_generated_autoload_exposes_only_enable_before_activation(),
        auth_source_kwallet_generated_autoload_performs_real_package_activation_workflow(),
    ]
}
