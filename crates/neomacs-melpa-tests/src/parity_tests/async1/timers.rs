use expect_test::expect;

use super::ParityBatchCase;

fn async1_default_template_ports_basic_nil_zero_and_empty_data_cases_deterministically()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_default_template_ports_basic_nil_zero_and_empty_data_cases_deterministically",
        r##"(let (values returns)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (push
            (async1-default-template
             "test"
             (lambda (value)
               (push
                (list :basic value)
                values))
             0.5
             "suffix")
            returns)
           (push
            (async1-default-template
             nil
             (lambda (value)
               (push
                (list :nil value)
                values))
             0.25
             "suffix")
            returns)
           (push
            (async1-default-template
             ""
             (lambda (value)
               (push
                (list :empty value)
                values))
             0
             "suffix")
            returns)
           (let ((trace
                  (async1-test-drain)))
             (list
              (nreverse returns)
              trace
              (nreverse values)
              async1-test-now))))"##,
        expect![[
            r#"OK (((:async1-test-timer 1) (:async1-test-timer 2) (:async1-test-timer 3)) ((:at 0 :id 3 :repeat nil :function :closure :arguments (" -> suffix")) (:at 0.25 :id 2 :repeat nil :function :closure :arguments ("suffix")) (:at 0.5 :id 1 :repeat nil :function :closure :arguments ("test -> suffix"))) ((:empty " -> suffix") (:nil "suffix") (:basic "test -> suffix")) 0.5)"#
        ]],
    )
}

fn async1_default_template_real_zero_delay_timer_runs_callback_via_editor_event_loop()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_default_template_real_zero_delay_timer_runs_callback_via_editor_event_loop",
        r##"(let (value
               (callback-count 0)
               timer)
         (setq timer
               (async1-default-template
                "real"
                (lambda (result)
                  (setq value result
                        callback-count
                        (1+ callback-count)))
                0
                "timer"))
         (list
          (timerp timer)
          (async1-test-await
           (lambda ()
             value)
           0.5)
          value
          callback-count))"##,
        expect![[r#"OK (t "real -> timer" "real -> timer" 1)"#]],
    )
}

fn async1_start_real_timer_pipeline_completes_sequential_and_parallel_workflow() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_start_real_timer_pipeline_completes_sequential_and_parallel_workflow",
        r##"(let (final-values)
         (let ((aggregator
                (lambda (results)
                  (mapconcat
                   #'identity
                   (sort
                    (copy-sequence results)
                    #'string<)
                   " | "))))
           (async1-start
            nil
            (list
             '(:result "root"
               :delay 0)
             (list
              :parallel
              '(:result "slower"
                :delay 0.02)
              :aggregator
              aggregator
              '(:result "faster"
                :delay 0.005))
             '(:result "tail"
               :delay 0))
            (lambda (result)
              (push result final-values))))
         (list
          (async1-test-await
           (lambda ()
             final-values)
           1)
          final-values
          (length final-values)))"##,
        expect![[r#"OK (#1=("root -> faster | root -> slower -> tail") #1# 1)"#]],
    )
}

fn async1_start_real_timer_custom_step_preserves_lexical_capture_across_callback() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_start_real_timer_custom_step_preserves_lexical_capture_across_callback",
        r##"(let ((captured "external")
               events
               final-values)
         (async1-start
          "seed"
          (list
           (lambda (data callback)
             (push
              (list :scheduled data captured)
              events)
             (run-at-time
              0 nil callback
              (concat data
                      " -> "
                      captured)))
           '(:result "built-in"
             :delay 0))
          (lambda (result)
            (push result final-values)
            (push :finished events)))
         (list
          (async1-test-await
           (lambda ()
             final-values)
           0.5)
          (nreverse events)
          final-values))"##,
        expect![[
            r#"OK (#1=("seed -> external -> built-in") ((:scheduled "seed" "external") :finished) #1#)"#
        ]],
    )
}

fn async1_start_real_simultaneous_zero_delay_parallel_uses_default_aggregate_and_print()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_real_simultaneous_zero_delay_parallel_uses_default_aggregate_and_print",
        r##"(let (printed)
         (cl-letf
             (((symbol-function 'print)
               (lambda (object &optional _stream)
                 (push object printed)
                 object)))
           (async1-start
            nil
            '((:parallel
               (:result "A"
                :delay 0)
               (:result "B"
                :delay 0))))
           (list
            (async1-test-await
             (lambda ()
               printed)
             0.5)
            printed
            (member
             (car printed)
             '("Final result: {A, B}"
               "Final result: {B, A}"))
            (length printed))))"##,
        expect![[r#"OK (#1=("Final result: {B, A}") #1# ("Final result: {B, A}") 1)"#]],
    )
}

pub(super) fn timers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async1_default_template_ports_basic_nil_zero_and_empty_data_cases_deterministically(),
        async1_default_template_real_zero_delay_timer_runs_callback_via_editor_event_loop(),
        async1_start_real_timer_pipeline_completes_sequential_and_parallel_workflow(),
        async1_start_real_timer_custom_step_preserves_lexical_capture_across_callback(),
        async1_start_real_simultaneous_zero_delay_parallel_uses_default_aggregate_and_print(),
    ]
}
