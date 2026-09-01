use expect_test::expect;

use super::ParityBatchCase;

fn async_http_queue_process_resets_workers_and_schedules_exact_staggered_initial_batch()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_process_resets_workers_and_schedules_exact_staggered_initial_batch",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/a"
                   "https://api.test/b")
                 5
                 10
                 nil))
               (next-id 0)
               events)
          (setf
           (async-http-queue--state-active-workers
            state)
           99)
          (cl-letf
              (((symbol-function 'run-at-time)
                (lambda
                    (delay repeat function &rest arguments)
                  (let ((event
                         (vector
                          :timer
                          (setq next-id (1+ (or next-id 0)))
                          delay
                          repeat
                          function
                          arguments
                          nil)))
                    (setq events
                          (append
                           events
                           (list event)))
                    event))))
            (list
             (async-http-queue--process state)
             (async-http-queue--state-active-workers
              state)
             (mapcar
              (lambda (event)
                (list
                 (aref event 1)
                 (round
                  (* 1000
                     (aref event 2)))
                 (aref event 3)
                 (eq
                  (aref event 4)
                  #'async-http-queue--process-next-pending)
                 (eq
                  (car (aref event 5))
                  state)))
              events))))"##,
        expect![
            "OK (nil 0 ((1 0 nil t t) (2 50 nil t t) (3 100 nil t t) (4 150 nil t t) (5 200 nil t t)))"
        ],
    )
}

fn async_http_queue_process_with_zero_or_negative_limit_schedules_nothing() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_process_with_zero_or_negative_limit_schedules_nothing",
        r##"(let ((zero
               (async-http-queue-test-state
                '("https://api.test/a")
                0
                10
                nil))
              (negative
               (async-http-queue-test-state
                '("https://api.test/a")
                -3
                10
                nil))
              events)
          (cl-letf
              (((symbol-function 'run-at-time)
                (lambda (&rest arguments)
                  (push arguments events)
                  :timer)))
            (list
             (async-http-queue--process zero)
             (async-http-queue--process negative)
             events
             (async-http-queue--state-active-workers
              zero)
             (async-http-queue--state-active-workers
              negative))))"##,
        expect!["OK (nil nil nil 0 0)"],
    )
}

fn async_http_queue_process_rejects_non_integer_concurrency_during_scheduling() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_process_rejects_non_integer_concurrency_during_scheduling",
        r##"(mapcar
          (lambda (limit)
            (let ((state
                   (async-http-queue-test-state
                    '("https://api.test/a")
                    1
                    10
                    nil)))
              (setf
               (async-http-queue--state-max-concurrent
                state)
               limit)
              (async-http-queue-test-error-data
               (lambda ()
                 (async-http-queue--process
                  state)))))
          '(nil 1.5 "2" two))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (number-or-marker-p nil)) (:ok nil) (:error wrong-type-argument (number-or-marker-p "2")) (:error wrong-type-argument (number-or-marker-p two)))"#
        ]],
    )
}

fn async_http_queue_process_next_respects_capacity_and_terminal_queue() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_process_next_respects_capacity_and_terminal_queue",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/a"
                   "https://api.test/b")
                 2
                 10
                 nil))
               starts)
          (setf
           (async-http-queue--state-active-workers
            state)
           2)
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state url _success _error)
                  (push url starts))))
            (let ((at-capacity
                   (async-http-queue--process-next-pending
                    state)))
              (setf
               (async-http-queue--state-active-workers
                state)
               0)
              (dolist
                  (url
                   '("https://api.test/a"
                     "https://api.test/b"))
                (async-http-queue--update-status
                 state
                 url
                 'done))
              (list
               at-capacity
               (async-http-queue--process-next-pending
                state)
               starts
               (async-http-queue-test-queue-snapshot
                state)
               (async-http-queue--state-active-workers
                state)))))"##,
        expect![[
            r#"OK (nil nil nil (("https://api.test/a" done nil) ("https://api.test/b" done nil)) 0)"#
        ]],
    )
}

