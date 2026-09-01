use expect_test::expect;

use super::ParityBatchCase;

fn auto_async_byte_compile_exact_descriptor_activation_and_payload_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_exact_descriptor_activation_and_payload_contract_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-async-byte-compile
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
           (featurep
            'auto-async-byte-compile)
           (package-installed-p
            'auto-async-byte-compile
            '(20160916 454))
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
            '("auto-async-byte-compile-pkg.el"
              "auto-async-byte-compile.el"))))"##,
        expect![[
            r#"OK (auto-async-byte-compile "20160916.454" "Automatically byte-compile when saved." nil nil ((:maintainers ("rubikitch" . "rubikitch@ruby-lang.org")) (:authors ("rubikitch" . "rubikitch@ruby-lang.org")) (:keywords "lisp" "convenience") (:revdesc . "8681e74ddb84") (:commit . "8681e74ddb8481789c5dbb3cafabb327db4c4484") (:url . "http://www.emacswiki.org/cgi-bin/wiki/download/auto-async-byte-compile.el")) t t (("auto-async-byte-compile-pkg.el" 472 "184515bce346995b52b8cac7de852463e2b99edddb7bea4a3e695bf51b202141") ("auto-async-byte-compile.el" 8655 "5a0fdba039540cdd984d4910632ef7bd84cd954deae9b86aef5b61e8263d6914")))"#
        ]],
    )
}

fn auto_async_byte_compile_complete_prefixed_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_complete_prefixed_symbol_inventory_matches",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (or
                     (string-prefix-p
                      "auto-async-byte-compile"
                      name)
                     (string-prefix-p
                      "enable-auto-async-byte-compile"
                      name)
                     (string-prefix-p
                      "aabc/"
                      name))
                    (not
                     (string-prefix-p
                      "auto-async-byte-compile-test-"
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
                   (and
                    (commandp symbol)
                    t)
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
            r#"OK ((aabc/-send-bug-report t nil nil nil t "auto-async-byte-compile.el") (aabc/bug-report-salutation nil nil t nil nil "auto-async-byte-compile.el") (aabc/byte-compile-start-process-args t nil nil nil nil "auto-async-byte-compile.el") (aabc/display-function t nil nil nil nil "auto-async-byte-compile.el") (aabc/doit t nil nil nil nil "auto-async-byte-compile.el") (aabc/emacs-command t nil nil nil nil "auto-async-byte-compile.el") (aabc/maintainer-mail-address nil nil t nil nil "auto-async-byte-compile.el") (aabc/process-sentinel t nil nil nil nil "auto-async-byte-compile.el") (aabc/result-buffer nil nil t nil nil "auto-async-byte-compile.el") (aabc/status t nil nil nil nil "auto-async-byte-compile.el") (auto-async-byte-compile t nil nil nil t "auto-async-byte-compile.el") (auto-async-byte-compile-autoloads nil nil nil nil nil nil) (auto-async-byte-compile-display-function nil nil t t nil "auto-async-byte-compile.el") (auto-async-byte-compile-exclude-files-regexp nil nil t t nil "auto-async-byte-compile.el") (auto-async-byte-compile-hook nil nil t t nil "auto-async-byte-compile.el") (auto-async-byte-compile-init-file nil nil t t nil "auto-async-byte-compile.el") (auto-async-byte-compile-mode t nil t nil t "auto-async-byte-compile.el") (auto-async-byte-compile-mode-hook nil nil t t nil "auto-async-byte-compile.el") (auto-async-byte-compile-mode-map nil nil nil nil nil nil) (auto-async-byte-compile-mode-off-hook nil nil nil nil nil nil) (auto-async-byte-compile-mode-on-hook nil nil nil nil nil nil) (auto-async-byte-compile-suppress-warnings nil nil t t nil "auto-async-byte-compile.el") (enable-auto-async-byte-compile-mode t nil nil nil nil "auto-async-byte-compile.el"))"#
        ]],
    )
}

