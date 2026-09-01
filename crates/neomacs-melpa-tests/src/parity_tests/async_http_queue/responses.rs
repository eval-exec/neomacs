use expect_test::expect;

use super::ParityBatchCase;

fn async_http_queue_fetch_json_success_uses_exact_url_contract_cancels_timer_and_kills_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_fetch_json_success_uses_exact_url_contract_cancels_timer_and_kills_buffer",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 3
                 17
                 :default))
               (response
                (async-http-queue-test-http-response
                 200
                 "{\"id\":42,\"title\":\"hello\",\"flags\":[true,false]}"))
               buffer
               retrieval-callback
               retrieval-arguments
               timeout-event
               canceled
               success
               failures)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (url callback &optional callback-arguments silent inhibit-cookies)
                  (setq
                   retrieval-callback callback
                   retrieval-arguments
                   (list
                    url
                    callback-arguments
                    silent
                    inhibit-cookies)
                   buffer
                   (async-http-queue-test-response-buffer
                    "json"
                    response))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (setq timeout-event
                        (vector
                         :timer
                         1
                         delay
                         repeat
                         function
                         arguments
                         nil))
                  timeout-event))
               ((symbol-function 'cancel-timer)
                (lambda (timer)
                  (push
                   (list
                    (eq timer timeout-event)
                    (aref timer 2))
                   canceled)
                  (aset timer 6 t))))
            (let ((return-value
                   (async-http-queue--fetch-url
                    state
                    "https://api.test/json"
                    (lambda (data)
                      (setq success
                            (list
                             (gethash "id" data)
                             (gethash "title" data)
                             (append
                              (gethash "flags" data)
                              nil))))
                    (lambda ()
                      (setq failures (1+ (or failures 0)))))))
              (with-current-buffer buffer
                (funcall retrieval-callback nil))
              (list
               (eq return-value timeout-event)
               retrieval-arguments
               success
               failures
               (buffer-live-p buffer)
               (nreverse canceled)
               (async-http-queue-test-timer-summary
                (list timeout-event))))))"##,
        expect![[
            r#"OK (t ("https://api.test/json" nil t nil) (42 "hello" (t :false)) nil nil ((t 17)) ((1 17 nil t)))"#
        ]],
    )
}

