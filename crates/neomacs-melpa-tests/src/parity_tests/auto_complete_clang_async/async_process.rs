use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_candidate_state_machine_sends_once_waits_acknowledges_and_preempts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_candidate_state_machine_sends_once_waits_acknowledges_and_preempts",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-candidate"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair)))
                           (unwind-protect
                               (with-temp-buffer
                                 (insert
                                  "object.member")
                                 (setq
                                  ac-clang-completion-process
                                  process)
                                 (let ((ac-prefix
                                        "mem")
                                       sends)
                                   (cl-letf
                                       (((symbol-function
                                          'ac-clang-send-completion-request)
                                         (lambda (candidate-process)
                                           (push
                                            (list
                                             candidate-process
                                             ac-clang-saved-prefix
                                             (buffer-string))
                                            sends)
                                           :sent)))
                                     (mapcar
                                      (lambda (fixture)
                                        (setq
                                         ac-clang-status
                                         (car fixture)
                                         ac-clang-current-candidate
                                         (cdr fixture))
                                        (list
                                         (car fixture)
                                         (ac-clang-candidate)
                                         ac-clang-status
                                         ac-clang-current-candidate
                                         ac-clang-saved-prefix
                                         (nreverse
                                          (prog1 sends
                                            (setq sends nil)))))
                                      '((idle . ("stale"))
                                        (wait . ("waiting"))
                                        (acknowledged . ("ready"))
                                        (preempted . ("ignored")))))))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[
            r#"OK ((idle nil wait nil "mem" (((:process "acclang-candidate" signal) "mem" "object.member"))) (wait #1=("waiting") wait #1# "mem" nil) (acknowledged #2=("ready") idle #2# "mem" nil) (preempted nil preempted ("ignored") "mem" nil))"#
        ]],
    )
}

fn auto_complete_clang_async_filter_completion_response_parses_candidates_and_runs_real_state_transition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_filter_completion_response_parses_candidates_and_runs_real_state_transition",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-filter"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair))
                                (ac-clang-status
                                 'wait)
                                (ac-clang-saved-prefix
                                 "fo")
                                calls)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'ac-start)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons
                                         :start
                                         arguments)
                                        calls)
                                       :started))
                                    ((symbol-function
                                      'ac-update)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons
                                         :update
                                         arguments)
                                        calls)
                                       :updated)))
                                 (ac-clang-filter-output
                                  process
                                  (concat
                                   "COMPLETION: format : [#int#]format(<#const char *fmt#>)\n"
                                   "COMPLETION: fork : [#void#]fork()\n"
                                   "$"))
                                 (list
                                  ac-clang-status
                                  (mapcar
                                   #'acclang-test-candidate-summary
                                   ac-clang-current-candidate)
                                  (nreverse calls)
                                  (acclang-test-process-buffer-string
                                   process)
                                  (marker-position
                                   (process-mark process))))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[
            r#"OK (idle (("fork" "[#void#]fork()" nil) ("format" "[#int#]format(<#const char *fmt#>)" nil)) ((:start :force-init t) (:update)) "COMPLETION: format : [#int#]format(<#const char *fmt#>)\nCOMPLETION: fork : [#void#]fork()\n$" 92)"#
        ]],
    )
}

fn auto_complete_clang_async_filter_preempted_response_restarts_completion_without_parsing_stale_data()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_filter_preempted_response_restarts_completion_without_parsing_stale_data",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-preempted"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair))
                                (ac-clang-status
                                 'preempted)
                                (ac-clang-current-candidate
                                 '("preserved"))
                                calls)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'ac-start)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons
                                         :start
                                         arguments)
                                        calls)
                                       :started))
                                    ((symbol-function
                                      'ac-update)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons
                                         :update
                                         arguments)
                                        calls)
                                       :updated)))
                                 (ac-clang-filter-output
                                  process
                                  "COMPLETION: stale : old$")
                                 (list
                                  ac-clang-status
                                  ac-clang-current-candidate
                                  (nreverse calls)
                                  (acclang-test-process-buffer-string
                                   process)))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[r#"OK (idle ("preserved") ((:start) (:update)) "COMPLETION: stale : old$")"#]],
    )
}

