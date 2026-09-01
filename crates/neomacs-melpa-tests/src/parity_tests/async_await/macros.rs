use expect_test::expect;

use super::ParityBatchCase;

fn async_defun_awaits_resolved_values_in_source_order_and_returns_last_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_defun_awaits_resolved_values_in_source_order_and_returns_last_value",
        r##"(let (events)
          (async-defun parity-ordered (input)
            (push (list :start input) events)
            (let ((first
                   (await
                    (promise-resolve
                     (+ input 2)))))
              (push (list :first first) events)
              (let ((second
                     (await
                      (promise-resolve
                       (* first 3)))))
                (push (list :second second) events)
                (list input first second))))
          (let ((outcome
                 (async-await-test-settle
                  (parity-ordered 4))))
            (list
             outcome
             (nreverse events))))"##,
        expect!["OK ((fulfilled (:fullfilled (4 6 18))) ((:start 4) (:first 6) (:second 18)))"],
    )
}

fn async_defun_preserves_required_optional_and_rest_argument_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_defun_preserves_required_optional_and_rest_argument_semantics",
        r##"(progn
          (async-defun parity-arguments
              (required &optional optional &rest rest)
            (await
             (list
              required
              optional
              rest
              (length rest))))
          (list
           (help-function-arglist
            'parity-arguments t)
           (async-await-test-settle
            (parity-arguments :required))
           (async-await-test-settle
            (parity-arguments
             :required :optional 1 2 3))))"##,
        expect![
            "OK ((required &optional optional &rest rest) (fulfilled (:fullfilled (:required nil nil 0))) (fulfilled (:fullfilled (:required :optional (1 2 3) 3))))"
        ],
    )
}

fn async_defun_preserves_docstring_declarations_and_interactive_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_defun_preserves_docstring_declarations_and_interactive_contract",
        r##"(progn
          (async-defun parity-command (count)
            "Return COUNT asynchronously."
            (interactive "p")
            (await (* count 2)))
          (list
           (documentation 'parity-command t)
           (interactive-form 'parity-command)
           (commandp 'parity-command)
           (help-function-arglist
            'parity-command t)
           (async-await-test-settle
            (parity-command 6))))"##,
        expect![[
            r#"OK ("Return COUNT asynchronously." (interactive "p") t (count) (fulfilled (:fullfilled 12)))"#
        ]],
    )
}

fn async_lambda_captures_lexical_state_across_multiple_awaits() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_lambda_captures_lexical_state_across_multiple_awaits",
        r##"(let* ((factor 7)
                 (offset 3)
                 (function
                  (async-lambda (value)
                    (let ((scaled
                           (await
                            (promise-resolve
                             (* value factor)))))
                      (await
                       (promise-resolve
                        (+ scaled offset)))))))
          (async-await-test-settle
           (funcall function 5)))"##,
        expect!["OK (fulfilled (:fullfilled 38))"],
    )
}

