use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_schedule_replaces_prior_timer_and_only_latest_activation_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_schedule_replaces_prior_timer_and_only_latest_activation_runs",
        r##"(let ((loop
                (asyncloop-create))
               logged)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (cl-letf
               (((symbol-function
                  'asyncloop-log)
                 (lambda (_loop format-string &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    logged))))
             (let ((first
                    (progn
                      (asyncloop-schedule loop 5)
                      (asyncloop-timer loop)))
                   second)
               (asyncloop-schedule loop 2)
               (setq second
                     (asyncloop-timer loop))
               (list
                first
                second
                (asyncloop-scheduled loop)
                (sort
                 (copy-sequence
                  asyncloop-test-cancelled)
                 #'<)
                (asyncloop-test-drain)
                (asyncloop-scheduled loop)
                logged)))))"##,
        expect![[
            r#"OK ((:asyncloop-test-timer 1) (:asyncloop-test-timer 2) t (1) ((:ran :at 2 :id 2 :repeat nil :function asyncloop-eat) (:skipped :at 5 :id 1)) nil ("Scheduled loop found cleared, doing nothing"))"#
        ]],
    )
}

fn asyncloop_chomp_executes_real_stateful_pipeline_in_exact_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_chomp_executes_real_stateful_pipeline_in_exact_order",
        r##"(let* ((state
                 '(:value 2))
                events
                (loop
                 (asyncloop-create
                  :starttime
                  (current-time)
                  :remainder
                  (list
                   (lambda (_loop)
                     (plist-put
                      state
                      :value
                      (*
                       3
                       (plist-get state :value)))
                     (push
                      (list :multiply
                            (plist-get state :value))
                      events)
                     :multiplied)
                   (lambda (_loop)
                     (plist-put
                      state
                      :value
                      (+
                       4
                       (plist-get state :value)))
                     (push
                      (list :add
                            (plist-get state :value))
                      events)
                     :added)
                   (lambda (_loop)
                     (plist-put state :saved t)
                     (push
                      (list :save
                            (plist-get state :value))
                      events)
                     :saved)))))
         (cl-letf
             (((symbol-function
                'asyncloop-log)
               (lambda (_loop _format &rest _arguments)
                 nil))
              ((symbol-function
                'input-pending-p)
               (lambda () nil)))
           (list
            (asyncloop-chomp loop)
            state
            (nreverse events)
            (asyncloop-remainder loop)
            asyncloop-recursion-ctr)))"##,
        expect!["OK (t (:value 10 :saved t) ((:multiply 6) (:add 10) (:save 10)) nil 2)"],
    )
}

fn asyncloop_chomp_return_contract_distinguishes_immediate_and_protected_modes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_chomp_return_contract_distinguishes_immediate_and_protected_modes",
        r##"(let (events)
         (cl-labels
             ((make-loop
               (immediate label)
               (asyncloop-create
                :starttime
                (current-time)
                :immediate-break-on-user-activity
                immediate
                :remainder
                (list
                 (lambda (_loop)
                   (push label events)
                   label)))))
           (cl-letf
               (((symbol-function
                  'asyncloop-log)
                 (lambda (&rest _arguments)
                   nil))
                ((symbol-function
                  'input-pending-p)
                 (lambda () nil)))
             (let* ((protected
                     (make-loop nil :protected))
                    (immediate
                     (make-loop t :immediate))
                    (protected-return
                     (asyncloop-chomp protected))
                    (immediate-return
                     (asyncloop-chomp immediate)))
               (list
                protected-return
                immediate-return
                (nreverse events)
                (asyncloop-remainder protected)
                (asyncloop-remainder immediate))))))"##,
        expect!["OK (t nil (:protected :immediate) nil nil)"],
    )
}

