use expect_test::expect;

use super::ParityBatchCase;

fn live_subprocess_annotations_report_status_buffer_thread_command_and_identifier_shapes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "live_subprocess_annotations_report_status_buffer_thread_command_and_identifier_shapes",
        r##"(let* ((buffer
                     (generate-new-buffer
                      " *all-the-icons-ivy-rich-process*"))
                    (process
                     (make-process
                      :name "all-the-icons-ivy-rich-process"
                      :buffer buffer
                      :command '("sh" "-c" "sleep 30")
                      :noquery t))
                    result)
               (unwind-protect
                   (progn
                     (while
                         (eq
                          (process-status process)
                          'connect)
                       (accept-process-output process 0.01))
                     (let ((identifier
                            (all-the-icons-ivy-rich-process-id
                             (process-name process)))
                           (status
                            (all-the-icons-ivy-rich-process-status
                             (process-name process)))
                           (tty
                            (all-the-icons-ivy-rich-process-tty-name
                             (process-name process))))
                       (setq
                        result
                        (list
                         (and
                          (stringp identifier)
                          (string-match-p
                           "\\`[0-9]+\\'"
                           identifier)
                          t)
                         (list
                          (substring-no-properties status)
                          (get-text-property 0 'face status))
                         (all-the-icons-ivy-rich-process-buffer-name
                          (process-name process))
                         (and
                          (stringp tty)
                          (string-match-p
                           "\\`/dev/pts/[0-9]+\\'"
                           tty)
                          t)
                         (all-the-icons-ivy-rich-process-thread
                          (process-name process))
                         (all-the-icons-ivy-rich-process-command
                          (process-name process))))))
                 (when (process-live-p process)
                   (delete-process process))
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer)))
               result)"##,
        expect![[
            r#"OK (t ("run" all-the-icons-ivy-rich-process-status-face) " *all-the-icons-ivy-rich-process*" t #("Main        " 0 12 (face all-the-icons-ivy-rich-process-thread-face)) "sh -c sleep 30")"#
        ]],
    )
}

fn pipe_and_missing_process_annotations_cover_non_child_and_absent_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "pipe_and_missing_process_annotations_cover_non_child_and_absent_candidates",
        r##"(let* ((buffer
                     (generate-new-buffer
                      " *all-the-icons-ivy-rich-pipe*"))
                    (process
                     (make-pipe-process
                      :name "all-the-icons-ivy-rich-pipe"
                      :buffer buffer
                      :noquery t))
                    result)
               (unwind-protect
                   (setq
                    result
                    (list
                     (list
                      (all-the-icons-ivy-rich-process-id
                       (process-name process))
                      (all-the-icons-ivy-rich-process-status
                       (process-name process))
                      (all-the-icons-ivy-rich-process-buffer-name
                       (process-name process))
                      (all-the-icons-ivy-rich-process-tty-name
                       (process-name process))
                      (all-the-icons-ivy-rich-process-command
                       (process-name process)))
                     (mapcar
                      (lambda (function)
                        (funcall
                         function
                         "all-the-icons-ivy-rich-no-process"))
                      '(all-the-icons-ivy-rich-process-id
                        all-the-icons-ivy-rich-process-status
                        all-the-icons-ivy-rich-process-buffer-name
                        all-the-icons-ivy-rich-process-tty-name
                        all-the-icons-ivy-rich-process-command))))
                 (when (process-live-p process)
                   (delete-process process))
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer)))
               result)"##,
        expect![[
            r#"OK (("--" #("open" 0 4 (face all-the-icons-ivy-rich-process-status-face)) " *all-the-icons-ivy-rich-pipe*" "--" "(serial port ?)") (nil nil nil nil nil))"#
        ]],
    )
}

pub(super) fn processes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        live_subprocess_annotations_report_status_buffer_thread_command_and_identifier_shapes(),
        pipe_and_missing_process_annotations_cover_non_child_and_absent_candidates(),
    ]
}