fn auto_async_byte_compile_all_callable_metadata_docs_and_sources_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_all_callable_metadata_docs_and_sources_are_exact",
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
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auto-async-byte-compile-mode
            enable-auto-async-byte-compile-mode
            auto-async-byte-compile
            aabc/doit
            aabc/process-sentinel
            aabc/display-function
            aabc/status
            aabc/emacs-command
            aabc/byte-compile-start-process-args
            aabc/-send-bug-report))"##,
        expect![[
            r#"OK ((auto-async-byte-compile-mode t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (&optional arg) "With no argument, toggles the auto-async-byte-compile-mode.\nWith a numeric argument, turn mode on iff ARG is positive.\n\nThis minor-mode performs `batch-byte-compile' automatically after saving elisp files." "auto-async-byte-compile.el") (enable-auto-async-byte-compile-mode nil nil nil nil "auto-async-byte-compile.el") (auto-async-byte-compile t (interactive nil) nil "Byte-compile this file asynchronously." "auto-async-byte-compile.el") (aabc/doit nil nil nil nil "auto-async-byte-compile.el") (aabc/process-sentinel nil nil (proc state) nil "auto-async-byte-compile.el") (aabc/display-function nil nil (process-name result-buffer status) nil "auto-async-byte-compile.el") (aabc/status nil nil (exitstatus buffer) nil "auto-async-byte-compile.el") (aabc/emacs-command nil nil nil nil "auto-async-byte-compile.el") (aabc/byte-compile-start-process-args nil nil (file) nil "auto-async-byte-compile.el") (aabc/-send-bug-report t (interactive nil) nil nil "auto-async-byte-compile.el"))"#
        ]],
    )
}

fn auto_async_byte_compile_custom_group_and_every_option_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_custom_group_and_every_option_contract_match",
        r##"(list
          (list
           (get
            'auto-async-byte-compile
            'custom-group)
           (get
            'auto-async-byte-compile
            'group-documentation)
           (get
            'auto-async-byte-compile
            'custom-prefix))
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
           '(auto-async-byte-compile-init-file
             auto-async-byte-compile-display-function
             auto-async-byte-compile-hook
             auto-async-byte-compile-exclude-files-regexp
             auto-async-byte-compile-suppress-warnings)))"##,
        expect![[
            r#"OK ((((auto-async-byte-compile-init-file custom-variable) (auto-async-byte-compile-display-function custom-variable) (auto-async-byte-compile-hook custom-variable) (auto-async-byte-compile-exclude-files-regexp custom-variable) (auto-async-byte-compile-suppress-warnings custom-variable)) "auto-async-byte-compile" nil) ((auto-async-byte-compile-init-file t "~/.emacs.d/initfuncs.el" "~/.emacs.d/initfuncs.el" ("~/.emacs.d/initfuncs.el") string nil "*Load this file when batch-byte-compile is running." t nil "auto-async-byte-compile.el") (auto-async-byte-compile-display-function t display-buffer display-buffer ('display-buffer) symbol nil "*Display function of auto byte-compile result." t nil "auto-async-byte-compile.el") (auto-async-byte-compile-hook t nil nil (nil) hook nil "*Hook after completing auto byte-compile.\nThe variable `exitstatus' is exit status of byte-compile process." t nil "auto-async-byte-compile.el") (auto-async-byte-compile-exclude-files-regexp t nil nil (nil) string nil "*Regexp of files to exclude auto byte-compile." t nil "auto-async-byte-compile.el") (auto-async-byte-compile-suppress-warnings t nil nil (nil) boolean nil "*If non-nil, do not display warnings." t nil "auto-async-byte-compile.el")))"#
        ]],
    )
    .fresh_process()
}