fn asyncloop_immediate_mode_preserves_interrupted_stage_then_retries_after_one_second()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_immediate_mode_preserves_interrupted_stage_then_retries_after_one_second",
        r##"(let (events loop pending-input)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setq loop
                 (asyncloop-create
                  :scheduled t
                  :just-launched t
                  :starttime
                  (current-time)
                  :immediate-break-on-user-activity t
                  :remainder
                  (list
                   (lambda (_loop)
                     (push :worker-completed events)
                     :completed))))
           (cl-letf
               (((symbol-function
                  'asyncloop-log)
                 (lambda (&rest _arguments)
                   nil))
                ((symbol-function
                  'asyncloop-notify-simultaneity)
                 #'ignore)
                ((symbol-function
                  'input-pending-p)
                 (lambda ()
                   pending-input)))
             (setq pending-input t)
             (setq unread-command-events
                   '(120))
             (let ((interrupted-return
                    (asyncloop-eat loop))
                   (interrupted-state
                    (list
                     events
                     (length
                      (asyncloop-remainder loop))
                     (asyncloop-scheduled loop)
                     asyncloop-test-now
                     (mapcar
                      (lambda (event)
                        (list
                         (asyncloop-test-event-due event)
                         (asyncloop-test-event-id event)
                         (asyncloop-test-event-function event)))
                      asyncloop-test-timer-queue))))
               (setq pending-input nil)
               (setq unread-command-events nil)
               (list
                interrupted-return
                interrupted-state
                (asyncloop-test-drain)
                (nreverse events)
                (asyncloop-remainder loop)
                (asyncloop-scheduled loop)
                asyncloop-test-now)))))"##,
        expect![
            "OK ((:asyncloop-test-timer 1) (nil 1 t 0 ((1 1 asyncloop-eat))) ((:ran :at 1 :id 1 :repeat nil :function asyncloop-eat)) (:worker-completed) nil nil 1)"
        ],
    )
}

fn asyncloop_chomp_repeat_marker_supports_bounded_real_queue_consumption() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_chomp_repeat_marker_supports_bounded_real_queue_consumption",
        r##"(let ((pending
                '("alpha" "beta" "gamma" "delta"))
               processed
               loop)
         (setq loop
               (asyncloop-create
                :starttime
                (current-time)
                :remainder
                (list
                 (lambda (received-loop)
                   (push
                    (upcase
                     (pop pending))
                    processed)
                   (when pending
                     (push
                      t
                      (asyncloop-remainder
                       received-loop)))
                   (length pending))
                 (lambda (_loop)
                   (push
                    (format
                     "summary:%d"
                     (length processed))
                    processed)
                   :summarized))))
         (cl-letf
             (((symbol-function
                'asyncloop-log)
               (lambda (&rest _arguments)
                 nil))
              ((symbol-function
                'input-pending-p)
               (lambda () nil)))
           (list
            (asyncloop-chomp loop)
            pending
            (nreverse processed)
            (asyncloop-remainder loop)
            asyncloop-recursion-ctr)))"##,
        expect![[r#"OK (t nil ("ALPHA" "BETA" "GAMMA" "DELTA" "summary:4") nil 4)"#]],
    )
}

fn asyncloop_chomp_defers_remaining_work_one_second_when_input_is_pending() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_chomp_defers_remaining_work_one_second_when_input_is_pending",
        r##"(let (events)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let ((loop
                  (asyncloop-create
                   :starttime
                   (current-time)
                   :remainder
                   (list
                    (lambda (_loop)
                      (push :first events))
                    (lambda (_loop)
                      (push :second events))))))
             (cl-letf
                 (((symbol-function
                    'input-pending-p)
                   (lambda () t))
                  ((symbol-function
                    'asyncloop-log)
                   (lambda (&rest _arguments)
                     nil)))
               (let ((return
                      (asyncloop-chomp loop)))
                 (list
                  return
                  events
                  (length
                   (asyncloop-remainder loop))
                  (asyncloop-scheduled loop)
                  (mapcar
                   (lambda (event)
                     (list
                      (asyncloop-test-event-due event)
                      (asyncloop-test-event-id event)
                      (asyncloop-test-event-repeat event)
                      (asyncloop-test-event-function event)
                      :timer-handle
                      (asyncloop-test-event-timer event)))
                   asyncloop-test-timer-queue)))))))"##,
        expect![
            "OK (t (:first) 1 t ((1 1 nil asyncloop-eat :timer-handle (:asyncloop-test-timer 1))))"
        ],
    )
}

