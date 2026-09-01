use expect_test::expect;

use super::ParityBatchCase;

fn async_http_queue_mixed_end_to_end_workflow_refills_slots_and_preserves_input_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_mixed_end_to_end_workflow_refills_slots_and_preserves_input_order",
        r##"(let* ((urls
                '("https://api.test/one"
                  "https://api.test/two"
                  "https://api.test/three"))
               (responses
                `(("https://api.test/one"
                   . ,(async-http-queue-test-http-response
                       503
                       "{\"error\":\"busy\"}"))
                  ("https://api.test/two"
                   . ,(async-http-queue-test-http-response
                       200
                       "{\"id\":2,\"name\":\"two\"}"))
                  ("https://api.test/three"
                   . ,(async-http-queue-test-http-response
                       200
                       "{\"id\":3,\"name\":\"three\"}"))))
               (original-process
                (symbol-function
                 'async-http-queue--process))
               state
               events
               requests
               next-id
               messages
               errors
               result
               active-history)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (value)
                  (setq state value)
                  (funcall original-process value)))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function 'cancel-timer)
                (lambda (event)
                  (aset event 6 t)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &rest _)
                  (let ((buffer
                         (async-http-queue-test-response-buffer
                          (file-name-nondirectory url)
                          (alist-get
                           url
                           responses
                           nil
                           nil
                           #'equal))))
                    (setq requests
                          (append
                           requests
                           (list
                            (list url callback buffer))))
                    buffer)))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue
             urls
             :max-concurrent 2
             :callback
             (lambda (value)
               (setq result value))
             :error-callback
             (lambda (url)
               (push url errors)))
            (async-http-queue-test-run-timer-event
             (nth 0 events))
            (push
             (async-http-queue--state-active-workers
              state)
             active-history)
            (async-http-queue-test-run-timer-event
             (nth 1 events))
            (push
             (async-http-queue--state-active-workers
              state)
             active-history)
            (let ((request
                   (seq-find
                    (lambda (entry)
                      (equal
                       (car entry)
                       "https://api.test/two"))
                    requests)))
              (with-current-buffer (nth 2 request)
                (funcall (nth 1 request) nil)))
            (push
             (async-http-queue--state-active-workers
              state)
             active-history)
            (async-http-queue-test-run-timer-event
             (seq-find
              (lambda (event)
                (and
                 (= (aref event 2) 0.05)
                 (eq
                  (aref event 4)
                  #'async-http-queue--process-next-pending)
                 (> (aref event 1) 2)))
              events))
            (push
             (async-http-queue--state-active-workers
              state)
             active-history)
            (let ((request
                   (seq-find
                    (lambda (entry)
                      (equal
                       (car entry)
                       "https://api.test/one"))
                    requests)))
              (with-current-buffer (nth 2 request)
                (funcall (nth 1 request) nil)))
            (let ((request
                   (seq-find
                    (lambda (entry)
                      (equal
                       (car entry)
                       "https://api.test/three"))
                    requests)))
              (with-current-buffer (nth 2 request)
                (funcall (nth 1 request) nil)))
            (push
             (async-http-queue--state-active-workers
              state)
             active-history)
            (list
             (mapcar #'car requests)
             (nreverse active-history)
             (async-http-queue-test-state-snapshot
              state)
             (mapcar
              (lambda (value)
                (and
                 value
                 (list
                  (gethash "id" value)
                  (gethash "name" value))))
              (append result nil))
             (nreverse errors)
             (nreverse messages)
             (seq-count
              (lambda (event)
                (aref event 6))
              events)
             (mapcar
              #'buffer-live-p
              (mapcar
               (lambda (request)
                 (nth 2 request))
               requests)))))"##,
        expect![[
            r#"OK (("https://api.test/one" "https://api.test/two" "https://api.test/three") (1 2 1 2 0) (:queue (("https://api.test/one" error nil) ("https://api.test/two" done #s(hash-table test equal data ("id" 2 "name" "two"))) ("https://api.test/three" done #s(hash-table test equal data ("id" 3 "name" "three")))) :active 0 :limit 2 :timeout 10 :parser json-parse-buffer :completion t :error t) (nil (2 "two") (3 "three")) ("https://api.test/one") ("Fetching 3 URLs..." "HTTP 503 error fetching URL: https://api.test/one" "Loaded 2 URLs (1 failed)") 3 (nil nil nil))"#
        ]],
    )
}

fn async_http_queue_limit_one_starts_each_request_only_after_prior_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_limit_one_starts_each_request_only_after_prior_completion",
        r##"(let* ((urls
                '("https://api.test/a"
                  "https://api.test/b"
                  "https://api.test/c"))
               (original-process
                (symbol-function
                 'async-http-queue--process))
               state
               events
               requests
               result
               next-id
               checkpoints)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (value)
                  (setq state value)
                  (funcall original-process value)))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function 'cancel-timer)
                (lambda (event)
                  (aset event 6 t)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &rest _)
                  (let ((buffer
                         (async-http-queue-test-response-buffer
                          (file-name-nondirectory url)
                          (async-http-queue-test-http-response
                           200
                           (format
                            "{\"url\":%S}"
                            url)))))
                    (setq requests
                          (append
                           requests
                           (list
                            (list url callback buffer))))
                    buffer)))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue
             urls
             :max-concurrent 1
             :callback
             (lambda (value)
               (setq result value)))
            (async-http-queue-test-run-timer-event
             (car events))
            (push
             (list
              (length requests)
              (async-http-queue--state-active-workers
               state))
             checkpoints)
            (dotimes (index 3)
              (let ((request (nth index requests)))
                (with-current-buffer (nth 2 request)
                  (funcall (nth 1 request) nil)))
              (push
               (list
                (length requests)
                (async-http-queue--state-active-workers
                 state))
               checkpoints)
              (when (< index 2)
                (async-http-queue-test-run-timer-event
                 (seq-find
                  (lambda (event)
                    (and
                     (not (aref event 6))
                     (eq
                      (aref event 4)
                      #'async-http-queue--process-next-pending)
                     (> (aref event 1)
                        (1+ index))))
                  events))))
            (list
             (mapcar #'car requests)
             (nreverse checkpoints)
             (mapcar
              (lambda (value)
                (gethash "url" value))
              (append result nil))
             (async-http-queue-test-queue-snapshot
              state))))"##,
        expect![[
            r#"OK (("https://api.test/a" "https://api.test/b" "https://api.test/c") ((1 1) (1 0) (2 0) (3 0)) ("https://api.test/a" "https://api.test/b" "https://api.test/c") (("https://api.test/a" done #s(hash-table test equal data ("url" "https://api.test/a"))) ("https://api.test/b" done #s(hash-table test equal data ("url" "https://api.test/b"))) ("https://api.test/c" done #s(hash-table test equal data ("url" "https://api.test/c")))))"#
        ]],
    )
}

fn async_http_queue_timeout_in_full_queue_releases_slot_and_allows_next_request() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_timeout_in_full_queue_releases_slot_and_allows_next_request",
        r##"(let* ((urls
                '("https://api.test/slow"
                  "https://api.test/fast"))
               (responses
                `(("https://api.test/slow" . :timeout)
                  ("https://api.test/fast"
                   . ,(async-http-queue-test-http-response
                       200
                       "fast-body"))))
               (original-process
                (symbol-function
                 'async-http-queue--process))
               state
               events
               requests
               result
               errors
               messages
               next-id)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (value)
                  (setq state value)
                  (funcall original-process value)))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function 'cancel-timer)
                (lambda (event)
                  (aset event 6 t)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &rest _)
                  (let ((buffer
                         (generate-new-buffer
                          (concat
                           " *async-http-timeout-"
                           (file-name-nondirectory url)
                           "*"))))
                    (unless
                        (eq
                         (alist-get
                          url
                          responses
                          nil
                          nil
                          #'equal)
                         :timeout)
                      (with-current-buffer buffer
                        (insert
                         (alist-get
                          url
                          responses
                          nil
                          nil
                          #'equal))))
                    (setq requests
                          (append
                           requests
                           (list
                            (list url callback buffer))))
                    buffer)))
               ((symbol-function 'get-buffer-process)
                (lambda (_) nil))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue
             urls
             :max-concurrent 1
             :timeout 4
             :parser nil
             :callback
             (lambda (value)
               (setq result value))
             :error-callback
             (lambda (url)
               (push url errors)))
            (async-http-queue-test-run-timer-event
             (nth 0 events))
            (async-http-queue-test-run-timer-event
             (seq-find
              (lambda (event)
                (= (aref event 2) 4))
              events))
            (async-http-queue-test-run-timer-event
             (seq-find
              (lambda (event)
                (and
                 (= (aref event 2) 0.05)
                 (eq
                  (aref event 4)
                  #'async-http-queue--process-next-pending)))
              events))
            (let ((request
                   (seq-find
                    (lambda (entry)
                      (equal
                       (car entry)
                       "https://api.test/fast"))
                    requests)))
              (with-current-buffer (nth 2 request)
                (funcall (nth 1 request) nil)))
            (list
             (mapcar #'car requests)
             (append result nil)
             (nreverse errors)
             (async-http-queue-test-state-snapshot
              state)
             (nreverse messages)
             (mapcar
              #'buffer-live-p
              (mapcar
               (lambda (request)
                 (nth 2 request))
               requests)))))"##,
        expect![[
            r#"OK (("https://api.test/slow" "https://api.test/fast") (nil "fast-body") ("https://api.test/slow") (:queue (("https://api.test/slow" error nil) ("https://api.test/fast" done "fast-body")) :active 0 :limit 1 :timeout 4 :parser nil :completion t :error t) ("Fetching 2 URLs..." "Timeout fetching URL https://api.test/slow (4 seconds)" "Loaded 1 URLs (1 failed)") (nil nil))"#
        ]],
    )
}

fn async_http_queue_two_independent_queues_interleave_without_sharing_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_two_independent_queues_interleave_without_sharing_state",
        r##"(let* ((original-process
                (symbol-function
                 'async-http-queue--process))
               states
               events
               requests
               results
               next-id)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (state)
                  (setq states
                        (append states (list state)))
                  (funcall original-process state)))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function 'cancel-timer)
                (lambda (event)
                  (aset event 6 t)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &rest _)
                  (let ((buffer
                         (async-http-queue-test-response-buffer
                          (file-name-nondirectory url)
                          (async-http-queue-test-http-response
                           200
                           url))))
                    (setq requests
                          (append
                           requests
                           (list
                            (list url callback buffer))))
                    buffer)))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue
             '("https://first.test/a")
             :max-concurrent 1
             :parser nil
             :callback
             (lambda (value)
               (push
                (list :first (append value nil))
                results)))
            (async-http-queue
             '("https://second.test/b")
             :max-concurrent 1
             :parser nil
             :callback
             (lambda (value)
               (push
                (list :second (append value nil))
                results)))
            (async-http-queue-test-run-timer-event
             (nth 1 events))
            (async-http-queue-test-run-timer-event
             (nth 0 events))
            (dolist
                (url
                 '("https://second.test/b"
                   "https://first.test/a"))
              (let ((request
                     (seq-find
                      (lambda (entry)
                        (equal (car entry) url))
                      requests)))
                (with-current-buffer (nth 2 request)
                  (funcall (nth 1 request) nil))))
            (list
             (mapcar #'car requests)
             (nreverse results)
             (mapcar
              #'async-http-queue-test-state-snapshot
              states)
             (eq (car states) (cadr states)))))"##,
        expect![[
            r#"OK (("https://second.test/b" "https://first.test/a") ((:second ("https://second.test/b")) (:first ("https://first.test/a"))) ((:queue (("https://first.test/a" done "https://first.test/a")) :active 0 :limit 1 :timeout 10 :parser nil :completion t :error nil) (:queue (("https://second.test/b" done "https://second.test/b")) :active 0 :limit 1 :timeout 10 :parser nil :completion t :error nil)) nil)"#
        ]],
    )
}

