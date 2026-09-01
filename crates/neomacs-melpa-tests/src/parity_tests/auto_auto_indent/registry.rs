use expect_test::expect;

use super::ParityBatchCase;

fn auto_auto_indent_exact_package_descriptor_origin_and_dependencies_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_exact_package_descriptor_origin_and_dependencies_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-auto-indent
                                   package-alist)))
                                (extras
                                 (package-desc-extras descriptor)))
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (alist-get :commit extras)
           (alist-get :revdesc extras)
           (alist-get :url extras)
           (file-name-nondirectory
            (directory-file-name
             (package-desc-dir descriptor)))))"##,
        expect![[
            r#"OK (auto-auto-indent "20131106.1903" "Indents code as you type." ((es-lib (0 1)) (cl-lib (1 0))) "0139378577f936d34b20276af6f022fb457af490" "0139378577f9" "https://github.com/sabof/auto-auto-indent" "auto-auto-indent-20131106.1903")"#
        ]],
    )
}

fn auto_auto_indent_installed_payload_inventory_and_exact_archive_hashes_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_installed_payload_inventory_and_exact_archive_hashes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-auto-indent
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
          (mapcar
           (lambda (name)
             (let ((file
                    (expand-file-name
                     name
                     directory)))
               (if
                   (member
                    name
                    '("auto-auto-indent-pkg.el"
                      "auto-auto-indent.el"))
                   (list
                    name
                    :archive
                    (file-attribute-size
                     (file-attributes file))
                    (with-temp-buffer
                      (set-buffer-multibyte nil)
                      (insert-file-contents-literally
                       file)
                      (secure-hash
                       'sha256
                       (current-buffer))))
                 (list name :generated t))))
           (sort
            (directory-files
             directory
             nil
             "\\`[^.]")
            #'string<)))"##,
        expect![[
            r#"OK (("auto-auto-indent-autoloads.el" :generated t) ("auto-auto-indent-pkg.el" :archive 303 "d7432b94b26217127b8e1ff04031da73914ddbb5d356ab080fce2dc68daa77e3") ("auto-auto-indent.el" :archive 14282 "504be02f5545a58d4a0cb5b2d7433aa0a63c8828779b99d6883e22378e30fcb1") ("auto-auto-indent.elc" :generated t))"#
        ]],
    )
}

fn auto_auto_indent_complete_prefixed_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_complete_prefixed_symbol_inventory_matches",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (or
                     (string-prefix-p "aai-" name)
                     (string-prefix-p
                      "auto-auto-indent"
                      name))
                    (not
                     (string-prefix-p
                      "auto-auto-indent-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (commandp symbol)
                    t)
                   (and
                    (macrop symbol)
                    t)
                   (local-variable-if-set-p symbol))
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
            "OK ((aai--change-flag nil t nil nil t) (aai--indent-region t nil nil nil nil) (aai--init t nil nil nil nil) (aai--major-mode-setup t nil nil nil nil) (aai--minor-mode-setup t nil nil nil nil) (aai--timer nil t nil nil nil) (aai-after-change-indentation nil t nil nil nil) (aai-backspace t nil t nil nil) (aai-before-change-function t nil nil nil nil) (aai-correct-position-this t nil nil nil nil) (aai-debug nil t nil nil nil) (aai-delete-char t nil t nil nil) (aai-dont-indent-commands nil t nil nil nil) (aai-indent-defun t nil nil nil nil) (aai-indent-forward t nil nil nil nil) (aai-indent-function nil t nil nil nil) (aai-indent-limit nil t nil nil nil) (aai-indent-line-maybe t nil nil nil nil) (aai-indentable-line-p-function nil t nil nil nil) (aai-indented-yank t nil t nil nil) (aai-indented-yank-limit nil t nil nil nil) (aai-mode t t t nil t) (aai-mode-hook nil t nil nil nil) (aai-mouse-yank t nil t nil nil) (aai-mouse-yank-dont-indent t nil t nil nil) (aai-newline-and-indent t nil t nil nil) (aai-on-timer t nil nil nil nil) (aai-open-line t nil t nil nil) (aai-post-command-hook t nil nil nil nil) (aai-timer-delay nil t nil nil nil) (auto-auto-indent nil nil nil nil nil) (auto-auto-indent-autoloads nil nil nil nil nil) (auto-auto-indent-mode t t t nil t) (auto-auto-indent-mode-hook nil t nil nil nil) (auto-auto-indent-mode-map nil t nil nil nil) (auto-auto-indent-mode-off-hook nil nil nil nil nil) (auto-auto-indent-mode-on-hook nil nil nil nil nil))"
        ],
    )
}

