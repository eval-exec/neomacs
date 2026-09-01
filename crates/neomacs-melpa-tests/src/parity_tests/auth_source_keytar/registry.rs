use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_keytar_exact_package_descriptor_and_origin_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_exact_package_descriptor_and_origin_contract_match",
        r##"(let ((descriptor
                                (cadr
                                 (assq
                                  'auth-source-keytar
                                  package-alist))))
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (package-desc-extras descriptor)
           (file-name-nondirectory
            (directory-file-name
             (package-desc-dir descriptor)))))"##,
        expect![[
            r#"OK (auth-source-keytar "20251231.1726" "Integrate auth-source with keytar." ((emacs (24 4)) (keytar (0 1 2)) (s (1 12 0))) ((:maintainers ("Jen-Chieh" . "jcs090218@gmail.com")) (:authors ("Jen-Chieh" . "jcs090218@gmail.com")) (:keywords "convenience" "keytar" "password" "credential" "secret" "security") (:revdesc . "ae32dd807aa3") (:commit . "ae32dd807aa3cff59e4384ce8c9d7de259e45998") (:url . "https://github.com/emacs-grammarly/auth-source-keytar")) "auth-source-keytar-20251231.1726")"#
        ]],
    )
}

fn auth_source_keytar_installed_payload_matches_exact_melpa_archive_files_and_hashes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_installed_payload_matches_exact_melpa_archive_files_and_hashes",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auth-source-keytar
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
          (mapcar
           (lambda (name)
             (let ((file
                    (expand-file-name
                     name
                     directory)))
               (list
                name
                (file-exists-p file)
                (file-attribute-size
                 (file-attributes file))
                (with-temp-buffer
                  (insert-file-contents-literally
                   file)
                  (secure-hash
                   'sha256
                   (current-buffer))))))
           '("auth-source-keytar-pkg.el"
             "auth-source-keytar.el")))"##,
        expect![[
            r#"OK (("auth-source-keytar-pkg.el" t 541 "24739cc370ae325b1f0562440e8d32d8be838194cb7518e93b48dc68272fad4b") ("auth-source-keytar.el" t 3573 "8545de8af9cdd25b356c818453fabc5b24efdcde96e34be5af8abc47e533f5aa"))"#
        ]],
    )
}

fn auth_source_keytar_complete_prefixed_symbol_inventory_records_every_public_and_private_surface()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_complete_prefixed_symbol_inventory_records_every_public_and_private_surface",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "auth-source-keytar"
                     name)
                    (not
                     (string-prefix-p
                      "auth-source-keytar-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (get
                     symbol
                     'group-documentation)
                    t)
                   (and
                    (commandp symbol)
                    t)
                   (and
                    (macrop symbol)
                    t))
                  symbols)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name
               (car left))
              (symbol-name
               (car right))))))"##,
        expect![
            "OK ((auth-source-keytar nil nil t nil nil) (auth-source-keytar--build-result t nil nil nil nil) (auth-source-keytar--read-password t nil nil nil nil) (auth-source-keytar-autoloads nil nil nil nil nil) (auth-source-keytar-backend-parse t nil nil nil nil) (auth-source-keytar-enable t nil nil nil nil) (auth-source-keytar-search t nil nil nil nil))"
        ],
    )
}

fn auth_source_keytar_all_callable_arglists_commands_docs_and_sources_are_exact() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_keytar_all_callable_arglists_commands_docs_and_sources_are_exact",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (interactive-form symbol)
             (copy-tree
              (help-function-arglist
               symbol
               t))
             (documentation symbol t)
             (when-let
                 ((source
                   (symbol-file symbol 'defun)))
               (file-name-nondirectory
                source))))
          '(auth-source-keytar-enable
            auth-source-keytar-search
            auth-source-keytar--read-password
            auth-source-keytar--build-result
            auth-source-keytar-backend-parse))"##,
        expect![[
            r#"OK ((auth-source-keytar-enable nil nil nil "Enable auth-source-keytar." "auth-source-keytar.el") (auth-source-keytar-search nil nil (&rest spec) "Given some search query, return matching credentials.\n\nCommon search keys: HOST, USER.\n\nSee `auth-source-search' for details on the parameters SPEC, SERVICE\nand ACCOUNT.\n\n(fn &rest SPEC &key SERVICE ACCOUNT HOST USER &allow-other-keys)" "auth-source-keytar.el") (auth-source-keytar--read-password nil nil (secret) "Read password from SECRET." "auth-source-keytar.el") (auth-source-keytar--build-result nil nil (service) "Build auth-source-keytar entry matching SERVICE." "auth-source-keytar.el") (auth-source-keytar-backend-parse nil nil (entry) "Create a keytar auth-source backend from ENTRY." "auth-source-keytar.el"))"#
        ]],
    )
}

