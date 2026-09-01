use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_gopass_descriptor_and_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_descriptor_and_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'auth-source-gopass
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name name directory))
                 '("auth-source-gopass-pkg.el"
                   "auth-source-gopass.el"))))
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
            r#"OK ((auth-source-gopass "20230109.1213" "Gopass integration for auth-source." ((emacs (24 4))) ((:maintainers ("Markus M. May" . "mmay@javafreedom.org")) (:authors ("Markus M. May" . "mmay@javafreedom.org")) (:revdesc . "6f7f0cc0d682") (:commit . "6f7f0cc0d682f66d11f7fac4fa5c1e79904232da") (:url . "https://github.com/"))) (("auth-source-gopass-pkg.el" 392 "f02b5312a9651f9f7564ee51a6bad055575c856b7927d95c3a0fe11d9e966b10") ("auth-source-gopass.el" 3650 "747be8d63cf6fb43cfca5ad9f21c20f9831a239bb42e85661a38bffef24c8082")))"#
        ]],
    )
}

fn auth_source_gopass_feature_and_definition_origins_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_feature_and_definition_origins_are_exact",
        r##"(list
         (featurep 'auth-source-gopass)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auth-source-gopass--gopass-construct-query-path
            auth-source-gopass-search
            auth-source-gopass-enable
            auth-source-gopass-backend-parse))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defvar))))
          '(auth-source-gopass-path-prefix
            auth-source-gopass-path-separator
            auth-source-gopass-executable
            auth-source-gopass-construct-query-path
            auth-source-gopass-backend)))"##,
        expect![[
            r#"OK (t ((auth-source-gopass--gopass-construct-query-path t "auth-source-gopass.el") (auth-source-gopass-search t "auth-source-gopass.el") (auth-source-gopass-enable t "auth-source-gopass.el") (auth-source-gopass-backend-parse t "auth-source-gopass.el")) ((auth-source-gopass-path-prefix t "auth-source-gopass.el") (auth-source-gopass-path-separator t "auth-source-gopass.el") (auth-source-gopass-executable t "auth-source-gopass.el") (auth-source-gopass-construct-query-path t "auth-source-gopass.el") (auth-source-gopass-backend t "auth-source-gopass.el")))"#
        ]],
    )
}

fn auth_source_gopass_public_functions_have_exact_callable_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_public_functions_have_exact_callable_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)
            (documentation symbol)))
         '(auth-source-gopass--gopass-construct-query-path
           auth-source-gopass-search
           auth-source-gopass-enable
           auth-source-gopass-backend-parse))"##,
        expect![[
            r#"OK ((auth-source-gopass--gopass-construct-query-path nil nil (_backend _type host user _port) "Construct the full entry-path for the gopass entry grom HOST and USER.\nUsually starting with the ‘auth-source-gopass-path-prefix’, followed by host\nand user, separated by the ‘auth-source-gopass-path-separator’.") (auth-source-gopass-search nil nil (&rest spec) "Searche gopass for the specified user and host.\nSPEC, BACKEND, TYPE, HOST, USER and PORT are required by auth-source.\n\n(fn &rest SPEC &key BACKEND TYPE HOST USER PORT &allow-other-keys)") (auth-source-gopass-enable nil nil nil "Enable the gopass auth source.") (auth-source-gopass-backend-parse nil nil (entry) "Create a gopass auth-source backend from ENTRY."))"#
        ]],
    )
}

fn auth_source_gopass_custom_options_have_exact_schema_and_defaults() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_custom_options_have_exact_schema_and_defaults",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (default-value symbol)
            (get symbol 'custom-type)
            (get symbol 'custom-group)
            (get symbol 'standard-value)))
         '(auth-source-gopass-path-prefix
           auth-source-gopass-path-separator
           auth-source-gopass-executable
           auth-source-gopass-construct-query-path))"##,
        expect![[
            r#"OK ((auth-source-gopass-path-prefix "accounts" string nil ((funcall #'#[nil ("accounts") #1=(t)]))) (auth-source-gopass-path-separator "/" string nil ((funcall #'#[nil ("/") #1#]))) (auth-source-gopass-executable "gopass" string nil ((funcall #'#[nil ("gopass") #1#]))) (auth-source-gopass-construct-query-path auth-source-gopass--gopass-construct-query-path function nil ((funcall #'#[nil ('auth-source-gopass--gopass-construct-query-path) #1#]))))"#
        ]],
    )
}

fn auth_source_gopass_load_history_records_public_definitions_and_provide() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_load_history_records_public_definitions_and_provide",
        r##"(let* ((file
                 (locate-library
                  "auth-source-gopass"))
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
            "OK ((defun . auth-source-gopass--gopass-construct-query-path) (defun . auth-source-gopass-search) (defun . auth-source-gopass-enable) (defun . auth-source-gopass-backend-parse) (provide . auth-source-gopass))"
        ],
    )
}

fn auth_source_gopass_reload_preserves_custom_values_and_single_parser_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_reload_preserves_custom_values_and_single_parser_hook",
        r##"(let ((source
                (locate-library
                 "auth-source-gopass")))
         (setq auth-source-gopass-path-prefix "custom"
               auth-source-gopass-path-separator "::"
               auth-source-gopass-executable "company-gopass"
               auth-source-gopass-construct-query-path #'identity)
         (load source nil t t)
         (load source nil t t)
         (list
          auth-source-gopass-path-prefix
          auth-source-gopass-path-separator
          auth-source-gopass-executable
          auth-source-gopass-construct-query-path
          (cl-count
           #'auth-source-gopass-backend-parse
           auth-source-backend-parser-functions)))"##,
        expect![[r#"OK ("custom" "::" "company-gopass" identity 1)"#]],
    )
}

fn auth_source_gopass_generated_autoload_registers_enable_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_generated_autoload_registers_enable_command",
        r##"(let* ((file
                 (locate-library
                  "auth-source-gopass-autoloads"))
                (history
                 (cdr
                  (assoc file load-history))))
         (list
          (featurep 'auth-source-gopass-autoloads)
          (featurep 'auth-source-gopass)
          (seq-filter
           (lambda (event)
             (memq
              (car-safe event)
              '(defun provide)))
           history)
          (fboundp 'auth-source-gopass-enable)
          (autoloadp
           (symbol-function
            'auth-source-gopass-enable))
          (commandp 'auth-source-gopass-enable)
          (help-function-arglist
           'auth-source-gopass-enable
           t)))"##,
        expect![[
            r#"OK (t nil ((defun . auth-source-gopass-enable) (provide . auth-source-gopass-autoloads)) t t nil "[Arg list not available until function definition is loaded.]")"#
        ]],
    )
}

pub(super) fn registry_auth_source_gopass_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_gopass_descriptor_and_sources_pin_exact_melpa_payload(),
        auth_source_gopass_feature_and_definition_origins_are_exact(),
        auth_source_gopass_public_functions_have_exact_callable_contracts(),
        auth_source_gopass_custom_options_have_exact_schema_and_defaults(),
        auth_source_gopass_load_history_records_public_definitions_and_provide(),
        auth_source_gopass_reload_preserves_custom_values_and_single_parser_hook(),
    ]
}

pub(super) fn registry_auth_source_gopass_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auth_source_gopass_generated_autoload_registers_enable_command()]
}
