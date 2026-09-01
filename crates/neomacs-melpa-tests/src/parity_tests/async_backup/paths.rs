use expect_test::expect;

use super::ParityBatchCase;

fn async_backup_explicit_file_builds_exact_timestamped_output_tree_and_child_command()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_explicit_file_builds_exact_timestamped_output_tree_and_child_command",
        r##"(let* ((file
                (async-backup-test-write-file
                 "project/src/report.txt"
                 "version one\n"))
               (async-backup-location
                (async-backup-test-path "backups"))
               (async-backup-time-format "%FT%H-%M-%S")
               captured)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (format-string &rest _)
                       (list
                        (unless (equal format-string
                                       async-backup-time-format)
                          (error "unexpected time format"))
                        "2026-07-27T13-20-45")
                       "2026-07-27T13-20-45"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :child)))
            (list
             (async-backup file)
             (async-backup-test-normalize-command captured)
             (file-directory-p
              (concat
               (directory-file-name async-backup-location)
               (file-name-directory file)))
             (file-exists-p file))))"##,
        expect![[
            r#"OK (:child ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//project/src/report.txt\" \"$ROOT//backups$ROOT//project/src/report-2026-07-27T13-20-45.txt\")") t t)"#
        ]],
    )
}

fn async_backup_file_without_extension_keeps_complete_base_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_file_without_extension_keeps_complete_base_name",
        r##"(let* ((file
                (async-backup-test-write-file
                 "project/bin/LICENSE"
                 "license body\n"))
               (async-backup-location
                (async-backup-test-path "backup-root/"))
               captured)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "STAMP"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :started)))
            (list
             (async-backup file)
             (async-backup-test-normalize-command captured)
             (file-directory-p
              (async-backup-test-path
               "backup-root")))))"##,
        expect![[
            r#"OK (:started ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//project/bin/LICENSE\" \"$ROOT//backup-root$ROOT//project/bin/LICENSE-STAMP\")") t)"#
        ]],
    )
}

fn async_backup_multi_extension_dotfile_unicode_and_quoted_names_are_escaped_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_multi_extension_dotfile_unicode_and_quoted_names_are_escaped_exactly",
        r##"(let ((async-backup-location
                (async-backup-test-path "backups"))
               commands)
          (dolist (entry
                   '(("names/archive.tar.gz" "archive")
                     ("names/.env" "secret")
                     ("names/β界 file.el" "unicode")
                     ("names/quote\"and\\\\slash.org" "quoted")))
            (let ((file
                   (async-backup-test-write-file
                    (car entry)
                    (cadr entry))))
              (cl-letf (((symbol-function 'format-time-string)
                         (lambda (&rest _) "T"))
                        ((symbol-function 'start-process)
                         (lambda (&rest command)
                           (push
                            (async-backup-test-normalize-command
                             command)
                            commands)
                           :started)))
                (async-backup file))))
          (nreverse commands))"##,
        expect![[
            r#"OK (("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//names/archive.tar.gz\" \"$ROOT//backups$ROOT//names/archive.tar-T.gz\")") ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//names/.env\" \"$ROOT//backups$ROOT//names/.env-T\")") ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//names/β界 file.el\" \"$ROOT//backups$ROOT//names/β界 file-T.el\")") ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//names/quote\\\"and\\\\\\\\slash.org\" \"$ROOT//backups$ROOT//names/quote\\\"and\\\\\\\\slash-T.org\")"))"#
        ]],
    )
}

fn async_backup_without_argument_uses_current_buffer_visited_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_without_argument_uses_current_buffer_visited_file",
        r##"(let* ((file
                (async-backup-test-write-file
                 "buffers/current.org"
                 "* saved\n"))
               (async-backup-location
                (async-backup-test-path "buffer-backups"))
               captured)
          (unwind-protect
              (with-current-buffer
                  (find-file-noselect file)
                (cl-letf (((symbol-function 'format-time-string)
                           (lambda (&rest _) "CURRENT"))
                          ((symbol-function 'start-process)
                           (lambda (&rest command)
                             (setq captured command)
                             :buffer-child)))
                  (list
                   (async-backup)
                   (equal (buffer-file-name) file)
                   (async-backup-test-normalize-command
                    captured))))
            (async-backup-test-kill-file-buffer file)))"##,
        expect![[
            r#"OK (:buffer-child t ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//buffers/current.org\" \"$ROOT//buffer-backups$ROOT//buffers/current-CURRENT.org\")"))"#
        ]],
    )
}

