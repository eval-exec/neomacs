use expect_test::expect;

use super::ParityBatchCase;

fn async_backup_real_child_copies_content_with_exact_process_command_and_status() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_real_child_copies_content_with_exact_process_command_and_status",
        r##"(let* ((input
                (async-backup-test-write-file
                 "process-success/input.txt"
                 "saved content\n"))
               (root
                (async-backup-test-path
                 "process-success/backups"))
               (stamp "2026-07-27T13-30-00")
               (output
                (async-backup-test-output-file
                 root input stamp))
               (async-backup-location root)
               process)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" input)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) stamp)))
            (setq process (async-backup input)))
          (unwind-protect
              (progn
                (async-backup-test-wait process)
                (list
                 (processp process)
                 (process-name process)
                 (process-status process)
                 (process-exit-status process)
                 (async-backup-test-normalize-command
                  (process-command process))
                 (equal
                  (process-buffer process)
                  (get-buffer "*async-backup*"))
                 (with-current-buffer
                     (process-buffer process)
                   (string-match-p
                    "copied:input.txt"
                    (buffer-string)))
                 (file-exists-p output)
                 (async-backup-test-read-file output)))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK (t "async-backup" exit 0 ("emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//process-success/input.txt\" \"$ROOT//process-success/backups$ROOT//process-success/input-2026-07-27T13-30-00.txt\")") t 0 t "saved content\n")"#
        ]],
    )
}

fn async_backup_returns_before_gate_opens_and_child_finishes_later() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_returns_before_gate_opens_and_child_finishes_later",
        r##"(let* ((input
                (async-backup-test-write-file
                 "process-async/input.org"
                 "* asynchronous\n"))
               (root
                (async-backup-test-path
                 "process-async/backups"))
               (gate
                (async-backup-test-path
                 "process-async/release-gate"))
               (output
                (async-backup-test-output-file
                 root input "ASYNC"))
               (async-backup-location root)
               process)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" input)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "ASYNC")))
            (setq process (async-backup input)))
          (unwind-protect
              (let ((before
                     (list
                      (process-live-p process)
                      (file-exists-p output)
                      :editor-continued)))
                (async-backup-test-write-file
                 "process-async/release-gate"
                 "go\n")
                (async-backup-test-wait process)
                (list
                 before
                 (process-status process)
                 (process-exit-status process)
                 (file-exists-p output)
                 (async-backup-test-read-file output)))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK (((run open listen connect stop) nil :editor-continued) exit 0 t "* asynchronous\n")"#
        ]],
    )
}

