use expect_test::expect;

use super::ParityBatchCase;

fn async1_default_aggregator_ports_empty_single_and_multiple_upstream_cases_strictly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_default_aggregator_ports_empty_single_and_multiple_upstream_cases_strictly",
        r##"(list
         (async1-default-aggregator nil)
         (async1-default-aggregator
          '("a"))
         (async1-default-aggregator
          '("a" "b" "c"))
         (async1-default-aggregator
          '("α" "" "line\nbreak"))
         (async1-test-error
          (lambda ()
            (async1-default-aggregator
             '("ok" 7 "after")))))"##,
        expect![[
            r#"OK ("" "a" "{a, b, c}" "{α, , line\nbreak}" (:error wrong-type-argument (sequencep 7)))"#
        ]],
    )
}

fn async1_create_function_preserves_a_live_function_and_its_callback_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_create_function_preserves_a_live_function_and_its_callback_contract",
        r##"(let (events)
         (let* ((step
                 (lambda (data callback)
                   (push
                    (list :step data)
                    events)
                   (funcall callback
                            (concat data
                                    " -> transformed"))))
                (created
                 (async1-create-function step))
                (return
                 (funcall created
                          "input"
                          (lambda (result)
                            (push
                             (list :callback result)
                             events)
                            :callback-return))))
           (list
            (eq step created)
            return
            (nreverse events))))"##,
        expect![[
            r#"OK (t :callback-return ((:step "input") (:callback "input -> transformed")))"#
        ]],
    )
}

fn async1_create_function_preserves_a_fbound_symbol_without_wrapping_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_create_function_preserves_a_fbound_symbol_without_wrapping_it",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'async1-test-symbol-step)
               (lambda (data callback)
                 (push data events)
                 (funcall callback
                          (upcase data)))))
           (let ((created
                  (async1-create-function
                   'async1-test-symbol-step))
                 callback-value)
             (list
              created
              (eq created
                  'async1-test-symbol-step)
              (funcall created
                       "payload"
                       (lambda (value)
                         (setq callback-value value)
                         :done))
              callback-value
              events))))"##,
        expect![[r#"OK (async1-test-symbol-step t :done "PAYLOAD" ("payload"))"#]],
    )
}

fn async1_create_function_explicit_plist_schedules_exact_delay_data_and_suffix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_create_function_explicit_plist_schedules_exact_delay_data_and_suffix",
        r##"(let (callback-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let* ((created
                   (async1-create-function
                    '(:result "compiled"
                      :delay 2.5)))
                  (start-return
                   (funcall created
                            "input"
                            (lambda (value)
                              (push value
                                    callback-values)
                              :callback-return)))
                  (queued
                   (mapcar
                    (lambda (event)
                      (list
                       (nth 0 event)
                       (nth 1 event)
                       (nth 2 event)
                       (if
                           (symbolp
                            (nth 3 event))
                           (nth 3 event)
                         :closure)
                       (nth 4 event)))
                    async1-test-timer-queue))
                  (trace
                   (async1-test-drain)))
             (list
              (functionp created)
              start-return
              queued
              trace
              callback-values
              async1-test-now))))"##,
        expect![[
            r#"OK (t (:async1-test-timer 1) ((2.5 1 nil :closure #1=("input -> compiled"))) ((:at 2.5 :id 1 :repeat nil :function :closure :arguments #1#)) ("input -> compiled") 2.5)"#
        ]],
    )
}

fn async1_create_function_plist_defaults_apply_independently_for_result_and_delay()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_create_function_plist_defaults_apply_independently_for_result_and_delay",
        r##"(let (values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (funcall
            (async1-create-function
             '(:result "Only result"))
            nil
            (lambda (value)
              (push
               (list :result-only value)
               values)))
           (funcall
            (async1-create-function
             '(:delay 3))
            "seed"
            (lambda (value)
              (push
               (list :delay-only value)
               values)))
           (let ((trace
                  (async1-test-drain)))
             (list
              trace
              (nreverse values)
              async1-test-now))))"##,
        expect![[
            r#"OK (((:at 1 :id 1 :repeat nil :function :closure :arguments ("Only result")) (:at 3 :id 2 :repeat nil :function :closure :arguments ("seed -> Result"))) ((:result-only "Only result") (:delay-only "seed -> Result")) 3)"#
        ]],
    )
}