fn async_http_queue_process_next_claims_first_pending_item_and_success_finishes_lifecycle()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_process_next_claims_first_pending_item_and_success_finishes_lifecycle",
        r##"(let* (success
               failure
               completion-results
               messages
               timers
               (state
                (async-http-queue-test-state
                 '("https://api.test/a")
                 1
                 10
                 nil
                 (lambda (value)
                   (push
                    (append value nil)
                    completion-results)))))
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state _url on-success on-error)
                  (setq success on-success
                        failure on-error)
                  :request))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (push
                   (list
                    delay
                    repeat
                    function
                    arguments)
                   timers)
                  :timer))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (let ((launch-result
                   (async-http-queue--process-next-pending
                    state))
                  (after-launch
                   nil))
              (setq after-launch
                    (async-http-queue-test-state-snapshot
                     state))
              (funcall
               success
               '((id . 7)
                 (name . "ready")))
              (list
               launch-result
               (functionp success)
               (functionp failure)
               after-launch
               (async-http-queue-test-state-snapshot
                state)
               (mapcar
                (lambda (timer)
                  (list
                   (car timer)
                   (cadr timer)
                   (eq
                    (nth 2 timer)
                    #'async-http-queue--process-next-pending)
                   (eq
                    (car (nth 3 timer))
                    state)))
                (nreverse timers))
               (nreverse completion-results)
               (nreverse messages)))))"##,
        expect![[
            r#"OK (:request t t (:queue (("https://api.test/a" processing nil)) :active 1 :limit 1 :timeout 10 :parser nil :completion t :error nil) (:queue (("https://api.test/a" done #1=((id . 7) (name . "ready")))) :active 0 :limit 1 :timeout 10 :parser nil :completion t :error nil) ((0.05 nil t t)) ((#1#)) ("Loaded 1 URLs"))"#
        ]],
    )
}

fn async_http_queue_process_next_error_runs_per_url_callback_before_completion() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_process_next_error_runs_per_url_callback_before_completion",
        r##"(let* (failure
               lifecycle
               timers
               (state
                (async-http-queue-test-state
                 '("https://api.test/fail")
                 1
                 10
                 nil
                 (lambda (value)
                   (push
                    (list
                     :complete
                     (append value nil))
                    lifecycle))
                 (lambda (url)
                   (push
                    (list :error url)
                    lifecycle)))))
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state _url _success on-error)
                  (setq failure on-error)
                  :request))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (push
                   (list delay repeat function arguments)
                   timers)
                  :timer))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue--process-next-pending
             state)
            (let ((before
                   (async-http-queue-test-state-snapshot
                    state)))
              (funcall failure)
              (list
               before
               (async-http-queue-test-state-snapshot
                state)
               (nreverse lifecycle)
               (mapcar
                (lambda (timer)
                  (list
                   (car timer)
                   (eq
                    (nth 2 timer)
                    #'async-http-queue--process-next-pending)))
                (nreverse timers))))))"##,
        expect![[
            r#"OK ((:queue (("https://api.test/fail" processing nil)) :active 1 :limit 1 :timeout 10 :parser nil :completion t :error t) (:queue (("https://api.test/fail" error nil)) :active 0 :limit 1 :timeout 10 :parser nil :completion t :error t) ((:error "https://api.test/fail") (:complete (nil))) ((0.05 t)))"#
        ]],
    )
}

fn async_http_queue_refills_slots_as_workers_finish_without_exceeding_limit() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_refills_slots_as_workers_finish_without_exceeding_limit",
        r##"(let* ((urls
                '("https://api.test/1"
                  "https://api.test/2"
                  "https://api.test/3"
                  "https://api.test/4"))
               (state
                (async-http-queue-test-state
                 urls
                 2
                 10
                 nil))
               callbacks
               starts
               refill-events
               active-history)
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state url success error)
                  (setq callbacks
                        (append
                         callbacks
                         (list
                          (list url success error))))
                  (push url starts)))
               ((symbol-function 'run-at-time)
                (lambda (_delay _repeat function &rest arguments)
                  (setq refill-events
                        (append
                         refill-events
                         (list
                          (cons function arguments))))
                  :timer))
               ((symbol-function
                 'async-http-queue--check-completion)
                (lambda (current)
                  (push
                   (async-http-queue--state-active-workers
                    current)
                   active-history))))
            (async-http-queue--process-next-pending
             state)
            (async-http-queue--process-next-pending
             state)
            (async-http-queue--process-next-pending
             state)
            (push
             (async-http-queue--state-active-workers
              state)
             active-history)
            (funcall (nth 1 (nth 0 callbacks)) :one)
            (apply
             (car (nth 0 refill-events))
             (cdr (nth 0 refill-events)))
            (funcall (nth 1 (nth 1 callbacks)) :two)
            (apply
             (car (nth 1 refill-events))
             (cdr (nth 1 refill-events)))
            (funcall (nth 1 (nth 2 callbacks)) :three)
            (funcall (nth 1 (nth 3 callbacks)) :four)
            (list
             (nreverse starts)
             (nreverse active-history)
             (length refill-events)
             (async-http-queue-test-state-snapshot
              state))))"##,
        expect![[
            r#"OK (("https://api.test/1" "https://api.test/2" "https://api.test/3" "https://api.test/4") (2 1 1 1 0) 4 (:queue (("https://api.test/1" done :one) ("https://api.test/2" done :two) ("https://api.test/3" done :three) ("https://api.test/4" done :four)) :active 0 :limit 2 :timeout 10 :parser nil :completion nil :error nil))"#
        ]],
    )
}

