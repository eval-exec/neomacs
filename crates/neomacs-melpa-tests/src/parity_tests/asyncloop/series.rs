use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_run_processes_a_practical_import_pipeline_once_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_run_processes_a_practical_import_pipeline_once_in_order",
        r##"(let ((records
                '(" one " "two" " one " "three"))
               state
               events
               loop)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setq loop
                 (asyncloop-run
                  (list
                   (lambda (_loop)
                     (setq state
                           (mapcar
                            #'string-trim
                            records))
                     (push
                      (list :normalized
                            (copy-sequence state))
                      events)
                     (length state))
                   (lambda (_loop)
                     (setq state
                           (delete-dups state))
                     (push
                      (list :deduplicated
                            (copy-sequence state))
                      events)
                     (length state))
                   (lambda (_loop)
                     (setq state
                           (sort state #'string<))
                     (push
                      (list :sorted
                            (copy-sequence state))
                      events)
                     state))))
           (let ((before
                  (list
                   events
                   (asyncloop-scheduled loop)
                   (asyncloop-just-launched loop)
                   (length
                    (asyncloop-remainder loop))))
                 (trace
                  (asyncloop-test-drain)))
             (list
              before
              trace
              state
              (nreverse events)
              (asyncloop-scheduled loop)
              (asyncloop-just-launched loop)
              (asyncloop-remainder loop)
              (length asyncloop-objects)))))"##,
        expect![[
            r#"OK ((nil t t 3) ((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) ("one" "three" "two") ((:normalized ("one" "two" "one" "three")) (:deduplicated ("one" "two" "three")) (:sorted ("one" "three" "two"))) nil nil nil 1)"#
        ]],
    )
}

fn asyncloop_run_deduplicates_back_to_back_hook_invocations_before_launch() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_run_deduplicates_back_to_back_hook_invocations_before_launch",
        r##"(let (events)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((functions
                   (list
                    (lambda (_loop)
                      (push :ran events))))
                  (first
                   (asyncloop-run functions))
                  (second
                   (asyncloop-run functions))
                  (queued-before
                   (length
                    asyncloop-test-timer-queue))
                  (trace
                   (asyncloop-test-drain)))
             (list
              (eq first second)
              queued-before
              trace
              events
              (length asyncloop-objects)
              (asyncloop-remainder first)))))"##,
        expect!["OK (t 1 ((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) (:ran) 1 nil)"],
    )
}

fn asyncloop_run_deduplicates_already_scheduled_hook_invocation_and_logs_reason() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_run_deduplicates_already_scheduled_hook_invocation_and_logs_reason",
        r##"(let (events logged)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((functions
                   (list
                    (lambda (_loop)
                      (push :ran events))))
                  (loop
                   (asyncloop-run functions)))
             (setf
              (asyncloop-just-launched loop)
              nil)
             (cl-letf
                 (((symbol-function
                    'asyncloop-log)
                   (lambda (_loop format-string &rest arguments)
                     (push
                      (apply #'format
                             format-string
                             arguments)
                      logged))))
               (let ((same
                      (asyncloop-run functions)))
                 (list
                  (eq loop same)
                  (length
                   asyncloop-test-timer-queue)
                  logged
                  (asyncloop-test-drain)
                  events))))))"##,
        expect![[
            r#"OK (t 1 ("Already running, letting it continue") ((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) (:ran))"#
        ]],
    )
}

fn asyncloop_completed_loop_reuses_identity_but_runs_full_series_again() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_completed_loop_reuses_identity_but_runs_full_series_again",
        r##"(let (events)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((functions
                   (list
                    (lambda (_loop)
                      (push
                       (1+
                        (length events))
                       events))))
                  (first
                   (asyncloop-run functions))
                  (first-trace
                   (asyncloop-test-drain))
                  (second
                   (asyncloop-run functions))
                  (queued-again
                   (length
                    asyncloop-test-timer-queue))
                  (second-trace
                   (asyncloop-test-drain)))
             (list
              (eq first second)
              first-trace
              queued-again
              second-trace
              (nreverse events)
              (length asyncloop-objects)
              (asyncloop-remainder second)))))"##,
        expect![
            "OK (t ((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) 1 ((:ran :at 0 :id 2 :repeat nil :function asyncloop-eat)) (1 2) 1 nil)"
        ],
    )
}

