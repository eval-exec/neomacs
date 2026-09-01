use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_normalize_header_capitalizes_hyphenated_components_and_edge_inputs_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_normalize_header_capitalizes_hyphenated_components_and_edge_inputs_exactly",
        r##"(mapcar
          (lambda (header)
            (list
             header
             (atomic-chrome-normalize-header
              header)))
          '("content-length"
            "CONTENT-TYPE"
            "x-forwarded-for"
            "etag"
            "a--b"
            "-leading"
            "trailing-"
            ""
            "ümlaut-header"
            "x_underscore"))"##,
        expect![[
            r#"OK (("content-length" "Content-Length") ("CONTENT-TYPE" "Content-Type") ("x-forwarded-for" "X-Forwarded-For") ("etag" "Etag") ("a--b" "A--B") ("-leading" "-Leading") ("trailing-" "Trailing-") ("" "") ("ümlaut-header" "Ümlaut-Header") ("x_underscore" "X_Underscore"))"#
        ]],
    )
}

fn atomic_chrome_httpd_parse_string_decodes_practical_request_line_headers_and_body()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_parse_string_decodes_practical_request_line_headers_and_body",
        r##"(mapcar
          (lambda (request)
            (list
             request
             (atomic-chrome-httpd-parse-string
              request)))
          '("GET / HTTP/1.1\r\nHost: localhost:4001\r\nAccept: application/json\r\n\r\n"
            "POST /connect HTTP/1.1\r\nHost: localhost:4001\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: 17\r\nX-Trace-Id: a:b:c\r\n\r\n{\"text\":\"hello\"}"
            "OPTIONS /socket?x=1 HTTP/1.0\nOrigin: https://example.test\nConnection: keep-alive\n\nbody"
            "POST /unicode HTTP/1.1\r\nContent-Length: 8\r\n\r\nλ😀"))"##,
        expect![[
            r#"OK (("GET / HTTP/1.1\15\nHost: localhost:4001\15\nAccept: application/json\15\n\15\n" (("GET" "/" "HTTP/1.1") ("Host" "localhost:4001") ("Accept" "application/json") ("Content" ""))) ("POST /connect HTTP/1.1\15\nHost: localhost:4001\15\nContent-Type: application/json; charset=utf-8\15\nContent-Length: 17\15\nX-Trace-Id: a:b:c\15\n\15\n{\"text\":\"hello\"}" (("POST" "/connect" "HTTP/1.1") ("Host" "localhost:4001") ("Content-Type" "application/json; charset=utf-8") ("Content-Length" "17") ("X-Trace-Id" "a:b:c") ("Content" "{\"text\":\"hello\"}"))) ("OPTIONS /socket?x=1 HTTP/1.0\nOrigin: https://example.test\nConnection: keep-alive\n\nbody" (("OPTIONS" "/socket?x=1" "HTTP/1.0") ("Origin" "https://example.test") ("Connection" "keep-alive") ("Content" nil))) ("POST /unicode HTTP/1.1\15\nContent-Length: 8\15\n\15\nλ😀" (("POST" "/unicode" "HTTP/1.1") ("Content-Length" "8") ("Content" "λ😀"))))"#
        ]],
    )
}

fn atomic_chrome_httpd_parse_string_preserves_duplicate_headers_and_exact_split_quirks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_parse_string_preserves_duplicate_headers_and_exact_split_quirks",
        r##"(mapcar
          (lambda (request)
            (atomic-chrome-test-error-data
             (lambda ()
               (atomic-chrome-httpd-parse-string
                request))))
          '("POST /dup HTTP/1.1\r\nX-Test: first\r\nX-Test: second\r\n\r\npayload"
            "GET   /spaces   HTTP/1.1\r\nHeader: value with: colons\r\n\r\n"
            "GET /lf HTTP/1.1\nHeader: value\n\nbody\nwith\nlines"
            ""
            "\r\n\r\n"
            "BROKEN"
            "GET / HTTP/1.1\r\nHeaderWithoutColon\r\n\r\n"))"##,
        expect![[
            r#"OK ((:ok (("POST" "/dup" "HTTP/1.1") ("X-Test" "first") ("X-Test" "second") ("Content" "payload"))) (:ok (("GET" "/spaces" "HTTP/1.1") ("Header" "value with: colons") ("Content" ""))) (:ok (("GET" "/lf" "HTTP/1.1") ("Header" "value") ("Body" "") ("With" "") ("Content" nil))) (:ok (nil ("Content" nil))) (:ok (nil ("Content" ""))) (:ok (("BROKEN") ("Content" nil))) (:ok (("GET" "/" "HTTP/1.1") ("Headerwithoutcolon" "") ("Content" ""))))"#
        ]],
    )
}