fn async_http_queue_large_batch_reports_exact_progress_and_final_summary() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_large_batch_reports_exact_progress_and_final_summary",
        r##"(let* ((urls
                (cl-loop
                 for index from 1 to 12
                 collect
                 (format
                  "https://api.test/%02d"
                  index)))
               (original-process
                (symbol-function
                 'async-http-queue--process))
               state
               events
               next-id
               failures
               results
               messages
               executed)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (value)
                  (setq state value)
                  (funcall original-process value)))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function
                 'async-http-queue--fetch-url)
                (lambda (_state url success error)
                  (if
                      (member
                       url
                       '("https://api.test/04"
                         "https://api.test/09"))
                      (progn
                        (push url failures)
                        (funcall error))
                    (funcall
                     success
                     (string-to-number
                      (substring url -2))))))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue
             urls
             :max-concurrent 3
             :parser nil
             :callback
             (lambda (value)
               (setq results value))
             :error-callback #'ignore)
            (while
                (let ((event
                       (seq-find
                        (lambda (candidate)
                          (not
                           (memq
                            (aref candidate 1)
                            executed)))
                        events)))
                  (when event
                    (push
                     (aref event 1)
                     executed)
                    (async-http-queue-test-run-timer-event
                     event)
                    t)))
            (list
             (append results nil)
             (nreverse failures)
             (nreverse messages)
             (async-http-queue-test-state-snapshot
              state)
             (length events))))"##,
        expect![[
            r#"OK ((1 2 3 nil 5 6 7 8 nil 10 11 12) ("https://api.test/04" "https://api.test/09") ("Fetching 12 URLs..." "Loading URLs... 1/12 completed" "Loading URLs... 2/12 completed" "Loading URLs... 3/12 completed" "Loading URLs... 3/12 completed (1 failed)" "Loading URLs... 4/12 completed (1 failed)" "Loading URLs... 5/12 completed (1 failed)" "Loading URLs... 6/12 completed (1 failed)" "Loading URLs... 7/12 completed (1 failed)" "Loading URLs... 7/12 completed (2 failed)" "Loading URLs... 8/12 completed (2 failed)" "Loading URLs... 9/12 completed (2 failed)" "Loaded 10 URLs (2 failed)") (:queue (("https://api.test/01" done 1) ("https://api.test/02" done 2) ("https://api.test/03" done 3) ("https://api.test/04" error nil) ("https://api.test/05" done 5) ("https://api.test/06" done 6) ("https://api.test/07" done 7) ("https://api.test/08" done 8) ("https://api.test/09" error nil) ("https://api.test/10" done 10) ("https://api.test/11" done 11) ("https://api.test/12" done 12)) :active 0 :limit 3 :timeout 10 :parser nil :completion t :error t) 15)"#
        ]],
    )
}

