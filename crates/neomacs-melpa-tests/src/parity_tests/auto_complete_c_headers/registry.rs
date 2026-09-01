use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_c_headers_descriptor_dependency_and_features_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_descriptor_dependency_and_features_are_exact",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'auto-complete-c-headers
                  package-alist)))
               (auto-complete-descriptor
                (cadr
                 (assq 'auto-complete
                       package-alist)))
               (popup-descriptor
                (cadr
                 (assq 'popup package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (list
           (package-desc-name
            auto-complete-descriptor)
           (package-version-join
            (package-desc-version
             auto-complete-descriptor)))
          (list
           (package-desc-name popup-descriptor)
           (package-version-join
            (package-desc-version
             popup-descriptor)))
          (featurep
           'auto-complete-c-headers)
          (featurep 'auto-complete)
          (featurep 'popup)))"##,
        expect![[
            r#"OK (auto-complete-c-headers "20150912.323" "An auto-complete source for C/C++ header files." ((auto-complete (1 4))) ((:maintainers ("Masafumi Oyamada" . "stillpedant@gmail.com")) (:authors ("Masafumi Oyamada" . "stillpedant@gmail.com")) (:keywords "c") (:revdesc . "52fef720c6f2") (:commit . "52fef720c6f274ad8de52bef39a343421006c511") (:url . "https://github.com/mooz/auto-complete-c-headers")) (auto-complete "20251231.1622") (popup "20251231.1622") t t t)"#
        ]],
    )
}

fn auto_complete_c_headers_installed_payload_bytes_are_pinned() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_installed_payload_bytes_are_pinned",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'auto-complete-c-headers
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (files
                '("auto-complete-c-headers-pkg.el"
                  "auto-complete-c-headers.el")))
         (mapcar
          (lambda (name)
            (let ((file
                   (expand-file-name
                    name directory)))
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
          files))"##,
        expect![[
            r#"OK (("auto-complete-c-headers-pkg.el" 470 "3eac4fd25dedf6f5135a6d4fa4cac7f42190d3836058afc46baa4d87319f29d6") ("auto-complete-c-headers.el" 6387 "c699f3f8fc8a8a7ceb16897a32bf7a922680d362ff288bf2253a07facda5a280"))"#
        ]],
    )
}

fn auto_complete_c_headers_default_configuration_and_source_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_default_configuration_and_source_are_exact",
        r##"(list
         achead:include-patterns
         achead:include-directories
         achead:get-include-directories-function
         achead:ac-prefix
         achead:inspect-remote-directories
         achead:include-cache
         achead:ac-latest-results-alist
         ac-source-c-headers)"##,
        expect![[
            r##"OK (("\\.\\(h\\|hpp\\|hh\\)$" "/[a-zA-Z-_]+$") ("." "/usr/include" "/usr/local/include") achead:get-include-directories "#\\(?:include\\|import\\)[ \11]*[<\"][ \11]*\\([^\"<>' \11\15\n]+\\)" t nil nil ((init setq achead:include-cache nil) (candidates . achead:ac-candidates) (prefix . "#\\(?:include\\|import\\)[ \11]*[<\"][ \11]*\\([^\"<>' \11\15\n]+\\)") (document . achead:documentation-for-candidate) (requires . 0) (symbol . "I") (action . ac-start) (limit)))"##
        ]],
    )
    .fresh_process()
}

fn auto_complete_c_headers_public_function_contracts_and_origins_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_public_function_contracts_and_origins_are_exact",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (fboundp symbol)
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)
            (file-name-nondirectory
             (symbol-file symbol 'defun))))
         '(achead:get-include-directories
           achead:get-include-directories-from-options
           achead:file-list-for-directory
           achead:path-should-be-displayed
           achead:get-include-file-candidates
           achead:documentation-for-candidate
           achead:ac-candidates))"##,
        expect![[
            r#"OK ((achead:get-include-directories t nil nil nil "auto-complete-c-headers.el") (achead:get-include-directories-from-options t nil nil (cmd-line-options) "auto-complete-c-headers.el") (achead:file-list-for-directory t nil nil (dir) "auto-complete-c-headers.el") (achead:path-should-be-displayed t nil nil (path) "auto-complete-c-headers.el") (achead:get-include-file-candidates t nil nil (&optional basedir) "auto-complete-c-headers.el") (achead:documentation-for-candidate t nil nil (candidate) "auto-complete-c-headers.el") (achead:ac-candidates t nil nil nil "auto-complete-c-headers.el"))"#
        ]],
    )
}

