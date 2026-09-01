use expect_test::expect;

use super::ParityBatchCase;

fn async1_sequential_step_invokes_a_custom_function_then_advances_the_exact_index()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_sequential_step_invokes_a_custom_function_then_advances_the_exact_index",
        r##"(let (events)
         (let ((return
                (async1--handle-sequential-step
                 (lambda (data callback)
                   (push
                    (list :step data)
                    events)
                   (funcall callback
                            (concat data " -> custom")))
                 "input"
                 (lambda (result index)
                   (push
                    (list :chain result index)
                    events)
                   :chain-return)
                 7)))
           (list
            return
            (nreverse events))))"##,
        expect![[r#"OK (:chain-return ((:step "input") (:chain "input -> custom" 8)))"#]],
    )
}

fn async1_sequential_step_schedules_plist_work_and_advances_only_after_callback() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_sequential_step_schedules_plist_work_and_advances_only_after_callback",
        r##"(let (events)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1--handle-sequential-step
                   '(:result "step"
                     :delay 2)
                   "input"
                   (lambda (result index)
                     (push
                      (list result index)
                      events)
                     :advanced)
                   4)))
             (let ((before
                    (copy-tree events))
                   (trace
                    (async1-test-drain)))
               (list
                start-return
                before
                trace
                events
                async1-test-now)))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) nil ((:at 2 :id 1 :repeat nil :function :closure :arguments ("input -> step"))) (("input -> step" 5)) 2)"#
        ]],
    )
}

fn async1_parallel_step_empty_branch_set_passes_input_through_synchronously() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_parallel_step_empty_branch_set_passes_input_through_synchronously",
        r##"(let (events)
         (list
          (async1--handle-parallel-step
           nil
           "unchanged"
           (lambda (result index)
             (push
              (list result index)
              events)
             :empty-finished)
           10)
          events))"##,
        expect![[r#"OK (:empty-finished (("unchanged" 11)))"#]],
    )
}

fn async1_parallel_step_waits_for_every_callback_and_aggregates_completion_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_parallel_step_waits_for_every_callback_and_aggregates_completion_order",
        r##"(let (callbacks chain-events)
         (let ((specs
                (mapcar
                 (lambda (label)
                   (lambda (data callback)
                     (push
                      (cons label callback)
                      callbacks)
                     (push
                      (list :started label data)
                      chain-events)
                     label))
                 '(a b c))))
           (let ((start-return
                  (async1--handle-parallel-step
                   specs
                   "seed"
                   (lambda (result index)
                     (push
                      (list :finished result index)
                      chain-events)
                     :parallel-finished)
                   2)))
             (let ((registered
                    (sort
                     (mapcar #'car callbacks)
                     (lambda (left right)
                       (string<
                        (symbol-name left)
                        (symbol-name right))))))
               (funcall
                (cdr
                 (assq 'b callbacks))
                "B")
               (let ((after-one
                      (copy-tree chain-events)))
                 (funcall
                  (cdr
                   (assq 'a callbacks))
                  "A")
                 (let ((after-two
                        (copy-tree chain-events)))
                   (funcall
                    (cdr
                     (assq 'c callbacks))
                    "C")
                   (list
                    start-return
                    registered
                    after-one
                    after-two
                    (nreverse chain-events))))))))"##,
        expect![[
            r#"OK (nil (a b c) ((:started c "seed") (:started b "seed") (:started a "seed")) ((:started c "seed") (:started b "seed") (:started a "seed")) ((:started a "seed") (:started b "seed") (:started c "seed") (:finished "{C, A, B}" 3)))"#
        ]],
    )
}

fn async1_parallel_step_virtual_timers_make_completion_and_push_order_explicit() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_parallel_step_virtual_timers_make_completion_and_push_order_explicit",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1--handle-parallel-step
                   '((:result "slow"
                      :delay 3)
                     (:result "fast"
                      :delay 1)
                     (:result "middle"
                      :delay 2))
                   "seed"
                   (lambda (result index)
                     (push
                      (list result index)
                      final-values)
                     :parallel-finished)
                   5))
                 (trace
                  (async1-test-drain)))
             (list
              start-return
              trace
              final-values
              async1-test-now))))"##,
        expect![[
            r#"OK (nil ((:at 1 :id 2 :repeat nil :function :closure :arguments ("seed -> fast")) (:at 2 :id 3 :repeat nil :function :closure :arguments ("seed -> middle")) (:at 3 :id 1 :repeat nil :function :closure :arguments ("seed -> slow"))) (("{seed -> slow, seed -> middle, seed -> fast}" 6)) 3)"#
        ]],
    )
}

