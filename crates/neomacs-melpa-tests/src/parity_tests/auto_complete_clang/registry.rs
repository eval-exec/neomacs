use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_descriptor_dependencies_versions_and_features_are_exact() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_descriptor_dependencies_versions_and_features_are_exact",
        r##"(let* ((descriptor
                (cadr
                 (assq 'auto-complete-clang
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
          (featurep 'auto-complete-clang)
          (featurep 'auto-complete)
          (featurep 'popup)))"##,
        expect![[
            r#"OK (auto-complete-clang "20140409.752" "Auto Completion source for clang for GNU Emacs." ((auto-complete (1 3 1))) ((:maintainers ("Brian Jiang" . "brianjcj@gmail.com")) (:authors ("Brian Jiang" . "brianjcj@gmail.com")) (:keywords "completion" "convenience") (:revdesc . "a195db1d0593") (:commit . "a195db1d0593b4fb97efe50885e12aa6764d998c") (:url . "https://github.com/brianjcj/auto-complete-clang")) (auto-complete "20251231.1622") (popup "20251231.1622") t t t)"#
        ]],
    )
}

fn auto_complete_clang_installed_payload_bytes_are_pinned() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_installed_payload_bytes_are_pinned",
        r##"(let* ((descriptor
                (cadr
                 (assq 'auto-complete-clang
                       package-alist)))
               (directory
                (package-desc-dir descriptor))
               (files
                '("auto-complete-clang-pkg.el"
                  "auto-complete-clang.el")))
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
            r#"OK (("auto-complete-clang-pkg.el" 475 "dcf23377063e7dac65adae4f94070fe7d83b61b8189ba826b48834af3f864362") ("auto-complete-clang.el" 15838 "dc8a30d8aef143066e7b80a6618e0f9a515b3e4d9a1da432601142c35f0ebb48"))"#
        ]],
    )
}

