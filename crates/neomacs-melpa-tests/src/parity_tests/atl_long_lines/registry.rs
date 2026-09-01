use expect_test::expect;

use super::ParityBatchCase;

fn atl_long_lines_exact_package_descriptor_origin_dependency_and_feature_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_exact_package_descriptor_origin_dependency_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'atl-long-lines package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'atl-long-lines)
          (package-installed-p
           'atl-long-lines
           '(20240101 929))
          (file-name-nondirectory
           (locate-library
            "atl-long-lines"))))"##,
        expect![[
            r#"OK (atl-long-lines "20240101.929" "Turn off truncate-lines when the line is long." nil ((emacs (24 3))) ((:maintainers ("Jen-Chieh" . "jcs090218@gmail.com")) (:authors ("Jen-Chieh" . "jcs090218@gmail.com")) (:keywords "convenience" "truncate" "lines" "auto" "long") (:revdesc . "82cdd4edefba") (:commit . "82cdd4edefba2d5b1d491bf3fcc487385819d713") (:url . "https://github.com/jcs-elpa/atl-long-lines")) t t "atl-long-lines.el")"#
        ]],
    )
}

fn atl_long_lines_archive_provided_payload_sizes_and_content_digests_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_archive_provided_payload_sizes_and_content_digests_match",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'atl-long-lines package-alist)))
                 (directory
                  (package-desc-dir descriptor)))
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
          '("atl-long-lines.el"
            "atl-long-lines-pkg.el")))"##,
        expect![[
            r#"OK (("atl-long-lines.el" 3803 "665e06b1058f1bf78ff2d217aa2d7b8ebb5cf850b671c3a4596c29eabc6a9f47") ("atl-long-lines-pkg.el" 473 "aace490034490f7eccbb4d209c1727544a1dfb219a12be1566cbed444db7d36f"))"#
        ]],
    )
}

fn atl_long_lines_complete_callable_command_arglist_and_source_surface_matches() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_long_lines_complete_callable_command_arglist_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "atl-long-lines"
                  (symbol-name symbol))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and
                    file
                    (string=
                     (file-name-nondirectory file)
                     "atl-long-lines.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (macrop symbol)
             (commandp symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist
               symbol
               t))
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name left)
              (symbol-name right))))))"##,
        expect![[
            r#"OK ((atl-long-lines--disable nil nil nil "nil" "atl-long-lines.el") (atl-long-lines--enable nil nil nil "nil" "atl-long-lines.el") (atl-long-lines--end-line-column nil nil nil "nil" "atl-long-lines.el") (atl-long-lines--mute-apply t nil nil "(&rest body)" "atl-long-lines.el") (atl-long-lines--start-timer nil nil nil "nil" "atl-long-lines.el") (atl-long-lines--turn-on-atl-long-lines-mode nil nil nil "nil" "atl-long-lines.el") (atl-long-lines-do-toggle nil nil nil "nil" "atl-long-lines.el") (atl-long-lines-mode nil t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) "(&optional arg)" "atl-long-lines.el") (atl-long-lines-mode--set-explicitly nil nil nil "nil" "atl-long-lines.el"))"#
        ]],
    )
}