fn atomic_chrome_httpd_process_filter_accumulates_incomplete_body_then_responds_once_complete()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_process_filter_accumulates_incomplete_body_then_responds_once_complete",
        r##"(let (previous
               events)
          (cl-letf
              (((symbol-function 'process-get)
                (lambda (process property)
                  (push
                   (list
                    'get
                    process
                    property
                    previous)
                   events)
                  previous))
               ((symbol-function 'process-put)
                (lambda (process property value)
                  (setq previous value)
                  (push
                   (list
                    'put
                    process
                    property
                    value)
                   events)
                  value))
               ((symbol-function
                 'atomic-chrome-httpd-send-response)
                (lambda (process)
                  (push
                   (list
                    'respond
                    process
                    previous)
                   events)
                  :responded)))
            (list
             (atomic-chrome-httpd-process-filter
              :client
              "POST / HTTP/1.1\r\nContent-Length: 11\r\n\r\nhello")
             previous
             (atomic-chrome-httpd-process-filter
              :client
             " world")
             previous
             (nreverse events))))"##,
        expect![[
            r#"OK ("POST / HTTP/1.1\15\nContent-Length: 11\15\n\15\nhello" "POST / HTTP/1.1\15\nContent-Length: 11\15\n\15\nhello" :responded "POST / HTTP/1.1\15\nContent-Length: 11\15\n\15\nhello" ((get :client :previous-string nil) (put :client :previous-string "POST / HTTP/1.1\15\nContent-Length: 11\15\n\15\nhello") (get :client :previous-string "POST / HTTP/1.1\15\nContent-Length: 11\15\n\15\nhello") (respond :client "POST / HTTP/1.1\15\nContent-Length: 11\15\n\15\nhello")))"#
        ]],
    )
}

fn atomic_chrome_httpd_process_filter_counts_encoded_bytes_for_multibyte_body() -> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_process_filter_counts_encoded_bytes_for_multibyte_body",
        r##"(let (previous
               events)
          (cl-letf
              (((symbol-function 'process-get)
                (lambda (_process _property)
                  previous))
               ((symbol-function 'process-put)
                (lambda (_process _property value)
                  (setq previous value)
                  (push
                   (list
                    'stored
                    (string-bytes value))
                   events)
                  value))
               ((symbol-function
                 'atomic-chrome-httpd-send-response)
                (lambda (_process)
                  (push
                   'responded
                   events)
                  :responded)))
            (let ((header
                   "POST /unicode HTTP/1.1\r\nContent-Length: 6\r\n\r\n"))
              (list
               (atomic-chrome-httpd-process-filter
                :client
                (concat header "λ"))
               (atomic-chrome-httpd-process-filter
                :client
                "😀")
               (nreverse events)
               (and previous
                    (string-bytes previous))))))"##,
        expect![[
            r#"OK ("POST /unicode HTTP/1.1\15\nContent-Length: 6\15\n\15\nλ" :responded ((stored 47) responded) 47)"#
        ]],
    )
}