fn auto_auto_indent_complete_callable_arglists_interactivity_docs_and_origins_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_complete_callable_arglists_interactivity_docs_and_origins_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (help-function-arglist symbol t)
             (and
              (interactive-form symbol)
              t)
             (commandp symbol)
             (documentation symbol t)
             (when-let
                 ((source
                   (symbol-file symbol 'defun)))
               (file-name-nondirectory source))))
          '(aai-indent-line-maybe
            aai-indent-forward
            aai--indent-region
            aai-indent-defun
            aai-indented-yank
            aai-mouse-yank
            aai-mouse-yank-dont-indent
            aai-delete-char
            aai-backspace
            aai-open-line
            aai-newline-and-indent
            aai-correct-position-this
            aai-before-change-function
            aai-on-timer
            aai-post-command-hook
            aai--major-mode-setup
            aai--minor-mode-setup
            aai--init
            auto-auto-indent-mode
            aai-mode))"##,
        expect![[
            r#"OK ((aai-indent-line-maybe nil nil nil "(indent-according-to-mode) when `aai-indentable-line-p-function' returns non-nil.\nAll indentation happends through this function." "auto-auto-indent.el") (aai-indent-forward nil nil nil "Indent current line, and (1- `aai-indent-limit') lines afterwards." "auto-auto-indent.el") (aai--indent-region (start end) nil nil "Indent region lines where `aai-indentable-line-p-function' returns non-nil." "auto-auto-indent.el") (aai-indent-defun nil nil nil "Indent current defun, if it is smaller than `aai-indent-limit'.\nOtherwise call `aai-indent-forward'." "auto-auto-indent.el") (aai-indented-yank (&optional dont-indent) t t nil "auto-auto-indent.el") (aai-mouse-yank (event &optional dont-indent) t t nil "auto-auto-indent.el") (aai-mouse-yank-dont-indent (event) t t nil "auto-auto-indent.el") (aai-delete-char (&optional from-backspace) t t "Like `delete-char', but deletes indentation, if point is at it, or before it." "auto-auto-indent.el") (aai-backspace nil t t "Like `backward-delete-char', but removes the resulting gap when point is at EOL." "auto-auto-indent.el") (aai-open-line nil t t "Open line, and indent the following." "auto-auto-indent.el") (aai-newline-and-indent nil t t nil "auto-auto-indent.el") (aai-correct-position-this nil nil nil "Go back to indentation if point is before indentation." "auto-auto-indent.el") (aai-before-change-function (&rest ignore) nil nil "Change tracking." "auto-auto-indent.el") (aai-on-timer (marker) nil nil nil "auto-auto-indent.el") (aai-post-command-hook nil nil nil "Correct the cursor, and possibly indent." "auto-auto-indent.el") (aai--major-mode-setup nil nil nil "Optimizations for speicfic modes" "auto-auto-indent.el") (aai--minor-mode-setup nil nil nil "Change interacting minor modes." "auto-auto-indent.el") (aai--init nil nil nil nil "auto-auto-indent.el") (auto-auto-indent-mode #1=(&optional arg) t t "Automatic automatic indentation.\n\nWorks pretty well for lisp out of the box.\nOther modes might need some tweaking to set up:\nIf you trust the mode's automatic indentation completely, you can add to it's\ninit hook:\n\n(set (make-local-variable 'aai-indent-function)\n     'aai-indent-defun)\n\nor\n\n(set (make-local-variable 'aai-indent-function)\n     'aai-indent-forward)\n\ndepending on whether the language has small and clearly\nidentifiable functions, that `beginning-of-defun' and\n`end-of-defun' can find.\n\nIf on the other hand you don't trust the mode at all, but like\nthe cursor correction and delete-char behaviour,\n\nyou can add\n\n(set (make-local-variable\n      'aai-after-change-indentation) nil)\n\nif the mode indents well in all but a few cases, you can change the\n`aai-indentable-line-p-function'. This is what I have in my php mode setup:\n\n(set (make-local-variable\n      'aai-indentable-line-p-function)\n     (lambda ()\n       (not (or (es-line-matches-p \"EOD\")\n                (es-line-matches-p \"EOT\")))))\n\nThis is a minor mode.  If called interactively, toggle the\n`Auto-Auto-Indent mode' mode.  If the prefix argument is positive,\nenable the mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `auto-auto-indent-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled." "auto-auto-indent.el") (aai-mode #1# t t "Automatic automatic indentation.\n\nWorks pretty well for lisp out of the box.\nOther modes might need some tweaking to set up:\nIf you trust the mode's automatic indentation completely, you can add to it's\ninit hook:\n\n(set (make-local-variable 'aai-indent-function)\n     'aai-indent-defun)\n\nor\n\n(set (make-local-variable 'aai-indent-function)\n     'aai-indent-forward)\n\ndepending on whether the language has small and clearly\nidentifiable functions, that `beginning-of-defun' and\n`end-of-defun' can find.\n\nIf on the other hand you don't trust the mode at all, but like\nthe cursor correction and delete-char behaviour,\n\nyou can add\n\n(set (make-local-variable\n      'aai-after-change-indentation) nil)\n\nif the mode indents well in all but a few cases, you can change the\n`aai-indentable-line-p-function'. This is what I have in my php mode setup:\n\n(set (make-local-variable\n      'aai-indentable-line-p-function)\n     (lambda ()\n       (not (or (es-line-matches-p \"EOD\")\n                (es-line-matches-p \"EOT\")))))\n\nThis is a minor mode.  If called interactively, toggle the\n`Auto-Auto-Indent mode' mode.  If the prefix argument is positive,\nenable the mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `auto-auto-indent-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled." "auto-auto-indent.el"))"#
        ]],
    )
    .fresh_process()
}

