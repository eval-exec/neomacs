use expect_test::expect;

use super::ParityBatchCase;

fn affe_async_setup_launches_daemon_connects_and_sends_start_then_initial_search() -> ParityBatchCase
{
    ParityBatchCase::value(
        "affe_async_setup_launches_daemon_connects_and_sends_start_then_initial_search",
        r##"(let (launches connection-name
                    connection-callback writes
                    overlay-ranges downstream)
               (cl-letf
                   (((symbol-function 'make-temp-name)
                     (lambda (_) "affe-fixture"))
                    ((symbol-function
                      'minibuffer-prompt-end)
                     (lambda () 9))
                    ((symbol-function 'make-overlay)
                     (lambda (beg end)
                       (push (list beg end)
                             overlay-ranges)
                       'indicator))
                    ((symbol-function 'call-process)
                     (lambda (program infile
                              destination display
                              &rest arguments)
                       (push
                        (list
                         (stringp program)
                         infile destination display
                         (nth 0 arguments)
                         (nth 1 arguments)
                         (string-prefix-p
                          "--chdir="
                          (nth 2 arguments))
                         (nth 3 arguments)
                         (file-name-nondirectory
                          (nth 4 arguments)))
                        launches)
                       0))
                    ((symbol-function 'affe--connect)
                     (lambda (name callback)
                       (setq connection-name name
                             connection-callback
                             callback)
                       'backend-client))
                    ((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push (list process string)
                             writes))))
                 (let* ((factory
                         (affe--async
                          '("rg" "--files")
                          "restricted"))
                        (runner
                         (funcall
                          factory
                          (lambda (action)
                            (push action downstream)
                            (list 'handled
                                  action)))))
                   (list
                    (funcall runner 'setup)
                    (nreverse downstream)
                    (nreverse launches)
                    connection-name
                    (functionp
                     connection-callback)
                    (nreverse overlay-ranges)
                    (nreverse writes)))))"##,
        expect![[
            r#"OK ((handled setup) (setup) ((t nil nil nil "-Q" "--daemon=affe-fixture" t "-l" "affe-backend.el")) "affe-fixture" t ((7 8)) ((backend-client "(start \"restricted\" \"rg\" \"--files\")\n") (backend-client "(search 20)\n")))"#
        ]],
    )
}

fn affe_async_string_actions_compile_filter_deduplicate_and_send_only_changed_regexps()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_async_string_actions_compile_filter_deduplicate_and_send_only_changed_regexps",
        r##"(let ((affe-count 7)
                    compiler-calls
                    downstream
                    writes)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push (list process string)
                             writes))))
                 (let* ((affe-regexp-compiler
                         (lambda (input type case)
                           (push (list input type case)
                                 compiler-calls)
                           (pcase input
                             ("alpha"
                              (cons '("a" "[")
                                    #'identity))
                             ("same"
                              (cons '("a")
                                    #'ignore))
                             ("invalid"
                              (cons '("[")
                                    #'ignore))
                             (_
                              (cons nil
                                    #'ignore)))))
                        (runner
                         (funcall
                          (affe--async '("producer"))
                          (lambda (action)
                            (push action downstream)
                            (list 'downstream
                                  action)))))
                   (list
                    (funcall runner "alpha")
                    (funcall runner "same")
                    (funcall runner "invalid")
                    (funcall runner "empty")
                    (funcall runner 'refresh)
                    (nreverse compiler-calls)
                    (nreverse downstream)
                    (nreverse writes)))))"##,
        expect![[
            r#"OK ((downstream "alpha") (downstream "same") (downstream "invalid") (downstream "empty") (downstream refresh) (("alpha" emacs ignore-case) ("same" emacs ignore-case) ("invalid" emacs ignore-case) ("empty" emacs ignore-case)) ("alpha" "same" "invalid" "empty" refresh) ((nil "(search 7 \"a\")\n")))"#
        ]],
    )
}

fn affe_async_callback_routes_protocol_messages_highlights_matches_and_formats_indicators()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_async_callback_routes_protocol_messages_highlights_matches_and_formats_indicators",
        r##"(let (callback displays events writes)
               (cl-letf
                   (((symbol-function 'make-temp-name)
                     (lambda (_) "affe-callback"))
                    ((symbol-function
                      'minibuffer-prompt-end)
                     (lambda () 5))
                    ((symbol-function 'make-overlay)
                     (lambda (&rest _) 'indicator))
                    ((symbol-function 'overlay-put)
                     (lambda (_ property value)
                       (push
                        (list property
                              (substring-no-properties
                               value))
                        displays)))
                    ((symbol-function 'call-process)
                     (lambda (&rest _) 0))
                    ((symbol-function 'affe--connect)
                     (lambda (_name function)
                       (setq callback function)
                       'backend-client))
                    ((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes))))
                 (let* ((affe-regexp-compiler
                         (lambda (&rest _)
                           (cons
                            '("body")
                            (lambda (match)
                              (push
                               (list 'highlight match)
                               events)))))
                        (runner
                         (funcall
                          (affe--async '("producer"))
                          (lambda (action)
                            (push
                             (list 'sink action)
                             events)
                            action))))
                   (funcall runner 'setup)
                   (funcall callback
                            '("(match \"\" \"plain\" \"\")"))
                   (funcall runner "body")
                   (funcall callback
                            '("(producer 999 nil)"))
                   (funcall callback
                            '("(search t)"))
                   (funcall callback
                            '("(producer 1200 t)"))
                   (funcall callback
                            '("(producer 1200000 nil)"))
                   (funcall callback
                            '("(match \"pre\" \"body\" \"suffix\")"
                              "flush"
                              "(log \"backend-log\\n\")"))
                   (list
                    (nreverse displays)
                    (nreverse events)
                    (with-current-buffer
                        (get-buffer " *affe*")
                      (buffer-string))
                    (nreverse writes)))))"##,
        expect![[
            r#"OK (((display " (total=0+):") (display " (total=999+):") (display " (total=999+):") (display " (total=1.2K):") (display " (total=1.2M+):") (display " (total=1.2M+):")) ((sink setup) (sink ("plain")) (sink "body") (highlight "body") (sink ("prebodysuffix")) (sink flush)) "backend-log\n" ("(start nil \"producer\")\n" "(search 20)\n" "(search 20 \"body\")\n"))"#
        ]],
    )
}

fn affe_async_destroy_sends_exit_deletes_indicator_and_preserves_sink_return() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_async_destroy_sends_exit_deletes_indicator_and_preserves_sink_return",
        r##"(let (callback deleted writes actions)
               (cl-letf
                   (((symbol-function 'make-temp-name)
                     (lambda (_) "affe-destroy"))
                    ((symbol-function
                      'minibuffer-prompt-end)
                     (lambda () 4))
                    ((symbol-function 'make-overlay)
                     (lambda (&rest _) 'indicator))
                    ((symbol-function 'delete-overlay)
                     (lambda (overlay)
                       (push overlay deleted)))
                    ((symbol-function 'call-process)
                     (lambda (&rest _) 0))
                    ((symbol-function 'affe--connect)
                     (lambda (_name function)
                       (setq callback function)
                       'backend-client))
                    ((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push (list process string)
                             writes))))
                 (let ((runner
                        (funcall
                         (affe--async '("producer"))
                         (lambda (action)
                           (push action actions)
                           (list 'sink-result
                                 action)))))
                   (funcall runner 'setup)
                   (list
                    (funcall runner 'destroy)
                    (functionp callback)
                    (nreverse actions)
                    (nreverse writes)
                    (nreverse deleted)))))"##,
        expect![[
            r#"OK ((sink-result destroy) t (setup destroy) ((backend-client "(start nil \"producer\")\n") (backend-client "(search 20)\n") (backend-client "exit\n")) (indicator))"#
        ]],
    )
}

fn affe_async_reports_missing_backend_before_constructing_action_runner() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_async_reports_missing_backend_before_constructing_action_runner",
        r##"(cl-letf
               (((symbol-function 'locate-library)
                 (lambda (&rest _) nil)))
               (condition-case error-data
                   (funcall
                    (affe--async '("producer"))
                    #'ignore)
                 (error
                  (list 'signal
                        (car error-data)
                        (cdr error-data)))))"##,
        expect![[r#"OK (signal error ("Could not locate the library ‘affe-backend.el’"))"#]],
    )
}

pub(super) fn async_frontend_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_async_setup_launches_daemon_connects_and_sends_start_then_initial_search(),
        affe_async_string_actions_compile_filter_deduplicate_and_send_only_changed_regexps(),
        affe_async_callback_routes_protocol_messages_highlights_matches_and_formats_indicators(),
        affe_async_destroy_sends_exit_deletes_indicator_and_preserves_sink_return(),
        affe_async_reports_missing_backend_before_constructing_action_runner(),
    ]
}