fn async1_create_function_empty_list_is_an_identity_sequential_subchain() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_create_function_empty_list_is_an_identity_sequential_subchain",
        r##"(let ((created
                (async1-create-function nil))
               callback-values)
         (list
          (functionp created)
          (funcall created
                   "unchanged"
                   (lambda (value)
                     (push value callback-values)
                     :identity-finished))
          callback-values))"##,
        expect![[r#"OK (t :identity-finished ("unchanged"))"#]],
    )
}

fn async1_create_function_nested_sequence_runs_as_one_composable_async_step() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_create_function_nested_sequence_runs_as_one_composable_async_step",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let* ((created
                   (async1-create-function
                    '((:result "inner-1"
                       :delay 1)
                      (:result "inner-2"
                       :delay 2))))
                  (start-return
                   (funcall created
                            "outer"
                            (lambda (value)
                              (push value final-values)
                              :nested-finished)))
                  (trace
                   (async1-test-drain)))
             (list
              start-return
              trace
              final-values
              async1-test-now))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("outer -> inner-1")) (:at 3 :id 2 :repeat nil :function :closure :arguments ("outer -> inner-1 -> inner-2"))) ("outer -> inner-1 -> inner-2") 3)"#
        ]],
    )
}

fn async1_create_function_reports_unknown_keys_symbol_values_and_explicit_nil_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_create_function_reports_unknown_keys_symbol_values_and_explicit_nil_values",
        r##"(mapcar
         (lambda (spec)
           (list
            spec
            (async1-test-error
             (lambda ()
               (async1-create-function
                spec)))))
         '((:invalid-key "value")
           (:result result-symbol
            :delay 0)
           (:result nil
            :delay 0)
           (:delay nil
            :result "value")
           (:result "value"
            :extra ignored)
           (:parallel
            (:result "branch"
             :delay 0))))"##,
        expect![[
            r#"OK (((:invalid-key "value") (:error error ("Unknown key :invalid-key in async function spec"))) ((:result result-symbol :delay 0) (:error error ("Unknown key result-symbol in async function spec"))) ((:result nil :delay 0) (:error error ("Unknown key nil in async function spec"))) ((:delay nil :result "value") (:error error ("Unknown key nil in async function spec"))) ((:result "value" :extra ignored) (:error error ("Unknown key :extra in async function spec"))) ((:parallel (:result "branch" :delay 0)) (:error error ("Unknown key :parallel in async function spec"))))"#
        ]],
    )
}

fn async1_create_function_tolerates_missing_delay_value_but_rejects_scalar_specs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_create_function_tolerates_missing_delay_value_but_rejects_scalar_specs",
        r##"(let (callback-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((created
                  (async1-create-function
                   '(:result "value"
                     :delay))))
             (funcall created
                      nil
                      (lambda (value)
                        (push value callback-values)))
             (let ((trace
                    (async1-test-drain)))
               (list
              trace
              callback-values
              (mapcar
                 (lambda (spec)
                   (async1-test-reset-scheduler)
                   (let (values)
                     (condition-case error
                         (let* ((created
                                 (async1-create-function
                                  spec))
                                (start-return
                                 (funcall
                                  created
                                  "seed"
                                  (lambda (value)
                                    (push value values))))
                                (scalar-trace
                                 (async1-test-drain)))
                           (list
                            :ok
                            (functionp created)
                            start-return
                            scalar-trace
                            values))
                       (error
                        (list
                         :error
                         (car error)
                         (cdr error))))))
                 '(7 "not-a-plist" [:result "x"])))))))"##,
        expect![[
            r#"OK (((:at 1 :id 1 :repeat nil :function :closure :arguments ("value"))) ("value") ((:error wrong-type-argument (sequencep 7)) (:ok t (:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("seed -> Result"))) ("seed -> Result")) (:ok t (:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("seed -> Result"))) ("seed -> Result"))))"#
        ]],
    )
}

pub(super) fn construction_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async1_default_aggregator_ports_empty_single_and_multiple_upstream_cases_strictly(),
        async1_create_function_preserves_a_live_function_and_its_callback_contract(),
        async1_create_function_preserves_a_fbound_symbol_without_wrapping_it(),
        async1_create_function_explicit_plist_schedules_exact_delay_data_and_suffix(),
        async1_create_function_plist_defaults_apply_independently_for_result_and_delay(),
        async1_create_function_empty_list_is_an_identity_sequential_subchain(),
        async1_create_function_nested_sequence_runs_as_one_composable_async_step(),
        async1_create_function_reports_unknown_keys_symbol_values_and_explicit_nil_values(),
        async1_create_function_tolerates_missing_delay_value_but_rejects_scalar_specs(),
    ]
}