fn async_http_queue_each_failure_is_terminal_and_never_retried() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_each_failure_is_terminal_and_never_retried",
        r##"(let* ((original-process
                (symbol-function
                 'async-http-queue--process))
               state
               events
               requests
               errors
               result
               next-id)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (value)
                  (setq state value)
                  (funcall original-process value)))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function 'cancel-timer)
                (lambda (event)
                  (aset event 6 t)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &rest _)
                  (let ((buffer
                         (async-http-queue-test-response-buffer
                          "no-retry"
                          (async-http-queue-test-http-response
                           429
                           "rate limited"))))
                    (setq requests
                          (append
                           requests
                           (list
                            (list url callback buffer))))
                    buffer)))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue
             '("https://api.test/rate-limited")
             :max-concurrent 1
             :parser nil
             :callback
             (lambda (value)
               (setq result value))
             :error-callback
             (lambda (url)
               (push url errors)))
            (async-http-queue-test-run-timer-event
             (car events))
            (with-current-buffer (nth 2 (car requests))
              (funcall (nth 1 (car requests)) nil))
            (dolist (event events)
              (when
                  (and
                   (not (aref event 6))
                   (eq
                    (aref event 4)
                    #'async-http-queue--process-next-pending))
                (async-http-queue-test-run-timer-event
                 event)))
            (list
             (mapcar #'car requests)
             (nreverse errors)
             (append result nil)
             (async-http-queue-test-state-snapshot
              state))))"##,
        expect![[
            r#"OK (("https://api.test/rate-limited") ("https://api.test/rate-limited") (nil) (:queue (("https://api.test/rate-limited" error nil)) :active 0 :limit 1 :timeout 10 :parser nil :completion t :error t))"#
        ]],
    )
}