fn auto_complete_clang_async_filter_accumulates_partial_chunks_until_dollar_terminated_chunk()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_filter_accumulates_partial_chunks_until_dollar_terminated_chunk",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-chunks"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair))
                                (ac-clang-status
                                 'wait)
                                (ac-clang-saved-prefix
                                 "al")
                                calls)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'ac-start)
                                     (lambda (&rest _arguments)
                                       (push :start calls)))
                                    ((symbol-function
                                      'ac-update)
                                     (lambda (&rest _arguments)
                                       (push :update calls))))
                                 (ac-clang-filter-output
                                  process
                                  "COMPLETION: alpha : [#int#]alpha\nCOMPLE")
                                 (let ((partial
                                        (list
                                         ac-clang-status
                                         ac-clang-current-candidate
                                         calls)))
                                   (ac-clang-filter-output
                                    process
                                    "TION: alpine : [#int#]alpine$")
                                   (list
                                    partial
                                    ac-clang-status
                                    (mapcar
                                     #'acclang-test-candidate-summary
                                     ac-clang-current-candidate)
                                    (nreverse calls)
                                    (acclang-test-process-buffer-string
                                     process))))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[
            r#"OK ((wait nil nil) idle (("alpine" "[#int#]alpine$" nil) ("alpha" "[#int#]alpha" nil)) (:start :update) "COMPLETION: alpha : [#int#]alpha\nCOMPLETION: alpine : [#int#]alpine$")"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_clang_async_syntax_check_switches_filter_and_sends_only_while_idle()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_syntax_check_switches_filter_and_sends_only_while_idle",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-syntax"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair)))
                           (unwind-protect
                               (with-temp-buffer
                                 (insert
                                  "int broken = ;")
                                 (setq
                                  ac-clang-completion-process
                                  process)
                                 (mapcar
                                  (lambda (status)
                                    (setq
                                     ac-clang-status
                                     status)
                                    (set-process-filter
                                     process
                                     #'ac-clang-filter-output)
                                    (with-current-buffer buffer
                                      (erase-buffer))
                                    (let (sends)
                                      (cl-letf
                                          (((symbol-function
                                             'ac-clang-send-syntaxcheck-request)
                                            (lambda (candidate-process)
                                              (push
                                               candidate-process
                                               sends)
                                              :sent)))
                                        (list
                                         status
                                         (ac-clang-syntax-check)
                                         ac-clang-status
                                         (process-filter process)
                                         sends))))
                                  '(idle
                                    wait
                                    acknowledged)))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[
            r#"OK ((idle :sent wait ac-clang-flymake-process-filter ((:process "acclang-syntax" signal))) (wait nil wait ac-clang-filter-output nil) (acknowledged nil acknowledged ac-clang-filter-output nil))"#
        ]],
    )
}

fn auto_complete_clang_async_flymake_filter_parses_chunks_finalizes_and_restores_completion_filter()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_flymake_filter_parses_chunks_finalizes_and_restores_completion_filter",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-flymake"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair))
                                (ac-clang-completion-process
                                 process)
                                (ac-clang-status
                                 'wait)
                                calls)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'flymake-log)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons :log arguments)
                                        calls)))
                                    ((symbol-function
                                      'flymake-parse-output-and-residual)
                                     (lambda (output)
                                       (push
                                        (list :parse output)
                                        calls)))
                                    ((symbol-function
                                      'flymake-parse-residual)
                                     (lambda ()
                                       (push :residual calls)))
                                    ((symbol-function
                                      'ac-clang-flymake-process-sentinel)
                                     (lambda ()
                                       (push :sentinel calls))))
                                 (set-process-filter
                                  process
                                  #'ac-clang-flymake-process-filter)
                                 (ac-clang-flymake-process-filter
                                  process
                                  "fixture.cpp:1:4: error: broken")
                                 (let ((partial
                                        (list
                                         ac-clang-status
                                         (process-filter process)
                                         (nreverse
                                          (prog1 calls
                                            (setq calls nil))))))
                                   (ac-clang-flymake-process-filter
                                    process
                                    "\n$")
                                   (list
                                    partial
                                    ac-clang-status
                                    (process-filter process)
                                    (nreverse calls)
                                    (acclang-test-process-buffer-string
                                     process))))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[
            r#"OK ((wait ac-clang-flymake-process-filter ((:parse "fixture.cpp:1:4: error: broken"))) idle ac-clang-filter-output ((:parse "\n$") :residual :sentinel) "fixture.cpp:1:4: error: broken\n$")"#
        ]],
    )
}