fn async_backup_relative_file_is_expanded_against_current_default_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_relative_file_is_expanded_against_current_default_directory",
        r##"(let* ((work
                (async-backup-test-path
                 "relative/project/"))
               (default-directory work)
               (file
                (async-backup-test-write-file
                 "relative/project/src/code.rs"
                 "fn main() {}\n"))
               (async-backup-location
                (async-backup-test-path "relative/backups"))
               captured)
          (make-directory work t)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "REL"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :started)))
            (list
             (async-backup "src/code.rs")
             (equal file
                    (expand-file-name
                     "src/code.rs"
                     default-directory))
             (async-backup-test-normalize-command captured))))"##,
        expect![[
            r#"OK (:started t ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//relative/project/src/code.rs\" \"$ROOT//relative/backups$ROOT//relative/project/src/code-REL.rs\")"))"#
        ]],
    )
}

fn async_backup_relative_location_is_expanded_before_appending_source_directory() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_relative_location_is_expanded_before_appending_source_directory",
        r##"(let* ((work
                (async-backup-test-path "location-work/"))
               (default-directory work)
               (file
                (async-backup-test-write-file
                 "location-work/input/a.md"
                 "# A\n"))
               (async-backup-location "../relative-backups/")
               captured)
          (make-directory work t)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "LOC"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :started)))
            (list
             (async-backup file)
             (async-backup-test-normalize-command captured)
             (file-directory-p
              (async-backup-test-path
               "relative-backups")))))"##,
        expect![[
            r#"OK (:started ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//location-work/input/a.md\" \"$ROOT//relative-backups$ROOT//location-work/input/a-LOC.md\")") t)"#
        ]],
    )
}

fn async_backup_location_with_or_without_trailing_separator_produces_same_target() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_location_with_or_without_trailing_separator_produces_same_target",
        r##"(let* ((file
                (async-backup-test-write-file
                 "slash/input.data"
                 "data\n"))
               (root
                (async-backup-test-path "slash/backups"))
               commands)
          (dolist (location
                   (list root
                         (file-name-as-directory root)))
            (let ((async-backup-location location))
              (cl-letf (((symbol-function 'format-time-string)
                         (lambda (&rest _) "SAME"))
                        ((symbol-function 'start-process)
                         (lambda (&rest command)
                           (push
                            (async-backup-test-normalize-command
                             command)
                            commands)
                           :started)))
                (async-backup file))))
          (let ((ordered (nreverse commands)))
            (list
             ordered
             (equal (car ordered)
                    (cadr ordered)))))"##,
        expect![[
            r#"OK ((("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//slash/input.data\" \"$ROOT//slash/backups$ROOT//slash/input-SAME.data\")") ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//slash/input.data\" \"$ROOT//slash/backups$ROOT//slash/input-SAME.data\")")) t)"#
        ]],
    )
}

fn async_backup_non_file_buffer_signals_before_creating_backup_tree_or_process() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_non_file_buffer_signals_before_creating_backup_tree_or_process",
        r##"(let ((async-backup-location
                (async-backup-test-path "nil-buffer/backups"))
               started)
          (with-temp-buffer
            (cl-letf (((symbol-function 'start-process)
                       (lambda (&rest _)
                         (setq started t)
                         :unexpected)))
              (list
               (async-backup-test-error-data
                #'async-backup)
               (file-exists-p
                async-backup-location)
               started))))"##,
        expect!["OK ((:error wrong-type-argument (stringp nil)) nil nil)"],
    )
}

