use expect_test::expect;

use super::ParityBatchCase;

fn atl_markup_post_command_cancels_live_timer_then_schedules_exact_idle_callback() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_post_command_cancels_live_timer_then_schedules_exact_idle_callback",
        r##"(let ((atl-markup--timer
                :old-timer)
               (atl-markup-delay
                0.375)
               events)
          (cl-letf
              (((symbol-function 'timerp)
                (lambda (value)
                  (push
                   (list 'timerp value)
                   events)
                  (eq
                   value
                   :old-timer)))
               ((symbol-function 'cancel-timer)
                (lambda (value)
                  (push
                   (list 'cancel value)
                   events)
                  :cancelled))
               ((symbol-function 'run-with-idle-timer)
                (lambda (&rest arguments)
                  (push
                   (cons 'schedule arguments)
                   events)
                  :new-timer)))
            (list
             (atl-markup--post-command-hook)
             atl-markup--timer
             (nreverse events))))"##,
        expect![
            "OK (:new-timer nil ((timerp :old-timer) (cancel :old-timer) (schedule 0.375 nil atl-markup--web-truncate-lines-by-face)))"
        ],
    )
}

fn atl_markup_post_command_keeps_non_timer_sentinel_while_still_scheduling() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_post_command_keeps_non_timer_sentinel_while_still_scheduling",
        r##"(let ((atl-markup--timer
                :not-a-timer)
               (atl-markup-delay
                2.5)
               events)
          (cl-letf
              (((symbol-function 'timerp)
                (lambda (value)
                  (push
                   (list 'timerp value)
                   events)
                  nil))
               ((symbol-function 'cancel-timer)
                (lambda (value)
                  (push
                   (list 'unexpected-cancel value)
                   events)))
               ((symbol-function 'run-with-idle-timer)
                (lambda (&rest arguments)
                  (push
                   (cons 'schedule arguments)
                   events)
                  :scheduled)))
            (list
             (atl-markup--post-command-hook)
             atl-markup--timer
             (nreverse events))))"##,
        expect![
            "OK (:scheduled :not-a-timer ((timerp :not-a-timer) (schedule 2.5 nil atl-markup--web-truncate-lines-by-face)))"
        ],
    )
}

fn atl_markup_post_command_preserves_live_timer_when_cancellation_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_post_command_preserves_live_timer_when_cancellation_signals",
        r##"(let ((atl-markup--timer
                :live)
               events)
          (cl-letf
              (((symbol-function 'timerp)
                (lambda (value)
                  (push
                   (list 'timerp value)
                   events)
                  t))
               ((symbol-function 'cancel-timer)
                (lambda (value)
                  (push
                   (list 'cancel value)
                   events)
                  (error
                   "cannot cancel %S"
                   value)))
               ((symbol-function 'run-with-idle-timer)
                (lambda (&rest arguments)
                  (push
                   (cons 'unexpected-schedule arguments)
                   events))))
            (list
             (atl-markup-test-error-data
              #'atl-markup--post-command-hook)
             atl-markup--timer
             (nreverse events))))"##,
        expect![[
            r#"OK ((:error error ("cannot cancel :live")) :live ((timerp :live) (cancel :live)))"#
        ]],
    )
}

fn atl_markup_post_command_clears_cancelled_timer_before_schedule_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_post_command_clears_cancelled_timer_before_schedule_failure",
        r##"(let ((atl-markup--timer
                :live)
               (atl-markup-delay
                0.1)
               events)
          (cl-letf
              (((symbol-function 'timerp)
                (lambda (value)
                  (push
                   (list 'timerp value)
                   events)
                  t))
               ((symbol-function 'cancel-timer)
                (lambda (value)
                  (push
                   (list 'cancel value)
                   events)
                  :cancelled))
               ((symbol-function 'run-with-idle-timer)
                (lambda (&rest arguments)
                  (push
                   (cons 'schedule arguments)
                   events)
                  (error
                   "scheduler unavailable"))))
            (list
             (atl-markup-test-error-data
              #'atl-markup--post-command-hook)
             atl-markup--timer
             (nreverse events))))"##,
        expect![[
            r#"OK ((:error error ("scheduler unavailable")) nil ((timerp :live) (cancel :live) (schedule 0.1 nil atl-markup--web-truncate-lines-by-face)))"#
        ]],
    )
}

fn atl_markup_repeated_post_commands_discard_scheduled_tokens_and_never_cancel_them()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_repeated_post_commands_discard_scheduled_tokens_and_never_cancel_them",
        r##"(let ((atl-markup--timer nil)
               (atl-markup-delay 0.2)
               (sequence 0)
               events
               results)
          (cl-letf
              (((symbol-function 'timerp)
                (lambda (value)
                  (push
                   (list 'timerp value)
                   events)
                  nil))
               ((symbol-function 'cancel-timer)
                (lambda (value)
                  (push
                   (list 'unexpected-cancel value)
                   events)))
               ((symbol-function 'run-with-idle-timer)
                (lambda (&rest arguments)
                  (setq sequence
                        (1+ sequence))
                  (let ((token
                         (intern
                          (format
                           "timer-%d"
                           sequence))))
                    (push
                     (append
                      (list 'schedule token)
                      arguments)
                     events)
                    token))))
            (dotimes (_ 3)
              (push
               (atl-markup--post-command-hook)
               results))
            (list
             (nreverse results)
             atl-markup--timer
             (nreverse events))))"##,
        expect![
            "OK ((timer-1 timer-2 timer-3) nil ((timerp nil) (schedule timer-1 0.2 nil atl-markup--web-truncate-lines-by-face) (timerp nil) (schedule timer-2 0.2 nil atl-markup--web-truncate-lines-by-face) (timerp nil) (schedule timer-3 0.2 nil atl-markup--web-truncate-lines-by-face)))"
        ],
    )
}

fn atl_markup_post_command_forwards_edge_delay_values_without_package_validation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_post_command_forwards_edge_delay_values_without_package_validation",
        r##"(mapcar
          (lambda (delay)
            (let ((atl-markup--timer nil)
                  (atl-markup-delay delay)
                  captured)
              (cl-letf
                  (((symbol-function 'timerp)
                    (lambda (_value)
                      nil))
                   ((symbol-function 'run-with-idle-timer)
                    (lambda (&rest arguments)
                      (setq captured arguments)
                      :scheduled)))
                (list
                 (copy-tree delay)
                 (atl-markup--post-command-hook)
                 atl-markup--timer
                 (copy-tree captured)
                 (eq
                  delay
                  (car captured))))))
          '(0.0
            -1.5
            99
            nil
            "later"
            (1 2)))"##,
        expect![[
            r#"OK ((0.0 :scheduled nil (0.0 nil atl-markup--web-truncate-lines-by-face) t) (-1.5 :scheduled nil (-1.5 nil atl-markup--web-truncate-lines-by-face) t) (99 :scheduled nil (99 nil atl-markup--web-truncate-lines-by-face) t) (nil :scheduled nil (nil nil atl-markup--web-truncate-lines-by-face) t) ("later" :scheduled nil ("later" nil atl-markup--web-truncate-lines-by-face) t) ((1 2) :scheduled nil ((1 2) nil atl-markup--web-truncate-lines-by-face) t))"#
        ]],
    )
}

fn atl_markup_global_timer_state_crosses_buffers_and_is_cancelled_from_second_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_global_timer_state_crosses_buffers_and_is_cancelled_from_second_buffer",
        r##"(let ((first
                (generate-new-buffer
                 " *atl-first*"))
               (second
                (generate-new-buffer
                 " *atl-second*"))
               events)
          (unwind-protect
              (progn
                (with-current-buffer first
                  (setq atl-markup--timer
                        :first-buffer-timer))
                (cl-letf
                    (((symbol-function 'timerp)
                      (lambda (value)
                        (eq
                         value
                         :first-buffer-timer)))
                     ((symbol-function 'cancel-timer)
                      (lambda (value)
                        (push
                         (list 'cancel value)
                         events)))
                     ((symbol-function 'run-with-idle-timer)
                      (lambda (&rest arguments)
                        (push
                         (cons 'schedule arguments)
                         events)
                        :second-buffer-timer)))
                  (with-current-buffer second
                    (atl-markup--post-command-hook))
                  (list
                   (with-current-buffer first
                     (list
                      atl-markup--timer
                      (local-variable-p
                       'atl-markup--timer)))
                   (with-current-buffer second
                     (list
                      atl-markup--timer
                      (local-variable-p
                       'atl-markup--timer)))
                   (nreverse events))))
            (kill-buffer first)
            (kill-buffer second)))"##,
        expect![
            "OK ((nil nil) (nil nil) ((cancel :first-buffer-timer) (schedule 0.1 nil atl-markup--web-truncate-lines-by-face)))"
        ],
    )
}

pub(super) fn timers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_markup_post_command_cancels_live_timer_then_schedules_exact_idle_callback(),
        atl_markup_post_command_keeps_non_timer_sentinel_while_still_scheduling(),
        atl_markup_post_command_preserves_live_timer_when_cancellation_signals(),
        atl_markup_post_command_clears_cancelled_timer_before_schedule_failure(),
        atl_markup_repeated_post_commands_discard_scheduled_tokens_and_never_cancel_them(),
        atl_markup_post_command_forwards_edge_delay_values_without_package_validation(),
        atl_markup_global_timer_state_crosses_buffers_and_is_cancelled_from_second_buffer(),
    ]
}