fn async_http_queue_fetch_raw_and_custom_parser_receive_exact_body_start() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_fetch_raw_and_custom_parser_receive_exact_body_start",
        r##"(let (observations)
          (dolist
              (case
               '(("raw" nil "λ raw\nsecond line")
                 ("custom" custom "alpha,beta,gamma")))
            (let* ((name (car case))
                   (mode (cadr case))
                   (body (nth 2 case))
                   (parser
                    (cond
                     ((null mode) nil)
                     (t
                      (lambda ()
                        (list
                         :point (point)
                         :prefix
                         (buffer-substring-no-properties
                          (point)
                          (min
                           (+ (point) 5)
                           (point-max)))
                         :parts
                         (split-string
                          (buffer-substring-no-properties
                           (point)
                           (point-max))
                          ","))))))
                   (state
                    (async-http-queue-test-state
                     nil
                     1
                     9
                     parser))
                   (response
                    (async-http-queue-test-http-response
                     201
                     body
                     "\n"
                     "Created"))
                   buffer
                   callback
                   result
                   failures)
              (cl-letf
                  (((symbol-function 'url-retrieve)
                    (lambda (_url function &rest _)
                      (setq
                       callback function
                       buffer
                       (async-http-queue-test-response-buffer
                        name
                        response))
                      buffer))
                   ((symbol-function 'run-at-time)
                    (lambda (&rest _)
                      :timer))
                   ((symbol-function 'cancel-timer)
                    #'ignore))
                (async-http-queue--fetch-url
                 state
                 (concat "https://api.test/" name)
                 (lambda (data)
                   (setq result data))
                 (lambda ()
                   (setq failures (1+ (or failures 0)))))
                (with-current-buffer buffer
                  (funcall callback nil))
                (push
                 (list
                  name
                  result
                  failures
                  (buffer-live-p buffer))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK (("raw" "λ raw\nsecond line" nil nil) ("custom" (:point 76 :prefix "alpha" :parts ("alpha" "beta" "gamma")) nil nil))"#
        ]],
    )
}

fn async_http_queue_http_status_boundary_matrix_routes_success_and_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_http_status_boundary_matrix_routes_success_and_failure",
        r##"(let (observations)
          (dolist (status '(199 200 204 299 300 404 500))
            (let* ((state
                    (async-http-queue-test-state
                     nil
                     1
                     5
                     nil))
                   (response
                    (async-http-queue-test-http-response
                     status
                     (format "body-%d" status)))
                   buffer
                   callback
                   result
                   failures
                   messages)
              (cl-letf
                  (((symbol-function 'url-retrieve)
                    (lambda (_url function &rest _)
                      (setq
                       callback function
                       buffer
                       (async-http-queue-test-response-buffer
                        (number-to-string status)
                        response))
                      buffer))
                   ((symbol-function 'run-at-time)
                    (lambda (&rest _)
                      :timer))
                   ((symbol-function 'cancel-timer)
                    #'ignore)
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (push
                       (apply
                        #'format
                        format-string
                        arguments)
                       messages))))
                (async-http-queue--fetch-url
                 state
                 (format
                  "https://api.test/status/%d"
                  status)
                 (lambda (data)
                   (setq result data))
                 (lambda ()
                   (setq failures (1+ (or failures 0)))))
                (with-current-buffer buffer
                  (funcall callback nil))
                (push
                 (list
                  status
                  result
                  failures
                  (nreverse messages)
                  (buffer-live-p buffer))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK ((199 nil 1 ("HTTP 199 error fetching URL: https://api.test/status/199") nil) (200 "body-200" nil nil nil) (204 "body-204" nil nil nil) (299 "body-299" nil nil nil) (300 nil 1 ("HTTP 300 error fetching URL: https://api.test/status/300") nil) (404 nil 1 ("HTTP 404 error fetching URL: https://api.test/status/404") nil) (500 nil 1 ("HTTP 500 error fetching URL: https://api.test/status/500") nil))"#
        ]],
    )
}

fn async_http_queue_transport_error_takes_precedence_over_valid_http_body() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_transport_error_takes_precedence_over_valid_http_body",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 5
                 nil))
               (response
                (async-http-queue-test-http-response
                 200
                 "would otherwise succeed"))
               buffer
               callback
               success
               failures
               messages)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (_url function &rest _)
                  (setq
                   callback function
                   buffer
                   (async-http-queue-test-response-buffer
                    "transport-error"
                    response))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (&rest _)
                  :timer))
               ((symbol-function 'cancel-timer)
                #'ignore)
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue--fetch-url
             state
             "https://api.test/transport"
             (lambda (data)
               (setq success data))
             (lambda ()
               (setq failures (1+ (or failures 0)))))
            (with-current-buffer buffer
              (funcall
               callback
               '(:redirects 2
                 :error
                 (error connection-refused))))
            (list
             success
             failures
             (nreverse messages)
             (buffer-live-p buffer))))"##,
        expect![[
            r#"OK (nil 1 ("Error fetching URL https://api.test/transport: Download failed: (error connection-refused)") nil)"#
        ]],
    )
}

