use super::ParityBatchCase;
use expect_test::expect;

fn package_loads_with_reformatter_and_registers_the_generated_public_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_loads_with_reformatter_and_registers_the_generated_public_surface",
        r##"
(list
 (featurep 'astyle)
 (featurep 'reformatter)
 (mapcar
  (lambda (symbol)
    (list
     symbol
     (fboundp symbol)
     (commandp symbol)
     (help-function-arglist
      symbol t)
     (file-name-nondirectory
      (or
       (symbol-file
        symbol 'defun)
       ""))))
  '(astyle--format-args
    astyle-region
    astyle-buffer
    astyle-on-save-mode)))
"##,
        expect![[
            r#"OK (t t ((astyle--format-args t nil nil "astyle.el") (astyle-region t t (beg end &optional display-errors) "astyle.el") (astyle-buffer t t (&optional display-errors) "astyle.el") (astyle-on-save-mode t t (&optional arg) "astyle.el")))"#
        ]],
    )
}

fn customization_registry_exposes_exact_defaults_types_group_and_lighter() -> ParityBatchCase {
    ParityBatchCase::value(
        "customization_registry_exposes_exact_defaults_types_group_and_lighter",
        r##"
(list
 (mapcar
  (lambda (symbol)
    (list
     symbol
     (symbol-value symbol)
     (get symbol 'custom-type)
     (get symbol 'custom-group)))
  '(astyle-style
    astyle-indent
    astyle-default-rc-name
    astyle-custom-args
    astyle-on-save-mode-lighter))
 astyle-default-args
 (default-boundp
  'c-basic-offset)
 (get 'astyle 'custom-group)
 (get 'astyle 'group-documentation))
"##,
        expect![[
            r#"OK (((astyle-style "google" string nil) (astyle-indent nil integer nil) (astyle-default-rc-name ".astylerc" string nil) (astyle-custom-args nil (repeat string) nil) (astyle-on-save-mode-lighter " astyle" string nil)) ("--pad-oper" "--pad-header" "--break-blocks" "--delete-empty-lines" "--align-pointer=type" "--align-reference=name") nil ((astyle-style custom-variable) (astyle-indent custom-variable) (astyle-default-rc-name custom-variable) (astyle-custom-args custom-variable)) "Astyle functions and settings.")"#
        ]],
    )
    .fresh_process()
}

fn installed_archive_metadata_dependency_and_source_identity_match_the_exact_pin() -> ParityBatchCase
{
    ParityBatchCase::value(
        "installed_archive_metadata_dependency_and_source_identity_match_the_exact_pin",
        r##"
(let* ((description
        (cadr
         (assq
          'astyle
          package-alist)))
       (source
        (expand-file-name
         "astyle.el"
         (package-desc-dir
          description))))
  (list
   (package-version-join
    (package-desc-version
     description))
   (mapcar
    (lambda (dependency)
      (list
       (car dependency)
       (package-version-join
        (cadr dependency))))
    (package-desc-reqs
     description))
   (file-attribute-size
    (file-attributes source))
   (let ((contents
          (astyle-test-read-file
           source)))
     (list
      (and
       (string-match-p
        "Package-Version: 20200328\\.616"
        contents)
       t)
      (and
       (string-match-p
        "Package-Revision: 04ff2941f08c"
        contents)
       t)
      (and
       (string-match-p
        "(reformatter-define astyle"
        contents)
       t)
      (and
       (string-match-p
        "(provide 'astyle)"
        contents)
       t)))
   (file-name-nondirectory
    (or
     (symbol-file
      'astyle--format-args
      'defun)
     ""))))
"##,
        expect![[
            r#"OK ("20200328.616" ((emacs "24.4") (reformatter "0.3")) 3854 (t t t t) "astyle.el")"#
        ]],
    )
}

fn on_save_mode_state_is_buffer_local_and_uses_the_generated_lighter_and_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "on_save_mode_state_is_buffer_local_and_uses_the_generated_lighter_and_hook",
        r##"
(let ((first
       (generate-new-buffer
        " *astyle-first*"))
      (second
       (generate-new-buffer
        " *astyle-second*")))
  (unwind-protect
      (progn
        (with-current-buffer first
          (astyle-on-save-mode 1))
        (list
         (with-current-buffer first
           (list
            astyle-on-save-mode
            (local-variable-p
             'astyle-on-save-mode)
            (local-variable-p
             'before-save-hook)
            (memq
             'astyle-buffer
             before-save-hook)))
         (with-current-buffer second
           (list
            astyle-on-save-mode
            (local-variable-p
             'astyle-on-save-mode)
            (memq
             'astyle-buffer
             before-save-hook)))
         astyle-on-save-mode-lighter))
    (kill-buffer first)
    (kill-buffer second)))
"##,
        expect![[r#"OK ((t t t (astyle-buffer t)) (nil nil nil) " astyle")"#]],
    )
}

fn generated_autoloads_publish_buffer_region_and_on_save_mode_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "generated_autoloads_publish_buffer_region_and_on_save_mode_commands",
        r##"
(list
 (mapcar
  (lambda (symbol)
    (let ((definition
           (symbol-function
            symbol)))
      (list
       symbol
       (autoloadp definition)
       (nth 1 definition)
       (nth 3 definition)
       (nth 4 definition)
       (commandp symbol))))
  '(astyle-buffer
    astyle-region
    astyle-on-save-mode))
 (featurep 'astyle-autoloads)
 (featurep 'astyle))
"##,
        expect![[
            r#"OK (((astyle-buffer t "astyle" t nil t) (astyle-region t "astyle" t nil t) (astyle-on-save-mode t "astyle" t nil t)) t nil)"#
        ]],
    )
}

pub(super) fn registry_astyle_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_loads_with_reformatter_and_registers_the_generated_public_surface(),
        customization_registry_exposes_exact_defaults_types_group_and_lighter(),
        installed_archive_metadata_dependency_and_source_identity_match_the_exact_pin(),
        on_save_mode_state_is_buffer_local_and_uses_the_generated_lighter_and_hook(),
    ]
}

pub(super) fn registry_astyle_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![generated_autoloads_publish_buffer_region_and_on_save_mode_commands()]
}