fn auto_async_byte_compile_mode_and_internal_variable_metadata_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_mode_and_internal_variable_metadata_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (and
              (boundp symbol)
              (symbol-value symbol))
             (documentation-property
              symbol
              'variable-documentation
              t)
             (get symbol 'permanent-local)
             (get symbol 'risky-local-variable)
             (when-let
                 ((source
                   (symbol-file symbol 'defvar)))
               (file-name-nondirectory
                source))))
          '(auto-async-byte-compile-mode
            auto-async-byte-compile-mode-map
            aabc/result-buffer
            aabc/maintainer-mail-address
            aabc/bug-report-salutation))"##,
        expect![[
            r#"OK ((auto-async-byte-compile-mode t nil "Non-nil if Auto-Async-Byte-Compile mode is enabled.\nUse the command `auto-async-byte-compile-mode' to change this\nvariable." nil nil "auto-async-byte-compile.el") (auto-async-byte-compile-mode-map nil nil nil nil nil nil) (aabc/result-buffer t " *auto-async-byte-compile*" nil nil nil "auto-async-byte-compile.el") (aabc/maintainer-mail-address t "rubikitch@ruby-lang.org" nil nil nil "auto-async-byte-compile.el") (aabc/bug-report-salutation t "Describe bug below, using a precise recipe.\n\nWhen I executed M-x ...\n\nHow to send a bug report:\n  1) Be sure to use the LATEST version of auto-async-byte-compile.el.\n  2) Enable debugger. M-x toggle-debug-on-error or (setq debug-on-error t)\n  3) Use Lisp version instead of compiled one: (load \"auto-async-byte-compile.el\")\n  4) If you got an error, please paste *Backtrace* buffer.\n  5) Type C-c C-c to send.\n# If you are a Japanese, please write in Japanese:-)" nil nil nil "auto-async-byte-compile.el"))"#
        ]],
    )
}

fn auto_async_byte_compile_source_load_history_records_complete_definition_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_async_byte_compile_source_load_history_records_complete_definition_order",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-async-byte-compile.el"
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
            'auto-async-byte-compile)))"##,
        expect![[
            r#"OK ("auto-async-byte-compile.el" ((require . cl) (defun . auto-async-byte-compile-mode) (defun . enable-auto-async-byte-compile-mode) (defun . auto-async-byte-compile) (defun . aabc/doit) (defun . aabc/process-sentinel) (defun . aabc/display-function) (defun . aabc/status) (defun . aabc/emacs-command) (defun . aabc/byte-compile-start-process-args) (defun . aabc/-send-bug-report) (provide . auto-async-byte-compile)) t)"#
        ]],
    )
}

fn auto_async_byte_compile_generated_autoload_has_no_callable_autoloads() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_generated_autoload_has_no_callable_autoloads",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-async-byte-compile-autoloads.el"
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
            'auto-async-byte-compile-autoloads)
           (featurep
            'auto-async-byte-compile)
           events
           (fboundp
            'auto-async-byte-compile)
           (fboundp
            'auto-async-byte-compile-mode)
           (and
            (boundp
             'definition-prefixes)
            (gethash
             "auto-async-byte-compile"
             definition-prefixes))))"##,
        expect![[
            r#"OK (t nil ((provide . auto-async-byte-compile-autoloads)) nil nil ("auto-async-byte-compile" "auto-async-byte-compile"))"#
        ]],
    )
}

pub(super) fn registry_auto_async_byte_compile_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_async_byte_compile_exact_descriptor_activation_and_payload_contract_match(),
        auto_async_byte_compile_complete_prefixed_symbol_inventory_matches(),
        auto_async_byte_compile_all_callable_metadata_docs_and_sources_are_exact(),
        auto_async_byte_compile_custom_group_and_every_option_contract_match(),
        auto_async_byte_compile_mode_and_internal_variable_metadata_match(),
        auto_async_byte_compile_source_load_history_records_complete_definition_order(),
    ]
}

pub(super) fn registry_auto_async_byte_compile_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_async_byte_compile_generated_autoload_has_no_callable_autoloads()]
}
