use expect_test::expect;

use super::ParityBatchCase;

fn affe_backend_send_log_flush_status_and_refresh_emit_exact_protocol_records() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_send_log_flush_status_and_refresh_emit_exact_protocol_records",
        r##"(let* ((affe-backend--client 'client)
                     (affe-backend--search-found 0)
                     (affe-backend--search-limit 0)
                     (affe-backend--search-head
                      (list nil))
                     (affe-backend--search-tail
                      affe-backend--search-head)
                     (affe-backend--producer-head
                      (list nil))
                     (affe-backend--producer-tail
                      affe-backend--producer-head)
                     (affe-backend--producer-total 42)
                     (affe-backend--producer-done nil)
                     writes)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push
                        (list process string)
                        writes))))
                 (affe-backend--send
                  '(direct "line\nbreak"))
                 (affe-backend--log
                  "value=%s/%d\n" "α" 3)
                 (affe-backend--flush)
                 (setq
                  affe-backend--search-found
                  1)
                 (affe-backend--flush)
                 (affe-backend--producer-refresh)
                 (setq
                  affe-backend--producer-done t)
                 (affe-backend--producer-refresh)
                 (affe-backend--search-status)
                 (affe-backend--search-refresh)
                 (setq
                  affe-backend--search-limit
                  2
                  affe-backend--search-found
                  0
                  affe-backend--producer-done
                  nil)
                 (affe-backend--search-status)
                 (affe-backend--search-refresh)
                 (list
                  affe-backend--search-limit
                  (nreverse writes))))"##,
        expect![[
            r#"OK (2 ((client "(direct \"line\\nbreak\")\n") (client "(log \"value=α/3\\n\")\n") (client "flush\n") (client "(producer 42 nil)\n") (client "(producer 42 t)\n") (client "(search nil)\n") (client "(search t)\n") (client "(search t)\n") (client "(search t)\n")))"#
        ]],
    )
}

fn affe_backend_server_filter_buffers_search_request_then_rotates_queue_and_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_server_filter_buffers_search_request_then_rotates_queue_and_matches",
        r##"(let* ((affe-backend--client-rest "")
                     (affe-backend--client 'old-client)
                     (affe-backend--search-head
                      (list nil))
                     (affe-backend--search-tail
                      affe-backend--search-head)
                     (affe-backend--search-found 0)
                     (affe-backend--search-limit 0)
                     (affe-backend--search-regexps
                      nil)
                     (affe-backend--producer-head
                      (list nil
                            "Alpha"
                            "Beta"
                            "alphabet"))
                     (affe-backend--producer-tail
                      (last
                       affe-backend--producer-head))
                     (affe-backend--producer-done t)
                     timers writes)
               (cl-letf
                   (((symbol-function 'run-at-time)
                     (lambda (&rest arguments)
                       (push arguments timers)
                       'timer))
                    ((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push
                        (list process string)
                        writes))))
                 (affe-backend--server-filter
                  'new-client
                  "(search 2 \"alpha\")")
                 (let ((fragment
                        affe-backend--client-rest))
                   (affe-backend--server-filter
                    'new-client "\n")
                   (list
                    fragment
                    affe-backend--client-rest
                    affe-backend--search-limit
                    affe-backend--search-found
                    affe-backend--search-regexps
                    (cdr
                     affe-backend--search-head)
                    (cdr
                     affe-backend--producer-head)
                    (nreverse timers)
                    (nreverse writes)))))"##,
        expect![[
            r#"OK ("(search 2 \"alpha\")" "" 0 2 ("alpha") ("Alpha" "Beta" "alphabet") nil ((0.5 nil affe-backend--flush)) ((old-client "(search t)\n") (old-client "(search t)\n") (old-client "flush\n") (old-client "(match nil \"Alpha\" nil)\n") (old-client "(search t)\n") (old-client "(match nil \"alphabet\" nil)\n") (old-client "(search nil)\n")))"#
        ]],
    )
}

fn affe_backend_server_filter_start_sets_client_timers_restriction_and_producer_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_server_filter_start_sets_client_timers_restriction_and_producer_process",
        r##"(let* ((affe-backend--client-rest "")
                     (affe-backend--client nil)
                     (affe-backend--search-limit 0)
                     process-arguments timers writes)
               (cl-letf
                   (((symbol-function 'run-at-time)
                     (lambda (&rest arguments)
                       (push arguments timers)
                       'timer))
                    ((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (setq process-arguments
                             arguments)
                       'producer))
                    ((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push
                        (list process string)
                        writes))))
                 (affe-backend--server-filter
                  'new-client
                  "(start \"capture\" \"rg\" \"--files\")\n")
                 (list
                  affe-backend--client
                  affe-backend--restrict-regexp
                  (nreverse timers)
                  (plist-get process-arguments
                             :name)
                  (plist-get process-arguments
                             :command)
                  (plist-get process-arguments
                             :connection-type)
                  (plist-get process-arguments
                             :filter)
                  (plist-get process-arguments
                             :sentinel)
                  (nreverse writes))))"##,
        expect![[
            r#"OK (new-client "capture" ((0.5 0.5 affe-backend--producer-refresh) (0.1 0.1 affe-backend--search-refresh)) "rg" ("rg" "--files") pipe affe-backend--producer-filter affe-backend--producer-sentinel ((new-client "(log \"Starting (\\\"rg\\\" \\\"--files\\\")\\n\")\n")))"#
        ]],
    )
}

fn affe_backend_server_filter_exit_kills_backend_and_continues_complete_records_only()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_server_filter_exit_kills_backend_and_continues_complete_records_only",
        r##"(let ((affe-backend--client-rest "")
                    (affe-backend--search-limit 0)
                    exits)
               (cl-letf
                   (((symbol-function 'kill-emacs)
                     (lambda (&optional status)
                       (push status exits)
                       'killed)))
                 (affe-backend--server-filter
                  'client "ex")
                 (let ((fragment
                        affe-backend--client-rest))
                   (affe-backend--server-filter
                    'client "it\n")
                   (list
                    fragment
                    affe-backend--client-rest
                    (nreverse exits)))))"##,
        expect![[r#"OK ("ex" "" (nil))"#]],
    )
}

fn affe_backend_setup_assigns_utf8_coding_and_server_filter() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_setup_assigns_utf8_coding_and_server_filter",
        r##"(let ((server-process 'server)
                    calls)
               (cl-letf
                   (((symbol-function
                      'set-process-coding-system)
                     (lambda (&rest arguments)
                       (push
                        (cons 'coding arguments)
                        calls)))
                    ((symbol-function
                      'set-process-filter)
                     (lambda (&rest arguments)
                       (push
                        (cons 'filter arguments)
                        calls))))
                 (list
                  (affe-backend--setup)
                  (nreverse calls))))"##,
        expect![
            "OK (#1=((filter server affe-backend--server-filter)) ((coding server utf-8 utf-8) . #1#))"
        ],
    )
}

pub(super) fn backend_protocol_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_backend_send_log_flush_status_and_refresh_emit_exact_protocol_records(),
        affe_backend_server_filter_buffers_search_request_then_rotates_queue_and_matches(),
        affe_backend_server_filter_start_sets_client_timers_restriction_and_producer_process(),
        affe_backend_server_filter_exit_kills_backend_and_continues_complete_records_only(),
        affe_backend_setup_assigns_utf8_coding_and_server_filter(),
    ]
}
