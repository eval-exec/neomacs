use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_real_idle_timer_event_handler_runs_series_after_current_call_stack() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_real_idle_timer_event_handler_runs_series_after_current_call_stack",
        r##"(let (events loop timer)
         (unwind-protect
             (progn
               (asyncloop-reset-all)
               (setq loop
                     (asyncloop-run
                      (list
                       (lambda (_loop)
                         (push :load events))
                       (lambda (_loop)
                         (push :transform events))
                       (lambda (_loop)
                         (push :save events)))))
               (setq timer
                     (asyncloop-timer loop))
               (let ((immediate
                      (list
                       events
                       (timerp timer)
                       (and
                        (memq timer
                              timer-idle-list)
                        t)
                       (asyncloop-scheduled loop)
                       (asyncloop-just-launched loop)
                       (length
                        (asyncloop-remainder loop)))))
                 (timer-event-handler timer)
                 (list
                  immediate
                  (nreverse events)
                  (asyncloop-scheduled loop)
                  (asyncloop-just-launched loop)
                  (asyncloop-remainder loop))))
           (asyncloop-reset-all)))"##,
        expect!["OK ((nil t t t t 3) (:load :transform :save) nil nil nil)"],
    )
}

fn asyncloop_real_idle_timer_pause_then_resume_preserves_remaining_stage_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_real_idle_timer_pause_then_resume_preserves_remaining_stage_order",
        r##"(let (events loop first-timer second-timer)
         (unwind-protect
             (progn
               (asyncloop-reset-all)
               (setq loop
                     (asyncloop-run
                      (list
                       (lambda (received-loop)
                         (push :load events)
                         (asyncloop-pause
                          received-loop))
                       (lambda (_loop)
                         (push :transform events))
                       (lambda (_loop)
                         (push :save events)))))
               (setq first-timer
                     (asyncloop-timer loop))
               (timer-event-handler first-timer)
               (let ((paused-state
                      (list
                       (reverse events)
                       (asyncloop-paused loop)
                       (asyncloop-scheduled loop)
                       (length
                        (asyncloop-remainder loop)))))
                 (asyncloop-resume loop)
                 (setq second-timer
                       (asyncloop-timer loop))
                 (timer-event-handler second-timer)
                 (list
                  paused-state
                  (eq first-timer second-timer)
                  (nreverse events)
                  (asyncloop-paused loop)
                  (asyncloop-scheduled loop)
                  (asyncloop-remainder loop))))
           (asyncloop-reset-all)))"##,
        expect!["OK (((:load) t nil 2) nil (:load :transform :save) nil nil nil)"],
    )
}

fn asyncloop_real_idle_timer_cancel_removes_pending_dispatch_and_worker_side_effects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_real_idle_timer_cancel_removes_pending_dispatch_and_worker_side_effects",
        r##"(let (events loop timer before)
         (unwind-protect
             (progn
               (asyncloop-reset-all)
               (setq loop
                     (asyncloop-run
                      (list
                       (lambda (_loop)
                         (push :must-not-run events)))))
               (setq timer
                     (asyncloop-timer loop)
                     before
                     (list
                      (timerp timer)
                      (and
                       (memq timer
                             timer-idle-list)
                       t)
                      (asyncloop-scheduled loop)
                      (asyncloop-just-launched loop)))
               (asyncloop-cancel loop 'quietly)
               (list
                before
                events
                (and
                 (memq timer
                       timer-idle-list)
                 t)
                (asyncloop-remainder loop)
                (asyncloop-paused loop)
                (asyncloop-scheduled loop)
                (asyncloop-just-launched loop)))
           (asyncloop-reset-all)))"##,
        expect!["OK ((t t t t) nil nil nil nil nil nil)"],
    )
}

fn asyncloop_real_idle_timer_handlers_follow_real_queue_order_without_cross_talk() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_real_idle_timer_handlers_follow_real_queue_order_without_cross_talk",
        r##"(let (events loop-a loop-b timer-a timer-b queued)
         (unwind-protect
             (progn
               (asyncloop-reset-all)
               (setq loop-a
                     (asyncloop-run
                      (list
                       (lambda (_loop)
                         (push :a-load events))
                       (lambda (_loop)
                         (push :a-save events)))))
               (setq loop-b
                     (asyncloop-run
                      (list
                       (lambda (_loop)
                         (push :b-load events))
                       (lambda (_loop)
                         (push :b-save events)))
                      :on-interrupt-discovered
                      #'ignore))
               (setq timer-a
                     (asyncloop-timer loop-a)
                     timer-b
                     (asyncloop-timer loop-b))
               (setq queued
                     (seq-filter
                      (lambda (timer)
                        (or
                         (eq timer timer-a)
                         (eq timer timer-b)))
                      timer-idle-list))
               (let ((queued-order
                      (mapcar
                       (lambda (timer)
                         (if
                             (eq timer timer-a)
                             :a
                           :b))
                       queued)))
                 (dolist (timer queued)
                   (timer-event-handler timer))
               (list
                queued-order
                (timerp timer-a)
                (timerp timer-b)
                (eq loop-a loop-b)
                (nreverse events)
                (asyncloop-remainder loop-a)
                (asyncloop-remainder loop-b)
                (length asyncloop-objects))))
           (asyncloop-reset-all)))"##,
        expect!["OK ((:b :a) t t nil (:b-load :b-save :a-load :a-save) nil nil 2)"],
    )
}

pub(super) fn timers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_real_idle_timer_event_handler_runs_series_after_current_call_stack(),
        asyncloop_real_idle_timer_pause_then_resume_preserves_remaining_stage_order(),
        asyncloop_real_idle_timer_cancel_removes_pending_dispatch_and_worker_side_effects(),
        asyncloop_real_idle_timer_handlers_follow_real_queue_order_without_cross_talk(),
    ]
}
