use expect_test::expect;

use super::ParityBatchCase;

fn auto_async_byte_compile_status_exit_and_warning_matrix_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_status_exit_and_warning_matrix_match",
        r##"(mapcar
          (lambda (case)
            (let ((buffer
                   (generate-new-buffer
                    " *aabc-status*")))
              (unwind-protect
                  (with-current-buffer buffer
                    (insert
                     (cadr case))
                    (list
                     case
                     (aabc/status
                      (car case)
                      buffer)
                     (point)))
                (kill-buffer buffer))))
          '((0 "")
            (0 "file.el:1:1:Warning: fixture")
            (1 "")
            (1 "file.el:1:1:Warning: fixture")
            (2 "")
            (2 "file.el:1:1:Warning: fixture")
            (127 "plain failure text")))"##,
        expect![[
            r#"OK (((0 "") normal 1) ((0 "file.el:1:1:Warning: fixture") warning 21) ((1 "") error 1) ((1 "file.el:1:1:Warning: fixture") error 29) ((2 "") normal 1) ((2 "file.el:1:1:Warning: fixture") warning 21) ((127 "plain failure text") normal 1))"#
        ]],
    )
}

fn auto_async_byte_compile_status_warning_scan_uses_default_case_folding_and_is_unanchored()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_status_warning_scan_uses_default_case_folding_and_is_unanchored",
        r##"(mapcar
          (lambda (text)
            (let ((buffer
                   (generate-new-buffer
                    " *aabc-warning-scan*")))
              (unwind-protect
                  (with-current-buffer buffer
                    (insert text)
                    (list
                     text
                     (aabc/status 0 buffer)
                     (point)))
                (kill-buffer buffer))))
          '(":Warning:"
            "prefix:Warning:suffix"
            ":warning:"
            ":WARNING:"
            "Warning:"
            ":Warning"
            ":\nWarning:"
            "before\n:Warning:\nafter"))"##,
        expect![[
            r#"OK ((":Warning:" warning 10) ("prefix:Warning:suffix" warning 16) (":warning:" warning 10) (":WARNING:" warning 10) ("Warning:" normal 1) (":Warning" normal 1) (":\nWarning:" normal 1) ("before\n:Warning:\nafter" warning 17))"#
        ]],
    )
}

fn auto_async_byte_compile_status_resets_and_mutates_result_buffer_point_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_async_byte_compile_status_resets_and_mutates_result_buffer_point_exactly",
        r##"(let ((buffer
                                (generate-new-buffer
                                 " *aabc-point*")))
          (unwind-protect
              (with-current-buffer buffer
                (insert
                 "012345:Warning:tail")
                (goto-char
                 (point-max))
                (list
                 (point)
                 (aabc/status 0 buffer)
                 (point)
                 (aabc/status 1 buffer)
                 (point)
                 (aabc/status 0 buffer)
                 (point)))
            (kill-buffer buffer)))"##,
        expect!["OK (20 warning 16 error 16 warning 16)"],
    )
}

