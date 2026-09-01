use expect_test::expect;

use super::ParityBatchCase;

fn async_start_process_future_reports_success_and_cleans_its_output_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_start_process_future_reports_success_and_cleans_its_output_buffer",
        r##"(let* ((process
                      (async-start-process
                       "neomacs-async-success"
                       "sh"
                       nil
                       "-c"
                       "printf 'alpha\\nbeta\\n'"))
                     (result (async-get process)))
               (list
                (eq result process)
                (process-status process)
                (process-exit-status process)
                (buffer-live-p
                 (process-buffer process))))"##,
        expect![[r#"OK (t exit 0 nil)"#]],
    )
}

fn async_start_process_callback_can_observe_stdout_before_cleanup() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_start_process_callback_can_observe_stdout_before_cleanup",
        r##"(let (observed)
               (let ((process
                      (async-start-process
                       "neomacs-async-callback"
                       "sh"
                       (lambda (finished)
                         (setq observed
                               (list
                                (process-exit-status
                                 finished)
                                (with-current-buffer
                                    (process-buffer
                                     finished)
                                  (buffer-string))
                                (buffer-live-p
                                 (process-buffer
                                  finished)))))
                       "-c"
                       "printf 'callback-output'")))
                 (async-wait process)
                 (list
                  observed
                  (async-get process)
                  (buffer-live-p
                   (process-buffer process)))))"##,
        expect![[r#"OK ((0 "callback-output" t) nil nil)"#]],
    )
}

fn async_start_process_future_returns_the_exact_nonzero_exit_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_start_process_future_returns_the_exact_nonzero_exit_failure",
        r##"(let* ((process
                      (async-start-process
                       "neomacs-async-failure"
                       "sh"
                       nil
                       "-c"
                       "printf 'partial'; exit 7"))
                     (result (async-get process)))
               (list
                result
                (process-status process)
                (process-exit-status process)
                (buffer-live-p
                 (process-buffer process))))"##,
        expect![[
            r#"OK ((error "Async process 'neomacs-async-failure' failed with exit code 7") exit 7 nil)"#
        ]],
    )
}

fn async_process_noquery_option_controls_the_process_query_flag() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_process_noquery_option_controls_the_process_query_flag",
        r##"(let (query noquery)
               (unwind-protect
                   (progn
                     (let ((async-process-noquery-on-exit
                            nil))
                       (setq query
                             (async-start-process
                              "neomacs-async-query"
                              "sh" nil "-c"
                              "sleep 0.2")))
                     (let ((async-process-noquery-on-exit
                            t))
                       (setq noquery
                             (async-start-process
                              "neomacs-async-noquery"
                              "sh" nil "-c"
                              "sleep 0.2")))
                     (list
                      (process-query-on-exit-flag query)
                      (process-query-on-exit-flag
                       noquery)))
                 (when query
                   (async-wait query)
                   (async-get query))
                 (when noquery
                   (async-wait noquery)
                   (async-get noquery))))"##,
        expect![[r#"OK (t nil)"#]],
    )
}

pub(super) fn processes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_start_process_future_reports_success_and_cleans_its_output_buffer(),
        async_start_process_callback_can_observe_stdout_before_cleanup(),
        async_start_process_future_returns_the_exact_nonzero_exit_failure(),
        async_process_noquery_option_controls_the_process_query_flag(),
    ]
}
