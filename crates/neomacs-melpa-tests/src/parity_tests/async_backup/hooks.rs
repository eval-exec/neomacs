use expect_test::expect;

use super::ParityBatchCase;

fn async_backup_global_after_save_hook_backs_up_the_content_just_written_to_disk() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_global_after_save_hook_backs_up_the_content_just_written_to_disk",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-global/project/notes.org"
                 "* original\n"))
               (root
                (async-backup-test-path
                 "hooks-global/backups"))
               (output
                (async-backup-test-output-file
                 root file "GLOBAL"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               process)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" file)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (add-hook 'after-save-hook #'async-backup)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (erase-buffer)
                (insert "* saved through the real hook\n")
                (cl-letf (((symbol-function 'format-time-string)
                           (lambda (&rest _) "GLOBAL")))
                  (save-buffer))
                (setq process (get-process "async-backup"))
                (async-backup-test-wait process)
                (list
                 (memq #'async-backup after-save-hook)
                 (buffer-modified-p)
                 (async-backup-test-read-file file)
                 (process-status process)
                 (process-exit-status process)
                 (file-exists-p output)
                 (async-backup-test-read-file output)))
            (remove-hook 'after-save-hook #'async-backup)
            (async-backup-test-kill-file-buffer file)
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK ((async-backup) nil "* saved through the real hook\n" exit 0 t "* saved through the real hook\n")"#
        ]],
    )
}

fn async_backup_local_after_save_hook_returns_from_save_while_child_is_gated() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_local_after_save_hook_returns_from_save_while_child_is_gated",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-async/input.txt"
                 "before\n"))
               (root
                (async-backup-test-path
                 "hooks-async/backups"))
               (gate
                (async-backup-test-path
                 "hooks-async/release"))
               (output
                (async-backup-test-output-file
                 root file "LATER"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               process)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" file)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (add-hook
                 'after-save-hook
                 (lambda ()
                   (setq process (async-backup)))
                 nil
                 t)
                (goto-char (point-max))
                (insert "saved before child exits\n")
                (cl-letf (((symbol-function 'format-time-string)
                           (lambda (&rest _) "LATER")))
                  (save-buffer))
                (let ((before-release
                       (list
                        (buffer-modified-p)
                        (processp process)
                        (process-live-p process)
                        (file-exists-p output)
                        (async-backup-test-read-file file))))
                  (async-backup-test-write-file
                   "hooks-async/release"
                   "continue\n")
                  (async-backup-test-wait process)
                  (list
                   before-release
                   (process-status process)
                   (process-exit-status process)
                   (file-exists-p output)
                   (async-backup-test-read-file output))))
            (async-backup-test-kill-file-buffer file)
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK ((nil t (run open listen connect stop) nil "before\nsaved before child exits\n") exit 0 t "before\nsaved before child exits\n")"#
        ]],
    )
}

fn async_backup_buffer_local_hook_runs_only_for_the_buffer_where_it_was_added() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_buffer_local_hook_runs_only_for_the_buffer_where_it_was_added",
        r##"(let* ((file-a
                (async-backup-test-write-file
                 "hooks-local/a.txt"
                 "A0\n"))
               (file-b
                (async-backup-test-write-file
                 "hooks-local/b.txt"
                 "B0\n"))
               (async-backup-location
                (async-backup-test-path
                 "hooks-local/backups"))
               (make-backup-files nil)
               (create-lockfiles nil)
               launches)
          (unwind-protect
              (cl-letf
                  (((symbol-function 'format-time-string)
                    (lambda (&rest _) "LOCAL"))
                   ((symbol-function 'start-process)
                    (lambda (&rest command)
                      (push
                       (list
                        (file-name-nondirectory
                         (buffer-file-name))
                        (async-backup-test-normalize-command
                         command))
                       launches)
                      :child)))
                (with-current-buffer (find-file-noselect file-a)
                  (add-hook
                   'after-save-hook
                   #'async-backup
                   nil
                   t)
                  (goto-char (point-max))
                  (insert "A1\n")
                  (save-buffer))
                (with-current-buffer (find-file-noselect file-b)
                  (goto-char (point-max))
                  (insert "B1\n")
                  (save-buffer))
                (list
                 (nreverse launches)
                 (with-current-buffer
                     (find-buffer-visiting file-a)
                   (memq #'async-backup after-save-hook))
                 (with-current-buffer
                     (find-buffer-visiting file-b)
                   (memq #'async-backup after-save-hook))
                 (async-backup-test-read-file file-a)
                 (async-backup-test-read-file file-b)))
            (async-backup-test-kill-file-buffer file-a)
            (async-backup-test-kill-file-buffer file-b)))"##,
        expect![[
            r#"OK ((("a.txt" ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//hooks-local/a.txt\" \"$ROOT//hooks-local/backups$ROOT//hooks-local/a-LOCAL.txt\")"))) (async-backup t) nil "A0\nA1\n" "B0\nB1\n")"#
        ]],
    )
}

fn async_backup_removing_local_hook_prevents_launch_and_backup_tree_creation() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_removing_local_hook_prevents_launch_and_backup_tree_creation",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-remove/input.txt"
                 "old\n"))
               (root
                (async-backup-test-path
                 "hooks-remove/backups"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               launches)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (add-hook
                 'after-save-hook
                 #'async-backup
                 nil
                 t)
                (let ((present-before
                       (memq #'async-backup
                             after-save-hook)))
                  (remove-hook
                   'after-save-hook
                   #'async-backup
                   t)
                  (goto-char (point-max))
                  (insert "new\n")
                  (cl-letf
                      (((symbol-function 'start-process)
                        (lambda (&rest command)
                          (push command launches)
                          :unexpected)))
                    (save-buffer))
                  (list
                   present-before
                   (memq #'async-backup
                         after-save-hook)
                   launches
                   (file-exists-p root)
                   (async-backup-test-read-file file))))
            (async-backup-test-kill-file-buffer file)))"##,
        expect![[r#"OK ((async-backup t) nil nil nil "old\nnew\n")"#]],
    )
}

