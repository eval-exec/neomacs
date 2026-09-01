use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_1password_exact_descriptor_activation_and_payload_contract_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_exact_descriptor_activation_and_payload_contract_match",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'auth-source-1password
                  package-alist)))
               (directory
                (package-desc-dir descriptor)))
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-kind descriptor)
           (package-desc-reqs descriptor)
           (package-desc-extras descriptor)
           (featurep 'auth-source-1password)
           (package-installed-p
            'auth-source-1password
            '(20260221 2058))
           (file-name-nondirectory
            (locate-library
             "auth-source-1password"))
           (mapcar
            (lambda (file)
              (let ((path
                     (expand-file-name
                      file
                      directory)))
                (list
                 file
                 (file-attribute-size
                  (file-attributes path))
                 (with-temp-buffer
                   (insert-file-contents-literally
                    path)
                   (secure-hash
                    'sha256
                    (current-buffer))))))
            '("auth-source-1password-pkg.el"
              "auth-source-1password.el"))))"##,
        expect![[
            r#"OK (auth-source-1password "20260221.2058" "1password integration for auth-source." nil ((emacs (24 4))) ((:maintainers ("Dominick LoBraico" . "auth-source-1password@lobrai.co")) (:authors ("Dominick LoBraico" . "auth-source-1password@lobrai.co")) (:revdesc . "10961bdc8a3e") (:commit . "10961bdc8a3ed551dde29fde416843058bea2374") (:url . "https://github.com/dlobraico")) t t "auth-source-1password.el" (("auth-source-1password-pkg.el" 437 "8bce0c076e1fd6bd5d85c765fa08644c11c8d0ef98c29668e89997ffde6cf64b") ("auth-source-1password.el" 3463 "3743528b6d5e00fa478badfdfb221ba4c849054ac5a0ec9e57a8568635b335bb")))"#
        ]],
    )
}

fn auth_source_1password_complete_prefixed_symbol_and_source_inventory_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_complete_prefixed_symbol_and_source_inventory_match",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "auth-source-1password"
                     name)
                    (not
                     (string-prefix-p
                      "auth-source-1password-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (and
                    (macrop symbol)
                    t)
                   (boundp symbol)
                   (and
                    (custom-variable-p symbol)
                    t)
                   (when
                       (fboundp symbol)
                     (copy-tree
                      (help-function-arglist
                       symbol
                       t)))
                   (when-let
                       ((source
                         (or
                          (symbol-file symbol 'defun)
                          (symbol-file symbol 'defvar))))
                     (file-name-nondirectory
                      source)))
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
            r#"OK ((auth-source-1password nil nil nil nil nil nil) (auth-source-1password--1password-construct-entry-path t nil nil nil (_backend _type host user _port) "auth-source-1password.el") (auth-source-1password-autoloads nil nil nil nil nil nil) (auth-source-1password-backend nil nil t nil nil "auth-source-1password.el") (auth-source-1password-backend-parse t nil nil nil (entry) "auth-source-1password.el") (auth-source-1password-construct-secret-reference nil nil t t nil "auth-source-1password.el") (auth-source-1password-enable t nil nil nil nil "auth-source-1password.el") (auth-source-1password-executable nil nil t t nil "auth-source-1password.el") (auth-source-1password-search t nil nil nil (&rest spec) "auth-source-1password.el") (auth-source-1password-vault nil nil t t nil "auth-source-1password.el"))"#
        ]],
    )
}

fn auth_source_1password_all_callable_metadata_docs_and_sources_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_all_callable_metadata_docs_and_sources_are_exact",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (macrop symbol)
             (commandp symbol)
             (interactive-form symbol)
             (copy-tree
              (help-function-arglist
               symbol
               t))
             (documentation symbol t)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auth-source-1password--1password-construct-entry-path
            auth-source-1password-search
            auth-source-1password-enable
            auth-source-1password-backend-parse))"##,
        expect![[
            r#"OK ((auth-source-1password--1password-construct-entry-path nil nil nil (_backend _type host user _port) "Construct the full entry-path for the 1password entry for HOST and USER.\nUsually starting with the `auth-source-1password-vault', followed\nby host and user." "auth-source-1password.el") (auth-source-1password-search nil nil nil (&rest spec) "Searche 1password for the specified user and host.\nSPEC, BACKEND, TYPE, HOST, USER and PORT are required by auth-source.\n\n(fn &rest SPEC &key BACKEND TYPE HOST USER PORT &allow-other-keys)" "auth-source-1password.el") (auth-source-1password-enable nil nil nil nil "Enable the 1password auth source." "auth-source-1password.el") (auth-source-1password-backend-parse nil nil nil (entry) "Create a 1password auth-source backend from ENTRY." "auth-source-1password.el"))"#
        ]],
    )
}