fn auth_source_keytar_custom_group_prefix_parent_and_documentation_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_custom_group_prefix_parent_and_documentation_are_exact",
        r##"(list
          (and
           (get
            'auth-source-keytar
            'group-documentation)
           t)
          (get
           'auth-source-keytar
           'custom-prefix)
          (get
           'auth-source-keytar
           'custom-group)
          (get
           'auth-source-keytar
           'group-documentation)
          (get
           'auth-source-keytar
           'custom-loads)
          (member
           '(auth-source-keytar custom-group)
           (car
            (get
             'auth-source
             'custom-group))))"##,
        expect![[
            r#"OK (t "auth-source-keytar-" nil "Keytar integration within auth-source." nil nil)"#
        ]],
    )
}

fn auth_source_keytar_exact_runtime_dependency_pins_are_active_and_loaded() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_exact_runtime_dependency_pins_are_active_and_loaded",
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
               (file-name-nondirectory
                (directory-file-name
                 (package-desc-dir descriptor)))
               (featurep package)
               (file-name-nondirectory
                (locate-library
                 (symbol-name package))))))
          '(auth-source-keytar
            keytar
            s))"##,
        expect![[
            r#"OK ((auth-source-keytar "20251231.1726" "auth-source-keytar-20251231.1726" t "auth-source-keytar.el") (keytar "20251231.1727" "keytar-20251231.1727" t "keytar.el") (s "20220902.1511" "s-20220902.1511" t "s.el"))"#
        ]],
    )
}

fn auth_source_keytar_source_load_history_records_requires_definitions_and_feature()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_source_load_history_records_requires_definitions_and_feature",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auth-source-keytar.el"
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
            'auth-source-keytar)))"##,
        expect![[
            r#"OK ("auth-source-keytar.el" ((require . auth-source) (require . keytar) (require . s) (defun . auth-source-keytar-enable) (require . help) (defun . auth-source-keytar-search) (defun . auth-source-keytar--read-password) (defun . auth-source-keytar--build-result) (defun . auth-source-keytar-backend-parse) (provide . auth-source-keytar)) t)"#
        ]],
    )
}

fn auth_source_keytar_source_reload_redefines_functions_preserves_group_and_deduplicates_hook()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_source_reload_redefines_functions_preserves_group_and_deduplicates_hook",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE"))
                               (old-search
                                (symbol-function
                                 'auth-source-keytar-search)))
          (fset
           'auth-source-keytar-search
           (lambda (&rest _)
             :fixture))
          (load source nil t t)
          (let ((reloaded-search
                 (symbol-function
                  'auth-source-keytar-search)))
            (load source nil t t)
            (list
             (eq
              old-search
              reloaded-search)
             (eq
              reloaded-search
              (symbol-function
               'auth-source-keytar-search))
             (get
              'auth-source-keytar
              'group-documentation)
             (length
              (seq-filter
               (lambda (function)
                 (eq
                  function
                  #'auth-source-keytar-backend-parse))
               auth-source-backend-parser-functions))
             (featurep
              'auth-source-keytar))))"##,
        expect![[r#"OK (nil nil "Keytar integration within auth-source." 1 t)"#]],
    )
    .fresh_process()
}

fn auth_source_keytar_generated_autoload_registers_enable_without_loading_runtime_dependencies()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_generated_autoload_registers_enable_without_loading_runtime_dependencies",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auth-source-keytar-autoloads.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(defun provide)))
                                  (cdr history)))
                                (definition
                                 (symbol-function
                                  'auth-source-keytar-enable)))
          (list
           (featurep
            'auth-source-keytar-autoloads)
           (featurep
            'auth-source-keytar)
           (featurep 'keytar)
           (featurep 's)
           events
           (and
            (boundp
             'definition-prefixes)
            (gethash
             "auth-source-keytar"
             definition-prefixes))
           (autoloadp definition)
           (nth 1 definition)
           (nth 4 definition)
           (commandp
            'auth-source-keytar-enable)
           (help-function-arglist
            'auth-source-keytar-enable
            t)))"##,
        expect![[
            r#"OK (t nil nil nil ((defun . auth-source-keytar-enable) (provide . auth-source-keytar-autoloads)) nil t "auth-source-keytar" nil nil "[Arg list not available until function definition is loaded.]")"#
        ]],
    )
}

pub(super) fn registry_auth_source_keytar_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_keytar_exact_package_descriptor_and_origin_contract_match(),
        auth_source_keytar_installed_payload_matches_exact_melpa_archive_files_and_hashes(),
        auth_source_keytar_complete_prefixed_symbol_inventory_records_every_public_and_private_surface(),
        auth_source_keytar_all_callable_arglists_commands_docs_and_sources_are_exact(),
        auth_source_keytar_custom_group_prefix_parent_and_documentation_are_exact(),
        auth_source_keytar_exact_runtime_dependency_pins_are_active_and_loaded(),
        auth_source_keytar_source_load_history_records_requires_definitions_and_feature(),
        auth_source_keytar_source_reload_redefines_functions_preserves_group_and_deduplicates_hook(),
    ]
}

pub(super) fn registry_auth_source_keytar_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_keytar_generated_autoload_registers_enable_without_loading_runtime_dependencies(
        ),
    ]
}
