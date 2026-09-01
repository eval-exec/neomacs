use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_log_formats_and_returns_text_even_without_a_log_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_log_formats_and_returns_text_even_without_a_log_buffer",
        r##"(let ((loop
                (asyncloop-create)))
         (list
          (asyncloop-log loop
            "Processed %d %s: %S"
            3
            "records"
            '(:status ok))
          (asyncloop-log-buffer loop)
          (asyncloop-test-error
           (lambda ()
             (asyncloop-log loop
               "%d"
               "wrong")))))"##,
        expect![[
            r#"OK ("Processed 3 records: (:status ok)" nil (:signal error ("Format specifier doesn’t match argument type")))"#
        ]],
    )
}

fn asyncloop_log_appends_timestamped_multiline_workflow_messages_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_log_appends_timestamped_multiline_workflow_messages_in_order",
        r##"(let* ((buffer
                 (generate-new-buffer
                  " *asyncloop-log-test*"))
                (loop
                 (asyncloop-create
                  :log-buffer buffer)))
         (unwind-protect
             (with-temp-buffer
               (let ((origin
                      (current-buffer)))
                 (list
                  (asyncloop-log loop
                    "Indexed %d files"
                    12)
                  (asyncloop-log loop
                    "Saved %s\nwith detail"
                    "manifest")
                  (eq origin
                      (current-buffer))
                  (asyncloop-test-log-text buffer))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("Indexed 12 files" "Saved manifest\nwith detail" t "<TIME>: Indexed 12 files\n<TIME>: Saved manifest\nwith detail\n")"#
        ]],
    )
}