fn async_backup_output_parent_collision_signals_before_predicates_or_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_output_parent_collision_signals_before_predicates_or_process",
        r##"(let* ((file
                (async-backup-test-write-file
                 "blocked/input.txt"
                 "input\n"))
               (root
                (async-backup-test-write-file
                 "blocked/root-as-file"
                 "not a directory\n"))
               (async-backup-location root)
               predicate-called
               started)
          (let ((async-backup-predicates
                 (list
                  (lambda (_file)
                    (setq predicate-called t)
                    t))))
            (cl-letf (((symbol-function 'start-process)
                       (lambda (&rest _)
                         (setq started t)
                         :unexpected)))
              (list
               (async-backup-test-error-data
                (lambda ()
                  (async-backup file)))
               predicate-called
               started
               (file-regular-p root)))))"##,
        expect![[
            r#"OK ((:error file-already-exists ("File exists" "[ORACLE-SANDBOX]/blocked/root-as-file")) nil nil t)"#
        ]],
    )
}

fn async_backup_directory_input_still_builds_a_child_copy_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_directory_input_still_builds_a_child_copy_command",
        r##"(let* ((directory
                (async-backup-test-path "directory-input/source/"))
               (async-backup-location
                (async-backup-test-path "directory-input/backups"))
               captured)
          (make-directory directory t)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "DIR"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :started)))
            (list
             (async-backup-test-error-data
              (lambda ()
                (async-backup directory)))
             (async-backup-test-normalize-command captured))))"##,
        expect![[
            r#"OK ((:ok :started) ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//directory-input/source/\" \"$ROOT//directory-input/backups$ROOT//directory-input/source/-DIR\")"))"#
        ]],
    )
}

fn async_backup_missing_input_still_constructs_target_and_launches_child() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_missing_input_still_constructs_target_and_launches_child",
        r##"(let* ((file
                (async-backup-test-path "missing/source.txt"))
               (async-backup-location
                (async-backup-test-path "missing/backups"))
               captured)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "MISS"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :started)))
            (list
             (file-exists-p file)
             (async-backup file)
             (async-backup-test-normalize-command captured)
             (file-directory-p
              (concat
               (directory-file-name async-backup-location)
               (file-name-directory file))))))"##,
        expect![[
            r#"OK (nil :started ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//missing/source.txt\" \"$ROOT//missing/backups$ROOT//missing/source-MISS.txt\")") t)"#
        ]],
    )
}

fn async_backup_symlink_path_is_preserved_in_predicate_and_child_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_symlink_path_is_preserved_in_predicate_and_child_command",
        r##"(let* ((target
                (async-backup-test-write-file
                 "symlink/real/source.txt"
                 "linked\n"))
               (link
                (async-backup-test-path
                 "symlink/alias/source-link.txt"))
               (async-backup-location
                (async-backup-test-path "symlink/backups"))
               seen
               captured)
          (make-directory (file-name-directory link) t)
          (make-symbolic-link target link)
          (let ((async-backup-predicates
                 (list
                  (lambda (file)
                    (setq seen file)
                    t))))
            (cl-letf (((symbol-function 'format-time-string)
                       (lambda (&rest _) "LINK"))
                      ((symbol-function 'start-process)
                       (lambda (&rest command)
                         (setq captured command)
                         :started)))
              (list
               (async-backup link)
               (equal seen link)
               (file-symlink-p link)
               (async-backup-test-normalize-command
                captured)))))"##,
        expect![[
            r#"OK (:started t "[ORACLE-SANDBOX]/symlink/real/source.txt" ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//symlink/alias/source-link.txt\" \"$ROOT//symlink/backups$ROOT//symlink/alias/source-link-LINK.txt\")"))"#
        ]],
    )
}

pub(super) fn paths_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_backup_explicit_file_builds_exact_timestamped_output_tree_and_child_command(),
        async_backup_file_without_extension_keeps_complete_base_name(),
        async_backup_multi_extension_dotfile_unicode_and_quoted_names_are_escaped_exactly(),
        async_backup_without_argument_uses_current_buffer_visited_file(),
        async_backup_relative_file_is_expanded_against_current_default_directory(),
        async_backup_relative_location_is_expanded_before_appending_source_directory(),
        async_backup_location_with_or_without_trailing_separator_produces_same_target(),
        async_backup_non_file_buffer_signals_before_creating_backup_tree_or_process(),
        async_backup_output_parent_collision_signals_before_predicates_or_process(),
        async_backup_directory_input_still_builds_a_child_copy_command(),
        async_backup_missing_input_still_constructs_target_and_launches_child(),
        async_backup_symlink_path_is_preserved_in_predicate_and_child_command(),
    ]
}
