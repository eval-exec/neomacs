use expect_test::expect;

use super::ParityBatchCase;

fn atl_long_lines_start_timer_schedules_one_shot_idle_callback_with_custom_delay() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_long_lines_start_timer_schedules_one_shot_idle_callback_with_custom_delay",
        r##"(let ((atl-long-lines-delay 1.25)
               (atl-long-lines--timer nil)
               calls
               (sentinel
                (list :scheduled)))
         (cl-letf
             (((symbol-function 'timerp)
               (lambda (_value) nil))
              ((symbol-function
                'run-with-idle-timer)
               (lambda
                   (seconds repeat function
                            &rest arguments)
                 (push
                  (list
                   seconds
                   repeat
                   function
                   arguments)
                  calls)
                 sentinel)))
           (list
            (atl-long-lines--start-timer)
            (eq
             atl-long-lines--timer
             sentinel)
            (nreverse calls))))"##,
        expect!["OK ((:scheduled) t ((1.25 nil atl-long-lines-do-toggle nil)))"],
    )
}

fn atl_long_lines_start_timer_cancels_an_existing_timer_before_rescheduling() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_start_timer_cancels_an_existing_timer_before_rescheduling",
        r##"(let ((atl-long-lines-delay 0.4)
               (old
                (list :old))
               (new
                (list :new))
               events)
         (setq
          atl-long-lines--timer
          old)
         (cl-letf
             (((symbol-function 'timerp)
               (lambda (value)
                 (eq value old)))
              ((symbol-function 'cancel-timer)
               (lambda (timer)
                 (push
                  (list :cancel timer)
                  events)))
              ((symbol-function
                'run-with-idle-timer)
               (lambda
                   (delay repeat function
                          &rest arguments)
                 (push
                  (list
                   :schedule
                   delay
                   repeat
                   function
                   arguments)
                  events)
                 new)))
           (atl-long-lines--start-timer)
           (list
            (nreverse events)
            (eq
             atl-long-lines--timer
             new))))"##,
        expect!["OK (((:cancel (:old)) (:schedule 0.4 nil atl-long-lines-do-toggle nil)) t)"],
    )
}

fn atl_long_lines_start_timer_does_not_cancel_a_non_timer_sentinel() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_start_timer_does_not_cancel_a_non_timer_sentinel",
        r##"(let ((atl-long-lines--timer
                :not-a-timer)
               cancelled
               (new
                (list :new)))
         (cl-letf
             (((symbol-function 'timerp)
               (lambda (_value) nil))
              ((symbol-function 'cancel-timer)
               (lambda (value)
                 (push value cancelled)))
              ((symbol-function
                'run-with-idle-timer)
               (lambda (&rest _arguments)
                 new)))
           (atl-long-lines--start-timer)
           (list
            cancelled
            (eq
             atl-long-lines--timer
             new))))"##,
        expect!["OK (nil t)"],
    )
}

fn atl_long_lines_repeated_start_timer_keeps_only_the_latest_scheduled_handle() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_repeated_start_timer_keeps_only_the_latest_scheduled_handle",
        r##"(let ((atl-long-lines--timer nil)
               (next-id 0)
               live
               cancelled)
         (cl-letf
             (((symbol-function 'timerp)
               (lambda (value)
                 (memq value live)))
              ((symbol-function 'cancel-timer)
               (lambda (value)
                 (setq
                  live
                  (delq value live))
                 (push value cancelled)))
              ((symbol-function
                'run-with-idle-timer)
               (lambda (&rest _arguments)
                 (let ((timer
                        (list
                         :timer
                         (setq
                          next-id
                          (1+ next-id)))))
                   (push timer live)
                   timer))))
           (dotimes
               (_ 4)
             (atl-long-lines--start-timer))
           (list
            (cadr
             atl-long-lines--timer)
            (mapcar #'cadr live)
            (eq
             atl-long-lines--timer
             (car live))
            (mapcar
             #'cadr
             (nreverse cancelled))
            next-id)))"##,
        expect!["OK (4 (4) t (1 2 3) 4)"],
    )
}

fn atl_long_lines_start_timer_forwards_fractional_zero_and_negative_delays_unchanged()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_start_timer_forwards_fractional_zero_and_negative_delays_unchanged",
        r##"(let (observed)
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
                 (list :timer delay))))
           (mapcar
            (lambda (delay)
              (let ((atl-long-lines-delay
                     delay)
                    (atl-long-lines--timer
                     nil))
                (atl-long-lines--start-timer)
                atl-long-lines--timer))
            '(0 0.125 -1 3))
           (nreverse observed)))"##,
        expect![
            "OK ((0 nil atl-long-lines-do-toggle nil) (0.125 nil atl-long-lines-do-toggle nil) (-1 nil atl-long-lines-do-toggle nil) (3 nil atl-long-lines-do-toggle nil))"
        ],
    )
}