fn asyncloop_log_mode_initializes_practical_buffer_and_quit_key_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_log_mode_initializes_practical_buffer_and_quit_key_contract",
        r##"(with-temp-buffer
         (asyncloop-log-mode)
         (list
          major-mode
          mode-name
          truncate-lines
          buffer-read-only
          window-point-insertion-type
          (eq
           (key-binding
            (kbd "C-g"))
           'asyncloop-keyboard-quit)
          (eq
           (lookup-key
            asyncloop-log-mode-map
            [remap keyboard-quit])
           'asyncloop-keyboard-quit)
          (derived-mode-p
           'special-mode)))"##,
        expect![[r#"OK (asyncloop-log-mode "Asyncloop-Log" t nil t t t special-mode)"#]],
    )
}

fn asyncloop_log_mode_runs_hook_and_activates_generated_syntax_and_abbrev_tables() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_log_mode_runs_hook_and_activates_generated_syntax_and_abbrev_tables",
        r##"(let ((asyncloop-log-mode-abbrev-table
                (make-abbrev-table))
               (asyncloop-log-mode-syntax-table
                (copy-syntax-table
                 asyncloop-log-mode-syntax-table))
               hook-events)
         (define-abbrev
          asyncloop-log-mode-abbrev-table
          "al"
          "asyncloop")
         (modify-syntax-entry
          ?_
          "w"
          asyncloop-log-mode-syntax-table)
         (let ((asyncloop-log-mode-hook
                (list
                 (lambda ()
                   (push
                    (list
                     major-mode
                     mode-name
                     buffer-read-only
                     truncate-lines)
                    hook-events)))))
           (with-temp-buffer
             (asyncloop-log-mode)
             (insert "al")
             (expand-abbrev)
             (list
              (buffer-string)
              (nreverse hook-events)
              (eq
               (syntax-table)
               asyncloop-log-mode-syntax-table)
              (eq
               local-abbrev-table
               asyncloop-log-mode-abbrev-table)
              (char-syntax ?_)
              (eq
               (current-local-map)
               asyncloop-log-mode-map)))))"##,
        expect![[r#"OK ("asyncloop" ((asyncloop-log-mode "Asyncloop-Log" nil t)) t t 119 t)"#]],
    )
}

fn asyncloop_keyboard_quit_cancels_only_the_loop_owned_by_current_log_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_keyboard_quit_cancels_only_the_loop_owned_by_current_log_buffer",
        r##"(let* ((buffer-a
                 (generate-new-buffer
                  " *asyncloop-quit-a*"))
                (buffer-b
                 (generate-new-buffer
                  " *asyncloop-quit-b*"))
                (loop-a
                 (asyncloop-create
                  :log-buffer buffer-a
                  :remainder '(a)
                  :scheduled t
                  :paused t
                  :just-launched t))
                (loop-b
                 (asyncloop-create
                  :log-buffer buffer-b
                  :remainder '(b)
                  :scheduled t
                  :paused t
                  :just-launched t))
                (asyncloop-objects
                 `((1 . ,loop-a)
                   (2 . ,loop-b))))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'cancel-timer)
                   #'asyncloop-test-cancel-timer)
                  ((symbol-function
                    'keyboard-quit)
                   (lambda ()
                     :delegated-quit)))
               (with-current-buffer buffer-a
                 (list
                  (asyncloop-keyboard-quit)
                  (list
                   (asyncloop-remainder loop-a)
                   (asyncloop-scheduled loop-a)
                   (asyncloop-paused loop-a)
                   (asyncloop-just-launched loop-a))
                  (list
                   (asyncloop-remainder loop-b)
                   (asyncloop-scheduled loop-b)
                   (asyncloop-paused loop-b)
                   (asyncloop-just-launched loop-b))
                  (asyncloop-test-log-text buffer-a)
                  (asyncloop-test-log-text buffer-b))))
           (kill-buffer buffer-a)
           (kill-buffer buffer-b)))"##,
        expect![[
            r#"OK (nil (nil nil nil nil) ((b) t t t) "<TIME>: Loop reset due to quit in buffer  *asyncloop-quit-a*\n" "")"#
        ]],
    )
}

fn asyncloop_clock_funcall_logs_result_and_preserves_callback_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_clock_funcall_logs_result_and_preserves_callback_side_effects",
        r##"(let* ((buffer
                 (generate-new-buffer
                  " *asyncloop-clock-success*"))
                (loop
                 (asyncloop-create
                  :log-buffer buffer))
                events)
         (unwind-protect
             (let ((return
                    (asyncloop-clock-funcall
                     loop
                     (lambda (received-loop)
                       (push
                        (eq received-loop loop)
                        events)
                       '(:indexed 4 :skipped 1)))))
               (list
                (replace-regexp-in-string
                 "Took [[:digit:].]+s"
                 "Took <ELAPSED>s"
                 return)
                events
                (replace-regexp-in-string
                 "Took [[:digit:].]+s"
                 "Took <ELAPSED>s"
                 (asyncloop-test-log-text buffer))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("Took <ELAPSED>s: lambda: (:indexed 4 :skipped 1)" (t) "<TIME>: Took <ELAPSED>s: lambda: (:indexed 4 :skipped 1)\n")"#
        ]],
    )
}

fn asyncloop_clock_funcall_logs_and_resignals_exact_worker_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_clock_funcall_logs_and_resignals_exact_worker_error",
        r##"(let* ((loop
                 (asyncloop-create))
                logged)
         (cl-letf
             (((symbol-function
                'asyncloop-log)
               (lambda (_loop format-string &rest arguments)
                 (push
                  (apply #'format
                         format-string
                         arguments)
                  logged))))
           (list
            (asyncloop-test-error
             (lambda ()
               (asyncloop-clock-funcall
                loop
                (lambda (_loop)
                  (signal
                   'file-error
                   '("Cannot index" "/missing"))))))
            logged)))"##,
        expect![[
            r#"OK ((:signal file-error ("Cannot index" "/missing")) ("During lambda: (file-error \"Cannot index\" \"/missing\")"))"#
        ]],
    )
}

pub(super) fn logging_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_log_formats_and_returns_text_even_without_a_log_buffer(),
        asyncloop_log_appends_timestamped_multiline_workflow_messages_in_order(),
        asyncloop_log_mode_initializes_practical_buffer_and_quit_key_contract(),
        asyncloop_log_mode_runs_hook_and_activates_generated_syntax_and_abbrev_tables(),
        asyncloop_keyboard_quit_cancels_only_the_loop_owned_by_current_log_buffer(),
        asyncloop_clock_funcall_logs_result_and_preserves_callback_side_effects(),
        asyncloop_clock_funcall_logs_and_resignals_exact_worker_error(),
    ]
}