fn auto_complete_clang_defaults_custom_types_faces_and_source_alists_are_exact() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_defaults_custom_types_faces_and_source_alists_are_exact",
        r##"(list
         (and ac-clang-executable
              (file-name-nondirectory
               ac-clang-executable))
         ac-clang-auto-save
         ac-clang-lang-option-function
         ac-clang-flags
         ac-clang-prefix-header
         ac-clang-completion-pattern
         ac-clang-error-buffer-name
         ac-template-start-point
         ac-template-candidates
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (get symbol 'custom-type)
             (get symbol 'custom-group)))
          '(ac-clang-executable
            ac-clang-auto-save
            ac-clang-lang-option-function
            ac-clang-flags))
         (get 'ac-clang-candidate-face
              'face-defface-spec)
         (get 'ac-clang-selection-face
              'face-defface-spec)
         ac-source-clang
         ac-source-template)"##,
        expect![[
            r#"OK ("clang" nil nil nil nil "^COMPLETION: \\(%s[^ \n:]*\\)\\(?: : \\)*\\(.*$\\)" "*clang error*" nil ("ok" "no" "yes:)") ((ac-clang-executable file nil) (ac-clang-auto-save (choice (const :tag "Off" nil) (const :tag "On" t)) nil) (ac-clang-lang-option-function function nil) (ac-clang-flags (repeat (string :tag "Argument" "")) nil)) ((t (:background "lightgray" :foreground "navy"))) ((t (:background "navy" :foreground "white"))) ((candidates . ac-clang-candidate) (candidate-face . ac-clang-candidate-face) (selection-face . ac-clang-selection-face) (prefix . ac-clang-prefix) (requires . 0) (document . ac-clang-document) (action . ac-clang-action) (cache) (symbol . "c")) ((candidates . ac-template-candidate) (prefix . ac-template-prefix) (requires . 0) (action . ac-template-action) (document . ac-clang-document) (cache) (symbol . "t")))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_clang_complete_public_function_contract_and_definition_origins_are_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_complete_public_function_contract_and_definition_origins_are_exact",
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
         '(ac-clang-set-prefix-header
           ac-clang-set-cflags
           ac-clang-set-cflags-from-shell-command
           ac-clang-parse-output
           ac-clang-handle-error
           ac-clang-call-process
           ac-clang-build-location
           ac-clang-lang-option
           ac-clang-build-complete-args
           ac-clang-clean-document
           ac-clang-document
           ac-in-string/comment
           ac-clang-candidate
           ac-clang-action
           ac-clang-prefix
           ac-clang-same-count-in-string
           ac-clang-split-args
           ac-template-candidate
           ac-template-action
           ac-template-prefix
           ac-complete-clang
           ac-complete-template))"##,
        expect![[
            r#"OK ((ac-clang-set-prefix-header t t (interactive (let ((def (car (directory-files "." t "\\([^.]h\\|[^h]\\).pch\\'" t)))) (list (read-file-name (concat "Clang prefix header(current: " ac-clang-prefix-header ") : ") (if def (progn (file-name-directory def))) def nil (if def (progn (file-name-nondirectory def))))))) (ph) "auto-complete-clang.el") (ac-clang-set-cflags t t (interactive nil) nil "auto-complete-clang.el") (ac-clang-set-cflags-from-shell-command t t (interactive nil) nil "auto-complete-clang.el") (ac-clang-parse-output t nil nil (prefix) "auto-complete-clang.el") (ac-clang-handle-error t nil nil (res args) "auto-complete-clang.el") (ac-clang-call-process t nil nil (prefix &rest args) "auto-complete-clang.el") (ac-clang-build-location t nil nil (pos) "auto-complete-clang.el") (ac-clang-lang-option t nil nil nil "auto-complete-clang.el") (ac-clang-build-complete-args t nil nil (pos) "auto-complete-clang.el") (ac-clang-clean-document t nil nil (s) "auto-complete-clang.el") (ac-clang-document t nil nil (item) "auto-complete-clang.el") (ac-in-string/comment t nil nil nil "auto-complete-clang.el") (ac-clang-candidate t nil nil nil "auto-complete-clang.el") (ac-clang-action t t (interactive nil) nil "auto-complete-clang.el") (ac-clang-prefix t nil nil nil "auto-complete-clang.el") (ac-clang-same-count-in-string t nil nil (c1 c2 s) "auto-complete-clang.el") (ac-clang-split-args t nil nil (s) "auto-complete-clang.el") (ac-template-candidate t nil nil nil "auto-complete-clang.el") (ac-template-action t t (interactive nil) nil "auto-complete-clang.el") (ac-template-prefix t nil nil nil "auto-complete-clang.el") (ac-complete-clang t t (interactive nil) nil "auto-complete-clang.el") (ac-complete-template t t (interactive nil) nil "auto-complete-clang.el"))"#
        ]],
    )
}

fn auto_complete_clang_source_load_history_records_functions_faces_commands_and_provider()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_source_load_history_records_functions_faces_commands_and_provider",
        r##"(let* ((file
                 (locate-library
                  "auto-complete-clang"))
                (history
                 (cdr
                  (assoc file load-history))))
         (seq-filter
          (lambda (event)
            (and
             (consp event)
             (memq
              (car event)
              '(defun defface defvar
                provide))))
          history))"##,
        expect![
            "OK ((provide . auto-complete-clang) (defun . ac-clang-set-prefix-header) (defun . ac-clang-set-cflags) (defun . ac-clang-set-cflags-from-shell-command) (defun . ac-clang-parse-output) (defun . ac-clang-handle-error) (defun . ac-clang-call-process) (defun . ac-clang-build-location) (defun . ac-clang-lang-option) (defun . ac-clang-build-complete-args) (defun . ac-clang-clean-document) (defun . ac-clang-document) (defface . ac-clang-candidate-face) (defface . ac-clang-selection-face) (defun . ac-in-string/comment) (defun . ac-clang-candidate) (defun . ac-clang-action) (defun . ac-clang-prefix) (defun . ac-complete-clang) (defun . ac-clang-same-count-in-string) (defun . ac-clang-split-args) (defun . ac-template-candidate) (defun . ac-template-action) (defun . ac-template-prefix) (defun . ac-complete-template))"
        ],
    )
}