fn asyncloop_chomp_prunes_deep_call_stack_after_one_hundred_workers() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_chomp_prunes_deep_call_stack_after_one_hundred_workers",
        r##"(let (executed)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((worker
                   (lambda (_loop)
                     (push
                      (length executed)
                      executed)))
                  (loop
                   (asyncloop-create
                    :starttime
                    (current-time)
                    :remainder
                    (make-list 205 worker))))
             (cl-letf
                 (((symbol-function
                    'asyncloop-log)
                   (lambda (&rest _arguments)
                     nil)))
               (let ((initial-return
                      (asyncloop-chomp loop))
                     (first-wave
                      (length executed))
                     (queued
                      (length
                       asyncloop-test-timer-queue))
                     (trace
                      (asyncloop-test-drain)))
                 (list
                  initial-return
                  first-wave
                  queued
                  (length trace)
                  (length executed)
                  (asyncloop-remainder loop)
                  asyncloop-recursion-ctr))))))"##,
        expect!["OK (t 100 1 2 205 nil 4)"],
    )
}

fn asyncloop_eat_rejects_ghost_activation_without_mutating_queued_work() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_eat_rejects_ghost_activation_without_mutating_queued_work",
        r##"(let* ((worker
                 (lambda (_loop)
                   :should-not-run))
                (loop
                 (asyncloop-create
                  :scheduled nil
                  :just-launched t
                  :remainder
                  (list worker)))
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
            (asyncloop-eat loop)
            (asyncloop-just-launched loop)
            (asyncloop-scheduled loop)
            (eq
             (car
              (asyncloop-remainder loop))
             worker)
            logged)))"##,
        expect![[
            r#"OK (#1=("Unscheduled timer activation. Hands off the wheel, ghost!") nil nil t #1#)"#
        ]],
    )
}

fn asyncloop_eat_handles_scheduled_loop_cleared_before_timer_fires() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_eat_handles_scheduled_loop_cleared_before_timer_fires",
        r##"(let ((loop
                (asyncloop-create
                 :scheduled t
                 :just-launched t
                 :remainder nil))
               logged)
         (cl-letf
             (((symbol-function
                'asyncloop-log)
               (lambda (_loop format-string &rest arguments)
                 (push
                  (apply #'format
                         format-string
                         arguments)
                  logged)))
              ((symbol-function
                'asyncloop-notify-simultaneity)
               (lambda (_loop)
                 (push :notified logged))))
           (list
            (asyncloop-eat loop)
            (asyncloop-just-launched loop)
            (asyncloop-scheduled loop)
            (asyncloop-remainder loop)
            logged)))"##,
        expect![[r#"OK (#1=("Scheduled loop found cleared, doing nothing") nil nil nil #1#)"#]],
    )
}

pub(super) fn scheduling_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_schedule_replaces_prior_timer_and_only_latest_activation_runs(),
        asyncloop_chomp_executes_real_stateful_pipeline_in_exact_order(),
        asyncloop_chomp_return_contract_distinguishes_immediate_and_protected_modes(),
        asyncloop_immediate_mode_preserves_interrupted_stage_then_retries_after_one_second(),
        asyncloop_chomp_repeat_marker_supports_bounded_real_queue_consumption(),
        asyncloop_chomp_defers_remaining_work_one_second_when_input_is_pending(),
        asyncloop_chomp_prunes_deep_call_stack_after_one_hundred_workers(),
        asyncloop_eat_rejects_ghost_activation_without_mutating_queued_work(),
        asyncloop_eat_handles_scheduled_loop_cleared_before_timer_fires(),
    ]
}