fn async_http_queue_invalid_http_response_matrix_reports_error_and_cleans_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_invalid_http_response_matrix_reports_error_and_cleans_buffers",
        r##"(let (observations)
          (dolist
              (response
               '("garbage"
                 "HTTP/2 200 OK\r\n\r\nbody"
                 "http/1.1 200 OK\r\n\r\nbody"
                 "HTTP/1.1 20 OK\r\n\r\nbody"
                 "HTTP/1.1 200OK\r\n\r\nbody"))
            (let* ((state
                    (async-http-queue-test-state
                     nil
                     1
                     5
                     nil))
                   buffer
                   callback
                   success
                   failures
                   messages)
              (cl-letf
                  (((symbol-function 'url-retrieve)
                    (lambda (_url function &rest _)
                      (setq
                       callback function
                       buffer
                       (async-http-queue-test-response-buffer
                        "invalid"
                        response))
                      buffer))
                   ((symbol-function 'run-at-time)
                    (lambda (&rest _)
                      :timer))
                   ((symbol-function 'cancel-timer)
                    #'ignore)
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (push
                       (apply
                        #'format
                        format-string
                        arguments)
                       messages))))
                (async-http-queue--fetch-url
                 state
                 "https://api.test/invalid"
                 (lambda (data)
                   (setq success data))
                 (lambda ()
                   (setq failures (1+ (or failures 0)))))
                (with-current-buffer buffer
                  (funcall callback nil))
                (push
                 (list
                  response
                  success
                  failures
                  (nreverse messages)
                  (buffer-live-p buffer))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK (("garbage" nil 1 ("Invalid HTTP response for URL: https://api.test/invalid") nil) ("HTTP/2 200 OK\15\n\15\nbody" nil 1 ("Invalid HTTP response for URL: https://api.test/invalid") nil) ("http/1.1 200 OK\15\n\15\nbody" "body" nil nil nil) ("HTTP/1.1 20 OK\15\n\15\nbody" nil 1 ("Invalid HTTP response for URL: https://api.test/invalid") nil) ("HTTP/1.1 200OK\15\n\15\nbody" "body" nil nil nil))"#
        ]],
    )
}

fn async_http_queue_parser_error_and_nil_result_both_route_to_error_callback() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_parser_error_and_nil_result_both_route_to_error_callback",
        r##"(let (observations)
          (dolist (mode '(signal nil-result))
            (let* ((parser
                    (if (eq mode 'signal)
                        (lambda ()
                          (error
                           "parser exploded at %d"
                           (point)))
                      (lambda () nil)))
                   (state
                    (async-http-queue-test-state
                     nil
                     1
                     5
                     parser))
                   (response
                    (async-http-queue-test-http-response
                     200
                     "valid body"))
                   buffer
                   callback
                   success
                   failures
                   messages)
              (cl-letf
                  (((symbol-function 'url-retrieve)
                    (lambda (_url function &rest _)
                      (setq
                       callback function
                       buffer
                       (async-http-queue-test-response-buffer
                        (symbol-name mode)
                        response))
                      buffer))
                   ((symbol-function 'run-at-time)
                    (lambda (&rest _)
                      :timer))
                   ((symbol-function 'cancel-timer)
                    #'ignore)
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (push
                       (apply
                        #'format
                        format-string
                        arguments)
                       messages))))
                (async-http-queue--fetch-url
                 state
                 (format "https://api.test/%s" mode)
                 (lambda (data)
                   (setq success data))
                 (lambda ()
                   (setq failures (1+ (or failures 0)))))
                (with-current-buffer buffer
                  (funcall callback nil))
                (push
                 (list
                  mode
                  success
                  failures
                  (nreverse messages)
                  (buffer-live-p buffer))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK ((signal nil 1 ("Error fetching URL https://api.test/signal: parser exploded at 77") nil) (nil-result nil 1 nil nil))"#
        ]],
    )
}

fn async_http_queue_response_callback_is_guarded_against_duplicate_delivery() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_response_callback_is_guarded_against_duplicate_delivery",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 5
                 nil))
               (response
                (async-http-queue-test-http-response
                 200
                 "first"))
               buffer
               callback
               timer
               cancellations
               successes
               failures)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (_url function &rest _)
                  (setq
                   callback function
                   buffer
                   (async-http-queue-test-response-buffer
                    "duplicate"
                    response))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (setq timer
                        (vector
                         :timer 1 delay repeat
                         function arguments nil))
                  timer))
               ((symbol-function 'cancel-timer)
                (lambda (value)
                  (push (eq value timer)
                        cancellations)
                  (aset value 6 t))))
            (async-http-queue--fetch-url
             state
             "https://api.test/duplicate"
             (lambda (data)
               (push data successes))
             (lambda ()
               (setq failures (1+ (or failures 0)))))
            (with-current-buffer buffer
              (funcall callback nil))
            (with-temp-buffer
              (insert
               (async-http-queue-test-http-response
                200
                "second"))
              (funcall callback nil))
            (list
             (nreverse successes)
             failures
             (nreverse cancellations)
             (buffer-live-p buffer)
             (aref timer 6))))"##,
        expect![[r#"OK (("first") nil (t t) nil t)"#]],
    )
}

