use expect_test::expect;

use super::ParityBatchCase;

fn auto_read_only_descriptor_and_payload_pin_exact_melpa_archive() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_descriptor_and_payload_pin_exact_melpa_archive",
        r##"(let* ((descriptor
                (cadr
                 (assq 'auto-read-only package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name name directory))
                 '("auto-read-only-pkg.el"
                   "auto-read-only.el"))))
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
            r#"OK ((auto-read-only "20260521.1659" "Automatically make the buffer to read-only." ((emacs (25 1)) (cl-lib (0 5))) ((:maintainers ("USAMI Kenta" . "tadsan@zonu.me")) (:authors ("USAMI Kenta" . "tadsan@zonu.me")) (:keywords "files" "convenience") (:revdesc . "206d4559762f") (:commit . "206d4559762fe6ef9e91de8f9dc43e1e41c0f42c") (:url . "https://github.com/zonuexe/auto-read-only.el"))) (("auto-read-only-pkg.el" 462 "153507ee5a39eb0dfbd99c1fb64704a5b614f657a60558020022f2fd86b73d7e") ("auto-read-only.el" 3534 "0d20fe367b4437fc3a5c1dd52726cdcbc92659ef0cff969449f9a74148c4a9d3")))"#
        ]],
    )
}

fn auto_read_only_complete_function_and_variable_inventory_has_exact_origins() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_complete_function_and_variable_inventory_has_exact_origins",
        r##"(list
         (featurep 'auto-read-only)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auto-read-only--hook-find-file
            auto-read-only-mode
            auto-read-only))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defvar))))
          '(auto-read-only-file-regexps
            auto-read-only-function
            auto-read-only-mode-lighter
            auto-read-only-mode
            auto-read-only-mode-hook)))"##,
        expect![[
            r#"OK (t ((auto-read-only--hook-find-file t "auto-read-only.el") (auto-read-only-mode t "auto-read-only.el") (auto-read-only t "auto-read-only.el")) ((auto-read-only-file-regexps t "auto-read-only.el") (auto-read-only-function t "auto-read-only.el") (auto-read-only-mode-lighter t "auto-read-only.el") (auto-read-only-mode t "auto-read-only.el") (auto-read-only-mode-hook t "auto-read-only.el")))"#
        ]],
    )
}

fn auto_read_only_entry_points_pin_interactive_arglist_and_documentation_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_entry_points_pin_interactive_arglist_and_documentation_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)
            (documentation symbol t)))
         '(auto-read-only--hook-find-file
           auto-read-only-mode
           auto-read-only))"##,
        expect![[
            r#"OK ((auto-read-only--hook-find-file nil nil nil "To apply read-only if detect called `find-file' interactivly.") (auto-read-only-mode t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (&optional arg) "Minor mode for appply auto-read-only.\n\nThis is a global minor mode.  If called interactively, toggle the\n`Auto-Read-Only mode' mode.  If the prefix argument is positive, enable\nthe mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate `(default-value \\='auto-read-only-mode)'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled.") (auto-read-only nil nil nil "Apply read-only mode."))"#
        ]],
    )
}

fn auto_read_only_custom_group_options_and_lighter_have_exact_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_custom_group_options_and_lighter_have_exact_metadata",
        r##"(list
         auto-read-only-mode-lighter
         (get 'auto-read-only 'group-documentation)
         (get 'auto-read-only 'custom-prefix)
         (get 'auto-read-only 'custom-group)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (get symbol 'standard-value)
             (get symbol 'custom-type)
             (get symbol 'custom-group)
             (get symbol 'variable-documentation)))
          '(auto-read-only-file-regexps
            auto-read-only-function)))"##,
        expect![[
            r#"OK (" AutoRO" "Automatically make the buffer read-only." "auto-read-only-" ((auto-read-only-file-regexps custom-variable) (auto-read-only-function custom-variable) (auto-read-only-mode custom-variable)) ((auto-read-only-file-regexps ((eval-when-compile (list (concat (regexp-opt '(".elc" ".pyc")) "\\'") (rx "/share/" (+ any) "/site-lisp/") (rx (literal (expand-file-name user-emacs-directory)) (or "el-get" "elpa") "/") (rx "/" (or ".bundle" ".cask") "/")))) (repeat regexp) nil "List of buffer filename prefix regexp patterns to apply read-only.") (auto-read-only-function (nil) (choice (const :tag "No specific (default to use `view-mode')" nil) (function :tag "Arbitrary function/minor-mode like read-only.")) nil "Fuction for make the buffer read-only.")))"#
        ]],
    )
}