fn auto_auto_indent_all_variable_defaults_aliases_and_locality_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_all_variable_defaults_aliases_and_locality_match",
        r##"(mapcar
          (lambda (symbol)
            (let ((value
                   (and
                    (boundp symbol)
                    (symbol-value symbol))))
              (list
               symbol
               (cond
                ((keymapp value)
                 :keymap)
                ((functionp value)
                 :function)
                (t
                 value))
               (local-variable-if-set-p symbol)
               (documentation-property
                symbol
                'variable-documentation
                t)
               (indirect-variable symbol))))
          '(aai-indent-function
            aai-indentable-line-p-function
            aai-after-change-indentation
            aai-indent-limit
            aai-indented-yank-limit
            aai-dont-indent-commands
            aai-mode-hook
            aai--timer
            aai-timer-delay
            aai-debug
            aai--change-flag
            auto-auto-indent-mode
            auto-auto-indent-mode-hook
            auto-auto-indent-mode-map
            aai-mode))"##,
        expect![[
            r#"OK ((aai-indent-function :function nil "Indentation function to use call for automatic indentation." aai-indent-function) (aai-indentable-line-p-function :function nil "For mode-specifc cusomizations." aai-indentable-line-p-function) (aai-after-change-indentation t nil "Whether to reindent after every change.\nUseful when you want to keep the keymap and cursor repositioning." aai-after-change-indentation) (aai-indent-limit 30 nil "Maximum number of lines for after-change indentation." aai-indent-limit) (aai-indented-yank-limit 4000 nil "Maximum number of character to indent for `aai-indented-yank'" aai-indented-yank-limit) (aai-dont-indent-commands (delete-horizontal-space quoted-insert backward-paragraph kill-region self-insert-command) nil "Commands after which not to indent." aai-dont-indent-commands) (aai-mode-hook nil nil nil aai-mode-hook) (aai--timer nil nil nil aai--timer) (aai-timer-delay 0.5 nil "Indent after this ammout of second, following a sequence of self-insert commands.\nDon't indent when nil" aai-timer-delay) (aai-debug nil nil nil aai-debug) (aai--change-flag nil t nil aai--change-flag) (auto-auto-indent-mode nil t "Non-nil if Auto-Auto-Indent mode is enabled.\nUse the command `auto-auto-indent-mode' to change this variable." auto-auto-indent-mode) (auto-auto-indent-mode-hook nil nil "Hook run after entering or leaving `auto-auto-indent-mode'.\nNo problems result if this variable is not bound.\n`add-hook' automatically binds it.  (This is true for all hook variables.)" auto-auto-indent-mode-hook) (auto-auto-indent-mode-map :keymap nil "Keymap for `auto-auto-indent-mode'." auto-auto-indent-mode-map) (aai-mode nil t "Non-nil if Auto-Auto-Indent mode is enabled.\nUse the command `auto-auto-indent-mode' to change this variable." auto-auto-indent-mode))"#
        ]],
    )
    .fresh_process()
}

