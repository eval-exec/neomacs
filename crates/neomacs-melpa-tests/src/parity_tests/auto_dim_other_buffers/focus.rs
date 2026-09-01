use expect_test::expect;

use super::ParityBatchCase;

fn auto_dim_other_buffers_focus_change_handles_gain_loss_and_disabled_loss_with_exact_state_transitions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_focus_change_handles_gain_loss_and_disabled_loss_with_exact_state_transitions",
        r##"(mapcar
          (lambda (case)
            (let ((buffer
                   (generate-new-buffer
                    " *adob-focus-target*"))
                  events)
              (unwind-protect
                  (let ((adob--last-buffer buffer)
                        (adob--last-window
                         (selected-window))
                        (adob--focus-change-timer
                         :pending)
                        (adob--focus-change-last-state
                         (nth 1 case))
                        (auto-dim-other-buffers-dim-on-focus-out
                         (nth 2 case))
                        (adob-test-focus-state
                         (nth 0 case)))
                    (cl-letf
                        (((symbol-function
                           'frame-focus-state)
                          (lambda (&optional _frame)
                            adob-test-focus-state))
                         ((symbol-function
                           'adob--update)
                          (lambda ()
                            (push :update events)))
                         ((symbol-function
                           'set-window-parameter)
                          (lambda (window parameter value)
                            (push
                             (list
                              :parameter
                              (eq
                               window
                               adob--last-window)
                              parameter
                              value)
                             events)))
                         ((symbol-function
                           'force-window-update)
                          (lambda (window)
                            (push
                             (list
                              :force
                              (eq
                               window
                               adob--last-window))
                             events))))
                      (list
                       case
                       (adob--focus-change)
                       adob--focus-change-timer
                       adob--focus-change-last-state
                       (and
                        (bufferp adob--last-buffer)
                        (buffer-name
                         adob--last-buffer))
                       (windowp
                        adob--last-window)
                       (nreverse events))))
                (when (buffer-live-p buffer)
                  (kill-buffer buffer)))))
          '((t nil t)
            (nil t t)
            (nil t nil)
            (t t t)))"##,
        expect![[
            r#"OK (((t nil t) #1=(:update) nil t " *adob-focus-target*" t #1#) ((nil t t) nil nil nil nil nil ((:parameter t adob--dim t) (:force t))) ((nil t nil) nil nil nil " *adob-focus-target*" t nil) ((t t t) nil nil t " *adob-focus-target*" t nil))"#
        ]],
    )
}

fn auto_dim_other_buffers_focus_change_skips_all_work_when_focus_state_is_unchanged()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_focus_change_skips_all_work_when_focus_state_is_unchanged",
        r##"(let ((adob--focus-change-last-state
                                :same)
                               (adob--focus-change-timer
                                :pending)
                               events)
          (cl-letf
              (((symbol-function
                 'frame-focus-state)
                (lambda (&optional _frame)
                  :same))
               ((symbol-function
                 'adob--update)
                (lambda ()
                  (push :update events)))
               ((symbol-function
                 'set-window-parameter)
                (lambda (&rest arguments)
                  (push
                   (list :parameter arguments)
                   events))))
            (list
             (adob--focus-change)
             adob--focus-change-timer
             adob--focus-change-last-state
             events)))"##,
        expect!["OK (nil nil :same nil)"],
    )
}