fn async_backup_two_real_saves_create_immutable_versioned_backups() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_two_real_saves_create_immutable_versioned_backups",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-versions/project/state.el"
                 "(setq state 0)\n"))
               (root
                (async-backup-test-path
                 "hooks-versions/backups"))
               (output-one
                (async-backup-test-output-file
                 root file "ONE"))
               (output-two
                (async-backup-test-output-file
                 root file "TWO"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               process
               stamps)
          (async-backup-test-install-emacs-stub)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (add-hook
                 'after-save-hook
                 (lambda ()
                   (setq process (async-backup)))
                 nil
                 t)
                (setq stamps '("ONE" "TWO"))
                (cl-letf (((symbol-function 'format-time-string)
                           (lambda (&rest _)
                             (pop stamps))))
                  (erase-buffer)
                  (insert "(setq state 1)\n")
                  (setenv "ASYNC_BACKUP_TEST_INPUT" file)
                  (setenv "ASYNC_BACKUP_TEST_OUTPUT"
                          output-one)
                  (save-buffer)
                  (async-backup-test-wait process)
                  (erase-buffer)
                  (insert "(setq state 2)\n")
                  (setenv "ASYNC_BACKUP_TEST_OUTPUT"
                          output-two)
                  (save-buffer)
                  (async-backup-test-wait process))
                (list
                 (async-backup-test-read-file file)
                 (async-backup-test-read-file output-one)
                 (async-backup-test-read-file output-two)
                 (equal
                  (async-backup-test-read-file output-one)
                  "(setq state 1)\n")
                 (equal
                  (async-backup-test-read-file output-two)
                  "(setq state 2)\n")
                 stamps))
            (async-backup-test-kill-file-buffer file)
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[r#"OK ("(setq state 2)\n" "(setq state 1)\n" "(setq state 2)\n" t t nil)"#]],
    )
}

fn async_backup_repeated_timestamp_reports_collision_and_preserves_first_saved_version()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_repeated_timestamp_reports_collision_and_preserves_first_saved_version",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-collision/project/state.txt"
                 "zero\n"))
               (root
                (async-backup-test-path
                 "hooks-collision/backups"))
               (output
                (async-backup-test-output-file
                 root file "SAME"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               process
               statuses)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" file)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (add-hook
                 'after-save-hook
                 (lambda ()
                   (setq process (async-backup)))
                 nil
                 t)
                (cl-letf (((symbol-function 'format-time-string)
                           (lambda (&rest _) "SAME")))
                  (erase-buffer)
                  (insert "first\n")
                  (save-buffer)
                  (async-backup-test-wait process)
                  (push (process-exit-status process)
                        statuses)
                  (erase-buffer)
                  (insert "second\n")
                  (save-buffer)
                  (async-backup-test-wait process)
                  (push (process-exit-status process)
                        statuses))
                (list
                 (nreverse statuses)
                 (async-backup-test-read-file file)
                 (async-backup-test-read-file output)
                 (with-current-buffer
                     (get-buffer "*async-backup*")
                   (and
                    (string-match-p
                     "backup collision"
                     (buffer-string))
                    t))))
            (async-backup-test-kill-file-buffer file)
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[r#"OK ((0 73) "second\n" "first\n" t)"#]],
    )
}

