use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_pause_resume_and_cancel_form_a_complete_reusable_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_pause_resume_and_cancel_form_a_complete_reusable_lifecycle",
        r##"(let (events)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((buffer
                   (generate-new-buffer
                    " *asyncloop-lifecycle*"))
                  (loop
                   (asyncloop-create
                    :log-buffer buffer
                    :remainder
                    '(stage-2 stage-3)
                    :scheduled t
                    :just-launched t)))
             (unwind-protect
                 (let ((before
                        (list
                         (asyncloop-paused loop)
                         (asyncloop-scheduled loop)
                         (asyncloop-just-launched loop)
                         (asyncloop-remainder loop))))
                   (asyncloop-pause loop)
                   (push
                    (list
                     :paused
                     (asyncloop-paused loop)
                     (asyncloop-scheduled loop)
                     (asyncloop-just-launched loop)
                     (asyncloop-remainder loop))
                    events)
                   (asyncloop-resume loop)
                   (push
                    (list
                     :resumed
                     (asyncloop-paused loop)
                     (asyncloop-scheduled loop)
                     (asyncloop-just-launched loop)
                     (asyncloop-remainder loop)
                     (asyncloop-timer loop))
                    events)
                   (asyncloop-cancel loop)
                   (push
                    (list
                     :cancelled
                     (asyncloop-paused loop)
                     (asyncloop-scheduled loop)
                     (asyncloop-just-launched loop)
                     (asyncloop-remainder loop))
                    events)
                   (list
                    before
                    (nreverse events)
                    (sort
                     (copy-sequence
                      asyncloop-test-cancelled)
                     #'<)
                    (asyncloop-test-log-text buffer)))
               (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((nil t t #1=(stage-2 stage-3)) ((:paused t nil nil #1#) (:resumed nil t nil #1# (:asyncloop-test-timer 1)) (:cancelled nil nil nil nil)) (1) "<TIME>: Loop told to pause\n<TIME>: Loop told to resume\n<TIME>: Loop told to cancel\n")"#
        ]],
    )
}

fn asyncloop_worker_can_pause_mid_series_then_resume_remaining_practical_work() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_worker_can_pause_mid_series_then_resume_remaining_practical_work",
        r##"(let (events loop)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setq loop
                 (asyncloop-run
                  (list
                   (lambda (received-loop)
                     (push :loaded events)
                     (asyncloop-pause received-loop)
                     :paused-after-load)
                   (lambda (_loop)
                     (push :transformed events)
                     :transformed)
                   (lambda (_loop)
                     (push :saved events)
                     :saved))))
           (let ((first-trace
                  (asyncloop-test-drain)))
             (let ((paused-state
                    (list
                     (reverse events)
                     (asyncloop-paused loop)
                     (asyncloop-scheduled loop)
                     (length
                      (asyncloop-remainder loop)))))
               (asyncloop-resume loop)
               (let ((second-trace
                      (asyncloop-test-drain)))
                 (list
                  first-trace
                  paused-state
                  second-trace
                  (reverse events)
                  (asyncloop-paused loop)
                  (asyncloop-scheduled loop)
                  (asyncloop-remainder loop)))))))"##,
        expect![
            "OK (((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) ((:loaded) t nil 2) ((:ran :at 0 :id 2 :repeat nil :function asyncloop-eat)) (:loaded :transformed :saved) nil nil nil)"
        ],
    )
}