fn auto_complete_c_headers_source_load_history_records_complete_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_source_load_history_records_complete_contract",
        r##"(let* ((file
                 (locate-library
                  "auto-complete-c-headers"))
                (history
                 (cdr
                  (assoc file load-history))))
         (seq-filter
          (lambda (event)
            (and
             (consp event)
             (memq
              (car event)
              '(defun defvar provide))))
          history))"##,
        expect![
            "OK ((defun . achead:get-include-directories) (defun . achead:get-include-directories-from-options) (defun . achead:file-list-for-directory) (defun . achead:path-should-be-displayed) (defun . achead:get-include-file-candidates) (defun . achead:documentation-for-candidate) (defun . achead:ac-candidates) (defun . ac-complete-c-headers) (provide . auto-complete-c-headers))"
        ],
    )
}

fn auto_complete_c_headers_reload_resets_defvars_but_redefines_functions_and_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_reload_resets_defvars_but_redefines_functions_and_source",
        r##"(let ((source
                (locate-library
                 "auto-complete-c-headers"))
               (achead:include-patterns
                '("\\.inc\\'"))
               (achead:include-directories
                '("/custom/include"))
               (achead:inspect-remote-directories
                nil)
               (before
                (symbol-function
                 'achead:path-should-be-displayed)))
         (setq ac-source-c-headers
               '((sentinel . custom)))
         (load source nil t t)
         (list
          achead:include-patterns
          achead:include-directories
          achead:inspect-remote-directories
          (eq before
              (symbol-function
               'achead:path-should-be-displayed))
          ac-source-c-headers))"##,
        expect![[
            r##"OK (("\\.inc\\'") ("/custom/include") nil nil ((init setq achead:include-cache nil) (candidates . achead:ac-candidates) (prefix . "#\\(?:include\\|import\\)[ \11]*[<\"][ \11]*\\([^\"<>' \11\15\n]+\\)") (document . achead:documentation-for-candidate) (requires . 0) (symbol . "I") (action . ac-start) (limit)))"##
        ]],
    )
}

fn auto_complete_c_headers_source_entries_resolve_to_callable_completion_behavior()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_source_entries_resolve_to_callable_completion_behavior",
        r##"(let ((init
                (cdr
                 (assq 'init
                       ac-source-c-headers)))
               (candidates
                (cdr
                 (assq 'candidates
                       ac-source-c-headers)))
               (document
                (cdr
                 (assq 'document
                       ac-source-c-headers)))
               (action
                (cdr
                 (assq 'action
                       ac-source-c-headers))))
         (list
          init
          candidates
          document
          action
          (functionp candidates)
          (functionp document)
          (functionp action)
          (cdr
           (assq 'requires
                 ac-source-c-headers))
          (cdr
           (assq 'symbol
                 ac-source-c-headers))
          (cdr
           (assq 'limit
                 ac-source-c-headers))))"##,
        expect![[
            r#"OK ((setq achead:include-cache nil) achead:ac-candidates achead:documentation-for-candidate ac-start t t t 0 "I" nil)"#
        ]],
    )
}

fn auto_complete_c_headers_generated_autoload_provides_feature_without_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_c_headers_generated_autoload_provides_feature_without_source",
        r##"(let* ((file
                 (locate-library
                  "auto-complete-c-headers-autoloads"))
                (history
                 (cdr
                  (assoc file load-history))))
         (list
          (featurep
           'auto-complete-c-headers-autoloads)
          (featurep
           'auto-complete-c-headers)
          (seq-filter
           (lambda (event)
             (memq
              (car-safe event)
              '(defun defvar provide)))
           history)
          (boundp 'ac-source-c-headers)
          (fboundp
           'achead:ac-candidates)))"##,
        expect!["OK (t nil ((provide . auto-complete-c-headers-autoloads)) nil nil)"],
    )
}

pub(super) fn registry_auto_complete_c_headers_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_c_headers_descriptor_dependency_and_features_are_exact(),
        auto_complete_c_headers_installed_payload_bytes_are_pinned(),
        auto_complete_c_headers_default_configuration_and_source_are_exact(),
        auto_complete_c_headers_public_function_contracts_and_origins_are_exact(),
        auto_complete_c_headers_source_load_history_records_complete_contract(),
        auto_complete_c_headers_reload_resets_defvars_but_redefines_functions_and_source(),
        auto_complete_c_headers_source_entries_resolve_to_callable_completion_behavior(),
    ]
}

pub(super) fn registry_auto_complete_c_headers_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_c_headers_generated_autoload_provides_feature_without_source()]
}