fn atl_long_lines_start_timer_creates_a_real_registered_one_shot_idle_timer() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_start_timer_creates_a_real_registered_one_shot_idle_timer",
        r##"(let ((atl-long-lines-delay 60)
               (atl-long-lines--timer nil))
         (unwind-protect
             (progn
               (atl-long-lines--start-timer)
               (list
                (atl-long-lines-test-timer-shape
                 atl-long-lines--timer)
                (and
                 (memq
                  atl-long-lines--timer
                  timer-idle-list)
                 t)))
           (when
               (timerp
                atl-long-lines--timer)
             (cancel-timer
              atl-long-lines--timer))))"##,
        expect!["OK ((t atl-long-lines-do-toggle nil nil idle) t)"],
    )
}

fn atl_long_lines_real_reschedule_unregisters_old_idle_timer_and_registers_new_one()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_real_reschedule_unregisters_old_idle_timer_and_registers_new_one",
        r##"(let ((atl-long-lines-delay 60)
               (atl-long-lines--timer nil)
               old
               new)
         (unwind-protect
             (progn
               (atl-long-lines--start-timer)
               (setq old
                     atl-long-lines--timer)
               (atl-long-lines--start-timer)
               (setq new
                     atl-long-lines--timer)
               (list
                (timerp old)
                (timerp new)
                (eq old new)
                (and
                 (memq old timer-idle-list)
                 t)
                (and
                 (memq new timer-idle-list)
                 t)))
           (when
               (timerp new)
             (cancel-timer new))
           (when
               (timerp old)
             (cancel-timer old))))"##,
        expect!["OK (t t nil nil t)"],
    )
}

fn atl_long_lines_disabling_mode_removes_hook_but_leaves_already_scheduled_timer_active()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_disabling_mode_removes_hook_but_leaves_already_scheduled_timer_active",
        r##"(with-temp-buffer
         (let ((atl-long-lines-delay 60)
               (atl-long-lines--timer nil)
               timer)
           (unwind-protect
               (progn
                 (atl-long-lines-mode 1)
                 (run-hooks
                  'post-command-hook)
                 (setq timer
                       atl-long-lines--timer)
                 (atl-long-lines-mode -1)
                 (list
                  atl-long-lines-mode
                  (atl-long-lines-test-hook-count
                   #'atl-long-lines--start-timer
                   post-command-hook)
                  (timerp timer)
                  (and
                   (memq
                    timer
                    timer-idle-list)
                   t)
                  (eq
                   timer
                   atl-long-lines--timer)))
             (when
                 (timerp timer)
               (cancel-timer timer)))))"##,
        expect!["OK (nil 0 t t t)"],
    )
}

pub(super) fn timers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_long_lines_start_timer_schedules_one_shot_idle_callback_with_custom_delay(),
        atl_long_lines_start_timer_cancels_an_existing_timer_before_rescheduling(),
        atl_long_lines_start_timer_does_not_cancel_a_non_timer_sentinel(),
        atl_long_lines_repeated_start_timer_keeps_only_the_latest_scheduled_handle(),
        atl_long_lines_start_timer_forwards_fractional_zero_and_negative_delays_unchanged(),
        atl_long_lines_start_timer_creates_a_real_registered_one_shot_idle_timer(),
        atl_long_lines_real_reschedule_unregisters_old_idle_timer_and_registers_new_one(),
        atl_long_lines_disabling_mode_removes_hook_but_leaves_already_scheduled_timer_active(),
    ]
}