fn auto_complete_clang_reload_preserves_defcustom_values_but_redefines_sources_and_functions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_reload_preserves_defcustom_values_but_redefines_sources_and_functions",
        r##"(let ((source
                (locate-library
                 "auto-complete-clang"))
               (ac-clang-executable
                "/custom/clang")
               (ac-clang-auto-save t)
               (ac-clang-flags
                '("-DRELOAD"))
               (ac-clang-prefix-header
                "prefix.pch")
               (before
                (symbol-function
                 'ac-clang-parse-output)))
         (setq ac-source-clang
               '((sentinel . clang)))
         (setq ac-source-template
               '((sentinel . template)))
         (load source nil t t)
         (list
          ac-clang-executable
          ac-clang-auto-save
          ac-clang-flags
          ac-clang-prefix-header
          (eq before
              (symbol-function
               'ac-clang-parse-output))
          ac-source-clang
          ac-source-template))"##,
        expect![[
            r#"OK ("/custom/clang" t ("-DRELOAD") "prefix.pch" nil ((candidates . ac-clang-candidate) (candidate-face . ac-clang-candidate-face) (selection-face . ac-clang-selection-face) (prefix . ac-clang-prefix) (requires . 0) (document . ac-clang-document) (action . ac-clang-action) (cache) (symbol . "c")) ((candidates . ac-template-candidate) (prefix . ac-template-prefix) (requires . 0) (action . ac-template-action) (document . ac-clang-document) (cache) (symbol . "t")))"#
        ]],
    )
}

fn auto_complete_clang_source_entries_resolve_to_callable_completion_contracts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_source_entries_resolve_to_callable_completion_contracts",
        r##"(mapcar
         (lambda (source)
           (let ((value
                  (symbol-value source)))
             (list
              source
              (mapcar #'car value)
              (mapcar
               (lambda (key)
                 (let ((entry
                        (cdr
                         (assq key value))))
                   (list
                    key entry
                    (and entry
                         (functionp
                          entry)))))
               '(candidates prefix
                 document action))
              (cdr (assq 'requires value))
              (cdr (assq 'cache value))
              (cdr (assq 'symbol value)))))
         '(ac-source-clang
           ac-source-template))"##,
        expect![[
            r#"OK ((ac-source-clang (candidates candidate-face selection-face prefix requires document action cache symbol) ((candidates ac-clang-candidate t) (prefix ac-clang-prefix t) (document ac-clang-document t) (action ac-clang-action t)) 0 nil "c") (ac-source-template (candidates prefix requires action document cache symbol) ((candidates ac-template-candidate t) (prefix ac-template-prefix t) (document ac-clang-document t) (action ac-template-action t)) 0 nil "t"))"#
        ]],
    )
}

fn auto_complete_clang_generated_autoload_file_only_registers_its_feature() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_generated_autoload_file_only_registers_its_feature",
        r##"(let* ((file
                 (locate-library
                  "auto-complete-clang-autoloads"))
                (history
                 (cdr
                  (assoc file load-history))))
         (list
          (featurep
           'auto-complete-clang-autoloads)
          (featurep 'auto-complete-clang)
          (seq-filter
           (lambda (event)
             (memq
              (car-safe event)
              '(defun defvar provide)))
           history)
          (fboundp 'ac-clang-candidate)
          (boundp 'ac-source-clang)))"##,
        expect!["OK (t nil ((provide . auto-complete-clang-autoloads)) nil nil)"],
    )
}

pub(super) fn registry_auto_complete_clang_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_descriptor_dependencies_versions_and_features_are_exact(),
        auto_complete_clang_installed_payload_bytes_are_pinned(),
        auto_complete_clang_defaults_custom_types_faces_and_source_alists_are_exact(),
        auto_complete_clang_complete_public_function_contract_and_definition_origins_are_exact(),
        auto_complete_clang_source_load_history_records_functions_faces_commands_and_provider(),
        auto_complete_clang_reload_preserves_defcustom_values_but_redefines_sources_and_functions(),
        auto_complete_clang_source_entries_resolve_to_callable_completion_contracts(),
    ]
}

pub(super) fn registry_auto_complete_clang_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_clang_generated_autoload_file_only_registers_its_feature()]
}