fn asyncloop_identity_distinguishes_behavioral_options_and_log_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_identity_distinguishes_behavioral_options_and_log_destination",
        r##"(let ((buffer
                (generate-new-buffer
                 " *asyncloop-identity*")))
         (unwind-protect
             (progn
               (asyncloop-test-reset)
               (asyncloop-test-with-scheduler
                 (let* ((functions
                         (list #'ignore))
                        (plain
                         (asyncloop-run functions))
                        (immediate
                         (asyncloop-run
                          functions
                          :immediate-break-on-user-activity t))
                        (logged
                         (asyncloop-run
                          functions
                          :log-buffer-name
                          (buffer-name buffer))))
                   (list
                    (eq plain immediate)
                    (eq plain logged)
                    (eq immediate logged)
                    (length asyncloop-objects)
                    (mapcar
                     (lambda (loop)
                       (list
                        (asyncloop-immediate-break-on-user-activity loop)
                        (and
                         (asyncloop-log-buffer loop)
                         (buffer-name
                          (asyncloop-log-buffer loop)))))
                     (list plain immediate logged))
                    (length
                     asyncloop-test-timer-queue)))))
           (kill-buffer buffer)))"##,
        expect![[r#"OK (nil nil nil 3 ((nil nil) (t nil) (nil " *asyncloop-identity*")) 3)"#]],
    )
}

fn asyncloop_run_recovers_half_finished_series_and_calls_recovery_hook_first() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_run_recovers_half_finished_series_and_calls_recovery_hook_first",
        r##"(let (events)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((functions
                   (list
                    (lambda (_loop)
                      (push :first events))
                    (lambda (_loop)
                      (push :second events))
                    (lambda (_loop)
                      (push :third events))))
                  (recovery
                   (lambda (_loop)
                     (push :recovered events)
                     :recovery-complete))
                  (loop
                   (asyncloop-run
                    functions
                    :on-interrupt-discovered recovery)))
             (setf
              (asyncloop-just-launched loop)
              nil
              (asyncloop-scheduled loop)
              nil
              (asyncloop-remainder loop)
              (cdr functions))
             (let ((same
                    (asyncloop-run
                     functions
                     :on-interrupt-discovered recovery))
                   (trace
                    (asyncloop-test-drain)))
               (list
                (eq loop same)
                trace
                (nreverse events)
                (asyncloop-remainder loop)
                (asyncloop-scheduled loop)
                (asyncloop-just-launched loop))))))"##,
        expect![
            "OK (t ((:skipped :at 0 :id 1) (:ran :at 0 :id 2 :repeat nil :function asyncloop-eat)) (:recovered :second :third) nil nil nil)"
        ],
    )
}

fn asyncloop_recovery_hook_can_cancel_stale_transaction_instead_of_resuming() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_recovery_hook_can_cancel_stale_transaction_instead_of_resuming",
        r##"(let (events)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((functions
                   (list
                    (lambda (_loop)
                      (push :first events))
                    (lambda (_loop)
                      (push :unsafe-second events))))
                  (recovery
                   (lambda (loop)
                     (push :cleanup events)
                     (asyncloop-cancel
                      loop
                      'quietly)
                     :cancelled-stale-transaction))
                  (loop
                   (asyncloop-run
                    functions
                    :on-interrupt-discovered recovery)))
             (setf
              (asyncloop-just-launched loop)
              nil
              (asyncloop-scheduled loop)
              nil
              (asyncloop-remainder loop)
              (cdr functions))
             (asyncloop-run
              functions
              :on-interrupt-discovered recovery)
             (list
              (asyncloop-test-drain)
              (nreverse events)
              (asyncloop-remainder loop)
              (asyncloop-scheduled loop)
              (asyncloop-just-launched loop)
              (sort
               (copy-sequence
                asyncloop-test-cancelled)
               #'<)))))"##,
        expect!["OK (((:skipped :at 0 :id 1)) (:cleanup) nil nil nil (1))"],
    )
}