fn atl_long_lines_complete_declared_variable_defaults_custom_and_source_surface_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_complete_declared_variable_defaults_custom_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "atl-long-lines"
                  (symbol-name symbol))
                 (boundp symbol)
                 (let ((file
                        (symbol-file symbol 'defvar)))
                   (and
                    file
                    (string=
                     (file-name-nondirectory file)
                     "atl-long-lines.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (let* ((standard
                    (get symbol
                         'standard-value))
                   (standard-form
                    (and
                     (consp standard)
                     (= (length standard) 1)
                     (car standard))))
              (list
               symbol
               (default-value symbol)
               (special-variable-p symbol)
               (local-variable-if-set-p
                symbol)
               (and
                (custom-variable-p symbol)
                t)
               (get symbol 'custom-type)
               (get symbol 'custom-group)
               (and standard-form t)
               (and
                standard-form
                (equal
                 (eval standard-form t)
                 (default-value symbol)))
               (file-name-nondirectory
                (symbol-file symbol 'defvar)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name left)
              (symbol-name right))))))"##,
        expect![[
            r#"OK ((atl-long-lines--timer nil t nil nil nil nil nil nil "atl-long-lines.el") (atl-long-lines-delay 0.4 t nil t float nil t t "atl-long-lines.el") (atl-long-lines-mode nil t t nil nil nil nil nil "atl-long-lines.el") (atl-long-lines-mode--set-explicitly nil t t nil nil nil nil nil "atl-long-lines.el") (atl-long-lines-mode--suppress-set-explicitly nil t nil nil nil nil nil nil "atl-long-lines.el") (atl-long-lines-mode-hook (atl-long-lines-mode--set-explicitly) t nil t hook nil nil nil "atl-long-lines.el"))"#
        ]],
    )
}

fn atl_long_lines_custom_group_mode_metadata_and_documentation_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_custom_group_mode_metadata_and_documentation_match",
        r##"(list
         (get 'atl-long-lines
              'custom-group)
         (get 'atl-long-lines
              'group-documentation)
         (get 'atl-long-lines
              'custom-prefix)
         (get 'atl-long-lines
              'custom-links)
         (list
          (default-value
           'atl-long-lines-delay)
          (get 'atl-long-lines-delay
               'custom-type)
          (get 'atl-long-lines-delay
               'custom-group)
          (documentation-property
           'atl-long-lines-delay
           'variable-documentation))
         (list
          (default-value
           'atl-long-lines-mode)
          (get 'atl-long-lines-mode
               'permanent-local)
          (get 'atl-long-lines-mode
               'variable-documentation))
         (list
          (default-value
           'global-atl-long-lines-mode)
          (get 'global-atl-long-lines-mode
               'variable-documentation))
         (documentation
          'atl-long-lines-do-toggle)
         (documentation
          'atl-long-lines--mute-apply))"##,
        expect![[
            r#"OK (((atl-long-lines-delay custom-variable) (global-atl-long-lines-mode custom-variable)) "Turn off truncate-lines when the line is long." "atl-long-lines-" ((url-link :tag "Repository" "https://github.com/jcs-elpa/atl-long-lines")) (0.4 float nil "Seconds to delay before trigger function ‘toggle-truncate-lines’.") (nil nil "Non-nil if Atl-Long-Lines mode is enabled.\nUse the command `atl-long-lines-mode' to change this variable.") (nil "Non-nil if Global Atl-Long-Lines mode is enabled.\nSee the `global-atl-long-lines-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `global-atl-long-lines-mode'.") "Do toggle truncate lines at current position." "Execute BODY without message.")"#
        ]],
    )
}

fn atl_long_lines_generated_global_mode_public_command_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_generated_global_mode_public_command_surface_matches",
        r##"(list
         (fboundp
          'global-atl-long-lines-mode)
         (commandp
          'global-atl-long-lines-mode)
         (interactive-form
          'global-atl-long-lines-mode)
         (prin1-to-string
          (help-function-arglist
           'global-atl-long-lines-mode
           t))
         (documentation
          'global-atl-long-lines-mode)
         (boundp
          'global-atl-long-lines-mode)
         (default-value
          'global-atl-long-lines-mode)
         (and
          (custom-variable-p
           'global-atl-long-lines-mode)
          t)
         (get
          'global-atl-long-lines-mode
          'custom-type)
         (get
          'global-atl-long-lines-mode
          'custom-group)
         (list
          (fboundp
           'global-atl-long-lines-mode-enable-in-buffer)
          (commandp
           'global-atl-long-lines-mode-enable-in-buffer)
          (prin1-to-string
           (help-function-arglist
            'global-atl-long-lines-mode-enable-in-buffer
            t))
          (documentation
           'global-atl-long-lines-mode-enable-in-buffer)
          (file-name-nondirectory
           (symbol-file
            'global-atl-long-lines-mode-enable-in-buffer
            'defun)))
         (list
          (boundp
           'global-atl-long-lines-mode-hook)
          (default-value
           'global-atl-long-lines-mode-hook)
          (special-variable-p
           'global-atl-long-lines-mode-hook)
          (local-variable-if-set-p
           'global-atl-long-lines-mode-hook)
          (and
           (custom-variable-p
            'global-atl-long-lines-mode-hook)
           t)
          (file-name-nondirectory
           (symbol-file
            'global-atl-long-lines-mode-hook
            'defvar))))"##,
        expect![[
            r#"OK (t t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) "(&optional arg)" "Toggle Atl-Long-Lines mode in many buffers.\nSpecifically, Atl-Long-Lines mode is enabled in all buffers where\n‘atl-long-lines--turn-on-atl-long-lines-mode’ would do it.\n\nWith prefix ARG, enable Global Atl-Long-Lines mode if ARG is positive;\notherwise, disable it.\n\nIf called from Lisp, toggle the mode if ARG is ‘toggle’.\nEnable the mode if ARG is nil, omitted, or is a positive number.\nDisable the mode if ARG is a negative number.\n\nSee ‘atl-long-lines-mode’ for more information on Atl-Long-Lines\nmode." t nil t boolean nil (t nil "nil" nil "atl-long-lines.el") (t nil t nil t "atl-long-lines.el"))"#
        ]],
    )
}

fn atl_long_lines_reloading_source_preserves_customized_default_and_existing_timer_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_reloading_source_preserves_customized_default_and_existing_timer_state",
        r##"(let* ((source
                  (getenv
                   "NEOMACS_PACKAGE_SOURCE"))
                 (sentinel
                  (list :existing-timer)))
         (setq-default
          atl-long-lines-delay
          1.75)
         (setq
          atl-long-lines--timer
          sentinel)
         (load source nil t)
         (list
          (default-value
           'atl-long-lines-delay)
          (eq
           atl-long-lines--timer
           sentinel)
          (featurep
           'atl-long-lines)
          (file-name-nondirectory
           (symbol-file
            'atl-long-lines-do-toggle
            'defun))))"##,
        expect![[r#"OK (1.75 t t "atl-long-lines.el")"#]],
    )
}

fn atl_long_lines_generated_autoload_contract_registers_modes_without_loading_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_generated_autoload_contract_registers_modes_without_loading_source",
        r##"(let ((autoload-file
                (symbol-file
                 'atl-long-lines-mode
                 'defun))
               (minor-mode-documentation
                "Minor mode 'atl-long-lines-mode'.

This is a minor mode.  If called interactively, toggle the
`Atl-Long-Lines mode' mode.  If the prefix argument is positive, enable
the mode, and if it is zero or negative, disable the mode.

If called from Lisp, toggle the mode if ARG is `toggle'.  Enable the
mode if ARG is nil, omitted, or is a positive number.  Disable the mode
if ARG is a negative number.

To check whether the minor mode is enabled in the current buffer,
evaluate the variable `atl-long-lines-mode'.

The mode's hook is called both when the mode is enabled and when it is
disabled.

(fn &optional ARG)")
               (global-mode-documentation
                "Toggle Atl-Long-Lines mode in many buffers.
Specifically, Atl-Long-Lines mode is enabled in all buffers where
`atl-long-lines--turn-on-atl-long-lines-mode' would do it.

With prefix ARG, enable Global Atl-Long-Lines mode if ARG is positive;
otherwise, disable it.

If called from Lisp, toggle the mode if ARG is `toggle'.
Enable the mode if ARG is nil, omitted, or is a positive number.
Disable the mode if ARG is a negative number.

See `atl-long-lines-mode' for more information on Atl-Long-Lines
mode.

(fn &optional ARG)"))
         (list
          (featurep
           'atl-long-lines-autoloads)
          (featurep
           'atl-long-lines)
          (autoloadp
           (symbol-function
            'atl-long-lines-mode))
          (autoloadp
           (symbol-function
            'global-atl-long-lines-mode))
          (commandp
           'atl-long-lines-mode)
          (commandp
           'global-atl-long-lines-mode)
          (file-name-nondirectory
           autoload-file)
          (equal
           autoload-file
           (symbol-file
            'global-atl-long-lines-mode
            'defun))
          (and
           (boundp
            'atl-long-lines-mode)
           t)
          (and
           (boundp
            'global-atl-long-lines-mode)
           t)
          (default-value
           'global-atl-long-lines-mode)
          (get
           'global-atl-long-lines-mode
           'globalized-minor-mode)
          (get
           'global-atl-long-lines-mode
           'custom-autoload)
          (get
           'global-atl-long-lines-mode
           'custom-loads)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (help-function-arglist
               symbol
               t)
              (equal
               (documentation symbol t)
               (if
                   (eq
                    symbol
                    'atl-long-lines-mode)
                   minor-mode-documentation
                 global-mode-documentation))))
           '(atl-long-lines-mode
             global-atl-long-lines-mode))
          (let ((files
                 (if
                     (hash-table-p
                      definition-prefixes)
                     (gethash
                      "atl-long-lines-"
                      definition-prefixes)
                   (cdr
                    (assoc
                     "atl-long-lines-"
                     definition-prefixes)))))
            (sort
             (delete-dups
              (copy-sequence files))
             #'string<))
          (let ((history
                 (assoc
                  (locate-library
                   "atl-long-lines-autoloads")
                  load-history)))
            (list
             (and history t)
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
              (cdr history)))))))"##,
        expect![[
            r#"OK (t nil t t t t "atl-long-lines.el" t nil t nil t t ("atl-long-lines") ((atl-long-lines-mode "[Arg list not available until function definition is loaded.]" t) (global-atl-long-lines-mode "[Arg list not available until function definition is loaded.]" t)) ("atl-long-lines") (t ((defun atl-long-lines-mode) (defun global-atl-long-lines-mode) (provide atl-long-lines-autoloads))))"#
        ]],
    )
}

pub(super) fn registry_atl_long_lines_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_long_lines_exact_package_descriptor_origin_dependency_and_feature_contract_match(),
        atl_long_lines_archive_provided_payload_sizes_and_content_digests_match(),
        atl_long_lines_complete_callable_command_arglist_and_source_surface_matches(),
        atl_long_lines_complete_declared_variable_defaults_custom_and_source_surface_matches(),
        atl_long_lines_custom_group_mode_metadata_and_documentation_match(),
        atl_long_lines_generated_global_mode_public_command_surface_matches(),
        atl_long_lines_reloading_source_preserves_customized_default_and_existing_timer_state(),
    ]
}

pub(super) fn registry_atl_long_lines_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![atl_long_lines_generated_autoload_contract_registers_modes_without_loading_source()]
}
