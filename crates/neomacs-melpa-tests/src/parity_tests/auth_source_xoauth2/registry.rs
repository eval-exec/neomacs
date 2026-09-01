use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_xoauth2_descriptor_and_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_descriptor_and_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'auth-source-xoauth2
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name name directory))
                 '("auth-source-xoauth2-pkg.el"
                   "auth-source-xoauth2.el"))))
         (list
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (package-desc-extras descriptor))
          (mapcar
           (lambda (file)
             (list
              (file-name-nondirectory file)
              (file-attribute-size
               (file-attributes file))
              (with-temp-buffer
                (insert-file-contents-literally file)
                (secure-hash
                 'sha256
                 (current-buffer)))))
           sources)))"##,
        expect![[
            r#"OK ((auth-source-xoauth2 "20220804.2219" "Integrate auth-source with XOAUTH2." ((emacs (26 1))) ((:maintainers ("Cesar Crusius" . "ccrusius@google.com")) (:authors ("Cesar Crusius" . "ccrusius@google.com")) (:revdesc . "99a03f8ce835") (:commit . "99a03f8ce835412943d311b2746e77fcf5a1b500") (:url . "https://github.com/ccrusius/auth-source-xoauth2"))) (("auth-source-xoauth2-pkg.el" 419 "48758de84d025a9d7ae07624f15b4a216b7f534a1dea7c6eaef1324d83650eb8") ("auth-source-xoauth2.el" 13178 "6e554d868b29a7b0c1e9cb72e38776de6b51a7dfa02da5d036f484009e9bb639")))"#
        ]],
    )
}

fn auth_source_xoauth2_feature_and_definition_origins_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_feature_and_definition_origins_are_exact",
        r##"(list
         (featurep 'auth-source-xoauth2)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auth-source-xoauth2-search
            auth-source-xoauth2--search
            auth-source-xoauth2--url-post
            auth-source-xoauth2-enable
            auth-source-xoauth2-backend-parse
            auth-source-xoauth2--file-creds
            auth-source-xoauth2-pass--find-match
            auth-source-xoauth2--smtpmail-auth-method
            auth-source-xoauth2--pass-get
            auth-source-xoauth2-pass-creds))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defvar))))
          '(auth-source-xoauth2-creds
            auth-source-xoauth2-use-curl
            auth-source-xoauth2-backend)))"##,
        expect![[
            r#"OK (t ((auth-source-xoauth2-search t "auth-source-xoauth2.el") (auth-source-xoauth2--search t "auth-source-xoauth2.el") (auth-source-xoauth2--url-post t "auth-source-xoauth2.el") (auth-source-xoauth2-enable t "auth-source-xoauth2.el") (auth-source-xoauth2-backend-parse t "auth-source-xoauth2.el") (auth-source-xoauth2--file-creds t "auth-source-xoauth2.el") (auth-source-xoauth2-pass--find-match t "auth-source-xoauth2.el") (auth-source-xoauth2--smtpmail-auth-method t "auth-source-xoauth2.el") (auth-source-xoauth2--pass-get t "auth-source-xoauth2.el") (auth-source-xoauth2-pass-creds t "auth-source-xoauth2.el")) ((auth-source-xoauth2-creds t "auth-source-xoauth2.el") (auth-source-xoauth2-use-curl t "auth-source-xoauth2.el") (auth-source-xoauth2-backend t "auth-source-xoauth2.el")))"#
        ]],
    )
}

fn auth_source_xoauth2_public_functions_have_exact_callable_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_public_functions_have_exact_callable_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)))
         '(auth-source-xoauth2-search
           auth-source-xoauth2--search
           auth-source-xoauth2--url-post
           auth-source-xoauth2-enable
           auth-source-xoauth2-backend-parse
           auth-source-xoauth2--file-creds
           auth-source-xoauth2-pass--find-match
           auth-source-xoauth2--smtpmail-auth-method
           auth-source-xoauth2--pass-get
           auth-source-xoauth2-pass-creds))"##,
        expect![
            "OK ((auth-source-xoauth2-search nil nil (&rest spec)) (auth-source-xoauth2--search nil nil (host user port)) (auth-source-xoauth2--url-post nil nil (url data)) (auth-source-xoauth2-enable nil nil nil) (auth-source-xoauth2-backend-parse nil nil (entry)) (auth-source-xoauth2--file-creds nil nil (file host user port)) (auth-source-xoauth2-pass--find-match nil nil (host user port)) (auth-source-xoauth2--smtpmail-auth-method nil nil (process user password)) (auth-source-xoauth2--pass-get nil nil (key entry)) (auth-source-xoauth2-pass-creds nil nil (host user port)))"
        ],
    )
}