fn atomic_chrome_httpd_process_filter_missing_length_responds_immediately_and_missing_body_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_process_filter_missing_length_responds_immediately_and_missing_body_errors",
        r##"(let (events
               previous)
          (cl-letf
              (((symbol-function 'process-get)
                (lambda (_process _property)
                  previous))
               ((symbol-function 'process-put)
                (lambda (_process _property value)
                  (setq previous value)
                  (push
                   (list 'put value)
                   events)
                  value))
               ((symbol-function
                 'atomic-chrome-httpd-send-response)
                (lambda (process)
                  (push
                   (list 'respond process)
                   events)
                  :responded)))
            (list
             (atomic-chrome-httpd-process-filter
              :no-length
              "GET / HTTP/1.1\r\nHost: local")
             (atomic-chrome-test-error-data
              (lambda ()
                (atomic-chrome-httpd-process-filter
                 :no-body
                 "POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\n")))
             previous
             (nreverse events))))"##,
        expect![[
            r#"OK (:responded (:ok "POST / HTTP/1.1\15\nContent-Length: 3\15\n\15\n") "POST / HTTP/1.1\15\nContent-Length: 3\15\n\15\n" ((respond :no-length) (put "POST / HTTP/1.1\15\nContent-Length: 3\15\n\15\n")))"#
        ]],
    )
}

fn atomic_chrome_httpd_send_response_starts_ghost_socket_once_and_writes_exact_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_send_response_starts_ghost_socket_once_and_writes_exact_protocol",
        r##"(let ((atomic-chrome-server-ghost-text
                nil)
               events)
          (cl-letf
              (((symbol-function 'processp)
                (lambda (process)
                  (memq process
                        '(:first :second))))
               ((symbol-function
                 'atomic-chrome-start-websocket-server)
                (lambda (port)
                  (push
                   (list 'start port)
                   events)
                  :ghost-websocket))
               ((symbol-function 'process-send-string)
                (lambda (process string)
                  (push
                   (list
                    'send
                    process
                    string)
                   events)
                  :sent))
               ((symbol-function 'process-send-eof)
                (lambda (process)
                  (push
                   (list 'eof process)
                   events)
                  :eof)))
            (list
             (atomic-chrome-httpd-send-response
              :not-process)
             (atomic-chrome-httpd-send-response
              :first)
             atomic-chrome-server-ghost-text
             (atomic-chrome-httpd-send-response
              :second)
             atomic-chrome-server-ghost-text
             (nreverse events))))"##,
        expect![[
            r#"OK (nil :eof :ghost-websocket :eof :ghost-websocket ((start 64293) (send :first "HTTP/1.0 200 OK\nContent-Type: application/json\n\n{\"ProtocolVersion\":1,\"WebSocketPort\":64293}") (eof :first) (send :second "HTTP/1.0 200 OK\nContent-Type: application/json\n\n{\"ProtocolVersion\":1,\"WebSocketPort\":64293}") (eof :second)))"#
        ]],
    )
}

fn atomic_chrome_httpd_send_response_propagates_start_and_write_failures_at_exact_state_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_httpd_send_response_propagates_start_and_write_failures_at_exact_state_boundaries",
        r##"(let (scenarios)
          (dolist
              (failure
               '(:start :send :eof))
            (let ((atomic-chrome-server-ghost-text
                   nil)
                  events)
              (cl-letf
                  (((symbol-function 'processp)
                    (lambda (_process)
                      t))
                   ((symbol-function
                     'atomic-chrome-start-websocket-server)
                    (lambda (port)
                      (push
                       (list 'start port)
                       events)
                      (if
                          (eq failure :start)
                          (error "start failure")
                        :ghost-server)))
                   ((symbol-function
                     'process-send-string)
                    (lambda (process string)
                      (push
                       (list
                        'send
                        process
                        string)
                       events)
                      (if
                          (eq failure :send)
                          (error "send failure")
                        :sent)))
                   ((symbol-function 'process-send-eof)
                    (lambda (process)
                      (push
                       (list 'eof process)
                       events)
                      (if
                          (eq failure :eof)
                          (error "eof failure")
                        :eof))))
                (push
                 (list
                  failure
                  (atomic-chrome-test-error-data
                   (lambda ()
                     (atomic-chrome-httpd-send-response
                      :process)))
                  atomic-chrome-server-ghost-text
                  (nreverse events))
                 scenarios))))
          (nreverse scenarios))"##,
        expect![[
            r#"OK ((:start (:error error ("start failure")) nil ((start 64293))) (:send (:error error ("send failure")) :ghost-server ((start 64293) (send :process "HTTP/1.0 200 OK\nContent-Type: application/json\n\n{\"ProtocolVersion\":1,\"WebSocketPort\":64293}"))) (:eof (:error error ("eof failure")) :ghost-server ((start 64293) (send :process "HTTP/1.0 200 OK\nContent-Type: application/json\n\n{\"ProtocolVersion\":1,\"WebSocketPort\":64293}") (eof :process))))"#
        ]],
    )
}

pub(super) fn httpd_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_normalize_header_capitalizes_hyphenated_components_and_edge_inputs_exactly(),
        atomic_chrome_httpd_parse_string_decodes_practical_request_line_headers_and_body(),
        atomic_chrome_httpd_parse_string_preserves_duplicate_headers_and_exact_split_quirks(),
        atomic_chrome_httpd_process_filter_accumulates_incomplete_body_then_responds_once_complete(),
        atomic_chrome_httpd_process_filter_counts_encoded_bytes_for_multibyte_body(),
        atomic_chrome_httpd_process_filter_missing_length_responds_immediately_and_missing_body_errors(),
        atomic_chrome_httpd_send_response_starts_ghost_socket_once_and_writes_exact_protocol(),
        atomic_chrome_httpd_send_response_propagates_start_and_write_failures_at_exact_state_boundaries(),
    ]
}
