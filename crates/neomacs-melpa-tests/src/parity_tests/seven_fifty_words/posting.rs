use expect_test::expect;

use super::ParityBatchCase;

fn seven_fifty_words_file_starts_formatted_command_and_installs_live_process_sentinel()
-> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_file_starts_formatted_command_and_installs_live_process_sentinel",
        r##"(let ((750words-client-command
                    "client --input=%s")
                   events)
               (cl-letf
                   (((symbol-function
                      'generate-new-buffer)
                     (lambda (name)
                       (push
                        (list 'generate name)
                        events)
                       'output-buffer))
                    ((symbol-function
                      'async-shell-command)
                     (lambda (command buffer)
                       (push
                        (list
                         'async command buffer)
                        events)))
                    ((symbol-function
                      'get-buffer-process)
                     (lambda (buffer)
                       (push
                        (list
                         'get-process buffer)
                        events)
                       'client-process))
                    ((symbol-function
                      'process-live-p)
                     (lambda (process)
                       (push
                        (list 'live process)
                        events)
                       t))
                    ((symbol-function
                      '750words--post-process-fn)
                     (lambda (&rest args)
                       (push
                        (cons 'callback args)
                        events)))
                    ((symbol-function
                      'set-process-sentinel)
                     (lambda (process sentinel)
                       (push
                        (list
                         'sentinel process
                         (functionp sentinel))
                        events)
                       (funcall
                        sentinel process "done")
                       'installed)))
                 (list
                  (750words-file
                   "draft with spaces.txt")
                  (nreverse events))))"##,
        expect![[
            r#"OK (installed ((generate "*750words-client-command*") (async "client --input=draft with spaces.txt" output-buffer) (get-process output-buffer) (live client-process) (sentinel client-process t) (callback output-buffer client-process "done")))"#
        ]],
    )
}

fn seven_fifty_words_file_reports_non_live_process_without_installing_sentinel() -> ParityBatchCase
{
    ParityBatchCase::value(
        "seven_fifty_words_file_reports_non_live_process_without_installing_sentinel",
        r##"(let ((750words-client-command
                    "post %s")
                   events)
               (cl-letf
                   (((symbol-function
                      'generate-new-buffer)
                     (lambda (_) 'output-buffer))
                    ((symbol-function
                      'async-shell-command)
                     (lambda (&rest _)))
                    ((symbol-function
                      'get-buffer-process)
                     (lambda (_) 'dead-process))
                    ((symbol-function
                      'process-live-p)
                     (lambda (_) nil))
                    ((symbol-function
                      'set-process-sentinel)
                     (lambda (&rest _)
                       (error
                        "sentinel should not be installed")))
                    ((symbol-function 'message)
                     (lambda (format-string
                              &rest args)
                       (let ((text
                              (apply
                               #'format
                               format-string
                               args)))
                         (push text events)
                         text))))
                 (list
                  (750words-file "draft.txt")
                  (nreverse events))))"##,
        expect![[
            r#"OK ("Running 'post draft.txt' failed." ("Running 'post draft.txt' failed."))"#
        ]],
    )
}

fn seven_fifty_words_post_process_handles_exit_and_signal_in_exact_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_post_process_handles_exit_and_signal_in_exact_order",
        r##"(let ((statuses '(exit signal))
                   events)
               (cl-letf
                   (((symbol-function
                      'process-status)
                     (lambda (process)
                       (push
                        (list 'status process)
                        events)
                       (prog1
                           (car statuses)
                         (setq statuses
                               (cdr statuses)))))
                    ((symbol-function
                      'switch-to-buffer-other-window)
                     (lambda (buffer)
                       (push
                        (list 'switch buffer)
                        events)))
                    ((symbol-function
                      'special-mode)
                     (lambda ()
                       (push 'special events)))
                    ((symbol-function
                      'shell-command-sentinel)
                     (lambda (process signal)
                       (push
                        (list
                         'shell process signal)
                        events)
                       'handled)))
                 (list
                  (750words--post-process-fn
                   'first-output
                   'first-process
                   "finished\n")
                  (750words--post-process-fn
                   'second-output
                   'second-process
                   "killed\n")
                  (nreverse events))))"##,
        expect![[
            r#"OK (handled handled ((status first-process) (switch first-output) special (shell first-process "finished\n") (status second-process) (switch second-output) special (shell second-process "killed\n")))"#
        ]],
    )
}

fn seven_fifty_words_post_process_ignores_running_and_stopped_processes() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_post_process_ignores_running_and_stopped_processes",
        r##"(let ((statuses '(run stop))
                   events)
               (cl-letf
                   (((symbol-function
                      'process-status)
                     (lambda (_)
                       (prog1
                           (car statuses)
                         (setq statuses
                               (cdr statuses)))))
                    ((symbol-function
                      'switch-to-buffer-other-window)
                     (lambda (&rest _)
                       (push 'switch events)))
                    ((symbol-function
                      'special-mode)
                     (lambda ()
                       (push 'special events)))
                    ((symbol-function
                      'shell-command-sentinel)
                     (lambda (&rest _)
                       (push 'shell events))))
                 (list
                  (750words--post-process-fn
                   'output 'process "running")
                  (750words--post-process-fn
                   'output 'process "stopped")
                  events)))"##,
        expect!["OK (nil nil nil)"],
    )
}

fn seven_fifty_words_region_writes_exact_bounds_then_posts_the_sandbox_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_region_writes_exact_bounds_then_posts_the_sandbox_file",
        r##"(let ((file
                    (expand-file-name
                     "region.txt"
                     (getenv "TMPDIR")))
                   observed)
               (unwind-protect
                   (with-temp-buffer
                     (insert "zero ONE two")
                     (cl-letf
                         (((symbol-function
                            'make-temp-file)
                           (lambda (prefix)
                             (setq observed
                                   (list
                                    'prefix
                                    prefix))
                             file))
                          ((symbol-function
                            '750words-file)
                           (lambda (path)
                             (setq observed
                                   (append
                                    observed
                                    (list
                                     'post path)))
                             (with-temp-buffer
                               (insert-file-contents
                                path)
                               (list
                                'posted
                                (buffer-string))))))
                       (list
                        (750words-region 6 9)
                        observed
                        (file-exists-p file))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK ((posted "ONE") (prefix "750words" post "[ORACLE-TMPDIR]/region.txt") t)"#]],
    )
}

fn seven_fifty_words_buffer_forwards_current_minimum_and_maximum() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_buffer_forwards_current_minimum_and_maximum",
        r##"(with-temp-buffer
               (insert "payload")
               (narrow-to-region 2 7)
               (let (observed)
                 (cl-letf
                     (((symbol-function
                        '750words-region)
                       (lambda (start end)
                         (setq observed
                               (list start end))
                         'posted)))
                   (list
                    (750words-buffer)
                    observed
                    (point-min)
                    (point-max)))))"##,
        expect!["OK (posted (2 7) 2 7)"],
    )
}

fn seven_fifty_words_region_or_buffer_dispatches_active_region_bounds_verbatim() -> ParityBatchCase
{
    ParityBatchCase::value(
        "seven_fifty_words_region_or_buffer_dispatches_active_region_bounds_verbatim",
        r##"(let (events)
               (cl-letf
                   (((symbol-function
                      'region-active-p)
                     (lambda () t))
                    ((symbol-function 'point)
                     (lambda () 9))
                    ((symbol-function 'mark)
                     (lambda () 3))
                    ((symbol-function
                      '750words-region)
                     (lambda (start end)
                       (push
                        (list
                         'region start end)
                        events)
                       'region-posted))
                    ((symbol-function
                      '750words-buffer)
                     (lambda ()
                       (push 'buffer events)
                       'buffer-posted)))
                 (list
                  (750words-region-or-buffer)
                  (nreverse events))))"##,
        expect!["OK (region-posted ((region 9 3)))"],
    )
}

fn seven_fifty_words_region_or_buffer_uses_whole_buffer_without_active_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_region_or_buffer_uses_whole_buffer_without_active_region",
        r##"(let (events)
               (cl-letf
                   (((symbol-function
                      'region-active-p)
                     (lambda () nil))
                    ((symbol-function
                      '750words-region)
                     (lambda (&rest _)
                       (push 'region events)))
                    ((symbol-function
                      '750words-buffer)
                     (lambda ()
                       (push 'buffer events)
                       'buffer-posted)))
                 (list
                  (750words-region-or-buffer)
                  (nreverse events))))"##,
        expect!["OK (buffer-posted (buffer))"],
    )
}

pub(super) fn posting_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        seven_fifty_words_file_starts_formatted_command_and_installs_live_process_sentinel(),
        seven_fifty_words_file_reports_non_live_process_without_installing_sentinel(),
        seven_fifty_words_post_process_handles_exit_and_signal_in_exact_order(),
        seven_fifty_words_post_process_ignores_running_and_stopped_processes(),
        seven_fifty_words_region_writes_exact_bounds_then_posts_the_sandbox_file(),
        seven_fifty_words_buffer_forwards_current_minimum_and_maximum(),
        seven_fifty_words_region_or_buffer_dispatches_active_region_bounds_verbatim(),
        seven_fifty_words_region_or_buffer_uses_whole_buffer_without_active_region(),
    ]
}