fn async_http_queue_completion_callback_can_reentrantly_start_second_queue() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_completion_callback_can_reentrantly_start_second_queue",
        r##"(let* ((events)
               (requests)
               (next-id 0)
               lifecycle)
          (cl-letf
              (((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
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
                          (append events (list event)))
                    event)))
               ((symbol-function 'cancel-timer)
                (lambda (event)
                  (aset event 6 t)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &rest _)
                  (let ((buffer
                         (async-http-queue-test-response-buffer
                          (file-name-nondirectory url)
                          (async-http-queue-test-http-response
                           200
                           url))))
                    (setq requests
                          (append
                           requests
                           (list
                            (list url callback buffer))))
                    buffer)))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue
             '("https://api.test/outer")
             :max-concurrent 1
             :parser nil
             :callback
             (lambda (outer)
               (push
                (list :outer (append outer nil))
                lifecycle)
               (async-http-queue
                '("https://api.test/inner")
                :max-concurrent 1
                :parser nil
                :callback
                (lambda (inner)
                  (push
                   (list :inner (append inner nil))
                   lifecycle)))))
            (async-http-queue-test-run-timer-event
             (nth 0 events))
            (with-current-buffer (nth 2 (nth 0 requests))
              (funcall (nth 1 (nth 0 requests)) nil))
            (async-http-queue-test-run-timer-event
             (car (last events)))
            (with-current-buffer (nth 2 (nth 1 requests))
              (funcall (nth 1 (nth 1 requests)) nil))
            (list
             (mapcar #'car requests)
             (nreverse lifecycle)
             (length events))))"##,
        expect![[
            r#"OK (("https://api.test/outer" "https://api.test/inner") ((:outer ("https://api.test/outer")) (:inner ("https://api.test/inner"))) 6)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_http_queue_mixed_end_to_end_workflow_refills_slots_and_preserves_input_order(),
        async_http_queue_limit_one_starts_each_request_only_after_prior_completion(),
        async_http_queue_timeout_in_full_queue_releases_slot_and_allows_next_request(),
        async_http_queue_two_independent_queues_interleave_without_sharing_state(),
        async_http_queue_large_batch_reports_exact_progress_and_final_summary(),
        async_http_queue_each_failure_is_terminal_and_never_retried(),
        async_http_queue_completion_callback_can_reentrantly_start_second_queue(),
    ]
}