fn asyncloop_worker_can_cancel_mid_series_and_prevent_all_followup_side_effects() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_worker_can_cancel_mid_series_and_prevent_all_followup_side_effects",
        r##"(let (events loop)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setq loop
                 (asyncloop-run
                  (list
                   (lambda (received-loop)
                     (push :validated events)
                     (asyncloop-cancel
                      received-loop
                      'quietly)
                     :cancelled-invalid-job)
                   (lambda (_loop)
                     (push :should-not-transform events))
                   (lambda (_loop)
                     (push :should-not-save events)))))
           (let ((trace
                  (asyncloop-test-drain)))
             (list
              trace
              events
              (asyncloop-paused loop)
              (asyncloop-scheduled loop)
              (asyncloop-just-launched loop)
              (asyncloop-remainder loop)
              (sort
               (copy-sequence
                asyncloop-test-cancelled)
               #'<)))))"##,
        expect![
            "OK (((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) (:validated) nil nil nil nil (1))"
        ],
    )
}

fn asyncloop_reset_all_cancels_every_registered_loop_and_clears_registry() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_reset_all_cancels_every_registered_loop_and_clears_registry",
        r##"(let* ((buffer-a
                 (generate-new-buffer
                  " *asyncloop-reset-a*"))
                (buffer-b
                 (generate-new-buffer
                  " *asyncloop-reset-b*"))
                (loop-a
                 (asyncloop-create
                  :log-buffer buffer-a
                  :timer
                  '(:asyncloop-test-timer 11)
                  :paused t
                  :scheduled t
                  :just-launched t
                  :remainder '(a)))
                (loop-b
                 (asyncloop-create
                  :log-buffer buffer-b
                  :timer
                  '(:asyncloop-test-timer 22)
                  :paused t
                  :scheduled t
                  :just-launched t
                  :remainder '(b)))
                (asyncloop-objects
                 `((101 . ,loop-a)
                   (202 . ,loop-b))))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'cancel-timer)
                   #'asyncloop-test-cancel-timer))
               (asyncloop-reset-all)
               (list
                asyncloop-objects
                (sort
                 (copy-sequence
                  asyncloop-test-cancelled)
                 #'<)
                (list
                 (asyncloop-remainder loop-a)
                 (asyncloop-paused loop-a)
                 (asyncloop-scheduled loop-a)
                 (asyncloop-just-launched loop-a))
                (list
                 (asyncloop-remainder loop-b)
                 (asyncloop-paused loop-b)
                 (asyncloop-scheduled loop-b)
                 (asyncloop-just-launched loop-b))
                (asyncloop-test-log-text buffer-a)
                (asyncloop-test-log-text buffer-b)))
           (kill-buffer buffer-a)
           (kill-buffer buffer-b)))"##,
        expect![[
            r#"OK (nil (11 22) (nil nil nil nil) (nil nil nil nil) "<TIME>: Loop told to cancel\n<TIME>: All asyncloops reset by command\n" "<TIME>: Loop told to cancel\n<TIME>: All asyncloops reset by command\n")"#
        ]],
    )
    .fresh_process()
}

fn asyncloop_reset_all_ignores_one_broken_loop_and_still_wipes_registry() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_reset_all_ignores_one_broken_loop_and_still_wipes_registry",
        r##"(let* ((healthy
                 (asyncloop-create
                  :timer
                  '(:asyncloop-test-timer 7)
                  :scheduled t
                  :remainder '(work)))
                (asyncloop-objects
                 `((1 . :not-a-loop)
                   (2 . ,healthy))))
         (cl-letf
             (((symbol-function
                'cancel-timer)
               #'asyncloop-test-cancel-timer))
           (list
            (asyncloop-reset-all)
            asyncloop-objects
            (asyncloop-remainder healthy)
            (asyncloop-scheduled healthy)
            asyncloop-test-cancelled)))"##,
        expect!["OK (nil nil (work) t nil)"],
    )
    .fresh_process()
}

fn asyncloop_notify_simultaneity_informs_current_and_other_idle_loops() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_notify_simultaneity_informs_current_and_other_idle_loops",
        r##"(let* ((this
                 (asyncloop-create
                  :timer :this-timer))
                (other-active
                 (asyncloop-create
                  :timer :other-timer))
                (other-inactive
                 (asyncloop-create
                  :timer :inactive-timer))
                (asyncloop-objects
                 `((1 . ,other-active)
                   (2 . ,other-inactive)))
                (original-timer-idle-list
                 timer-idle-list)
                logged)
         (unwind-protect
             (progn
               (setq timer-idle-list
                     '(:other-timer))
               (cl-letf
                   (((symbol-function
                      'asyncloop-log)
                     (lambda (loop format-string &rest arguments)
                       (push
                        (list
                         (cond
                          ((eq loop this)
                           :this)
                          ((eq loop other-active)
                           :active)
                          (t
                           :inactive))
                         (apply #'format
                                format-string
                                arguments))
                        logged))))
                 (list
                  (member
                   (asyncloop-timer other-active)
                   timer-idle-list)
                  (asyncloop-notify-simultaneity this)
                  (nreverse logged))))
           (setq timer-idle-list
                 original-timer-idle-list)))"##,
        expect![[
            r#"OK ((:other-timer) nil ((:this "Two or more asyncloops running, please wait...") (:active "Two or more asyncloops running, please wait...")))"#
        ]],
    )
}

fn asyncloop_notify_simultaneity_is_silent_when_no_other_idle_loop_is_active() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_notify_simultaneity_is_silent_when_no_other_idle_loop_is_active",
        r##"(let* ((this
                 (asyncloop-create
                  :timer :this-timer))
                (other
                 (asyncloop-create
                  :timer :inactive-timer))
                (asyncloop-objects
                 `((1 . ,other)))
                (original-timer-idle-list
                 timer-idle-list)
                logged)
         (unwind-protect
             (progn
               (setq timer-idle-list nil)
               (cl-letf
                   (((symbol-function
                      'asyncloop-log)
                     (lambda (&rest arguments)
                       (push arguments logged))))
                 (list
                  (asyncloop-notify-simultaneity this)
                  logged)))
           (setq timer-idle-list
                 original-timer-idle-list)))"##,
        expect!["OK (nil nil)"],
    )
}

fn asyncloop_resume_reports_invariant_when_called_on_just_launched_loop() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_resume_reports_invariant_when_called_on_just_launched_loop",
        r##"(let ((loop
                (asyncloop-create
                 :just-launched t
                 :paused t))
               messages)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setf
            (asyncloop-just-launched loop)
            t
            (asyncloop-paused loop)
            t)
           (cl-letf
               (((symbol-function
                  'asyncloop-log)
                 (lambda (_loop format-string &rest arguments)
                   (apply #'format
                          format-string
                          arguments)))
                ((symbol-function
                  'message)
                 (lambda (format-string &rest arguments)
                   (let ((text
                          (apply #'format
                                 format-string
                                 arguments)))
                     (push text messages)
                     text))))
             (list
              (asyncloop-resume loop)
              (asyncloop-paused loop)
              (asyncloop-scheduled loop)
              (asyncloop-just-launched loop)
              (asyncloop-timer loop)
              messages))))"##,
        expect![[
            r#"OK ("Please report bug: (asyncloop-just-launched loop) was t" nil t t (:asyncloop-test-timer 1) ("Please report bug: (asyncloop-just-launched loop) was t"))"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_pause_resume_and_cancel_form_a_complete_reusable_lifecycle(),
        asyncloop_worker_can_pause_mid_series_then_resume_remaining_practical_work(),
        asyncloop_worker_can_cancel_mid_series_and_prevent_all_followup_side_effects(),
        asyncloop_reset_all_cancels_every_registered_loop_and_clears_registry(),
        asyncloop_reset_all_ignores_one_broken_loop_and_still_wipes_registry(),
        asyncloop_notify_simultaneity_informs_current_and_other_idle_loops(),
        asyncloop_notify_simultaneity_is_silent_when_no_other_idle_loop_is_active(),
        asyncloop_resume_reports_invariant_when_called_on_just_launched_loop(),
    ]
}