fn auto_async_byte_compile_display_routes_normal_warning_and_error_statuses_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_display_routes_normal_warning_and_error_statuses_exactly",
        r##"(let ((buffer
                                (generate-new-buffer
                                 " *aabc-display*"))
                               calls)
          (unwind-protect
              (cl-letf
                  (((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((rendered
                             (apply
                              #'format
                              format-string
                              arguments)))
                        (push
                         (list :message rendered)
                         calls)
                        rendered)))
                   ((symbol-function 'display-buffer)
                    (lambda (target &rest arguments)
                      (push
                       (list
                        :display
                        (buffer-name target)
                        arguments)
                       calls)
                      :displayed)))
                (list
                 (let ((auto-async-byte-compile-suppress-warnings nil))
                   (aabc/display-function
                    "compile one.el"
                    buffer
                    'normal))
                 (let ((auto-async-byte-compile-suppress-warnings t))
                   (aabc/display-function
                    "compile two.el"
                    buffer
                    'warning))
                 (let ((auto-async-byte-compile-suppress-warnings nil))
                   (aabc/display-function
                    "compile three.el"
                    buffer
                    'warning))
                 (aabc/display-function
                  "compile four.el"
                  buffer
                  'error)
                 (aabc/display-function
                  "compile five.el"
                  buffer
                  :custom)
                 (nreverse calls)))
            (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("compile one.el completed" "compile two.el completed with warnings." :displayed :displayed :displayed ((:message "compile one.el completed") (:message "compile two.el completed with warnings.") (:display " *aabc-display*" nil) (:display " *aabc-display*" nil) (:display " *aabc-display*" nil)))"#
        ]],
    )
}

fn auto_async_byte_compile_display_uses_configured_function_return_and_propagates_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_display_uses_configured_function_return_and_propagates_failure",
        r##"(let ((buffer
                                (generate-new-buffer
                                 " *aabc-custom-display*"))
                               calls)
          (unwind-protect
              (list
               (let ((auto-async-byte-compile-display-function
                      (lambda (target)
                        (push
                         (buffer-name target)
                         calls)
                        :custom-return)))
                 (aabc/display-function
                  "fixture"
                  buffer
                  'error))
               calls
               (let ((auto-async-byte-compile-display-function
                      (lambda (_)
                        (error
                         "fixture display failed"))))
                 (auto-async-byte-compile-test-error-data
                  (lambda ()
                    (aabc/display-function
                     "fixture"
                     buffer
                     'warning)))))
            (kill-buffer buffer)))"##,
        expect![[
            r#"OK (:custom-return (" *aabc-custom-display*") (:error error ("fixture display failed")))"#
        ]],
    )
}

fn auto_async_byte_compile_process_sentinel_observes_process_data_and_orders_display_before_hook()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_process_sentinel_observes_process_data_and_orders_display_before_hook",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'process-exit-status)
                (lambda (process)
                  (push
                   (list :exit process)
                   events)
                  7))
               ((symbol-function 'process-name)
                (lambda (process)
                  (push
                   (list :name process)
                   events)
                  "fixture-process"))
               ((symbol-function 'process-buffer)
                (lambda (process)
                  (push
                   (list :buffer process)
                   events)
                  'fixture-buffer))
               ((symbol-function 'aabc/status)
                (lambda (exit-status result-buffer)
                  (push
                   (list
                    :status
                    exit-status
                    result-buffer)
                   events)
                  'warning))
               ((symbol-function 'aabc/display-function)
                (lambda (name buffer status)
                  (push
                   (list
                    :display
                    name
                    buffer
                    status)
                   events)
                  :displayed)))
            (let ((auto-async-byte-compile-hook
                   (list
                    (lambda ()
                      (push
                       (list
                        :hook
                        (boundp 'exitstatus))
                       events)))))
              (list
               (aabc/process-sentinel
                'fixture-process-object
                "ignored state")
               (nreverse events)))))"##,
        expect![[
            r#"OK (nil ((:exit fixture-process-object) (:status 7 " *auto-async-byte-compile*") (:name fixture-process-object) (:buffer fixture-process-object) (:display "fixture-process" fixture-buffer warning) (:hook nil)))"#
        ]],
    )
}

fn auto_async_byte_compile_process_sentinel_stops_before_hook_on_display_failure() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_async_byte_compile_process_sentinel_stops_before_hook_on_display_failure",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'process-exit-status)
                (lambda (_)
                  0))
               ((symbol-function 'aabc/status)
                (lambda (&rest _)
                  'normal))
               ((symbol-function 'process-name)
                (lambda (_)
                  "fixture"))
               ((symbol-function 'process-buffer)
                (lambda (_)
                  'fixture-buffer))
               ((symbol-function 'aabc/display-function)
                (lambda (&rest _)
                  (push :display events)
                  (error
                   "fixture display failure"))))
            (let ((auto-async-byte-compile-hook
                   (list
                    (lambda ()
                      (push :hook events)))))
              (list
               (auto-async-byte-compile-test-error-data
                (lambda ()
                  (aabc/process-sentinel
                   'fixture
                   "done")))
               (nreverse events)))))"##,
        expect![[r#"OK ((:error error ("fixture display failure")) (:display))"#]],
    )
}

fn auto_async_byte_compile_process_sentinel_propagates_hook_failure_after_display()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_process_sentinel_propagates_hook_failure_after_display",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'process-exit-status)
                (lambda (_)
                  0))
               ((symbol-function 'aabc/status)
                (lambda (&rest _)
                  'normal))
               ((symbol-function 'process-name)
                (lambda (_)
                  "fixture"))
               ((symbol-function 'process-buffer)
                (lambda (_)
                  'fixture-buffer))
               ((symbol-function 'aabc/display-function)
                (lambda (&rest _)
                  (push :display events)
                  :displayed)))
            (let ((auto-async-byte-compile-hook
                   (list
                    (lambda ()
                      (push :hook events)
                      (error
                       "fixture hook failure")))))
              (list
               (auto-async-byte-compile-test-error-data
                (lambda ()
                  (aabc/process-sentinel
                   'fixture
                   "done")))
               (nreverse events)))))"##,
        expect![[r#"OK ((:error error ("fixture hook failure")) (:display :hook))"#]],
    )
}

pub(super) fn status_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_async_byte_compile_status_exit_and_warning_matrix_match(),
        auto_async_byte_compile_status_warning_scan_uses_default_case_folding_and_is_unanchored(),
        auto_async_byte_compile_status_resets_and_mutates_result_buffer_point_exactly(),
        auto_async_byte_compile_display_routes_normal_warning_and_error_statuses_exactly(),
        auto_async_byte_compile_display_uses_configured_function_return_and_propagates_failure(),
        auto_async_byte_compile_process_sentinel_observes_process_data_and_orders_display_before_hook(),
        auto_async_byte_compile_process_sentinel_stops_before_hook_on_display_failure(),
        auto_async_byte_compile_process_sentinel_propagates_hook_failure_after_display(),
    ]
}