fn auto_complete_clang_async_preemptive_insert_starts_idle_completion_or_marks_busy_request()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_preemptive_insert_starts_idle_completion_or_marks_busy_request",
        r##"(mapcar
                           (lambda (status)
                             (with-temp-buffer
                               (let ((ac-clang-status
                                      status)
                                     starts)
                                 (cl-letf
                                     (((symbol-function
                                        'self-insert-command)
                                       (lambda (count)
                                         (insert
                                          (make-string
                                           count
                                           ?x))
                                         :inserted))
                                      ((symbol-function
                                        'ac-start)
                                       (lambda (&rest arguments)
                                         (push arguments starts)
                                         :started)))
                                   (list
                                    status
                                    (ac-clang-async-preemptive)
                                    (buffer-string)
                                    ac-clang-status
                                    starts)))))
                           '(idle
                             wait
                             acknowledged
                             preempted))"##,
        expect![[
            r#"OK ((idle :started "x" idle (nil)) (wait preempted "x" preempted nil) (acknowledged preempted "x" preempted nil) (preempted preempted "x" preempted nil))"#
        ]],
    )
}

fn auto_complete_clang_async_autotrigger_routes_enabled_input_to_preemption_and_disabled_input_directly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_autotrigger_routes_enabled_input_to_preemption_and_disabled_input_directly",
        r##"(mapcar
                           (lambda (enabled)
                             (with-temp-buffer
                               (let ((ac-clang-async-do-autocompletion-automatically
                                      enabled)
                                     calls)
                                 (cl-letf
                                     (((symbol-function
                                        'ac-clang-async-preemptive)
                                       (lambda ()
                                         (push :preemptive calls)
                                         (insert "P")
                                         :preempted))
                                      ((symbol-function
                                        'self-insert-command)
                                       (lambda (count)
                                         (push
                                          (list :insert count)
                                          calls)
                                         (insert "I")
                                         :inserted)))
                                   (list
                                    enabled
                                    (ac-clang-async-autocomplete-autotrigger)
                                    (buffer-string)
                                    (nreverse calls))))))
                           '(t nil))"##,
        expect![[r#"OK ((t :preempted "P" (:preemptive)) (nil :inserted "I" ((:insert 1))))"#]],
    )
}

fn auto_complete_clang_async_shutdown_and_reparse_wrappers_ignore_nil_or_forward_exact_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_shutdown_and_reparse_wrappers_ignore_nil_or_forward_exact_process",
        r##"(mapcar
                           (lambda (process)
                             (let ((ac-clang-completion-process
                                    process)
                                   calls)
                               (cl-letf
                                   (((symbol-function
                                      'ac-clang-send-shutdown-command)
                                     (lambda (candidate)
                                       (push
                                        (list :shutdown candidate)
                                        calls)
                                       :shutdown))
                                    ((symbol-function
                                      'ac-clang-send-reparse-request)
                                     (lambda (candidate)
                                       (push
                                        (list :reparse candidate)
                                        calls)
                                       :reparse)))
                                 (list
                                  process
                                  (ac-clang-shutdown-process)
                                  (ac-clang-reparse-buffer)
                                  (nreverse calls)))))
                           '(nil fixture-process))"##,
        expect![
            "OK ((nil nil nil nil) (fixture-process :shutdown :reparse ((:shutdown fixture-process) (:reparse fixture-process))))"
        ],
    )
}

fn auto_complete_clang_async_real_launch_installs_filter_hooks_keys_and_sends_initial_reparse_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_real_launch_installs_filter_hooks_keys_and_sends_initial_reparse_protocol",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (c++-mode)
                             (insert
                              "struct Widget { int member; };\n")
                             (let ((ac-clang-complete-executable
                                    "/bin/sh")
                                   (fixture-file
                                    (expand-file-name
                                     "./tmp/auto-complete-clang-async/fixture.cpp"))
                                   process
                                   process-buffer)
                               (unwind-protect
                                   (cl-letf
                                       (((symbol-function
                                          'ac-clang-build-complete-args)
                                         (lambda ()
                                           '("-c"
                                             "cat"))))
                                     (ac-clang-launch-completion-process-with-file
                                      fixture-file)
                                     (setq
                                      process
                                      ac-clang-completion-process
                                      process-buffer
                                      (process-buffer process))
                                     (accept-process-output
                                      process
                                      0.2)
                                     (list
                                      (process-live-p process)
                                      (process-filter process)
                                      (process-query-on-exit-flag
                                       process)
                                      (mapcar
                                       (lambda (hook)
                                         (list
                                          hook
                                          (memq
                                           (cdr hook)
                                           (symbol-value
                                            (car hook)))))
                                       '((kill-buffer-hook
                                          . ac-clang-shutdown-process)
                                         (before-revert-hook
                                          . ac-clang-shutdown-process)
                                         (before-save-hook
                                          . ac-clang-reparse-buffer)))
                                      (mapcar
                                       (lambda (key)
                                         (list
                                          key
                                          (local-key-binding
                                           (kbd key))))
                                       '("." ":" ">"))
                                      (with-current-buffer
                                          process-buffer
                                        (buffer-string))))
                                 (remove-hook
                                  'before-save-hook
                                  #'ac-clang-reparse-buffer)
                                 (when process
                                   (acclang-test-finish-process
                                    process
                                    process-buffer))))))"##,
        expect![[
            r#"OK ((run open listen connect stop) ac-clang-filter-output nil (((kill-buffer-hook . ac-clang-shutdown-process) (ac-clang-shutdown-process t)) ((before-revert-hook . ac-clang-shutdown-process) (ac-clang-shutdown-process t)) ((before-save-hook . ac-clang-reparse-buffer) (ac-clang-reparse-buffer))) (("." ac-clang-async-autocomplete-autotrigger) (":" ac-clang-async-autocomplete-autotrigger) (">" ac-clang-async-autocomplete-autotrigger)) "SOURCEFILE\nsource_length:31\nstruct Widget { int member; };\n\n\nREPARSE\n\n")"#
        ]],
    )
}

pub(super) fn async_process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_candidate_state_machine_sends_once_waits_acknowledges_and_preempts(),
        auto_complete_clang_async_filter_completion_response_parses_candidates_and_runs_real_state_transition(),
        auto_complete_clang_async_filter_preempted_response_restarts_completion_without_parsing_stale_data(),
        auto_complete_clang_async_filter_accumulates_partial_chunks_until_dollar_terminated_chunk(),
        auto_complete_clang_async_syntax_check_switches_filter_and_sends_only_while_idle(),
        auto_complete_clang_async_flymake_filter_parses_chunks_finalizes_and_restores_completion_filter(),
        auto_complete_clang_async_preemptive_insert_starts_idle_completion_or_marks_busy_request(),
        auto_complete_clang_async_autotrigger_routes_enabled_input_to_preemption_and_disabled_input_directly(),
        auto_complete_clang_async_shutdown_and_reparse_wrappers_ignore_nil_or_forward_exact_process(),
        auto_complete_clang_async_real_launch_installs_filter_hooks_keys_and_sends_initial_reparse_protocol(),
    ]
}
