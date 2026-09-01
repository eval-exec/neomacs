use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_run_rejects_empty_and_each_non_callable_stage_before_scheduling() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_run_rejects_empty_and_each_non_callable_stage_before_scheduling",
        r##"(let ((cases
                (list
                 nil
                 '(not-defined)
                 (list
                  #'ignore
                  42)
                 (list
                  #'ignore
                  "not-a-function")
                 (list
                  #'ignore
                  nil))))
         (mapcar
          (lambda (functions)
            (let ((asyncloop-objects nil))
              (list
               functions
               (asyncloop-test-error
                (lambda ()
                  (asyncloop-run functions)))
               asyncloop-objects)))
          cases))"##,
        expect![[
            r#"OK ((nil (:signal cl-assertion-failed (funs)) nil) ((not-defined) (:signal error ("Not a function or not yet defined as such: not-defined")) nil) ((ignore 42) (:signal error ("Not a function or not yet defined as such: 42")) nil) ((ignore "not-a-function") (:signal error ("Not a function or not yet defined as such: not-a-function")) nil) ((ignore nil) (:signal error ("Not a function or not yet defined as such: nil")) nil))"#
        ]],
    )
}

fn asyncloop_create_rejects_unknown_keyword_and_odd_constructor_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_create_rejects_unknown_keyword_and_odd_constructor_arguments",
        r##"(list
         (asyncloop-test-error
          (lambda ()
            (asyncloop-create
             :unknown-slot 1)))
         (asyncloop-test-error
          (lambda ()
            (asyncloop-create
             :paused)))
         (asyncloop-test-error
          (lambda ()
            (asyncloop-create
             'not-a-keyword
             1)))
         (let ((loop
                (asyncloop-create
                 :paused :truthy
                 :timer nil)))
           (list
            (asyncloop-paused loop)
            (asyncloop-timer loop))))"##,
        expect![[
            r#"OK ((:signal error ("Keyword argument :unknown-slot not one of (:starttime :log-buffer :immediate-break-on-user-activity :timer :paused :remainder :scheduled :just-launched)")) (:signal error ("Missing argument for :paused")) (:signal error ("Keyword argument not-a-keyword not one of (:starttime :log-buffer :immediate-break-on-user-activity :timer :paused :remainder :scheduled :just-launched)")) (:truthy nil))"#
        ]],
    )
}

fn asyncloop_lifecycle_functions_reject_non_loop_values_with_exact_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_lifecycle_functions_reject_non_loop_values_with_exact_signals",
        r##"(mapcar
         (lambda (operation)
           (list
            operation
            (asyncloop-test-error
             (lambda ()
               (funcall operation
                        :not-a-loop)))))
         '(asyncloop-cancel
           asyncloop-pause
           asyncloop-resume
           asyncloop-schedule
           asyncloop-eat
           asyncloop-chomp
           asyncloop-log))"##,
        expect![
            "OK ((asyncloop-cancel (:signal wrong-type-argument (asyncloop :not-a-loop))) (asyncloop-pause (:signal wrong-type-argument (asyncloop :not-a-loop))) (asyncloop-resume (:signal wrong-type-argument (asyncloop :not-a-loop))) (asyncloop-schedule (:signal wrong-type-argument (asyncloop :not-a-loop))) (asyncloop-eat (:signal wrong-type-argument (asyncloop :not-a-loop))) (asyncloop-chomp (:signal wrong-type-argument (asyncloop :not-a-loop))) (asyncloop-log (:signal wrong-type-argument (asyncloop :not-a-loop))))"
        ],
    )
}

fn asyncloop_non_immediate_interruption_cancels_remaining_series_and_records_reason()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_non_immediate_interruption_cancels_remaining_series_and_records_reason",
        r##"(let ((loop
                (asyncloop-create
                 :scheduled t
                 :just-launched t
                 :starttime
                 (current-time)
                 :remainder
                 (list
                  (lambda (_loop)
                    :interrupted-worker)
                  (lambda (_loop)
                    :must-not-run))))
               logged
               chomped)
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
               #'ignore)
              ((symbol-function
                'asyncloop-chomp)
               (lambda (received-loop)
                 (setq chomped
                       (eq received-loop loop))
                 nil))
              ((symbol-function
                'cancel-timer)
               #'asyncloop-test-cancel-timer)
              ((symbol-function
                'input-pending-p)
               (lambda () nil)))
           (list
            (asyncloop-eat loop)
            chomped
            (asyncloop-remainder loop)
            (asyncloop-scheduled loop)
            (asyncloop-paused loop)
            (asyncloop-just-launched loop)
            logged)))"##,
        expect![[r#"OK (#1=("Interrupted by a quit, cancelling loop") t nil nil nil nil #1#)"#]],
    )
}

fn asyncloop_immediate_worker_error_resignals_without_silently_advancing_stage() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asyncloop_immediate_worker_error_resignals_without_silently_advancing_stage",
        r##"(let ((loop
                (asyncloop-create
                 :scheduled t
                 :just-launched t
                 :immediate-break-on-user-activity t
                 :starttime
                 (current-time)
                 :remainder
                 (list
                  (lambda (_loop)
                    (signal
                     'wrong-type-argument
                     '(integerp "record-id")))
                  (lambda (_loop)
                    :must-not-run))))
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
               #'ignore)
              ((symbol-function
                'input-pending-p)
               (lambda () nil)))
           (list
            (asyncloop-test-error
             (lambda ()
               (asyncloop-eat loop)))
            (length
             (asyncloop-remainder loop))
            (asyncloop-scheduled loop)
            (asyncloop-just-launched loop)
            logged)))"##,
        expect![[
            r#"OK ((:signal wrong-type-argument (integerp "record-id")) 2 nil nil ("During lambda: (wrong-type-argument integerp \"record-id\")"))"#
        ]],
    )
}

pub(super) fn errors_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_run_rejects_empty_and_each_non_callable_stage_before_scheduling(),
        asyncloop_create_rejects_unknown_keyword_and_odd_constructor_arguments(),
        asyncloop_lifecycle_functions_reject_non_loop_values_with_exact_signals(),
        asyncloop_non_immediate_interruption_cancels_remaining_series_and_records_reason(),
        asyncloop_immediate_worker_error_resignals_without_silently_advancing_stage(),
    ]
}
