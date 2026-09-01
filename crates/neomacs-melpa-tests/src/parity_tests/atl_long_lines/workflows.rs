use expect_test::expect;

use super::ParityBatchCase;

fn global_atl_long_lines_cross_buffer_commands_share_and_supersede_one_timer() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_atl_long_lines_cross_buffer_commands_share_and_supersede_one_timer",
        r##"(let ((buffer-a
                (generate-new-buffer
                 " *atl-long-lines-a*"))
               (buffer-b
                (generate-new-buffer
                 " *atl-long-lines-b*"))
               (atl-long-lines--timer nil)
               (next-id 0)
               live
               events
               enabled
               disabled)
         (unwind-protect
             (cl-letf
                 (((symbol-function 'timerp)
                   (lambda (value)
                     (memq value live)))
                  ((symbol-function 'cancel-timer)
                   (lambda (timer)
                     (setq live
                           (delq timer live))
                     (push
                      (list
                       :cancel
                       (nth 1 timer)
                       (nth 2 timer))
                      events)))
                  ((symbol-function
                    'run-with-idle-timer)
                   (lambda
                       (delay repeat function
                              &rest arguments)
                     (let ((timer
                            (list
                             :timer
                             (setq next-id
                                   (1+ next-id))
                             (cond
                              ((eq
                                (current-buffer)
                                buffer-a)
                               :a)
                              ((eq
                                (current-buffer)
                                buffer-b)
                               :b)
                              (t :other)))))
                       (push timer live)
                       (push
                        (list
                         :schedule
                         (nth 1 timer)
                         (nth 2 timer)
                         delay
                         repeat
                         function
                         arguments)
                        events)
                       timer))))
               (with-current-buffer buffer-a
                 (fundamental-mode))
               (with-current-buffer buffer-b
                 (fundamental-mode))
               (global-atl-long-lines-mode 1)
               (setq enabled
                     (mapcar
                      (lambda (buffer)
                        (with-current-buffer buffer
                          (list
                           atl-long-lines-mode
                           (atl-long-lines-test-hook-count
                            #'atl-long-lines--start-timer
                            post-command-hook))))
                      (list buffer-a buffer-b)))
               (with-current-buffer buffer-a
                 (mapc
                  (lambda (function)
                    (when
                        (eq
                         function
                         #'atl-long-lines--start-timer)
                      (funcall function)))
                  post-command-hook))
               (with-current-buffer buffer-b
                 (mapc
                  (lambda (function)
                    (when
                        (eq
                         function
                         #'atl-long-lines--start-timer)
                      (funcall function)))
                  post-command-hook))
               (global-atl-long-lines-mode -1)
               (setq disabled
                     (mapcar
                      (lambda (buffer)
                        (with-current-buffer buffer
                          (list
                           atl-long-lines-mode
                           (atl-long-lines-test-hook-count
                            #'atl-long-lines--start-timer
                            post-command-hook))))
                      (list buffer-a buffer-b)))
               (list
                enabled
                (nreverse events)
                (mapcar #'cadr live)
                (list
                 (nth 1 atl-long-lines--timer)
                 (nth 2 atl-long-lines--timer))
                disabled))
           (when global-atl-long-lines-mode
             (global-atl-long-lines-mode -1))
           (kill-buffer buffer-a)
           (kill-buffer buffer-b)))"##,
        expect![[
            "OK (((t 1) (t 1)) ((:schedule 1 :a 0.4 nil atl-long-lines-do-toggle nil) (:cancel 1 :a) (:schedule 2 :b 0.4 nil atl-long-lines-do-toggle nil)) (2) (2 :b) ((nil 0) (nil 0)))"
        ]],
    )
    .fresh_process()
}

fn atl_long_lines_enabled_post_command_to_timer_callback_keeps_short_line_truncated()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_enabled_post_command_to_timer_callback_keeps_short_line_truncated",
        r##"(with-temp-buffer
         (insert "short")
         (goto-char
          (point-min))
         (let ((truncate-lines t)
               scheduled)
           (cl-letf
               (((symbol-function
                  'run-with-idle-timer)
                 (lambda
                     (delay repeat function
                            &rest arguments)
                   (setq
                    scheduled
                    (list
                     delay
                     repeat
                     function
                     arguments))
                   :fixture-timer))
                ((symbol-function 'timerp)
                 (lambda (_value) nil))
                ((symbol-function 'window-width)
                 (lambda (&optional _window)
                   10)))
             (atl-long-lines-mode 1)
             (run-hooks
              'post-command-hook)
             (let ((before
                    truncate-lines))
               (apply
                (nth 2 scheduled)
                (nth 3 scheduled))
               (list
                before
                truncate-lines
                scheduled
                atl-long-lines-mode
                (atl-long-lines-test-hook-count
                 #'atl-long-lines--start-timer
                 post-command-hook))))))"##,
        expect!["OK (t t (0.4 nil atl-long-lines-do-toggle nil) t 1)"],
    )
    .fresh_process()
}

fn atl_long_lines_cursor_navigation_recomputes_wrapping_for_each_visited_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_cursor_navigation_recomputes_wrapping_for_each_visited_line",
        r##"(with-temp-buffer
         (insert
          "tiny\n"
          "this is a realistically long source-code line\n"
          "medium\n")
         (let ((truncate-lines nil)
               scheduled
               states)
           (cl-letf
               (((symbol-function
                  'run-with-idle-timer)
                 (lambda
                     (_delay _repeat function
                             &rest arguments)
                   (setq
                    scheduled
                    (cons
                     function
                     arguments))
                   :fixture-timer))
                ((symbol-function 'timerp)
                 (lambda (_value) nil))
                ((symbol-function 'window-width)
                 (lambda (&optional _window)
                   12)))
             (atl-long-lines-mode 1)
             (dolist (line
                      '(0 1 2 1 0))
               (goto-char
                (point-min))
               (forward-line line)
               (run-hooks
                'post-command-hook)
               (apply
                (car scheduled)
                (cdr scheduled))
               (push
                (list
                 (line-number-at-pos)
                 (atl-long-lines--end-line-column)
                 truncate-lines)
                states))
             (nreverse states))))"##,
        expect!["OK ((1 4 t) (2 45 nil) (3 6 t) (2 45 nil) (1 4 t))"],
    )
}

fn atl_long_lines_rapid_post_commands_cancel_superseded_work_and_only_latest_callback_drives_ui()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_rapid_post_commands_cancel_superseded_work_and_only_latest_callback_drives_ui",
        r##"(with-temp-buffer
         (insert
          "short\n"
          "this line is substantially longer than the fixture window\n")
         (let ((truncate-lines nil)
               (next-id 0)
               live
               callbacks
               cancelled)
           (cl-letf
               (((symbol-function 'timerp)
                 (lambda (value)
                   (memq value live)))
                ((symbol-function 'cancel-timer)
                 (lambda (value)
                   (setq live
                         (delq value live))
                   (push value cancelled)))
                ((symbol-function
                  'run-with-idle-timer)
                 (lambda
                     (_delay _repeat function
                             &rest arguments)
                   (let ((timer
                          (list
                           :timer
                           (setq
                            next-id
                            (1+ next-id)))))
                     (push timer live)
                     (push
                      (list
                       timer
                       function
                       arguments)
                      callbacks)
                     timer)))
                ((symbol-function 'window-width)
                 (lambda (&optional _window)
                   10)))
             (atl-long-lines-mode 1)
             (goto-char
              (point-min))
             (run-hooks
              'post-command-hook)
             (forward-line 1)
             (run-hooks
              'post-command-hook)
             (let ((latest
                    (car callbacks)))
               (apply
                (nth 1 latest)
                (nth 2 latest)))
             (list
              (mapcar
               #'cadr
               (nreverse cancelled))
              (mapcar #'cadr live)
              (cadr
               atl-long-lines--timer)
              (eq
               atl-long-lines--timer
               (car live))
              truncate-lines
              (length callbacks)))))"##,
        expect!["OK ((1) (2) 2 t nil 2)"],
    )
}

fn atl_long_lines_runtime_delay_customization_affects_the_next_command_without_reenabling_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_runtime_delay_customization_affects_the_next_command_without_reenabling_mode",
        r##"(with-temp-buffer
         (let ((atl-long-lines-delay 0.4)
               observed)
           (cl-letf
               (((symbol-function 'timerp)
                 (lambda (_value) nil))
                ((symbol-function
                  'run-with-idle-timer)
                 (lambda
                     (delay repeat function
                            &rest arguments)
                   (push
                    (list
                     delay
                     repeat
                     function
                     arguments)
                    observed)
                   (list
                    :timer
                    delay))))
             (atl-long-lines-mode 1)
             (run-hooks
              'post-command-hook)
             (setq
              atl-long-lines-delay
              2.5)
             (run-hooks
              'post-command-hook)
             (list
              atl-long-lines-mode
              (nreverse observed)
              atl-long-lines--timer))))"##,
        expect![
            "OK (t ((0.4 nil atl-long-lines-do-toggle nil) (2.5 nil atl-long-lines-do-toggle nil)) (:timer 2.5))"
        ],
    )
}

fn atl_long_lines_disable_stops_future_post_command_scheduling_but_reenable_restores_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_disable_stops_future_post_command_scheduling_but_reenable_restores_it",
        r##"(with-temp-buffer
         (let ((schedules 0))
           (cl-letf
               (((symbol-function 'timerp)
                 (lambda (_value) nil))
                ((symbol-function
                  'run-with-idle-timer)
                 (lambda (&rest _arguments)
                   (setq
                    schedules
                    (1+ schedules))
                   (list
                    :timer
                    schedules))))
             (atl-long-lines-mode 1)
             (run-hooks
              'post-command-hook)
             (atl-long-lines-mode -1)
             (run-hooks
              'post-command-hook)
             (atl-long-lines-mode 1)
             (run-hooks
              'post-command-hook)
             (list
              schedules
              atl-long-lines-mode
              (atl-long-lines-test-hook-count
               #'atl-long-lines--start-timer
               post-command-hook)))))"##,
        expect!["OK (2 t 1)"],
    )
}

fn atl_long_lines_real_idle_timer_event_handler_applies_the_current_line_policy() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_long_lines_real_idle_timer_event_handler_applies_the_current_line_policy",
        r##"(with-temp-buffer
         (insert
          "this line is longer than twelve columns")
         (goto-char
          (point-min))
         (let ((atl-long-lines-delay 0)
               (truncate-lines t)
               timer)
           (unwind-protect
               (cl-letf
                   (((symbol-function
                      'window-width)
                     (lambda (&optional _window)
                       12)))
                 (atl-long-lines-mode 1)
                 (run-hooks
                  'post-command-hook)
                 (setq timer
                       atl-long-lines--timer)
                 (timer-event-handler
                  timer)
                 (list
                  truncate-lines
                  (timerp timer)
                  (and
                   (memq
                    timer
                    timer-idle-list)
                   t)
                  (eq
                   timer-event-last
                   timer)
                  atl-long-lines-mode))
             (when
                 (timerp timer)
               (cancel-timer timer)))))"##,
        expect!["OK (nil t nil t t)"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        global_atl_long_lines_cross_buffer_commands_share_and_supersede_one_timer(),
        atl_long_lines_enabled_post_command_to_timer_callback_keeps_short_line_truncated(),
        atl_long_lines_cursor_navigation_recomputes_wrapping_for_each_visited_line(),
        atl_long_lines_rapid_post_commands_cancel_superseded_work_and_only_latest_callback_drives_ui(),
        atl_long_lines_runtime_delay_customization_affects_the_next_command_without_reenabling_mode(),
        atl_long_lines_disable_stops_future_post_command_scheduling_but_reenable_restores_it(),
        atl_long_lines_real_idle_timer_event_handler_applies_the_current_line_policy(),
    ]
}