fn async_http_queue_timeout_kills_live_request_and_suppresses_late_response() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_timeout_kills_live_request_and_suppresses_late_response",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 3
                 nil))
               (response
                (async-http-queue-test-http-response
                 200
                 "too late"))
               buffer
               retrieval-callback
               timeout-event
               request-process
               successes
               failures
               messages)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (_url function &rest _)
                  (setq
                   retrieval-callback function
                   buffer
                   (async-http-queue-test-response-buffer
                    "timeout"
                    response))
                  (setq request-process
                        (make-pipe-process
                         :name
                         "async-http-queue-test-timeout-pipe"
                         :buffer buffer
                         :noquery t))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (setq timeout-event
                        (vector
                         :timer 1 delay repeat
                         function arguments nil))
                  timeout-event))
               ((symbol-function 'cancel-timer)
                (lambda (timer)
                  (aset timer 6 t)))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue--fetch-url
             state
             "https://api.test/slow"
             (lambda (data)
               (push data successes))
             (lambda ()
               (setq failures (1+ (or failures 0)))))
            (let ((before
                   (list
                    (processp request-process)
                    (process-live-p request-process)
                    (eq
                     (get-buffer-process buffer)
                     request-process))))
              (async-http-queue-test-run-timer-event
               timeout-event)
              (with-temp-buffer
                (insert response)
                (funcall retrieval-callback nil))
              (list
               before
               successes
               failures
               (nreverse messages)
               (process-live-p request-process)
               (process-status request-process)
               (buffer-live-p buffer)
               (aref timeout-event 6)))))"##,
        expect![[
            r#"OK ((t (open listen connect stop) t) nil 1 ("Timeout fetching URL https://api.test/slow (3 seconds)") nil closed nil t)"#
        ]],
    )
}

fn async_http_queue_timeout_handles_nil_retrieval_buffer_without_process_cleanup() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_timeout_handles_nil_retrieval_buffer_without_process_cleanup",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 2
                 nil))
               timer
               process-lookups
               failures)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (&rest _) nil))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (setq timer
                        (vector
                         :timer 1 delay repeat
                         function arguments nil))
                  timer))
               ((symbol-function 'get-buffer-process)
                (lambda (buffer)
                  (push buffer process-lookups)
                  nil))
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue--fetch-url
             state
             "https://api.test/no-buffer"
             #'ignore
             (lambda ()
               (setq failures (1+ (or failures 0)))))
            (async-http-queue-test-run-timer-event
             timer)
            (list
             failures
             process-lookups
             (aref timer 2)
             (aref timer 6))))"##,
        expect!["OK (1 nil 2 nil)"],
    )
}

fn async_http_queue_url_retrieve_signal_propagates_before_timeout_is_registered() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_url_retrieve_signal_propagates_before_timeout_is_registered",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 8
                 nil))
               timers
               callbacks)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (&rest _)
                  (error "resolver unavailable")))
               ((symbol-function 'run-at-time)
                (lambda (&rest arguments)
                  (push arguments timers)
                  :timer)))
            (list
             (async-http-queue-test-error-data
              (lambda ()
                (async-http-queue--fetch-url
                 state
                 "https://api.test/immediate-error"
                 (lambda (data)
                   (push
                    (list :success data)
                    callbacks))
                 (lambda ()
                   (push :error callbacks)))))
             timers
             callbacks)))"##,
        expect![[r#"OK ((:error error ("resolver unavailable")) nil nil)"#]],
    )
}

fn async_http_queue_success_callback_signal_propagates_after_response_buffer_cleanup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_success_callback_signal_propagates_after_response_buffer_cleanup",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 5
                 nil))
               (response
                (async-http-queue-test-http-response
                 200
                 "payload"))
               buffer
               callback
               failures)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (_url function &rest _)
                  (setq
                   callback function
                   buffer
                   (async-http-queue-test-response-buffer
                    "success-signal"
                    response))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (&rest _)
                  :timer))
               ((symbol-function 'cancel-timer)
                #'ignore))
            (async-http-queue--fetch-url
             state
             "https://api.test/success-signal"
             (lambda (_)
               (error "consumer exploded"))
             (lambda ()
               (setq failures (1+ (or failures 0)))))
            (let ((outcome
                   (async-http-queue-test-error-data
                    (lambda ()
                      (with-current-buffer buffer
                        (funcall callback nil))))))
              (list
               outcome
               failures
               (buffer-live-p buffer)))))"##,
        expect![[r#"OK ((:error error ("consumer exploded")) nil nil)"#]],
    )
}