fn auth_source_xoauth2_defaults_and_backend_object_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_defaults_and_backend_object_are_exact",
        r##"(list
         auth-source-xoauth2-creds
         auth-source-xoauth2-use-curl
         (mapcar
          (lambda (slot)
            (slot-value
             auth-source-xoauth2-backend
             slot))
          '(type source host user port data
            create-function search-function))
         (let ((name
                (object-name-string
                 auth-source-xoauth2-backend)))
           (list
            (stringp name)
            (> (length name) 0))))"##,
        expect![[
            r#"OK (nil nil (xoauth2 "." t t t nil ignore auth-source-xoauth2-search) (t t))"#
        ]],
    )
}

fn auth_source_xoauth2_source_load_history_records_definitions_and_provide() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_source_load_history_records_definitions_and_provide",
        r##"(let* ((file
                 (locate-library
                  "auth-source-xoauth2"))
                (history
                 (cdr
                  (assoc file load-history))))
         (seq-filter
          (lambda (event)
            (memq
             (car-safe event)
             '(defun defvar provide)))
          history))"##,
        expect![
            "OK ((defun . nnimap-capability) (defun . nnimap-command) (defun . nnimap-login) (defun . auth-source-xoauth2-search) (defun . auth-source-xoauth2--search) (defun . auth-source-xoauth2--url-post) (defun . auth-source-xoauth2-enable) (defun . auth-source-xoauth2-backend-parse) (defun . auth-source-xoauth2--file-creds) (defun . auth-source-xoauth2-pass--find-match) (defun . auth-source-xoauth2--smtpmail-auth-method) (defun . auth-source-xoauth2--pass-get) (defun . auth-source-xoauth2-pass-creds) (provide . auth-source-xoauth2))"
        ],
    )
}

fn auth_source_xoauth2_reload_preserves_configuration_and_parser_advice() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_reload_preserves_configuration_and_parser_advice",
        r##"(let ((source
                (locate-library
                 "auth-source-xoauth2")))
         (setq auth-source-xoauth2-creds
               '(:token-url "custom")
               auth-source-xoauth2-use-curl
               t)
         (load source nil t t)
         (load source nil t t)
         (list
          auth-source-xoauth2-creds
          auth-source-xoauth2-use-curl
          (and
           (advice-member-p
            #'auth-source-xoauth2-backend-parse
            'auth-source-backend-parse)
           t)
          (featurep 'auth-source-xoauth2)))"##,
        expect![[r#"OK ((:token-url "custom") t t t)"#]],
    )
}

fn auth_source_xoauth2_generated_autoload_registers_enable_function() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_generated_autoload_registers_enable_function",
        r##"(let* ((file
                 (locate-library
                  "auth-source-xoauth2-autoloads"))
                (history
                 (cdr
                  (assoc file load-history))))
         (list
          (featurep 'auth-source-xoauth2-autoloads)
          (featurep 'auth-source-xoauth2)
          (seq-filter
           (lambda (event)
             (memq
              (car-safe event)
              '(defun provide)))
           history)
          (fboundp 'auth-source-xoauth2-enable)
          (autoloadp
           (symbol-function
            'auth-source-xoauth2-enable))
          (commandp 'auth-source-xoauth2-enable)
          (help-function-arglist
           'auth-source-xoauth2-enable
           t)))"##,
        expect![[
            r#"OK (t nil ((defun . auth-source-xoauth2-enable) (provide . auth-source-xoauth2-autoloads)) t t nil "[Arg list not available until function definition is loaded.]")"#
        ]],
    )
}

pub(super) fn registry_auth_source_xoauth2_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_xoauth2_descriptor_and_sources_pin_exact_melpa_payload(),
        auth_source_xoauth2_feature_and_definition_origins_are_exact(),
        auth_source_xoauth2_public_functions_have_exact_callable_contracts(),
        auth_source_xoauth2_defaults_and_backend_object_are_exact(),
        auth_source_xoauth2_source_load_history_records_definitions_and_provide(),
        auth_source_xoauth2_reload_preserves_configuration_and_parser_advice(),
    ]
}

pub(super) fn registry_auth_source_xoauth2_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auth_source_xoauth2_generated_autoload_registers_enable_function()]
}