fn async_backup_child_outlives_visiting_buffer_and_copies_the_saved_disk_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_child_outlives_visiting_buffer_and_copies_the_saved_disk_file",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-kill/project/input.md"
                 "old\n"))
               (root
                (async-backup-test-path
                 "hooks-kill/backups"))
               (gate
                (async-backup-test-path
                 "hooks-kill/release"))
               (output
                (async-backup-test-output-file
                 root file "DETACHED"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               buffer
               process)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" file)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (unwind-protect
              (progn
                (setq buffer (find-file-noselect file))
                (with-current-buffer buffer
                  (add-hook
                   'after-save-hook
                   (lambda ()
                     (setq process (async-backup)))
                   nil
                   t)
                  (erase-buffer)
                  (insert "saved before buffer teardown\n")
                  (cl-letf
                      (((symbol-function
                         'format-time-string)
                        (lambda (&rest _) "DETACHED")))
                    (save-buffer)))
                (async-backup-test-kill-buffer buffer)
                (let ((before-release
                       (list
                        (buffer-live-p buffer)
                        (process-live-p process)
                        (file-exists-p output))))
                  (async-backup-test-write-file
                   "hooks-kill/release"
                   "continue\n")
                  (async-backup-test-wait process)
                  (list
                   before-release
                   (process-status process)
                   (process-exit-status process)
                   (async-backup-test-read-file file)
                   (async-backup-test-read-file output))))
            (async-backup-test-kill-buffer buffer)
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK ((nil (run open listen connect stop) nil) exit 0 "saved before buffer teardown\n" "saved before buffer teardown\n")"#
        ]],
    )
}

fn async_backup_false_predicate_does_not_break_save_but_still_creates_output_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_false_predicate_does_not_break_save_but_still_creates_output_directory",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-predicate/project/input.log"
                 "old\n"))
               (root
                (async-backup-test-path
                 "hooks-predicate/backups"))
               (expected-directory
                (concat
                 (directory-file-name root)
                 (file-name-directory file)))
               (async-backup-location root)
               (async-backup-predicates
                (list
                 (lambda (candidate)
                   (and
                    (string-suffix-p ".txt" candidate)
                    (file-readable-p candidate)))))
               (make-backup-files nil)
               (create-lockfiles nil)
               launches
               hook-result)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (add-hook
                 'after-save-hook
                 (lambda ()
                   (setq hook-result
                         (async-backup)))
                 nil
                 t)
                (erase-buffer)
                (insert "saved although filtered\n")
                (cl-letf
                    (((symbol-function 'start-process)
                      (lambda (&rest command)
                        (push command launches)
                        :unexpected)))
                  (list
                   (async-backup-test-error-data
                    #'save-buffer)
                   hook-result
                   launches
                   (file-directory-p
                    expected-directory)
                   (async-backup-test-read-file file))))
            (async-backup-test-kill-file-buffer file)))"##,
        expect![[r#"OK ((:ok nil) nil nil t "saved although filtered\n")"#]],
    )
}

fn async_backup_child_failure_is_observed_later_without_turning_save_into_an_error()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_child_failure_is_observed_later_without_turning_save_into_an_error",
        r##"(let* ((file
                (async-backup-test-write-file
                 "hooks-child-failure/project/input.txt"
                 "old\n"))
               (root
                (async-backup-test-path
                 "hooks-child-failure/backups"))
               (async-backup-location root)
               (make-backup-files nil)
               (create-lockfiles nil)
               process
               save-result)
          (async-backup-test-install-emacs-stub
           "printf '%s\\n' 'intentional child failure' >&2\nexit 42")
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (add-hook
                 'after-save-hook
                 (lambda ()
                   (setq process (async-backup)))
                 nil
                 t)
                (erase-buffer)
                (insert "new content is still saved\n")
                (cl-letf (((symbol-function 'format-time-string)
                           (lambda (&rest _) "FAIL")))
                  (setq save-result
                        (async-backup-test-error-data
                         #'save-buffer)))
                (async-backup-test-wait process)
                (list
                 save-result
                 (buffer-modified-p)
                 (async-backup-test-read-file file)
                 (process-status process)
                 (process-exit-status process)
                 (with-current-buffer
                     (process-buffer process)
                   (and
                    (string-match-p
                     "intentional child failure"
                     (buffer-string))
                    t))))
            (async-backup-test-kill-file-buffer file)
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[r#"OK ((:ok nil) nil "new content is still saved\n" exit 42 t)"#]],
    )
}

pub(super) fn hooks_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_backup_global_after_save_hook_backs_up_the_content_just_written_to_disk(),
        async_backup_local_after_save_hook_returns_from_save_while_child_is_gated(),
        async_backup_buffer_local_hook_runs_only_for_the_buffer_where_it_was_added(),
        async_backup_removing_local_hook_prevents_launch_and_backup_tree_creation(),
        async_backup_two_real_saves_create_immutable_versioned_backups(),
        async_backup_repeated_timestamp_reports_collision_and_preserves_first_saved_version(),
        async_backup_child_outlives_visiting_buffer_and_copies_the_saved_disk_file(),
        async_backup_false_predicate_does_not_break_save_but_still_creates_output_directory(),
        async_backup_child_failure_is_observed_later_without_turning_save_into_an_error(),
    ]
}