fn async1_parallel_step_accepts_custom_aggregator_at_beginning_middle_or_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_parallel_step_accepts_custom_aggregator_at_beginning_middle_or_end",
        r##"(let ((aggregator
                (lambda (results)
                  (concat
                   "["
                   (mapconcat #'identity
                              results
                              " | ")
                   "]"))))
         (cl-labels
             ((run
               (specs)
               (let (final)
                 (async1-test-reset-scheduler)
                 (cl-letf
                     (((symbol-function 'run-at-time)
                       #'async1-test-schedule))
                   (async1--handle-parallel-step
                    specs
                    "base"
                    (lambda (result index)
                      (setq final
                            (list result index)))
                    0)
                   (list
                    (async1-test-drain)
                    final)))))
           (list
            (run
             (list
              :aggregator
              aggregator
              '(:result "A" :delay 2)
              '(:result "B" :delay 1)))
            (run
             (list
              '(:result "A" :delay 2)
              :aggregator
              aggregator
              '(:result "B" :delay 1)))
            (run
             (list
              '(:result "A" :delay 2)
              '(:result "B" :delay 1)
              :aggregator
              aggregator)))))"##,
        expect![[
            r#"OK ((((:at 1 :id 2 :repeat nil :function :closure :arguments ("base -> B")) (:at 2 :id 1 :repeat nil :function :closure :arguments ("base -> A"))) ("[base -> A | base -> B]" 1)) (((:at 1 :id 2 :repeat nil :function :closure :arguments ("base -> B")) (:at 2 :id 1 :repeat nil :function :closure :arguments ("base -> A"))) ("[base -> A | base -> B]" 1)) (((:at 1 :id 2 :repeat nil :function :closure :arguments ("base -> B")) (:at 2 :id 1 :repeat nil :function :closure :arguments ("base -> A"))) ("[base -> A | base -> B]" 1)))"#
        ]],
    )
}

fn async1_start_runs_a_strict_sequential_pipeline_with_cumulative_due_times() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_runs_a_strict_sequential_pipeline_with_cumulative_due_times",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1-start
                   "seed"
                   '((:result "one"
                      :delay 1)
                     (:result "two"
                      :delay 2)
                     (:result "three"
                      :delay 3))
                   (lambda (result)
                     (push result final-values)
                     :all-finished)))
                 (trace
                  (async1-test-drain)))
             (list
              start-return
              trace
              final-values
              async1-test-now))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("seed -> one")) (:at 3 :id 2 :repeat nil :function :closure :arguments ("seed -> one -> two")) (:at 6 :id 3 :repeat nil :function :closure :arguments ("seed -> one -> two -> three"))) ("seed -> one -> two -> three") 6)"#
        ]],
    )
}

fn async1_start_runs_a_parallel_pipeline_and_uses_reverse_completion_aggregation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async1_start_runs_a_parallel_pipeline_and_uses_reverse_completion_aggregation",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1-start
                   nil
                   '((:parallel
                      (:result "A"
                       :delay 3)
                      (:result "B"
                       :delay 1)
                      (:result "C"
                       :delay 2)))
                   (lambda (result)
                     (push result final-values)
                     :parallel-done)))
                 (trace
                  (async1-test-drain)))
             (list
              start-return
              trace
              final-values))))"##,
        expect![[
            r#"OK (nil ((:at 1 :id 2 :repeat nil :function :closure :arguments ("B")) (:at 2 :id 3 :repeat nil :function :closure :arguments ("C")) (:at 3 :id 1 :repeat nil :function :closure :arguments ("A"))) ("{A, C, B}"))"#
        ]],
    )
}

