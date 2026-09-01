use expect_test::expect;

use super::ParityBatchCase;

fn async_backup_descriptor_and_source_inventory_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_descriptor_and_source_inventory_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr (assq 'async-backup package-alist)))
               (directory (package-desc-dir descriptor))
               (sources
                (sort
                 (directory-files directory t "\\.el\\'")
                 #'string<)))
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
               (file-attribute-size (file-attributes file))
               (with-temp-buffer
                 (insert-file-contents-literally file)
                 (secure-hash 'sha256 (current-buffer)))))
            sources)))"##,
        expect![[
            r#"OK ((async-backup "20230412.1534" "Backup on each save without freezing Emacs." ((emacs (24 4))) ((:maintainers ("contrapunctus" . "xmpp:contrapunctus@jabjab.de")) (:authors ("contrapunctus" . "xmpp:contrapunctus@jabjab.de")) (:keywords "files") (:revdesc . "d07a7bd4a5c3") (:commit . "d07a7bd4a5c3332a8a585680d67925385c595927") (:url . "https://codeberg.org/contrapunctus/async-backup"))) (("async-backup-autoloads.el" 818 "86b8c78b73cf8147df41b66873ddff286d43b0777ca94bfa0bc96482b424995e") ("async-backup-pkg.el" 461 "876420426f8cb4e0ab34a1bfd78808cf4ad92ff5d88a946d7dbf49a2b7e8479d") ("async-backup.el" 3286 "51e86a85cedea9a5bc6a0e42d107f1a163f07d9a6b5574e481037690e11bc5ff")))"#
        ]],
    )
}

fn async_backup_public_callable_surface_has_exact_command_and_arglist_contract() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_public_callable_surface_has_exact_command_and_arglist_contract",
        r##"(list
          (featurep 'async-backup)
          (fboundp 'async-backup)
          (commandp 'async-backup)
          (macrop 'async-backup)
          (help-function-arglist 'async-backup t)
          (car-safe (interactive-form 'async-backup))
          (documentation 'async-backup)
          (get 'async-backup 'function-documentation))"##,
        expect![[
            r#"OK (t t nil nil (&optional file) nil "Backup FILE, or file visited by current buffer." nil)"#
        ]],
    )
}

fn async_backup_all_declared_variables_have_exact_defaults_and_custom_metadata() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_all_declared_variables_have_exact_defaults_and_custom_metadata",
        r##"(list
          (get 'async-backup 'group-documentation)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (boundp symbol)
              (if (eq symbol 'async-backup-location)
                  (file-relative-name
                   (symbol-value symbol)
                   user-emacs-directory)
                (symbol-value symbol))
              (get symbol 'custom-type)
              (get symbol 'custom-group)
              (get symbol 'standard-value)
              (local-variable-if-set-p symbol)
              (get symbol 'safe-local-variable)))
           '(async-backup-location
             async-backup-time-format
             async-backup-predicates)))"##,
        expect![[
            r#"OK ("Backup on each save without freezing Emacs." ((async-backup-location t "async-backup" directory nil ((locate-user-emacs-file "async-backup")) nil nil) (async-backup-time-format t "%FT%H-%M-%S" string nil ("%FT%H-%M-%S") nil nil) (async-backup-predicates t #1=(identity) (repeat function) nil ('#1#) nil nil)))"#
        ]],
    )
}

fn async_backup_custom_options_drive_one_exact_runtime_configuration() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_custom_options_drive_one_exact_runtime_configuration",
        r##"(let* ((async-backup-location
                (async-backup-test-path "custom/backups/"))
               (async-backup-time-format "%Y--%j--%H%M")
               (async-backup-predicates
                (list
                 #'file-readable-p
                 (lambda (file)
                   (string-suffix-p ".org" file))))
               captured
               formats)
          (async-backup-test-write-file
           "notes/entry.org"
           "* entry\n")
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (format-string &rest _)
                       (push format-string formats)
                       "2026--209--1314"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :process)))
            (list
             (async-backup
              (async-backup-test-path "notes/entry.org"))
             async-backup-time-format
             (length async-backup-predicates)
             (nreverse formats)
             (async-backup-test-normalize-command captured)
             (file-directory-p
              (async-backup-test-path
               "custom/backups")))))"##,
        expect![[
            r#"OK (:process "%Y--%j--%H%M" 2 ("%Y--%j--%H%M") ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//notes/entry.org\" \"$ROOT//custom/backups$ROOT//notes/entry-2026--209--1314.org\")") t)"#
        ]],
    )
}

fn async_backup_generated_autoload_exposes_command_without_loading_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_generated_autoload_exposes_command_without_loading_payload",
        r##"(let ((definition
                (symbol-function 'async-backup)))
          (list
           (featurep 'async-backup)
           (autoloadp definition)
           (and (autoloadp definition)
                (nth 1 definition))
           (and (autoloadp definition)
                (nth 4 definition))
           (commandp 'async-backup)
           (help-function-arglist 'async-backup t)
           (get 'async-backup 'custom-autoload)
           (get 'async-backup-location 'custom-autoload)
           (get 'async-backup-time-format 'custom-autoload)
           (get 'async-backup-predicates 'custom-autoload)))"##,
        expect![[
            r#"OK (nil t "async-backup" nil nil "[Arg list not available until function definition is loaded.]" nil nil nil nil)"#
        ]],
    )
}

pub(super) fn registry_async_backup_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_backup_descriptor_and_source_inventory_pin_exact_melpa_payload(),
        async_backup_public_callable_surface_has_exact_command_and_arglist_contract(),
        async_backup_all_declared_variables_have_exact_defaults_and_custom_metadata(),
        async_backup_custom_options_drive_one_exact_runtime_configuration(),
    ]
}

pub(super) fn registry_async_backup_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![async_backup_generated_autoload_exposes_command_without_loading_payload()]
}
