use expect_test::expect;

use super::ParityBatchCase;

fn atl_markup_descriptor_and_archive_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_descriptor_and_archive_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'atl-markup
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name
                    name
                    directory))
                 '("atl-markup-pkg.el"
                   "atl-markup.el"))))
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
            r#"OK ((atl-markup "20240101.933" "Automatically truncate lines for markup languages." ((emacs (24 3))) ((:maintainers ("Jen-Chieh" . "jcs090218@gmail.com")) (:authors ("Jen-Chieh" . "jcs090218@gmail.com")) (:keywords "convenience" "automatic" "truncate" "visual" "lines") (:revdesc . "b616343ffe17") (:commit . "b616343ffe17060d521b214b8e90f5da1e880934") (:url . "https://github.com/jcs-elpa/atl-markup"))) (("atl-markup-pkg.el" 476 "bd2679cb82a061f1a7a3f6aadd39357f78176ecf6941896fdb5c5d1a01ffdd58") ("atl-markup.el" 4268 "8d94c8e0fb4830d4aee6804758206f2f1d13cea5311e73e531a5531d23daf313")))"#
        ]],
    )
}

fn atl_markup_complete_prefixed_symbol_inventory_records_every_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_complete_prefixed_symbol_inventory_records_every_surface",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "atl-markup"
                     name)
                    (not
                     (string-prefix-p
                      "atl-markup-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (custom-variable-p symbol)
                    t)
                   (get symbol 'custom-group)
                   (when
                       (fboundp symbol)
                     (copy-tree
                      (help-function-arglist
                       symbol
                       t))))
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
            "OK ((atl-markup nil nil nil ((atl-markup-ignore-regex custom-variable) (atl-markup-delay custom-variable)) nil) (atl-markup--comment-block-p t nil nil nil nil) (atl-markup--disable t nil nil nil nil) (atl-markup--enable t nil nil nil nil) (atl-markup--inside-tag-p t nil nil nil nil) (atl-markup--mute-apply t nil nil nil (fnc &rest args)) (atl-markup--post-command-hook t nil nil nil nil) (atl-markup--timer nil t nil nil nil) (atl-markup--web-truncate-lines-by-face t nil nil nil nil) (atl-markup-autoloads nil nil nil nil nil) (atl-markup-delay nil t t nil nil) (atl-markup-ignore-regex nil t t nil nil) (atl-markup-mode t t nil nil (&optional arg)) (atl-markup-mode-hook nil t t nil nil) (atl-markup-mode-map nil nil nil nil nil) (atl-markup-mode-off-hook nil nil nil nil nil) (atl-markup-mode-on-hook nil nil nil nil nil))"
        ],
    )
}

fn atl_markup_all_functions_have_exact_call_interactive_and_documentation_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_all_functions_have_exact_call_interactive_and_documentation_contracts",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
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
          '(atl-markup--comment-block-p
            atl-markup--mute-apply
            atl-markup--inside-tag-p
            atl-markup--web-truncate-lines-by-face
            atl-markup--post-command-hook
            atl-markup--enable
            atl-markup--disable
            atl-markup-mode))"##,
        expect![[
            r#"OK ((atl-markup--comment-block-p t nil nil nil "Return non-nil if current cursor is on comment." "atl-markup.el") (atl-markup--mute-apply t nil nil (fnc &rest args) "Execute FNC with ARGS without message." "atl-markup.el") (atl-markup--inside-tag-p t nil nil nil "Check if current point inside the tag." "atl-markup.el") (atl-markup--web-truncate-lines-by-face t nil nil nil "Enable/Disable the truncate lines mode depends on the face cursor currently on." "atl-markup.el") (atl-markup--post-command-hook t nil nil nil "Post command hook to do auto truncate lines in current buffer." "atl-markup.el") (atl-markup--enable t nil nil nil "Enable 'atl-markup-mode'." "atl-markup.el") (atl-markup--disable t nil nil nil "Disable 'atl-markup-mode'." "atl-markup.el") (atl-markup-mode t t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (&optional arg) "Minor mode 'atl-markup-mode'.\n\nThis is a minor mode.  If called interactively, toggle the `Atl-Markup\nmode' mode.  If the prefix argument is positive, enable the mode, and if\nit is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `atl-markup-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled." "atl-markup.el"))"#
        ]],
    )
}