fn asyncloop_run_refuses_implicit_resume_of_a_paused_partial_series() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_run_refuses_implicit_resume_of_a_paused_partial_series",
        r##"(let (events logged)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let* ((functions
                   (list
                    (lambda (_loop)
                      (push :first events))
                    (lambda (_loop)
                      (push :second events))))
                  (loop
                   (asyncloop-run functions)))
             (setf
              (asyncloop-just-launched loop)
              nil
              (asyncloop-scheduled loop)
              nil
              (asyncloop-paused loop)
              t
              (asyncloop-remainder loop)
              (cdr functions))
             (cl-letf
                 (((symbol-function
                    'asyncloop-log)
                   (lambda (_loop format-string &rest arguments)
                     (push
                      (apply #'format
                             format-string
                             arguments)
                      logged))))
               (let ((same
                      (asyncloop-run functions)))
                 (list
                  (eq loop same)
                  logged
                  (length
                   asyncloop-test-timer-queue)
                  events
                  (asyncloop-paused loop)
                  (length
                   (asyncloop-remainder loop))))))))"##,
        expect![[
            r#"OK (t ("Loop was paused, must be explicitly unpaused via `asyncloop-resume' or `asyncloop-cancel'") 1 nil t 1)"#
        ]],
    )
}

fn asyncloop_worker_can_replace_remainder_with_a_runtime_selected_branch() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_worker_can_replace_remainder_with_a_runtime_selected_branch",
        r##"(let (events loop)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (let ((success
                  (lambda (_loop)
                    (push :publish events)
                    :published))
                 (audit
                  (lambda (_loop)
                    (push :audit events)
                    :audited)))
             (setq loop
                   (asyncloop-run
                    (list
                     (lambda (received-loop)
                       (push :validate events)
                       (setf
                        (asyncloop-remainder
                         received-loop)
                        (list
                         t
                         audit
                         success))
                       :valid)
                     (lambda (_loop)
                       (push :obsolete-path events)))))
             (list
              (asyncloop-test-drain)
              (nreverse events)
              (asyncloop-remainder loop)))))"##,
        expect![
            "OK (((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat)) (:validate :audit :publish) nil)"
        ],
    )
}

fn asyncloop_reentrant_same_series_invocation_has_deterministic_single_registry_lifecycle()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_reentrant_same_series_invocation_has_deterministic_single_registry_lifecycle",
        r##"(let (events functions outer-loop nested-loop)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setq functions
                 (list
                  (lambda (_loop)
                    (push :first-enter events)
                    (unless nested-loop
                      (setq nested-loop
                            (asyncloop-run functions)))
                    (push :first-exit events))
                  (lambda (_loop)
                    (push :second events))))
           (setq outer-loop
                 (asyncloop-run functions))
           (let ((trace
                  (asyncloop-test-drain 20)))
             (list
              trace
              (nreverse events)
              (eq outer-loop nested-loop)
              (length asyncloop-objects)
              (asyncloop-remainder outer-loop)
              (asyncloop-scheduled outer-loop)
              asyncloop-test-timer-queue))))"##,
        expect![
            "OK (((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat) (:ran :at 0 :id 2 :repeat nil :function asyncloop-eat)) (:first-enter :first-exit :second) t 1 nil nil nil)"
        ],
    )
}