fn auto_auto_indent_exact_es_lib_dependency_is_active_and_loaded() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_exact_es_lib_dependency_is_active_and_loaded",
        r##"(mapcar
          (lambda (package)
            (let ((descriptor
                   (cadr
                    (assq package package-alist))))
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
          '(auto-auto-indent es-lib))"##,
        expect![[
            r#"OK ((auto-auto-indent "20131106.1903" "auto-auto-indent-20131106.1903" t "auto-auto-indent.el") (es-lib "20141111.1830" "es-lib-20141111.1830" t "es-lib.el"))"#
        ]],
    )
}

fn auto_auto_indent_source_load_history_records_requires_definitions_aliases_and_feature()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_source_load_history_records_requires_definitions_aliases_and_feature",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-auto-indent.el"
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
            'auto-auto-indent)))"##,
        expect![[
            r#"OK ("auto-auto-indent.el" ((require . cl-lib) (require . es-lib) (defun . aai-indent-line-maybe) (defun . aai-indent-forward) (defun . aai--indent-region) (defun . aai-indent-defun) (defun . aai-indented-yank) (defun . aai-mouse-yank) (defun . aai-mouse-yank-dont-indent) (defun . aai-delete-char) (defun . aai-backspace) (defun . aai-open-line) (defun . aai-newline-and-indent) (defun . aai-correct-position-this) (defun . aai-before-change-function) (defun . aai-on-timer) (defun . aai-post-command-hook) (defun . aai--major-mode-setup) (defun . aai--minor-mode-setup) (defun . aai--init) (defun . auto-auto-indent-mode) (defun . aai-mode) (provide . auto-auto-indent)) t)"#
        ]],
    )
}

fn auto_auto_indent_generated_autoload_exposes_only_mode_without_loading_dependencies()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_generated_autoload_exposes_only_mode_without_loading_dependencies",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-auto-indent-autoloads.el"
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
                                  'auto-auto-indent-mode)))
          (list
           (featurep
            'auto-auto-indent-autoloads)
           (featurep
            'auto-auto-indent)
           (featurep 'es-lib)
           events
           (autoloadp definition)
           (nth 1 definition)
           (nth 4 definition)
           (commandp
            'auto-auto-indent-mode)
           (fboundp 'aai-mode)
           (boundp 'aai-mode)))"##,
        expect![[
            r#"OK (t nil nil ((defun . auto-auto-indent-mode) (provide . auto-auto-indent-autoloads)) t "auto-auto-indent" nil t nil nil)"#
        ]],
    )
}

fn auto_auto_indent_generated_autoload_performs_real_mode_activation_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_generated_autoload_performs_real_mode_activation_workflow",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (auto-auto-indent-mode 1)
          (list
           (featurep
            'auto-auto-indent)
           (featurep 'es-lib)
           auto-auto-indent-mode
           aai-mode
           aai-indent-function
           (memq
            'aai-post-command-hook
            post-command-hook)
           (memq
            'aai-before-change-function
            before-change-functions)
           (autoloadp
            (symbol-function
             'auto-auto-indent-mode))))"##,
        expect![
            "OK (t t t t aai-indent-defun (aai-post-command-hook) (aai-before-change-function) nil)"
        ],
    )
}

pub(super) fn registry_auto_auto_indent_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_exact_package_descriptor_origin_and_dependencies_match(),
        auto_auto_indent_installed_payload_inventory_and_exact_archive_hashes_match(),
        auto_auto_indent_complete_prefixed_symbol_inventory_matches(),
        auto_auto_indent_complete_callable_arglists_interactivity_docs_and_origins_match(),
        auto_auto_indent_all_variable_defaults_aliases_and_locality_match(),
        auto_auto_indent_exact_es_lib_dependency_is_active_and_loaded(),
        auto_auto_indent_source_load_history_records_requires_definitions_aliases_and_feature(),
    ]
}

pub(super) fn registry_auto_auto_indent_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_generated_autoload_exposes_only_mode_without_loading_dependencies(),
        auto_auto_indent_generated_autoload_performs_real_mode_activation_workflow(),
    ]
}