fn auto_read_only_source_load_history_records_exact_owned_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_source_load_history_records_exact_owned_surface",
        r##"(let* ((file
                 (locate-library "auto-read-only"))
                (history
                 (cdr
                  (assoc file load-history))))
         (seq-filter
          (lambda (event)
            (and
             (consp event)
             (or
              (memq
               (car event)
               '(provide defun defvar))
              (and
               (eq
                (car event)
                'define-symbol-props)
               (eq
                (cadr event)
                'auto-read-only-mode)))))
          history))"##,
        expect![
            "OK ((defun . auto-read-only--hook-find-file) (defun . auto-read-only-mode) (defun . auto-read-only) (provide . auto-read-only))"
        ],
    )
}

fn auto_read_only_reload_preserves_custom_values_mode_state_and_hook_registration()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_reload_preserves_custom_values_mode_state_and_hook_registration",
        r##"(let ((source
                (locate-library "auto-read-only"))
               (auto-read-only-file-regexps
                '("/vendor/" "\\.lock\\'"))
               (auto-read-only-function
                #'read-only-mode)
               (auto-read-only-mode-lighter
                " Protected")
               (find-file-hook nil))
         (auto-read-only-mode 1)
         (load source nil t t)
         (load source nil t t)
         (prog1
             (list
              auto-read-only-file-regexps
              (eq
               auto-read-only-function
               #'read-only-mode)
              auto-read-only-mode-lighter
              auto-read-only-mode
              (auto-read-only-test-hook-count
               'auto-read-only--hook-find-file
               'find-file-hook)
              (featurep 'auto-read-only))
           (auto-read-only-mode -1)))"##,
        expect![[r#"OK (("/vendor/" "\\.lock\\'") t " Protected" t 1 t)"#]],
    )
}

fn auto_read_only_generated_autoloads_register_both_commands_without_loading_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_generated_autoloads_register_both_commands_without_loading_source",
        r##"(let* ((file
                 (locate-library
                  "auto-read-only-autoloads"))
                (history
                 (cdr
                  (assoc file load-history))))
         (list
          (featurep
           'auto-read-only-autoloads)
          (featurep 'auto-read-only)
          (seq-filter
           (lambda (event)
             (memq
              (car-safe event)
              '(defun provide)))
           history)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (fboundp symbol)
              (autoloadp
               (symbol-function symbol))
              (commandp symbol)
              (help-function-arglist symbol t)))
           '(auto-read-only-mode
             auto-read-only))))"##,
        expect![[
            r#"OK (t nil ((defun . auto-read-only-mode) (defun . auto-read-only) (provide . auto-read-only-autoloads)) ((auto-read-only-mode t t t "[Arg list not available until function definition is loaded.]") (auto-read-only t t nil "[Arg list not available until function definition is loaded.]")))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn registry_auto_read_only_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_read_only_descriptor_and_payload_pin_exact_melpa_archive(),
        auto_read_only_complete_function_and_variable_inventory_has_exact_origins(),
        auto_read_only_entry_points_pin_interactive_arglist_and_documentation_contracts(),
        auto_read_only_custom_group_options_and_lighter_have_exact_metadata(),
        auto_read_only_source_load_history_records_exact_owned_surface(),
        auto_read_only_reload_preserves_custom_values_mode_state_and_hook_registration(),
    ]
}

pub(super) fn registry_auto_read_only_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_read_only_generated_autoloads_register_both_commands_without_loading_source()]
}