fn asyncloop_worker_error_preserves_current_stage_then_same_run_retries_from_start()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_worker_error_preserves_current_stage_then_same_run_retries_from_start",
        r##"(let ((attempt 0)
               events
               recovered
               recovery
               functions
               loop)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
           (setq recovery
                 (lambda (_loop)
                   (setq recovered t)))
           (setq functions
                 (list
                  (lambda (_loop)
                    (setq attempt
                          (1+ attempt))
                    (push
                     (list :attempt attempt)
                     events)
                    (when
                        (= attempt 1)
                      (error
                       "transient import failure"))
                    :loaded)
                  (lambda (_loop)
                    (push :saved events)
                    :saved)))
           (setq loop
                 (asyncloop-run
                  functions
                  :on-interrupt-discovered
                  recovery))
           (let ((first
                  (asyncloop-test-error
                   #'asyncloop-test-drain))
                 (after-error
                  (list
                   (length
                    (asyncloop-remainder loop))
                   (asyncloop-scheduled loop)
                   (asyncloop-just-launched loop))))
             (asyncloop-run
              functions
              :on-interrupt-discovered
              recovery)
             (list
              first
              after-error
              (asyncloop-test-drain)
              (nreverse events)
              recovered
              attempt
              (asyncloop-remainder loop)))))"##,
        expect![[
            r#"OK ((:signal error ("transient import failure")) (2 nil nil) ((:ran :at 0 :id 2 :repeat nil :function asyncloop-eat)) ((:attempt 1) (:attempt 2) :saved) nil 2 nil)"#
        ]],
    )
}

fn asyncloop_two_simultaneous_series_follow_timer_insertion_order_without_cross_talk()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_two_simultaneous_series_follow_timer_insertion_order_without_cross_talk",
        r##"(let (events loop-a loop-b)
         (asyncloop-test-reset)
         (asyncloop-test-with-scheduler
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
           (list
            (asyncloop-test-drain)
            (nreverse events)
            (asyncloop-remainder loop-a)
            (asyncloop-remainder loop-b)
            (length asyncloop-objects))))"##,
        expect![
            "OK (((:ran :at 0 :id 1 :repeat nil :function asyncloop-eat) (:ran :at 0 :id 2 :repeat nil :function asyncloop-eat)) (:a-load :a-save :b-load :b-save) nil nil 2)"
        ],
    )
}

fn asyncloop_with_slots_reads_and_mutates_live_struct_places_lexically() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_with_slots_reads_and_mutates_live_struct_places_lexically",
        r##"(let ((loop
                (asyncloop-create
                 :paused nil
                 :scheduled t
                 :remainder '(one two))))
         (let ((result
                (asyncloop-with-slots
                    (paused scheduled remainder)
                    loop
                  (setq paused t)
                  (setq scheduled nil)
                  (push 'zero remainder)
                  (list
                   paused
                   scheduled
                   remainder))))
           (list
            result
            (asyncloop-paused loop)
            (asyncloop-scheduled loop)
            (asyncloop-remainder loop))))"##,
        expect!["OK ((t nil #1=(zero one two)) t nil #1#)"],
    )
}

pub(super) fn series_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_run_processes_a_practical_import_pipeline_once_in_order(),
        asyncloop_run_deduplicates_back_to_back_hook_invocations_before_launch(),
        asyncloop_run_deduplicates_already_scheduled_hook_invocation_and_logs_reason(),
        asyncloop_completed_loop_reuses_identity_but_runs_full_series_again(),
        asyncloop_identity_distinguishes_behavioral_options_and_log_destination(),
        asyncloop_run_recovers_half_finished_series_and_calls_recovery_hook_first(),
        asyncloop_recovery_hook_can_cancel_stale_transaction_instead_of_resuming(),
        asyncloop_run_refuses_implicit_resume_of_a_paused_partial_series(),
        asyncloop_worker_can_replace_remainder_with_a_runtime_selected_branch(),
        asyncloop_reentrant_same_series_invocation_has_deterministic_single_registry_lifecycle(),
        asyncloop_worker_error_preserves_current_stage_then_same_run_retries_from_start(),
        asyncloop_two_simultaneous_series_follow_timer_insertion_order_without_cross_talk(),
        asyncloop_with_slots_reads_and_mutates_live_struct_places_lexically(),
    ]
}