fn auto_dim_other_buffers_focus_hook_calls_immediately_at_zero_delay_or_schedules_once()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_focus_hook_calls_immediately_at_zero_delay_or_schedules_once",
        r##"(mapcar
          (lambda (spec)
            (let ((adob--focus-change-debounce-delay
                   (car spec))
                  (adob--focus-change-timer
                   (cadr spec))
                  events)
              (cl-letf
                  (((symbol-function
                     'adob--focus-change)
                    (lambda ()
                      (push :focus-change events)
                      :changed))
                   ((symbol-function
                     'run-with-timer)
                    (lambda (delay repeat callback)
                      (push
                       (list
                        :schedule
                        delay
                        repeat
                        (eq
                         callback
                         #'adob--focus-change))
                       events)
                      :new-timer)))
                (list
                 spec
                 (adob--focus-change-hook)
                 adob--focus-change-timer
                 (nreverse events)))))
          '((0 nil)
            (-1 nil)
            (0.015 nil)
            (0.5 :existing-timer)))"##,
        expect![
            "OK (((0 nil) :changed nil (:focus-change)) ((-1 nil) :changed nil (:focus-change)) ((0.015 nil) :new-timer :new-timer ((:schedule 0.015 nil t))) ((0.5 :existing-timer) nil :existing-timer nil))"
        ],
    )
}

fn auto_dim_other_buffers_scheduled_focus_callback_clears_timer_before_performing_update()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_scheduled_focus_callback_clears_timer_before_performing_update",
        r##"(let ((adob--focus-change-debounce-delay
                                0.25)
                               (adob--focus-change-timer nil)
                               (adob--focus-change-last-state nil)
                               callback
                               events)
          (cl-letf
              (((symbol-function
                 'run-with-timer)
                (lambda (_delay _repeat function)
                  (setq callback function)
                  :fixture-timer))
               ((symbol-function
                 'frame-focus-state)
                (lambda (&optional _frame)
                  t))
               ((symbol-function
                 'adob--update)
                (lambda ()
                  (push
                   (list
                    :update
                    adob--focus-change-timer
                    adob--focus-change-last-state)
                   events))))
            (adob--focus-change-hook)
            (let ((scheduled
                   adob--focus-change-timer))
              (funcall callback)
              (list
               scheduled
               adob--focus-change-timer
               adob--focus-change-last-state
               (nreverse events)))))"##,
        expect!["OK (:fixture-timer nil t ((:update nil t)))"],
    )
}

fn auto_dim_other_buffers_focus_out_dims_real_selected_window_and_focus_in_restores_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_focus_out_dims_real_selected_window_and_focus_in_restores_it",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-focus-workflow*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((auto-dim-other-buffers-affected-faces
                         '((default
                            . auto-dim-other-buffers)))
                        (auto-dim-other-buffers-dim-on-focus-out
                         t)
                        (adob--has-fringes nil)
                        (adob--focus-change-timer nil)
                        (adob--focus-change-last-state t)
                        (adob-test-focus-state nil))
                    (adob--initialize)
                    (cl-letf
                        (((symbol-function
                           'frame-focus-state)
                          (lambda (&optional _frame)
                            adob-test-focus-state)))
                      (adob--focus-change)
                      (let ((lost
                             (list
                              (adob-test-window-summary)
                              adob--last-buffer
                              adob--last-window
                              adob--focus-change-last-state)))
                        (setq
                         adob-test-focus-state
                         t)
                        (adob--focus-change)
                        (list
                         lost
                         (adob-test-window-summary)
                         (eq
                          adob--last-buffer
                          buffer)
                         (eq
                          adob--last-window
                          (selected-window))
                         adob--focus-change-last-state)))))
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((((t " *adob-focus-workflow*" t)) nil nil nil) ((t " *adob-focus-workflow*" nil)) t t t)"#
        ]],
    )
}

pub(super) fn focus_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_focus_change_handles_gain_loss_and_disabled_loss_with_exact_state_transitions(),
        auto_dim_other_buffers_focus_change_skips_all_work_when_focus_state_is_unchanged(),
        auto_dim_other_buffers_focus_hook_calls_immediately_at_zero_delay_or_schedules_once(),
        auto_dim_other_buffers_scheduled_focus_callback_clears_timer_before_performing_update(),
        auto_dim_other_buffers_focus_out_dims_real_selected_window_and_focus_in_restores_it(),
    ]
}
