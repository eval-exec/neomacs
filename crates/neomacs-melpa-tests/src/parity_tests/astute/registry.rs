use expect_test::expect;

use super::ParityBatchCase;

fn astute_exact_package_descriptor_dependency_origin_and_feature_contract_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_exact_package_descriptor_dependency_origin_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'astute package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'astute)
          (package-installed-p
           'astute
           '(20241015 444))
          (file-name-nondirectory
           (locate-library "astute"))))"##,
        expect![[
            r#"OK (astute "20241015.444" "A minor mode to redisplay `smart' typography." nil ((emacs (25 1))) ((:maintainers ("Paul W. Rankin" . "rnkn@rnkn.xyz")) (:authors ("Paul W. Rankin" . "rnkn@rnkn.xyz")) (:keywords "faces" "wp") (:revdesc . "69d413c95277") (:commit . "69d413c952771c0d06cda161fb25fe495fb895b0") (:url . "https://github.com/rnkn/astute")) t t "astute.el")"#
        ]],
    )
}

fn astute_installed_payload_inventory_sizes_and_content_digests_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_installed_payload_inventory_sizes_and_content_digests_match",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'astute package-alist)))
                 (directory
                  (package-desc-dir descriptor)))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (list
               file
               (file-attribute-size
                (file-attributes path))
               (with-temp-buffer
                 (set-buffer-multibyte nil)
                 (insert-file-contents-literally path)
                 (secure-hash
                  'sha256
                  (current-buffer))))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("astute-autoloads.el" 1298 "41b026cc15c61d538e81a3e4200756cbc306f8bab5269a5a9ea4d4711e795086") ("astute-pkg.el" 416 "ae4040cd38dbf88c61af593e942170ee8abf7bb48643bc2be459c589e0d410ce") ("astute.el" 6226 "28d2d8762125e26c005639a3089d9f31020b9a2a4ef73d6b5202edc0400781c9") ("astute.elc" 5749 "ce9f2fb2bdff030afae0f4ce6a5488e1857c6974fa8fa01e5b0662f609e61941"))"#
        ]],
    )
}

fn astute_complete_callable_command_arglist_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_complete_callable_command_arglist_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "astute"
                  (symbol-name symbol))
                 (not
                  (string-suffix-p
                   "--inliner"
                   (symbol-name symbol)))
                 (not
                  (string-suffix-p
                   "--cmacro"
                   (symbol-name symbol)))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "astute.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist symbol t))
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((astute-case-insensitize nil nil "(string)" "astute.el") (astute-init-font-lock nil nil "nil" "astute.el") (astute-mode t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) "(&optional arg)" "astute.el"))"#
        ]],
    )
}