fn async1_start_runs_the_readme_mixed_parallel_and_nested_sequential_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_runs_the_readme_mixed_parallel_and_nested_sequential_workflow",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1-start
                   nil
                   '((:result "root"
                      :delay 1)
                     (:parallel
                      ((:result "sub-a"
                        :delay 2)
                       (:result "sub-b"
                        :delay 1))
                      (:result "fast"
                       :delay 1)
                      (:result "slow"
                       :delay 3))
                     (:result "tail"
                      :delay 1))
                   (lambda (result)
                     (push result final-values)
                     :workflow-done)))
                 (trace
                  (async1-test-drain)))
             (list
              start-return
              trace
              final-values
              async1-test-now))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("root")) (:at 2 :id 3 :repeat nil :function :closure :arguments ("root -> fast")) (:at 3 :id 2 :repeat nil :function :closure :arguments ("root -> sub-a")) (:at 4 :id 4 :repeat nil :function :closure :arguments ("root -> slow")) (:at 4 :id 5 :repeat nil :function :closure :arguments ("root -> sub-a -> sub-b")) (:at 5 :id 6 :repeat nil :function :closure :arguments ("{root -> sub-a -> sub-b, root -> slow, root -> fast} -> tail"))) ("{root -> sub-a -> sub-b, root -> slow, root -> fast} -> tail") 5)"#
        ]],
    )
}

fn async1_start_composes_custom_async_functions_with_captured_external_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_composes_custom_async_functions_with_captured_external_data",
        r##"(let ((external "context")
               events
               final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((custom
                  (lambda (data callback)
                    (push
                     (list :custom data external)
                     events)
                    (run-at-time
                     2 nil callback
                     (format "%s -> %s"
                             data
                             external)))))
             (let ((start-return
                    (async1-start
                     "seed"
                     (list
                      '(:result "built-in"
                        :delay 1)
                      custom
                      '(:result "tail"
                        :delay 1))
                     (lambda (result)
                       (push result final-values)
                       :custom-done)))
                   (trace
                    (async1-test-drain)))
               (list
                start-return
                trace
                (nreverse events)
                final-values
                async1-test-now)))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("seed -> built-in")) (:at 3 :id 2 :repeat nil :function :closure :arguments ("seed -> built-in -> context")) (:at 4 :id 3 :repeat nil :function :closure :arguments ("seed -> built-in -> context -> tail"))) ((:custom "seed -> built-in" "context")) ("seed -> built-in -> context -> tail") 4)"#
        ]],
    )
}

fn async1_start_composes_mutable_lambda_factory_steps_in_declared_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_composes_mutable_lambda_factory_steps_in_declared_order",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((make-step
                  (lambda (number)
                    (lambda (data callback)
                      (run-at-time
                       number nil callback
                       (format "%s -> step-%d"
                               data
                               number))))))
             (let* ((sequence
                     (mapcar make-step
                             '(3 1 2 0)))
                    (start-return
                     (async1-start
                      "start"
                      sequence
                      (lambda (result)
                        (push result final-values)
                        :factory-done)))
                    (trace
                     (async1-test-drain)))
               (list
                start-return
                trace
                final-values
                async1-test-now)))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) ((:at 3 :id 1 :repeat nil :function :closure :arguments ("start -> step-3")) (:at 4 :id 2 :repeat nil :function :closure :arguments ("start -> step-3 -> step-1")) (:at 6 :id 3 :repeat nil :function :closure :arguments ("start -> step-3 -> step-1 -> step-2")) (:at 6 :id 4 :repeat nil :function :closure :arguments ("start -> step-3 -> step-1 -> step-2 -> step-0"))) ("start -> step-3 -> step-1 -> step-2 -> step-0") 6)"#
        ]],
    )
}

fn async1_start_empty_sequence_calls_final_callback_synchronously_and_returns_its_value()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_empty_sequence_calls_final_callback_synchronously_and_returns_its_value",
        r##"(let (events)
         (let ((return
                (async1-start
                 "already complete"
                 nil
                 (lambda (result)
                   (push result events)
                   :synchronous-finish))))
           (list
            return
            events)))"##,
        expect![[r#"OK (:synchronous-finish ("already complete"))"#]],
    )
}