fn auth_source_1password_custom_group_and_every_option_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_custom_group_and_every_option_contract_match",
        r##"(list
          (list
           (get
            'auth-source-1password
            'custom-group)
           (documentation-property
            'auth-source-1password
            'group-documentation
            t)
           (get
            'auth-source-1password
            'custom-tag)
           (get
            'auth-source-1password
            'custom-prefix)
           (get
            'auth-source-1password
            'custom-links))
          (mapcar
           (lambda (symbol)
             (let ((standard-value
                    (copy-tree
                     (get symbol 'standard-value))))
               (list
                symbol
                (and
                 (custom-variable-p symbol)
                 t)
                (symbol-value symbol)
                (default-value symbol)
                standard-value
                (and
                 (=
                  (length standard-value)
                  1)
                 (equal
                  (eval
                   (car standard-value)
                   t)
                  (default-value symbol)))
                (copy-tree
                 (get symbol 'custom-type))
                (get symbol 'custom-group)
                (documentation-property
                 symbol
                 'variable-documentation
                 t)
                (special-variable-p symbol)
                (local-variable-if-set-p
                 symbol)
                (file-name-nondirectory
                 (symbol-file symbol 'defvar)))))
           '(auth-source-1password-vault
             auth-source-1password-executable
             auth-source-1password-construct-secret-reference)))"##,
        expect![[
            r#"OK ((((auth-source-1password-vault custom-variable) (auth-source-1password-executable custom-variable) (auth-source-1password-construct-secret-reference custom-variable)) "1password auth source settings." "auth-source-1password" "1password-" nil) ((auth-source-1password-vault t "Personal" "Personal" ((funcall #'#[nil ("Personal") #1=(t)])) t string nil "1Password vault to use when searching for secrets." t nil "auth-source-1password.el") (auth-source-1password-executable t "op" "op" ((funcall #'#[nil ("op") #1#])) t string nil "Executable used for 1password." t nil "auth-source-1password.el") (auth-source-1password-construct-secret-reference t auth-source-1password--1password-construct-entry-path auth-source-1password--1password-construct-entry-path ((funcall #'#[nil ('auth-source-1password--1password-construct-entry-path) #1#])) t function nil "Function to construct the query path in the 1password store." t nil "auth-source-1password.el")))"#
        ]],
    )
}

fn auth_source_1password_backend_object_and_registration_contract_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_backend_object_and_registration_contract_are_exact",
        r##"(list
          (featurep
           'auth-source-1password)
          (auth-source-1password-test-backend-shape
           auth-source-1password-backend)
          (eq
           (slot-value
            auth-source-1password-backend
            'search-function)
           #'auth-source-1password-search)
          (boundp
           'auth-source-backend-parser-functions)
          (memq
           #'auth-source-1password-backend-parse
           auth-source-backend-parser-functions)
          (let ((count 0))
            (dolist
                (function
                 auth-source-backend-parser-functions
                 count)
              (when
                  (eq
                   function
                   #'auth-source-1password-backend-parse)
                (setq count
                      (1+ count)))))
          (advice-member-p
           #'auth-source-1password-backend-parse
           'auth-source-backend-parse))"##,
        expect![[
            r#"OK (t (t auth-source-backend password-store "." t t t nil ignore auth-source-1password-search) t t (auth-source-1password-backend-parse auth-source-backends-parser-secrets auth-source-backends-parser-macos-keychain auth-source-backends-parser-file) 1 nil)"#
        ]],
    )
}

fn auth_source_1password_generated_autoload_contract_registers_only_enable() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_generated_autoload_contract_registers_only_enable",
        r##"(let* ((history
                (seq-find
                 (lambda (entry)
                   (and
                    (stringp
                     (car entry))
                    (string-suffix-p
                     "auth-source-1password-autoloads.el"
                     (car entry))))
                 load-history))
               (events
                (seq-filter
                 (lambda (event)
                   (memq
                    (car-safe event)
                    '(defun provide)))
                 (cdr history)))
               (prefix-files
                (if
                    (hash-table-p
                     definition-prefixes)
                    (gethash
                     "auth-source-1password-"
                     definition-prefixes)
                  (cdr
                   (assoc
                    "auth-source-1password-"
                    definition-prefixes)))))
          (list
           (featurep
            'auth-source-1password-autoloads)
           (featurep
            'auth-source-1password)
           events
           (sort
            (delete-dups
             (copy-sequence
              prefix-files))
            #'string<)
           (mapcar
            (lambda (symbol)
              (let ((definition
                     (and
                      (fboundp symbol)
                      (symbol-function symbol))))
                (list
                 symbol
                 (autoloadp definition)
                 (and
                  (autoloadp definition)
                  (nth 1 definition))
                 (commandp symbol)
                 (help-function-arglist
                  symbol
                  t))))
            '(auth-source-1password-enable
              auth-source-1password-search
              auth-source-1password-backend-parse))
           (mapcar
            (lambda (symbol)
              (list
               symbol
               (boundp symbol)))
            '(auth-source-1password-vault
              auth-source-1password-executable
              auth-source-1password-backend))))"##,
        expect![[
            r#"OK (t nil ((defun . auth-source-1password-enable) (provide . auth-source-1password-autoloads)) ("auth-source-1password") ((auth-source-1password-enable t "auth-source-1password" nil "[Arg list not available until function definition is loaded.]") (auth-source-1password-search nil nil nil t) (auth-source-1password-backend-parse nil nil nil t)) ((auth-source-1password-vault nil) (auth-source-1password-executable nil) (auth-source-1password-backend nil)))"#
        ]],
    )
}

pub(super) fn registry_auth_source_1password_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_1password_exact_descriptor_activation_and_payload_contract_match(),
        auth_source_1password_complete_prefixed_symbol_and_source_inventory_match(),
        auth_source_1password_all_callable_metadata_docs_and_sources_are_exact(),
        auth_source_1password_custom_group_and_every_option_contract_match(),
        auth_source_1password_backend_object_and_registration_contract_are_exact(),
    ]
}

pub(super) fn registry_auth_source_1password_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auth_source_1password_generated_autoload_contract_registers_only_enable()]
}