fn async_backup_multiple_calls_launch_concurrent_children_instead_of_queueing() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_multiple_calls_launch_concurrent_children_instead_of_queueing",
        r##"(let* ((root
                (async-backup-test-path
                 "process-concurrent/backups"))
               (gate
                (async-backup-test-path
                 "process-concurrent/gate"))
               (async-backup-location root)
               processes
               outputs)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (dolist (name '("one.txt" "two.txt" "three.txt"))
            (let* ((input
                    (async-backup-test-write-file
                     (concat
                      "process-concurrent/source/"
                      name)
                     (concat "content-" name "\n")))
                   (stamp
                    (upcase
                     (file-name-base name)))
                   (output
                    (async-backup-test-output-file
                     root input stamp)))
              (setenv "ASYNC_BACKUP_TEST_INPUT" input)
              (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
              (cl-letf (((symbol-function 'format-time-string)
                         (lambda (&rest _) stamp)))
                (push (async-backup input) processes))
              (push output outputs)))
          (setq processes (nreverse processes)
                outputs (nreverse outputs))
          (unwind-protect
              (let ((before
                     (list
                      (mapcar #'process-name processes)
                      (mapcar #'process-live-p processes)
                      (length
                       (delete-dups
                        (copy-sequence processes)))
                      (mapcar #'file-exists-p outputs))))
                (async-backup-test-write-file
                 "process-concurrent/gate"
                 "go\n")
                (mapc #'async-backup-test-wait processes)
                (list
                 before
                 (mapcar #'process-exit-status processes)
                 (mapcar #'async-backup-test-read-file outputs)))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK ((("async-backup" "async-backup<1>" "async-backup<2>") (#1=(run open listen connect stop) #1# #1#) 3 (nil nil nil)) (0 0 0) ("content-one.txt\n" "content-two.txt\n" "content-three.txt\n"))"#
        ]],
    )
}

fn async_backup_same_timestamp_collision_leaves_one_success_and_one_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_same_timestamp_collision_leaves_one_success_and_one_failure",
        r##"(let* ((input
                (async-backup-test-write-file
                 "process-collision/input.txt"
                 "collision content\n"))
               (root
                (async-backup-test-path
                 "process-collision/backups"))
               (gate
                (async-backup-test-path
                 "process-collision/gate"))
               (output
                (async-backup-test-output-file
                 root input "SAME"))
               (async-backup-location root)
               processes)
          (async-backup-test-install-emacs-stub
           (concat
            "while [ ! -e \"$ASYNC_BACKUP_TEST_GATE\" ]; do :; done\n"
            "if (set -C; : > \"$ASYNC_BACKUP_TEST_OUTPUT\") 2>/dev/null; then\n"
            "  cp -- \"$ASYNC_BACKUP_TEST_INPUT\" \"$ASYNC_BACKUP_TEST_OUTPUT\"\n"
            "  printf '%s\\n' copied\n"
            "else\n"
            "  printf '%s\\n' 'backup collision' >&2\n"
            "  exit 73\n"
            "fi"))
          (setenv "ASYNC_BACKUP_TEST_INPUT" input)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "SAME")))
            (setq processes
                  (list
                   (async-backup input)
                   (async-backup input))))
          (unwind-protect
              (progn
                (async-backup-test-write-file
                 "process-collision/gate"
                 "go\n")
                (mapc #'async-backup-test-wait processes)
                (list
                 (mapcar #'process-name processes)
                 (sort
                  (mapcar #'process-exit-status processes)
                  #'<)
                 (file-exists-p output)
                 (async-backup-test-read-file output)
                 (with-current-buffer
                     (get-buffer "*async-backup*")
                   (and
                    (string-match-p
                     "backup collision"
                     (buffer-string))
                    t))))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[r#"OK (("async-backup" "async-backup<1>") (0 73) t "collision content\n" t)"#]],
    )
}

fn async_backup_missing_input_is_reported_by_child_without_creating_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_missing_input_is_reported_by_child_without_creating_output",
        r##"(let* ((input
                (async-backup-test-path
                 "process-missing/input.txt"))
               (root
                (async-backup-test-path
                 "process-missing/backups"))
               (output
                (async-backup-test-output-file
                 root input "MISS"))
               (async-backup-location root)
               process)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" input)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "MISS")))
            (setq process (async-backup input)))
          (unwind-protect
              (progn
                (async-backup-test-wait process)
                (list
                 (file-exists-p input)
                 (process-status process)
                 (process-exit-status process)
                 (file-exists-p output)
                 (with-current-buffer
                     (process-buffer process)
                   (and
                    (string-match-p
                     "cannot stat"
                     (buffer-string))
                    t))))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect!["OK (nil exit 1 nil t)"],
    )
}

fn async_backup_missing_emacs_executable_signals_after_creating_output_directory() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_missing_emacs_executable_signals_after_creating_output_directory",
        r##"(let* ((input
                (async-backup-test-write-file
                 "process-no-emacs/input.txt"
                 "input\n"))
               (root
                (async-backup-test-path
                 "process-no-emacs/backups"))
               (async-backup-location root)
               (exec-path nil)
               (process-environment
                (cons "PATH="
                      process-environment)))
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "NONE")))
            (list
             (async-backup-test-error-data
              (lambda ()
                (async-backup input)))
             (file-directory-p
              (concat
               (directory-file-name root)
               (file-name-directory input)))
             (get-buffer "*async-backup*"))))"##,
        expect![[
            r#"OK ((:error file-missing ("Searching for program" "No such file or directory" "emacs")) t (:buffer "*async-backup*"))"#
        ]],
    )
}

fn async_backup_child_stdout_and_stderr_share_persistent_process_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_child_stdout_and_stderr_share_persistent_process_buffer",
        r##"(let* ((input
                (async-backup-test-write-file
                 "process-output/input.txt"
                 "input\n"))
               (root
                (async-backup-test-path
                 "process-output/backups"))
               (async-backup-location root)
               process
               buffer)
          (async-backup-test-install-emacs-stub
           "printf '%s\\n' stdout-line\nprintf '%s\\n' stderr-line >&2")
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "OUT")))
            (setq process (async-backup input)))
          (setq buffer (process-buffer process))
          (unwind-protect
              (progn
                (async-backup-test-wait process)
                (list
                 (buffer-live-p buffer)
                 (process-status process)
                 (process-exit-status process)
                 (with-current-buffer buffer
                   (let ((text (buffer-string)))
                     (list
                      (string-match-p "stdout-line" text)
                      (string-match-p "stderr-line" text))))
                 (buffer-live-p buffer)))
            (async-backup-test-kill-buffer buffer)))"##,
        expect!["OK (t exit 0 (0 12) t)"],
    )
}