fn async1_start_without_final_callback_prints_the_exact_final_result_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_without_final_callback_prints_the_exact_final_result_once",
        r##"(let (printed)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule)
              ((symbol-function 'print)
               (lambda (object &optional _stream)
                 (push object printed)
                 object)))
           (let ((start-return
                  (async1-start
                   nil
                   '((:result "first"
                      :delay 1)
                     (:result "second"
                      :delay 1))))
                 (trace
                  (async1-test-drain)))
             (list
              start-return
              trace
              printed))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) ((:at 1 :id 1 :repeat nil :function :closure :arguments ("first")) (:at 2 :id 2 :repeat nil :function :closure :arguments ("first -> second"))) ("Final result: first -> second"))"#
        ]],
    )
}

fn async1_start_large_parallel_fanout_preserves_every_result_and_completion_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_large_parallel_fanout_preserves_every_result_and_completion_order",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let* ((specs
                   (mapcar
                    (lambda (number)
                      (list
                       :result
                       (format "branch-%d"
                               number)
                       :delay
                       number))
                    '(6 2 5 1 4 3)))
                  (start-return
                   (async1-start
                    "root"
                    (list
                     (cons :parallel specs))
                    (lambda (result)
                      (push result final-values)
                      :fanout-done)))
                  (trace
                   (async1-test-drain)))
             (list
              start-return
              trace
              final-values
              (length
               (car final-values))
              async1-test-now))))"##,
        expect![[
            r#"OK (nil ((:at 1 :id 4 :repeat nil :function :closure :arguments ("root -> branch-1")) (:at 2 :id 2 :repeat nil :function :closure :arguments ("root -> branch-2")) (:at 3 :id 6 :repeat nil :function :closure :arguments ("root -> branch-3")) (:at 4 :id 5 :repeat nil :function :closure :arguments ("root -> branch-4")) (:at 5 :id 3 :repeat nil :function :closure :arguments ("root -> branch-5")) (:at 6 :id 1 :repeat nil :function :closure :arguments ("root -> branch-6"))) ("{root -> branch-6, root -> branch-5, root -> branch-4, root -> branch-3, root -> branch-2, root -> branch-1}") 108 6)"#
        ]],
    )
}

fn async1_start_deep_tree_combines_two_nested_subchains_and_a_direct_branch() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_deep_tree_combines_two_nested_subchains_and_a_direct_branch",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1-start
                   "root"
                   '((:parallel
                      ((:result "left-1"
                        :delay 2)
                       (:result "left-2"
                        :delay 1))
                      ((:result "right-1"
                        :delay 1)
                       (:result "right-2"
                        :delay 3))
                      (:result "direct"
                       :delay 2))
                     (:result "joined"
                      :delay 1))
                   (lambda (result)
                     (push result final-values)
                     :tree-done)))
                 (trace
                  (async1-test-drain)))
             (list
              start-return
              trace
              final-values
              async1-test-now))))"##,
        expect![[
            r#"OK (nil ((:at 1 :id 2 :repeat nil :function :closure :arguments ("root -> right-1")) (:at 2 :id 1 :repeat nil :function :closure :arguments ("root -> left-1")) (:at 2 :id 3 :repeat nil :function :closure :arguments ("root -> direct")) (:at 3 :id 5 :repeat nil :function :closure :arguments ("root -> left-1 -> left-2")) (:at 4 :id 4 :repeat nil :function :closure :arguments ("root -> right-1 -> right-2")) (:at 5 :id 6 :repeat nil :function :closure :arguments ("{root -> right-1 -> right-2, root -> left-1 -> left-2, root -> direct} -> joined"))) ("{root -> right-1 -> right-2, root -> left-1 -> left-2, root -> direct} -> joined") 5)"#
        ]],
    )
}

fn async1_start_returns_the_first_step_value_and_calls_final_callback_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_returns_the_first_step_value_and_calls_final_callback_once",
        r##"(let (events)
         (let ((return
                (async1-start
                 "seed"
                 (list
                  (lambda (data callback)
                    (push
                     (list :first data)
                     events)
                    (funcall callback
                             "after-first")
                    :first-return)
                  (lambda (data callback)
                    (push
                     (list :second data)
                     events)
                    (funcall callback
                             "after-second")
                    :second-return))
                 (lambda (result)
                   (push
                    (list :final result)
                    events)
                   :final-return))))
           (list
            return
            (nreverse events))))"##,
        expect![[
            r#"OK (:first-return ((:first "seed") (:second "after-first") (:final "after-second")))"#
        ]],
    )
}