fn astute_complete_declared_variable_defaults_scope_custom_and_source_surface_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_complete_declared_variable_defaults_scope_custom_and_source_surface_matches",
        r##"(cl-labels
        ((stable
          (value)
          (cond
           ((and
             (functionp value)
             (not
              (symbolp value)))
            :function)
           ((consp value)
            (cons
             (stable
              (car value))
             (stable
              (cdr value))))
           ((vectorp value)
            (cons
             :vector
             (mapcar
              #'stable
              (append value nil))))
           (t value))))
       (let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "astute"
                  (symbol-name symbol))
                 (boundp symbol)
                 (let ((file
                        (symbol-file symbol 'defvar)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "astute.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (prin1-to-string
              (stable
               (default-value symbol)))
             (special-variable-p symbol)
             (local-variable-if-set-p symbol)
             (custom-variable-p symbol)
             (prin1-to-string
              (stable
               (get symbol 'custom-type)))
             (prin1-to-string
              (stable
               (get symbol 'custom-group)))
             (prin1-to-string
              (stable
               (get symbol 'safe-local-variable)))
             (file-name-nondirectory
              (symbol-file symbol 'defvar))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right)))))))"##,
        expect![[
            r#"OK ((astute--keywords "nil" t t nil "nil" "nil" "nil" "astute.el") (astute-double-quote-close-regexp "\"[[:alnum:][:punct:]]\\\\(\\\"\\\\)\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-double-quote-open-regexp "\"\\\\(\\\"\\\\)[[:alnum:][:punct:]]\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-em-dash-regexp "\"[^-]\\\\(---\\\\)[^-]\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-en-dash-regexp "\"[^-]\\\\(--\\\\)[^-]\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-lighter "\" “As”\"" t nil ((funcall #'#[nil ((format " %sAs%s" (char-to-string 8220) (char-to-string 8221))) #1=(t)])) "string" "nil" "stringp" "astute.el") (astute-mode "nil" t t nil "nil" "nil" "nil" "astute.el") (astute-mode-hook "nil" t nil (nil) "hook" "nil" "nil" "astute.el") (astute-prefix-single-quote-exceptions "(\"bout\" \"em\" \"n'\" \"cause\" \"round\" \"twas\" \"tis\")" t nil ((funcall #'#[nil ('("bout" "em" "n'" "cause" "round" "twas" "tis")) #1#])) "(repeat string)" "nil" "nil" "astute.el") (astute-single-quote-close-regexp "\"[[:alnum:][:punct:]]\\\\('\\\\)\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-single-quote-inner-regexp "\"[:alnum:]\\\\('\\\\)[:alnum:]\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-single-quote-open-regexp "\"\\\\('\\\\)[[:alnum:][:punct:]]\"" t nil nil "nil" "nil" "nil" "astute.el") (astute-transform-list "(single-quote double-quote en-dash em-dash)" t nil ((funcall #'#[nil ('(single-quote double-quote en-dash em-dash)) #1#])) "(set (const :tag \"Single Quotes\" single-quote) (const :tag \"Double Quotes\" double-quote) (const :tag \"En Dashes\" en-dash) (const :tag \"Em Dashes\" em-dash))" "nil" "listp" "astute.el"))"#
        ]],
    )
    .fresh_process()
}

fn astute_custom_group_members_types_safety_and_documentation_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_custom_group_members_types_safety_and_documentation_contract_match",
        r##"(list
         (get 'astute 'custom-group)
         (get 'astute 'group-documentation)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (get symbol 'custom-type)
             (get symbol 'custom-group)
             (get symbol 'safe-local-variable)
             (documentation-property
              symbol
              'variable-documentation)))
          '(astute-lighter
            astute-transform-list
            astute-prefix-single-quote-exceptions))
         (documentation 'astute-mode)
         (documentation 'astute-case-insensitize)
         (documentation 'astute-init-font-lock))"##,
        expect![[
            r#"OK (((astute-lighter custom-variable) (astute-transform-list custom-variable) (astute-prefix-single-quote-exceptions custom-variable)) "A minor mode to redisplay ``smart'' typography." ((astute-lighter string nil stringp "Mode-line indicator for ‘astute-mode’.") (astute-transform-list (set (const :tag "Single Quotes" single-quote) (const :tag "Double Quotes" double-quote) (const :tag "En Dashes" en-dash) (const :tag "Em Dashes" em-dash)) nil listp "List of characters to typographically transform.") (astute-prefix-single-quote-exceptions (repeat string) nil nil "List of regular expressions that should be prefixed by a closing quote.")) "Redisplay ‘smart’ typography.\n\nThis is a minor mode.  If called interactively, toggle the ‘Astute mode’\nmode.  If the prefix argument is positive, enable the mode, and if it is\nzero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is ‘toggle’.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable ‘astute-mode’.\n\nThe mode’s hook is called both when the mode is enabled and when it is\ndisabled." "Return a case-insensitive regular expression for STRING." "Return a new list of ‘font-lock-keywords’.")"#
        ]],
    )
}

fn astute_generated_autoload_registers_mode_without_eagerly_loading_package() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_generated_autoload_registers_mode_without_eagerly_loading_package",
        r##"(list
         (featurep 'astute)
         (fboundp 'astute-mode)
         (autoloadp
          (symbol-function 'astute-mode))
         (symbol-file 'astute-mode 'defun)
         (fboundp 'astute-case-insensitize)
         (fboundp 'astute-init-font-lock)
         (boundp 'astute-transform-list)
         (assoc 'astute-mode minor-mode-alist))"##,
        expect![[
            r#"OK (nil t t "[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/astute/20241015.444/69d413c952771c0d06cda161fb25fe495fb895b0/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/astute-20241015.444/astute.el" nil nil nil nil)"#
        ]],
    )
}

pub(super) fn registry_astute_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        astute_exact_package_descriptor_dependency_origin_and_feature_contract_match(),
        astute_installed_payload_inventory_sizes_and_content_digests_match(),
        astute_complete_callable_command_arglist_and_source_surface_matches(),
        astute_complete_declared_variable_defaults_scope_custom_and_source_surface_matches(),
        astute_custom_group_members_types_safety_and_documentation_contract_match(),
    ]
}

pub(super) fn registry_astute_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![astute_generated_autoload_registers_mode_without_eagerly_loading_package()]
}