fn one_async_lambda_is_reusable_without_state_leaking_between_invocations() -> ParityBatchCase {
    ParityBatchCase::value(
        "one_async_lambda_is_reusable_without_state_leaking_between_invocations",
        r##"(let ((function
                 (async-lambda (label values)
                   (let (seen)
                     (dolist (value values)
                       (push
                        (await
                         (promise-resolve
                          (list label value)))
                        seen))
                     (nreverse seen)))))
          (list
           (async-await-test-settle
            (funcall function :left '(1 2)))
           (async-await-test-settle
            (funcall function
                     :right
                     '(8 9 10)))))"##,
        expect![
            "OK ((fulfilled (:fullfilled ((:left 1) (:left 2)))) (fulfilled (:fullfilled ((:right 8) (:right 9) (:right 10)))))"
        ],
    )
}

fn nested_async_functions_flatten_each_others_promises_practically() -> ParityBatchCase {
    ParityBatchCase::value(
        "nested_async_functions_flatten_each_others_promises_practically",
        r##"(progn
          (async-defun parity-inner (value)
            (await
             (async-await-test-delay
              0.01
              (* value value))))
          (async-defun parity-outer (left right)
            (let ((left-value
                   (await
                    (parity-inner left)))
                  (right-value
                   (await
                    (parity-inner right))))
              (list
               left-value
               right-value
               (+ left-value
                  right-value))))
          (async-await-test-settle
           (parity-outer 3 4)))"##,
        expect!["OK (fulfilled (:fullfilled (9 16 25)))"],
    )
}

fn awaits_inside_loops_conditionals_and_let_star_keep_control_flow() -> ParityBatchCase {
    ParityBatchCase::value(
        "awaits_inside_loops_conditionals_and_let_star_keep_control_flow",
        r##"(progn
          (async-defun parity-control-flow (values)
            (let ((index 0)
                  result)
              (dolist (value values)
                (let* ((awaited
                        (await
                         (promise-resolve
                          (+ value index))))
                       (classification
                        (if (cl-evenp awaited)
                            :even
                          :odd)))
                  (push
                   (list
                    index value
                    awaited
                    classification)
                   result)
                  (setq index
                        (1+ index))))
              (nreverse result)))
          (async-await-test-settle
           (parity-control-flow
            '(2 4 7 9))))"##,
        expect![
            "OK (fulfilled (:fullfilled ((0 2 2 :even) (1 4 5 :odd) (2 7 9 :odd) (3 9 12 :even))))"
        ],
    )
}

fn await_accepts_non_promise_values_of_practical_elisp_types() -> ParityBatchCase {
    ParityBatchCase::value(
        "await_accepts_non_promise_values_of_practical_elisp_types",
        r##"(progn
          (async-defun parity-plain-values ()
            (list
             (await nil)
             (await 42)
             (await "text")
             (await :keyword)
             (await '(a b))
             (await [1 2 3])
             (await
              (let ((table
                     (make-hash-table
                      :test #'equal)))
                (puthash "key" "value" table)
                (gethash "key" table)))))
          (async-await-test-settle
           (parity-plain-values)))"##,
        expect![[r#"OK (fulfilled (:fullfilled (nil 42 "text" :keyword (a b) [1 2 3] "value")))"#]],
    )
}

fn promise_returned_as_final_body_value_is_adopted_and_flattened() -> ParityBatchCase {
    ParityBatchCase::value(
        "promise_returned_as_final_body_value_is_adopted_and_flattened",
        r##"(progn
          (async-defun parity-final-promise (value)
            (await
             (promise-resolve
              (list :observed value)))
            (async-await-test-delay
             0.01
             (list :final (* value 2))))
          (async-await-test-settle
           (parity-final-promise 11)))"##,
        expect!["OK (fulfilled (:fullfilled (:final 22)))"],
    )
}

fn macroexpansion_has_defun_and_lambda_shells_with_local_await_rewriting() -> ParityBatchCase {
    ParityBatchCase::value(
        "macroexpansion_has_defun_and_lambda_shells_with_local_await_rewriting",
        r##"(let* ((defun-expansion
                  (macroexpand-1
                   '(async-defun parity-expanded (x)
                      (await x))))
                 (lambda-expansion
                  (macroexpand-1
                   '(async-lambda (x)
                      (await x))))
                 (defun-tree
                  (flatten-tree
                   defun-expansion))
                 (lambda-tree
                  (flatten-tree
                   lambda-expansion)))
          (list
           (list
            (car defun-expansion)
            (cadr defun-expansion)
            (nth 2 defun-expansion)
            (not
             (null
              (memq 'async-await--awaiter
                    defun-tree)))
            (not
             (null
              (memq 'async-await--check-return-value
                    defun-tree)))
            (not
             (null
              (memq 'iter-yield
                    defun-tree))))
           (list
            (car lambda-expansion)
            (cadr lambda-expansion)
            (not
             (null
              (memq 'async-await--awaiter
                    lambda-tree)))
            (not
             (null
              (memq 'async-await--check-return-value
                    lambda-tree)))
            (not
             (null
              (memq 'iter-yield
                    lambda-tree))))
           (fboundp 'await)))"##,
        expect!["OK ((defun parity-expanded (x) t t t) (lambda (x) t t t) nil)"],
    )
}

fn invocation_runs_until_first_await_then_resumes_asynchronously() -> ParityBatchCase {
    ParityBatchCase::value(
        "invocation_runs_until_first_await_then_resumes_asynchronously",
        r##"(let (events)
          (async-defun parity-lifecycle ()
            (push :before-await events)
            (let ((value
                   (await
                    (async-await-test-delay
                     0.02 :resumed))))
              (push value events)
              :complete))
          (let ((promise
                 (parity-lifecycle)))
            (let ((immediate
                   (copy-sequence events))
                  (outcome
                   (async-await-test-settle
                    promise)))
              (list
               immediate
               outcome
               (nreverse events)))))"##,
        expect![
            "OK ((:before-await) (fulfilled (:fullfilled :complete)) (:before-await :resumed))"
        ],
    )
}

pub(super) fn macros_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_defun_awaits_resolved_values_in_source_order_and_returns_last_value(),
        async_defun_preserves_required_optional_and_rest_argument_semantics(),
        async_defun_preserves_docstring_declarations_and_interactive_contract(),
        async_lambda_captures_lexical_state_across_multiple_awaits(),
        one_async_lambda_is_reusable_without_state_leaking_between_invocations(),
        nested_async_functions_flatten_each_others_promises_practically(),
        awaits_inside_loops_conditionals_and_let_star_keep_control_flow(),
        await_accepts_non_promise_values_of_practical_elisp_types(),
        promise_returned_as_final_body_value_is_adopted_and_flattened(),
        macroexpansion_has_defun_and_lambda_shells_with_local_await_rewriting(),
        invocation_runs_until_first_await_then_resumes_asynchronously(),
    ]
}