fn async1_start_reports_a_later_invalid_step_when_its_scheduled_predecessor_completes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_reports_a_later_invalid_step_when_its_scheduled_predecessor_completes",
        r##"(let (final-values)
         (async1-test-reset-scheduler)
         (cl-letf
             (((symbol-function 'run-at-time)
               #'async1-test-schedule))
           (let ((start-return
                  (async1-start
                   nil
                   '((:result "valid"
                      :delay 1)
                     (:invalid-key "broken")
                     (:result "unreachable"
                      :delay 1))
                   (lambda (result)
                     (push result final-values)))))
             (list
              start-return
              (async1-test-error
               #'async1-test-drain)
              async1-test-now
              async1-test-timer-queue
              final-values))))"##,
        expect![[
            r#"OK ((:async1-test-timer 1) (:error error ("Unknown key :invalid-key in async function spec")) 1 nil nil)"#
        ]],
    )
}

fn async1_start_propagates_multiple_callback_invocations_through_the_remaining_pipeline()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_propagates_multiple_callback_invocations_through_the_remaining_pipeline",
        r##"(let (events final-values)
         (let ((return
                (async1-start
                 "seed"
                 (list
                  (lambda (data callback)
                    (push
                     (list :source data)
                     events)
                    (funcall callback
                             "first emission")
                    (funcall callback
                             "second emission")
                    :source-return)
                  (lambda (data callback)
                    (push
                     (list :downstream data)
                     events)
                    (funcall callback
                             (concat data " -> done"))))
                 (lambda (result)
                   (push result final-values)
                   :final-return))))
           (list
            return
            (nreverse events)
            (nreverse final-values))))"##,
        expect![[
            r#"OK (:source-return ((:source "seed") (:downstream "first emission") (:downstream "second emission")) ("first emission -> done" "second emission -> done"))"#
        ]],
    )
}

fn async1_start_stops_cleanly_when_a_custom_step_never_calls_its_callback() -> ParityBatchCase {
    ParityBatchCase::value(
        "async1_start_stops_cleanly_when_a_custom_step_never_calls_its_callback",
        r##"(let (events final-values)
         (let ((return
                (async1-start
                 "seed"
                 (list
                  (lambda (data _callback)
                    (push
                     (list :stalled data)
                     events)
                    :stalled-return)
                  (lambda (_data _callback)
                    (push :unexpected events)))
                 (lambda (result)
                   (push result final-values)))))
           (list
            return
            events
            final-values)))"##,
        expect![[r#"OK (:stalled-return ((:stalled "seed")) nil)"#]],
    )
}

pub(super) fn pipelines_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async1_sequential_step_invokes_a_custom_function_then_advances_the_exact_index(),
        async1_sequential_step_schedules_plist_work_and_advances_only_after_callback(),
        async1_parallel_step_empty_branch_set_passes_input_through_synchronously(),
        async1_parallel_step_waits_for_every_callback_and_aggregates_completion_order(),
        async1_parallel_step_virtual_timers_make_completion_and_push_order_explicit(),
        async1_parallel_step_accepts_custom_aggregator_at_beginning_middle_or_end(),
        async1_start_runs_a_strict_sequential_pipeline_with_cumulative_due_times(),
        async1_start_runs_a_parallel_pipeline_and_uses_reverse_completion_aggregation(),
        async1_start_runs_the_readme_mixed_parallel_and_nested_sequential_workflow(),
        async1_start_composes_custom_async_functions_with_captured_external_data(),
        async1_start_composes_mutable_lambda_factory_steps_in_declared_order(),
        async1_start_empty_sequence_calls_final_callback_synchronously_and_returns_its_value(),
        async1_start_without_final_callback_prints_the_exact_final_result_once(),
        async1_start_large_parallel_fanout_preserves_every_result_and_completion_order(),
        async1_start_deep_tree_combines_two_nested_subchains_and_a_direct_branch(),
        async1_start_returns_the_first_step_value_and_calls_final_callback_once(),
        async1_start_reports_a_later_invalid_step_when_its_scheduled_predecessor_completes(),
        async1_start_propagates_multiple_callback_invocations_through_the_remaining_pipeline(),
        async1_start_stops_cleanly_when_a_custom_step_never_calls_its_callback(),
    ]
}
