use expect_test::expect;

use super::ParityBatchCase;

fn rejected_await_is_caught_by_condition_case_and_execution_continues() -> ParityBatchCase {
    ParityBatchCase::value(
        "rejected_await_is_caught_by_condition_case_and_execution_continues",
        r##"(let (events)
          (async-defun parity-catch-rejection ()
            (condition-case reason
                (await
                 (promise-reject
                  'remote-failure))
              (error
               (push
                (list :caught reason)
                events)))
            (push :continued events)
            (list :done
                  (nreverse events)))
          (async-await-test-settle
           (parity-catch-rejection)))"##,
        expect![
            "OK (fulfilled (:fullfilled (:done ((:caught (error remote-failure)) :continued))))"
        ],
    )
}

fn uncaught_rejected_await_rejects_the_async_functions_promise_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "uncaught_rejected_await_rejects_the_async_functions_promise_exactly",
        r##"(progn
          (async-defun parity-uncaught-rejection ()
            (await
             (promise-reject
              'remote-failure))
            :unreachable)
          (async-await-test-settle
           (parity-uncaught-rejection)))"##,
        expect!["OK (rejected (:rejected (error remote-failure)))"],
    )
}

fn synchronous_body_error_before_first_await_becomes_promise_rejection() -> ParityBatchCase {
    ParityBatchCase::value(
        "synchronous_body_error_before_first_await_becomes_promise_rejection",
        r##"(progn
          (async-defun parity-error-before ()
            (error "before-await" 17)
            (await :unreachable))
          (let ((returned
                 (parity-error-before)))
            (list
             (promise-class-p returned)
             (async-await-test-settle
              returned))))"##,
        expect![[r#"OK (t (rejected (:rejected (error "before-await"))))"#]],
    )
}

fn synchronous_body_error_after_successful_await_becomes_promise_rejection() -> ParityBatchCase {
    ParityBatchCase::value(
        "synchronous_body_error_after_successful_await_becomes_promise_rejection",
        r##"(progn
          (async-defun parity-error-after ()
            (let ((value
                   (await
                    (promise-resolve 23))))
              (error "after-await" value)))
          (async-await-test-settle
           (parity-error-after)))"##,
        expect![[r#"OK (rejected (:rejected (error "after-await")))"#]],
    )
}

fn unwind_protect_cleanup_runs_when_awaited_promise_rejects() -> ParityBatchCase {
    ParityBatchCase::value(
        "unwind_protect_cleanup_runs_when_awaited_promise_rejects",
        r##"(let (events)
          (async-defun parity-cleanup ()
            (unwind-protect
                (progn
                  (push :body events)
                  (await
                   (promise-reject
                    'cleanup-trigger)))
              (push :cleanup events)))
          (let ((outcome
                 (async-await-test-settle
                  (parity-cleanup))))
            (list
             outcome
             (nreverse events))))"##,
        expect!["OK ((rejected (:rejected (void-function nil))) (:body :cleanup))"],
    )
}

fn nested_condition_cases_distinguish_await_rejection_from_local_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "nested_condition_cases_distinguish_await_rejection_from_local_errors",
        r##"(progn
          (async-defun parity-nested-errors (selector)
            (await
             (promise-resolve :entered))
            (condition-case outer
                (list
                 :value
                 (condition-case inner
                     (pcase selector
                       (:reject
                        (await
                         (promise-reject
                          '(network 503))))
                       (:local
                        (signal
                         'wrong-type-argument
                         '(integerp "bad")))
                       (_
                        (await
                         (promise-resolve
                          :ok))))
                   (wrong-type-argument
                    (list :local inner))))
              (error
               (list :outer outer))))
          (mapcar
           (lambda (selector)
             (list
              selector
              (async-await-test-settle
               (parity-nested-errors
                selector))))
           '(:ok :local :reject)))"##,
        expect![[
            r#"OK ((:ok (fulfilled (:fullfilled (:value :ok)))) (:local (fulfilled (:fullfilled (:value (:local (wrong-type-argument integerp "bad")))))) (:reject (fulfilled (:fullfilled (:outer (error (network 503)))))))"#
        ]],
    )
}

fn check_return_value_preserves_all_non_marker_values_by_identity() -> ParityBatchCase {
    ParityBatchCase::value(
        "check_return_value_preserves_all_non_marker_values_by_identity",
        r##"(let* ((cons-value
                  (list :ordinary 1 2))
                 (vector-value
                  [alpha beta])
                 (fake-marker
                  (intern
                   (symbol-name
                    async-await--is-error)))
                 (fake-error
                  (list fake-marker
                        :iterator
                        :reason)))
          (let ((values
                 (mapcar
                  #'async-await--check-return-value
                  (list nil 0 "text" :keyword
                        cons-value vector-value
                        fake-error))))
            (list
             (butlast values)
             (let ((returned-fake
                    (car (last values))))
               (list
                (eq (car returned-fake)
                    fake-marker)
                (cadr returned-fake)
                (caddr returned-fake)))
           (eq cons-value
               (async-await--check-return-value
                cons-value))
           (eq vector-value
               (async-await--check-return-value
                vector-value))
           (eq fake-error
               (async-await--check-return-value
                fake-error)))))"##,
        expect![[
            r#"OK ((nil 0 "text" :keyword (:ordinary 1 2) [alpha beta]) (t :iterator :reason) t t t)"#
        ]],
    )
}

fn check_return_value_closes_the_iterator_before_signaling_injected_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "check_return_value_closes_the_iterator_before_signaling_injected_error",
        r##"(let* ((cleaned nil)
                 (iterator
                  (funcall
                   (iter2-lambda ()
                     (unwind-protect
                         (iter-yield :ready)
                       (setq cleaned t))))))
          (let ((first
                 (iter-next iterator))
                (signal-data
                 (condition-case reason
                     (async-await--check-return-value
                      (list
                       async-await--is-error
                       iterator
                       '(remote 409)))
                   (error reason)))
                (after
                 (condition-case reason
                     (iter-next iterator)
                   (iter-end-of-sequence
                    (list
                     (car reason)
                     (cdr reason))))))
            (list
             first
             signal-data
             cleaned
             after)))"##,
        expect!["OK (:ready (error (remote 409)) t (iter-end-of-sequence nil))"],
    )
}

fn iter_throw_injects_marker_iterator_and_reason_into_suspended_generator() -> ParityBatchCase {
    ParityBatchCase::value(
        "iter_throw_injects_marker_iterator_and_reason_into_suspended_generator",
        r##"(let (iterator)
          (setq iterator
                (funcall
                 (iter2-lambda ()
                   (let ((injected
                          (iter-yield
                           :ready)))
                     (list
                      (eq
                       (car injected)
                       async-await--is-error)
                      (eq
                       (cadr injected)
                       iterator)
                      (caddr injected))))))
          (let ((first
                 (iter-next iterator))
                (injected-result
                 (condition-case reason
                     (async-await--iter-throw
                      iterator
                      '(remote timeout))
                   (iter-end-of-sequence
                    (list
                     (car reason)
                     (cdr reason))))))
            (list
             first
             injected-result)))"##,
        expect!["OK (:ready (iter-end-of-sequence (t t (remote timeout))))"],
    )
}

fn caught_rejection_can_rethrow_a_new_error_with_context() -> ParityBatchCase {
    ParityBatchCase::value(
        "caught_rejection_can_rethrow_a_new_error_with_context",
        r##"(progn
          (async-defun parity-rethrow ()
            (condition-case reason
                (await
                 (promise-reject
                  '(service unavailable)))
              (error
               (signal
                'user-error
                (list
                 "wrapped"
                 reason)))))
          (async-await-test-settle
           (parity-rethrow)))"##,
        expect![[
            r#"OK (rejected (:rejected (user-error "wrapped" (error (service unavailable)))))"#
        ]],
    )
}

fn rejection_reasons_keep_symbol_string_number_and_list_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "rejection_reasons_keep_symbol_string_number_and_list_shapes",
        r##"(progn
          (async-defun parity-reason-shape (reason)
            (condition-case data
                (await
                 (promise-reject
                  reason))
              (error
               (list
                (car data)
                (cdr data)))))
          (mapcar
           (lambda (reason)
             (list
              reason
              (async-await-test-settle
               (parity-reason-shape
                reason))))
           (list
            :symbol
            "string"
            404
            '(nested reason))))"##,
        expect![[
            r#"OK ((:symbol (fulfilled (:fullfilled (error (:symbol))))) ("string" (fulfilled (:fullfilled (error ("string"))))) (404 (fulfilled (:fullfilled (error (404))))) (#1=(nested reason) (fulfilled (:fullfilled (error (#1#))))))"#
        ]],
    )
}

pub(super) fn errors_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        rejected_await_is_caught_by_condition_case_and_execution_continues(),
        uncaught_rejected_await_rejects_the_async_functions_promise_exactly(),
        synchronous_body_error_before_first_await_becomes_promise_rejection(),
        synchronous_body_error_after_successful_await_becomes_promise_rejection(),
        unwind_protect_cleanup_runs_when_awaited_promise_rejects(),
        nested_condition_cases_distinguish_await_rejection_from_local_errors(),
        check_return_value_preserves_all_non_marker_values_by_identity(),
        check_return_value_closes_the_iterator_before_signaling_injected_error(),
        iter_throw_injects_marker_iterator_and_reason_into_suspended_generator(),
        caught_rejection_can_rethrow_a_new_error_with_context(),
        rejection_reasons_keep_symbol_string_number_and_list_shapes(),
    ]
}