fn async_http_queue_duplicate_urls_collapse_into_one_fetch_and_shared_result() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_duplicate_urls_collapse_into_one_fetch_and_shared_result",
        r##"(let* (completed
               (state
                (async-http-queue-test-state
                 '("https://api.test/same"
                   "https://api.test/same")
                 2
                 10
                 nil
                 (lambda (value)
                   (setq completed
                         (append value nil)))))
               success
               starts)
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state url on-success _on-error)
                  (push url starts)
                  (setq success on-success)))
               ((symbol-function 'run-at-time)
                (lambda (&rest _) :timer))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue--process-next-pending
             state)
            (let ((second-launch
                   (async-http-queue--process-next-pending
                    state))
                  (before
                   (async-http-queue-test-state-snapshot
                    state)))
              (funcall success :one-response)
              (list
               second-launch
               (nreverse starts)
               before
               (async-http-queue-test-state-snapshot
                state)
               completed))))"##,
        expect![[
            r#"OK (nil ("https://api.test/same") (:queue (("https://api.test/same" processing nil) ("https://api.test/same" processing nil)) :active 1 :limit 2 :timeout 10 :parser nil :completion t :error nil) (:queue (("https://api.test/same" done :one-response) ("https://api.test/same" done :one-response)) :active 0 :limit 2 :timeout 10 :parser nil :completion t :error nil) (:one-response :one-response))"#
        ]],
    )
}

fn async_http_queue_completion_callback_signal_propagates_after_terminal_state_update()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_completion_callback_signal_propagates_after_terminal_state_update",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/a")
                 1
                 10
                 nil
                 (lambda (_)
                   (error "completion exploded"))))
               success)
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state _url on-success _on-error)
                  (setq success on-success)))
               ((symbol-function 'run-at-time)
                (lambda (&rest _) :timer))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue--process-next-pending
             state)
            (list
             (async-http-queue-test-error-data
              (lambda ()
                (funcall success :payload)))
             (async-http-queue-test-state-snapshot
              state))))"##,
        expect![[
            r#"OK ((:error error ("completion exploded")) (:queue (("https://api.test/a" done :payload)) :active 0 :limit 1 :timeout 10 :parser nil :completion t :error nil))"#
        ]],
    )
}

fn async_http_queue_error_callback_signal_prevents_refill_and_completion_check() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_error_callback_signal_prevents_refill_and_completion_check",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/a"
                   "https://api.test/b")
                 1
                 10
                 nil
                 nil
                 (lambda (_)
                   (error "error callback exploded"))))
               failure
               timers
               completion-checks)
          (cl-letf
              (((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state _url _success on-error)
                  (setq failure on-error)))
               ((symbol-function 'run-at-time)
                (lambda (&rest arguments)
                  (push arguments timers)
                  :timer))
               ((symbol-function
                 'async-http-queue--check-completion)
                (lambda (_)
                  (cl-incf completion-checks))))
            (async-http-queue--process-next-pending
             state)
            (list
             (async-http-queue-test-error-data
              failure)
             (async-http-queue-test-state-snapshot
              state)
             timers
             completion-checks)))"##,
        expect![[
            r#"OK ((:error error ("error callback exploded")) (:queue (("https://api.test/a" error nil) ("https://api.test/b" pending nil)) :active 0 :limit 1 :timeout 10 :parser nil :completion nil :error t) nil nil)"#
        ]],
    )
}

pub(super) fn scheduling_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_http_queue_process_resets_workers_and_schedules_exact_staggered_initial_batch(),
        async_http_queue_process_with_zero_or_negative_limit_schedules_nothing(),
        async_http_queue_process_rejects_non_integer_concurrency_during_scheduling(),
        async_http_queue_process_next_respects_capacity_and_terminal_queue(),
        async_http_queue_process_next_claims_first_pending_item_and_success_finishes_lifecycle(),
        async_http_queue_process_next_error_runs_per_url_callback_before_completion(),
        async_http_queue_refills_slots_as_workers_finish_without_exceeding_limit(),
        async_http_queue_duplicate_urls_collapse_into_one_fetch_and_shared_result(),
        async_http_queue_completion_callback_signal_propagates_after_terminal_state_update(),
        async_http_queue_error_callback_signal_prevents_refill_and_completion_check(),
    ]
}