fn atl_markup_customization_group_defaults_and_minor_mode_metadata_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_customization_group_defaults_and_minor_mode_metadata_are_exact",
        r##"(list
          (get 'atl-markup 'custom-group)
          (documentation-property
           'atl-markup
           'group-documentation
           t)
          (get 'atl-markup 'custom-prefix)
          (get 'atl-markup 'custom-links)
          (mapcar
           (lambda (symbol)
             (let* ((standard-value
                     (get symbol 'standard-value))
                    (one-form
                     (=
                      (length standard-value)
                      1))
                    (default-value
                     (and
                      one-form
                      (eval
                       (car standard-value)
                       t))))
               (list
                symbol
                (and
                 (custom-variable-p symbol)
                 t)
                (symbol-value symbol)
                one-form
                default-value
                (equal
                 (symbol-value symbol)
                 default-value)
                (get symbol 'custom-type)
                (get symbol 'custom-group)
                (documentation-property
                 symbol
                 'variable-documentation
                 t))))
           '(atl-markup-ignore-regex
             atl-markup-delay
             atl-markup-mode-hook))
          (list
           (default-value
            'atl-markup-mode)
           (local-variable-if-set-p
            'atl-markup-mode)
           (assq
            'atl-markup-mode
            minor-mode-alist)
           (assq
            'atl-markup-mode
            minor-mode-map-alist)
           (get 'atl-markup-mode 'custom-type)
           (get 'atl-markup-mode 'custom-group)))"##,
        expect![[
            r#"OK (((atl-markup-ignore-regex custom-variable) (atl-markup-delay custom-variable)) "Automatically truncate lines for markup languages." "atl-markup-" ((url-link :tag "Repository" "https://github.com/jcs-elpa/atl-markup")) ((atl-markup-ignore-regex t "[ \11\15\n]" t "[ \11\15\n]" t string nil "Regular expression string that will ignore auto truncate lines' action.") (atl-markup-delay t 0.1 t 0.1 t float nil "Time delay to active auto truncate lines for markup languages.") (atl-markup-mode-hook t nil t nil t hook nil "Hook run after entering or leaving `atl-markup-mode'.\nNo problems result if this variable is not bound.\n`add-hook' automatically binds it.  (This is true for all hook variables.)")) (nil t (atl-markup-mode " ATL-MrkUp") nil nil nil))"#
        ]],
    )
}

fn atl_markup_internal_timer_variable_contract_is_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_internal_timer_variable_contract_is_exact",
        r##"(let ((symbol
                'atl-markup--timer))
          (list
           symbol
           (boundp symbol)
           (symbol-value symbol)
           (default-boundp symbol)
           (default-value symbol)
           (special-variable-p symbol)
           (and
            (custom-variable-p symbol)
            t)
           (documentation-property
            symbol
            'variable-documentation
            t)
           (when-let
               ((source
                 (symbol-file symbol 'defvar)))
             (file-name-nondirectory
              source))
           (copy-tree
            (get symbol 'standard-value))
           (local-variable-if-set-p symbol)
           (local-variable-p symbol)))"##,
        expect![[
            r#"OK (atl-markup--timer t nil t nil t nil "Timer to active auto truncate lines." "atl-markup.el" nil nil nil)"#
        ]],
    )
}

fn atl_markup_installed_source_byte_compiles_loads_and_drives_practical_navigation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_installed_source_byte_compiles_loads_and_drives_practical_navigation",
        r##"(progn
          (require 'bytecomp)
          (let* ((root
                  (atl-markup-test-root))
                 (source
                  (expand-file-name
                   "byte-compile/atl-markup.el"
                   root))
                 (compiled
                  (byte-compile-dest-file
                   source)))
            (make-directory
             (file-name-directory source)
             t)
            (copy-file
             (getenv "NEOMACS_PACKAGE_SOURCE")
             source
             t)
            (let ((byte-compile-error-on-warn
                   nil)
                  (byte-compile-warnings
                   nil))
              (byte-compile-file source))
            (let ((load-outcome
                   (atl-markup-test-error-data
                    (lambda ()
                      (load compiled nil nil t)))))
              (list
               (file-exists-p compiled)
               load-outcome
               (featurep 'atl-markup)
               (file-name-nondirectory
                (symbol-file
                 'atl-markup-mode
                 'defun))
               (with-temp-buffer
                 (insert
                  "<article id=\"entry\">body</article>")
                 (setq-local
                  post-command-hook nil)
                 (setq-local
                  truncate-lines nil)
                 (atl-markup-mode 1)
                 (goto-char
                  (point-min))
                 (search-forward "id")
                 (atl-markup--web-truncate-lines-by-face)
                 (let ((inside
                        truncate-lines))
                   (search-forward "body")
                   (atl-markup--web-truncate-lines-by-face)
                   (list
                    inside
                    truncate-lines
                    atl-markup-mode
                    (and
                     (memq
                      'atl-markup--post-command-hook
                      post-command-hook)
                     t))))
               (secure-hash
                'sha256
                (atl-markup-test-read-file
                 source))))))"##,
        expect![[
            r#"OK (t (:ok t) t "atl-markup.elc" (t nil t t) "8d94c8e0fb4830d4aee6804758206f2f1d13cea5311e73e531a5531d23daf313")"#
        ]],
    )
}

fn atl_markup_generated_autoload_preserves_feature_history_prefix_and_command_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_generated_autoload_preserves_feature_history_prefix_and_command_contract",
        r##"(let* ((history
                (seq-find
                 (lambda (entry)
                   (and
                    (stringp
                     (car entry))
                    (string-suffix-p
                     "atl-markup-autoloads.el"
                     (car entry))))
                 load-history))
               (history-contract
                (mapcar
                 (lambda (event)
                   (list
                    (car event)
                    (cdr event)))
                 (seq-filter
                  (lambda (event)
                    (memq
                     (car-safe event)
                     '(defun provide)))
                  (cdr history))))
               (definition
                (and
                 (fboundp 'atl-markup-mode)
                 (symbol-function
                  'atl-markup-mode))))
          (list
           (featurep 'atl-markup-autoloads)
           (featurep 'atl-markup)
           history-contract
           (and
            (boundp 'definition-prefixes)
            (sort
             (delete-dups
              (copy-sequence
               (gethash
                "atl-markup-"
                definition-prefixes)))
             #'string<))
           (autoloadp definition)
           (and
            (autoloadp definition)
            (nth 1 definition))
           (commandp 'atl-markup-mode)
           (help-function-arglist
            'atl-markup-mode
            t)
           (mapcar
            (lambda (symbol)
              (list
               symbol
               (fboundp symbol)
               (boundp symbol)))
            '(atl-markup--inside-tag-p
              atl-markup--post-command-hook
              atl-markup-delay))))"##,
        expect![[
            r#"OK (t nil ((defun atl-markup-mode) (provide atl-markup-autoloads)) ("atl-markup") t "atl-markup" t "[Arg list not available until function definition is loaded.]" ((atl-markup--inside-tag-p nil nil) (atl-markup--post-command-hook nil nil) (atl-markup-delay nil nil)))"#
        ]],
    )
}

pub(super) fn registry_atl_markup_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_markup_descriptor_and_archive_sources_pin_exact_melpa_payload(),
        atl_markup_complete_prefixed_symbol_inventory_records_every_surface(),
        atl_markup_all_functions_have_exact_call_interactive_and_documentation_contracts(),
        atl_markup_customization_group_defaults_and_minor_mode_metadata_are_exact(),
        atl_markup_internal_timer_variable_contract_is_exact(),
        atl_markup_installed_source_byte_compiles_loads_and_drives_practical_navigation(),
    ]
}

pub(super) fn registry_atl_markup_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![atl_markup_generated_autoload_preserves_feature_history_prefix_and_command_contract()]
}