fn async_http_queue_error_callback_signal_propagates_after_response_buffer_cleanup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_error_callback_signal_propagates_after_response_buffer_cleanup",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 5
                 nil))
               (response
                (async-http-queue-test-http-response
                 503
                 "unavailable"))
               buffer
               callback
               successes)
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (_url function &rest _)
                  (setq
                   callback function
                   buffer
                   (async-http-queue-test-response-buffer
                    "error-signal"
                    response))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (&rest _)
                  :timer))
               ((symbol-function 'cancel-timer)
                #'ignore)
               ((symbol-function 'message)
                #'ignore))
            (async-http-queue--fetch-url
             state
             "https://api.test/error-signal"
             (lambda (data)
               (push data successes))
             (lambda ()
               (error "failure consumer exploded")))
            (let ((outcome
                   (async-http-queue-test-error-data
                    (lambda ()
                      (with-current-buffer buffer
                        (funcall callback nil))))))
              (list
               outcome
               successes
               (buffer-live-p buffer)))))"##,
        expect![[r#"OK ((:error error ("failure consumer exploded")) nil nil)"#]],
    )
}

fn async_http_queue_non_numeric_timeout_breaks_timeout_message_before_cleanup() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_non_numeric_timeout_breaks_timeout_message_before_cleanup",
        r##"(let* ((state
                (async-http-queue-test-state
                 nil
                 1
                 1
                 nil))
               buffer
               timer
               failures)
          (setf
           (async-http-queue--state-timeout state)
           "soon")
          (cl-letf
              (((symbol-function 'url-retrieve)
                (lambda (&rest _)
                  (setq buffer
                        (generate-new-buffer
                         " *async-http-queue-test-non-numeric-timeout*"))
                  buffer))
               ((symbol-function 'run-at-time)
                (lambda (delay repeat function &rest arguments)
                  (setq timer
                        (vector
                         :timer 1 delay repeat
                         function arguments nil))
                  timer)))
            (unwind-protect
                (progn
                  (async-http-queue--fetch-url
                   state
                   "https://api.test/non-numeric-timeout"
                   #'ignore
                   (lambda ()
                     (setq failures (1+ (or failures 0)))))
                  (list
                   (async-http-queue-test-error-data
                    (lambda ()
                      (async-http-queue-test-run-timer-event
                       timer)))
                   failures
                   (buffer-live-p buffer)
                   (aref timer 2)))
              (async-http-queue-test-kill-buffer
               buffer))))"##,
        expect![[
            r#"OK ((:error error ("Format specifier doesn’t match argument type")) nil t "soon")"#
        ]],
    )
}

pub(super) fn responses_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_http_queue_fetch_json_success_uses_exact_url_contract_cancels_timer_and_kills_buffer(
        ),
        async_http_queue_fetch_raw_and_custom_parser_receive_exact_body_start(),
        async_http_queue_http_status_boundary_matrix_routes_success_and_failure(),
        async_http_queue_transport_error_takes_precedence_over_valid_http_body(),
        async_http_queue_invalid_http_response_matrix_reports_error_and_cleans_buffers(),
        async_http_queue_parser_error_and_nil_result_both_route_to_error_callback(),
        async_http_queue_response_callback_is_guarded_against_duplicate_delivery(),
        async_http_queue_timeout_kills_live_request_and_suppresses_late_response(),
        async_http_queue_timeout_handles_nil_retrieval_buffer_without_process_cleanup(),
        async_http_queue_url_retrieve_signal_propagates_before_timeout_is_registered(),
        async_http_queue_success_callback_signal_propagates_after_response_buffer_cleanup(),
        async_http_queue_error_callback_signal_propagates_after_response_buffer_cleanup(),
        async_http_queue_non_numeric_timeout_breaks_timeout_message_before_cleanup(),
    ]
}