fn async_backup_sequential_children_reuse_named_output_buffer_and_append_results() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_sequential_children_reuse_named_output_buffer_and_append_results",
        r##"(let* ((root
                (async-backup-test-path
                 "process-buffer/backups"))
               (async-backup-location root)
               processes)
          (async-backup-test-install-emacs-stub)
          (dolist (entry
                   '(("first.txt" "FIRST")
                     ("second.txt" "SECOND")))
            (let* ((input
                    (async-backup-test-write-file
                     (concat
                      "process-buffer/source/"
                      (car entry))
                     (concat (cadr entry) "\n")))
                   (output
                    (async-backup-test-output-file
                     root input (cadr entry))))
              (setenv "ASYNC_BACKUP_TEST_INPUT" input)
              (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
              (cl-letf (((symbol-function 'format-time-string)
                         (lambda (&rest _) (cadr entry))))
                (let ((process (async-backup input)))
                  (async-backup-test-wait process)
                  (push process processes)))))
          (setq processes (nreverse processes))
          (unwind-protect
              (let ((buffer (get-buffer "*async-backup*")))
                (list
                 (mapcar #'process-name processes)
                 (mapcar
                  (lambda (process)
                    (eq (process-buffer process)
                        buffer))
                  processes)
                 (with-current-buffer buffer
                   (let ((text (buffer-string)))
                     (list
                      (string-match-p
                       "copied:first.txt"
                       text)
                      (string-match-p
                       "copied:second.txt"
                       text))))))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[r#"OK (("async-backup" "async-backup") (t t) (0 48))"#]],
    )
}

fn async_backup_process_environment_is_snapshotted_independently_per_launch() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_process_environment_is_snapshotted_independently_per_launch",
        r##"(let* ((root
                (async-backup-test-path
                 "process-env/backups"))
               (gate
                (async-backup-test-path
                 "process-env/gate"))
               (async-backup-location root)
               processes
               expected)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (dolist (name '("alpha" "beta"))
            (let* ((input
                    (async-backup-test-write-file
                     (concat
                      "process-env/source/"
                      name)
                     (concat name "\n")))
                   (output
                    (async-backup-test-output-file
                     root input name)))
              (setenv "ASYNC_BACKUP_TEST_INPUT" input)
              (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
              (cl-letf (((symbol-function 'format-time-string)
                         (lambda (&rest _) name)))
                (push (async-backup input) processes))
              (push (cons output name) expected)))
          (setq processes (nreverse processes)
                expected (nreverse expected))
          (unwind-protect
              (progn
                (async-backup-test-write-file
                 "process-env/gate"
                 "go\n")
                (mapc #'async-backup-test-wait processes)
                (list
                 (mapcar #'process-exit-status processes)
                 (mapcar
                  (lambda (entry)
                    (list
                     (file-exists-p (car entry))
                     (async-backup-test-read-file
                      (car entry))
                     (cdr entry)))
                  expected)))
            (async-backup-test-kill-buffer
             (get-buffer "*async-backup*"))))"##,
        expect![[r#"OK ((0 0) ((t "alpha\n" "alpha") (t "beta\n" "beta")))"#]],
    )
}

fn async_backup_caller_can_explicitly_delete_running_child_and_output_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_caller_can_explicitly_delete_running_child_and_output_buffer",
        r##"(let* ((input
                (async-backup-test-write-file
                 "process-cleanup/input.txt"
                 "input\n"))
               (root
                (async-backup-test-path
                 "process-cleanup/backups"))
               (gate
                (async-backup-test-path
                 "process-cleanup/gate"))
               (output
                (async-backup-test-output-file
                 root input "CLEAN"))
               (async-backup-location root)
               process
               buffer)
          (async-backup-test-install-emacs-stub)
          (setenv "ASYNC_BACKUP_TEST_INPUT" input)
          (setenv "ASYNC_BACKUP_TEST_OUTPUT" output)
          (setenv "ASYNC_BACKUP_TEST_GATE" gate)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "CLEAN")))
            (setq process (async-backup input)))
          (setq buffer (process-buffer process))
          (let ((before
                 (list
                  (process-live-p process)
                  (buffer-live-p buffer)
                  (file-exists-p output))))
            (delete-process process)
            (async-backup-test-kill-buffer buffer)
            (list
             before
             (process-live-p process)
             (process-status process)
             (buffer-live-p buffer)
             (file-exists-p output))))"##,
        expect!["OK (((run open listen connect stop) t nil) nil signal nil nil)"],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_backup_real_child_copies_content_with_exact_process_command_and_status(),
        async_backup_returns_before_gate_opens_and_child_finishes_later(),
        async_backup_multiple_calls_launch_concurrent_children_instead_of_queueing(),
        async_backup_same_timestamp_collision_leaves_one_success_and_one_failure(),
        async_backup_missing_input_is_reported_by_child_without_creating_output(),
        async_backup_missing_emacs_executable_signals_after_creating_output_directory(),
        async_backup_child_stdout_and_stderr_share_persistent_process_buffer(),
        async_backup_sequential_children_reuse_named_output_buffer_and_append_results(),
        async_backup_process_environment_is_snapshotted_independently_per_launch(),
        async_backup_caller_can_explicitly_delete_running_child_and_output_buffer(),
    ]
}
