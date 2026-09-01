use expect_test::expect;

use super::ParityBatchCase;

fn asilea_one_step_run_reports_initial_candidate_finishes_and_does_not_call_accept_callback()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_one_step_run_reports_initial_candidate_finishes_and_does_not_call_accept_callback",
        r##"(let ((asilea-max-steps 1)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               callbacks)
         (asilea-test-reset
          '(("finished\n" "42.5\n")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-report-candidate-function
                      (lambda (state energy)
                        (push
                         (list :report state energy)
                         callbacks)))
                     (asilea-solution-accepted-function
                      (lambda (state energy)
                        (push
                         (list :accepted state energy)
                         callbacks)))
                     (asilea-finished-function
                      (lambda ()
                        (push
                         (list :finished)
                         callbacks))))
                 (list
                  (asilea-run
                   "measure"
                   [["-O0" "-O3"]
                    [nil "-g"]])
                  (length asilea-test-pending)
                  (asilea-test-drain)
                  (nreverse callbacks)
                  (length asilea-test-pending))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (nil 1 ((:start 1 "measure" (0 0) ("-O0") "finished\n" "42.5\n") (:sentinel 1) (:complete 1 "finished\n")) ((:report ("-O0") 42.5) (:finished)) 0)"#
        ]],
    )
}

fn asilea_multi_step_run_uses_accepted_state_for_neighbors_and_accepts_only_better_scores()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_multi_step_run_uses_accepted_state_for_neighbors_and_accepts_only_better_scores",
        r##"(let ((asilea-max-steps 4)
               (asilea-concurrent-jobs 1)
               (asilea-initial-temperature 100)
               (asilea-cooling-rate 0.5)
               (draws '(0 0 0 1 1 1 0 0))
               random-calls
               callbacks
               acceptance-calls)
         (asilea-test-reset
          '(("finished\n" "10")
            ("finished\n" "8")
            ("finished\n" "9")
            ("finished\n" "7")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-random-generator-function
                      (lambda (limit)
                        (push limit random-calls)
                        (pop draws)))
                     (asilea-acceptance-function
                      (lambda
                          (new old temperature random-function)
                        (push
                         (list
                          new old temperature
                          (eq
                           random-function
                           asilea-random-generator-function))
                         acceptance-calls)
                        (< new old)))
                     (asilea-report-candidate-function
                      (lambda (state energy)
                        (push
                         (list :report state energy)
                         callbacks)))
                     (asilea-solution-accepted-function
                      (lambda (state energy)
                        (push
                         (list :accepted state energy)
                         callbacks)))
                     (asilea-finished-function
                      (lambda ()
                        (push '(:finished) callbacks))))
                 (asilea-run
                  "measure"
                  [["-O0" "-O3"]
                   [nil "-g"]])
                 (list
                  (asilea-test-drain)
                  (nreverse callbacks)
                  (nreverse acceptance-calls)
                  (nreverse random-calls)
                  draws)))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0 0) ("-O0") "finished\n" "10") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (1 0) ("-O3") "finished\n" "8") (:sentinel 2) (:complete 2 "finished\n") (:start 3 "measure" (1 1) ("-O3" "-g") "finished\n" "9") (:sentinel 3) (:complete 3 "finished\n") (:start 4 "measure" (0 0) ("-O0") "finished\n" "7") (:sentinel 4) (:complete 4 "finished\n")) ((:report ("-O0") 10) (:report #1=("-O3") 8) (:accepted #1# 8) (:report ("-O3" "-g") 9) (:report #2=("-O0") 7) (:accepted #2# 7) (:finished)) ((8 10 50.0 t) (9 8 25.0 t) (7 8 12.5 t)) (2 2 2 2 2 2 2 2) nil)"#
        ]],
    )
}

fn asilea_nonzero_process_status_skips_parse_and_report_but_consumes_step() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_nonzero_process_status_skips_parse_and_report_but_consumes_step",
        r##"(let ((asilea-max-steps 3)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               callbacks
               parse-calls)
         (asilea-test-reset
          '(("exited abnormally with code 2\n" "999")
            ("killed\n" "888")
            ("finished\n" "7")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-parse-energy-function
                      (lambda (output)
                        (push output parse-calls)
                        (string-to-number output)))
                     (asilea-report-candidate-function
                      (lambda (state energy)
                        (push
                         (list state energy)
                         callbacks)))
                     (asilea-finished-function
                      (lambda ()
                        (push :finished callbacks))))
                 (asilea-run "measure" [["x"]])
                 (list
                  (asilea-test-drain)
                  (nreverse parse-calls)
                  (nreverse callbacks))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("x") "exited abnormally with code 2\n" "999") (:sentinel 1) (:complete 1 "exited abnormally with code 2\n") (:start 2 "measure" (0) ("x") "killed\n" "888") (:sentinel 2) (:complete 2 "killed\n") (:start 3 "measure" (0) ("x") "finished\n" "7") (:sentinel 3) (:complete 3 "finished\n")) ("7") ((("x") 7) :finished))"#
        ]],
    )
}

fn asilea_nil_and_false_energy_parses_skip_candidates_while_zero_is_valid() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_nil_and_false_energy_parses_skip_candidates_while_zero_is_valid",
        r##"(let ((asilea-max-steps 4)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               reports)
         (asilea-test-reset
          '(("finished\n" "skip")
            ("finished\n" "false")
            ("finished\n" "zero")
            ("finished\n" "five")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-parse-energy-function
                      (lambda (output)
                        (pcase output
                          ("skip" nil)
                          ("false" nil)
                          ("zero" 0)
                          (_ 5))))
                     (asilea-report-candidate-function
                      (lambda (state energy)
                        (push
                         (list state energy)
                         reports))))
                 (asilea-run "measure" [["x"]])
                 (list
                  (asilea-test-drain)
                  (nreverse reports))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("x") "finished\n" "skip") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (0) ("x") "finished\n" "false") (:sentinel 2) (:complete 2 "finished\n") (:start 3 "measure" (0) ("x") "finished\n" "zero") (:sentinel 3) (:complete 3 "finished\n") (:start 4 "measure" (0) ("x") "finished\n" "five") (:sentinel 4) (:complete 4 "finished\n")) ((("x") 0) (("x") 5)))"#
        ]],
    )
}

fn asilea_callback_errors_are_demoted_and_annealing_continues_to_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_callback_errors_are_demoted_and_annealing_continues_to_completion",
        r##"(let ((asilea-max-steps 4)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               (parse-count 0)
               (report-count 0)
               (accept-count 0)
               messages
               finished)
         (asilea-test-reset
          '(("finished\n" "parse-error")
            ("finished\n" "10")
            ("finished\n" "8")
            ("finished\n" "7")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel)
                  ((symbol-function
                    'message)
                   (lambda (format-string &rest arguments)
                     (let ((text
                            (apply
                             #'format
                             format-string
                             arguments)))
                       (push text messages)
                       text))))
               (let ((asilea-parse-energy-function
                      (lambda (output)
                        (cl-incf parse-count)
                        (if
                            (string-equal
                             output
                             "parse-error")
                            (error "bad parse")
                          (string-to-number output))))
                     (asilea-report-candidate-function
                      (lambda (_state _energy)
                        (cl-incf report-count)
                        (when (= report-count 1)
                          (error "bad report"))))
                     (asilea-acceptance-function
                      (lambda (&rest _arguments)
                        (error "bad acceptance")))
                     (asilea-solution-accepted-function
                      (lambda (_state _energy)
                        (cl-incf accept-count)
                        (error "bad accepted")))
                     (asilea-finished-function
                      (lambda ()
                        (setq finished t))))
                 (asilea-run "measure" [["x"]])
                 (list
                  (asilea-test-drain)
                  parse-count
                  report-count
                  accept-count
                  finished
                  (nreverse messages))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("x") "finished\n" "parse-error") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (0) ("x") "finished\n" "10") (:sentinel 2) (:complete 2 "finished\n") (:start 3 "measure" (0) ("x") "finished\n" "8") (:sentinel 3) (:complete 3 "finished\n") (:start 4 "measure" (0) ("x") "finished\n" "7") (:sentinel 4) (:complete 4 "finished\n")) 4 3 0 t ("Error in `asilea-parse-energy-function': (error \"bad parse\")" "Error in `asilea-report-candidate-function': (error \"bad report\")" "Error in `asilea-acceptance-function': (error \"bad acceptance\")" "Error in `asilea-acceptance-function': (error \"bad acceptance\")"))"#
        ]],
    )
}

fn asilea_accepted_solution_callback_error_is_demoted_after_state_and_energy_update()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_accepted_solution_callback_error_is_demoted_after_state_and_energy_update",
        r##"(let ((asilea-max-steps 3)
               (asilea-concurrent-jobs 1)
               (draws '(0 0 1 0 0))
               acceptance-calls
               accepted-calls
               messages)
         (asilea-test-reset
          '(("finished\n" "10")
            ("finished\n" "8")
            ("finished\n" "9")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel)
                  ((symbol-function
                    'message)
                   (lambda (format-string &rest arguments)
                     (let ((text
                            (apply
                             #'format
                             format-string
                             arguments)))
                       (push text messages)
                       text))))
               (let ((asilea-random-generator-function
                      (lambda (_limit)
                        (pop draws)))
                     (asilea-acceptance-function
                      (lambda (new old temperature _random)
                        (push
                         (list new old temperature)
                         acceptance-calls)
                        (< new old)))
                     (asilea-solution-accepted-function
                      (lambda (state energy)
                        (push
                         (list state energy)
                         accepted-calls)
                        (error
                         "accepted callback failed for %S"
                         energy))))
                 (asilea-run
                  "measure"
                  [["old" "new"]])
                 (list
                  (asilea-test-drain)
                  (nreverse acceptance-calls)
                  (nreverse accepted-calls)
                  (nreverse messages)
                  draws)))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("old") "finished\n" "10") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (1) ("new") "finished\n" "8") (:sentinel 2) (:complete 2 "finished\n") (:start 3 "measure" (0) ("old") "finished\n" "9") (:sentinel 3) (:complete 3 "finished\n")) ((8 10 1.99) (9 8 1.98005)) ((("new") 8)) ("Error in `asilea-solution-accepted-function': (error \"accepted callback failed for 8\")") nil)"#
        ]],
    )
}

fn asilea_finished_callback_error_is_demoted_without_escaping_last_sentinel() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_finished_callback_error_is_demoted_without_escaping_last_sentinel",
        r##"(let ((asilea-max-steps 1)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               messages
               finished-calls)
         (asilea-test-reset
          '(("finished\n" "1")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel)
                  ((symbol-function
                    'message)
                   (lambda (format-string &rest arguments)
                     (let ((text
                            (apply
                             #'format
                             format-string
                             arguments)))
                       (push text messages)
                       text))))
               (let ((asilea-finished-function
                      (lambda ()
                        (cl-incf finished-calls)
                        (error "finished callback failed"))))
                 (asilea-run "measure" [["x"]])
                 (list
                  (condition-case error-data
                      (list
                       :ok
                       (asilea-test-drain))
                    (error
                     (list
                      :error
                      (car error-data)
                      (cdr error-data))))
                  finished-calls
                  (nreverse messages))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK ((:ok ((:start 1 "measure" (0) ("x") "finished\n" "1") (:sentinel 1) (:complete 1 "finished\n"))) nil ("Error in `asilea-finished-function': (wrong-type-argument number-or-marker-p nil)"))"#
        ]],
    )
}

fn asilea_concurrent_jobs_have_independent_states_and_call_finished_once_after_last_job()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_concurrent_jobs_have_independent_states_and_call_finished_once_after_last_job",
        r##"(let ((asilea-max-steps 2)
               (asilea-concurrent-jobs 3)
               (draws '(0 1 2 0 1 0 0 0 2))
               starts
               reports
               finished)
         (asilea-test-reset
          '(("finished\n" "30")
            ("finished\n" "20")
            ("finished\n" "10")
            ("finished\n" "25")
            ("finished\n" "15")
            ("finished\n" "5")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-random-generator-function
                      (lambda (_limit)
                        (pop draws)))
                     (asilea-acceptance-function
                      (lambda (new old _temperature _random)
                        (< new old)))
                     (asilea-report-candidate-function
                      (lambda (state energy)
                        (push
                         (list state energy)
                         reports)))
                     (asilea-finished-function
                      (lambda ()
                        (cl-incf finished))))
                 (asilea-run
                  "measure"
                  [["a" "b" "c"]])
                 (setq starts
                       (seq-filter
                        (lambda (event)
                          (eq (car event) :start))
                        (asilea-test-drain)))
                 (list
                  starts
                  (nreverse reports)
                  finished
                  draws
                  (length asilea-test-pending))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("a") "finished\n" "30") (:start 2 "measure" (1) ("b") "finished\n" "20") (:start 3 "measure" (2) ("c") "finished\n" "10") (:start 4 "measure" (1) ("b") "finished\n" "25") (:start 5 "measure" (0) ("a") "finished\n" "15") (:start 6 "measure" (2) ("c") "finished\n" "5")) ((("a") 30) (("b") 20) (("c") 10) (("b") 25) (("a") 15) (("c") 5)) nil nil 0)"#
        ]],
    )
}

fn asilea_temperature_terminated_run_cools_until_final_temperature_inclusively() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_temperature_terminated_run_cools_until_final_temperature_inclusively",
        r##"(let ((asilea-max-steps nil)
               (asilea-concurrent-jobs 1)
               (asilea-initial-temperature 4)
               (asilea-final-temperature 1)
               (asilea-cooling-rate 0.5)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               acceptance-temperatures
               reports
               finished)
         (asilea-test-reset
          '(("finished\n" "10")
            ("finished\n" "9")
            ("finished\n" "8")
            ("finished\n" "unexpected")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-acceptance-function
                      (lambda (new old temperature _random)
                        (push
                         (list new old temperature)
                         acceptance-temperatures)
                        t))
                     (asilea-report-candidate-function
                      (lambda (_state energy)
                        (push energy reports)))
                     (asilea-finished-function
                      (lambda ()
                        (cl-incf finished))))
                 (asilea-run "measure" [["x"]])
                 (list
                  (asilea-test-drain)
                  (nreverse reports)
                  (nreverse acceptance-temperatures)
                  finished
                  asilea-test-process-specs)))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("x") "finished\n" "10") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (0) ("x") "finished\n" "9") (:sentinel 2) (:complete 2 "finished\n") (:start 3 "measure" (0) ("x") "finished\n" "8") (:sentinel 3) (:complete 3 "finished\n")) (10 9 8) ((9 10 2.0) (8 9 1.0)) nil (("finished\n" "unexpected")))"#
        ]],
    )
}

fn asilea_run_captures_configuration_callbacks_randomness_and_starting_directory() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_run_captures_configuration_callbacks_randomness_and_starting_directory",
        r##"(let ((asilea-max-steps 2)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               (asilea-parse-energy-function
                (lambda (output)
                  (list :old-parse output)))
               (asilea-report-candidate-function
                (lambda (state energy)
                  (push
                   (list :old-report state energy)
                   captured)))
               (asilea-finished-function
                (lambda ()
                  (push :old-finished captured)))
               (default-directory
                (file-name-as-directory
                 (getenv
                  "NEOMACS_TEST_SANDBOX_ROOT")))
               captured
               start-directories)
         (asilea-test-reset
          '(("finished\n" "one")
            ("finished\n" "two")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   (lambda (program state options)
                     (push
                      default-directory
                      start-directories)
                     (asilea-test-start-process
                      program state options)))
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (asilea-run "measure" [["x"]])
               (setq asilea-max-steps 99
                     asilea-random-generator-function
                     (lambda (_limit) 99)
                     asilea-parse-energy-function
                     (lambda (_output) :new-parse)
                     asilea-report-candidate-function
                     (lambda (&rest _arguments)
                       (push :new-report captured))
                     asilea-finished-function
                     (lambda ()
                       (push :new-finished captured))
                     default-directory "/")
               (list
                (asilea-test-drain)
                (nreverse captured)
                (nreverse start-directories)))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (((:start 1 "measure" (0) ("x") "finished\n" "one") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (0) ("x") "finished\n" "two") (:sentinel 2) (:complete 2 "finished\n")) nil ("[ORACLE-SANDBOX]/" "[ORACLE-SANDBOX]/"))"#
        ]],
    )
}

fn asilea_synchronous_run_drives_pending_processes_until_wrapped_finished_callback()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_synchronous_run_drives_pending_processes_until_wrapped_finished_callback",
        r##"(let ((asilea-max-steps 3)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               accept-calls
               finished)
         (asilea-test-reset
          '(("finished\n" "3")
            ("finished\n" "2")
            ("finished\n" "1")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   #'asilea-test-process-buffer)
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel)
                  ((symbol-function
                    'accept-process-output)
                   (lambda (&rest arguments)
                     (push arguments accept-calls)
                     (asilea-test-tick))))
               (let ((asilea-finished-function
                      (lambda ()
                        (cl-incf finished))))
                 (list
                  (asilea-run-synchronously
                   "measure"
                   [["x"]])
                  finished
                  (length asilea-test-pending)
                  (nreverse accept-calls)
                  (nreverse asilea-test-events))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK (nil nil 0 (nil nil nil) ((:start 1 "measure" (0) ("x") "finished\n" "3") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (0) ("x") "finished\n" "2") (:sentinel 2) (:complete 2 "finished\n") (:start 3 "measure" (0) ("x") "finished\n" "1") (:sentinel 3) (:complete 3 "finished\n")))"#
        ]],
    )
}

fn asilea_sentinel_internal_error_finishes_job_then_resignals_original_condition() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_sentinel_internal_error_finishes_job_then_resignals_original_condition",
        r##"(let ((asilea-max-steps 2)
               (asilea-concurrent-jobs 1)
               (asilea-random-generator-function
                (lambda (_limit) 0))
               finished)
         (asilea-test-reset
          '(("finished\n" "10")))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'asilea--start-process)
                   #'asilea-test-start-process)
                  ((symbol-function
                    'process-buffer)
                   (lambda (_process)
                     (signal
                      'file-error
                      '("fixture process buffer failed"))))
                  ((symbol-function
                    'set-process-sentinel)
                   #'asilea-test-set-process-sentinel))
               (let ((asilea-finished-function
                      (lambda ()
                        (cl-incf finished))))
                 (asilea-run "measure" [["x"]])
                 (list
                  (condition-case error-data
                      (list
                       :ok
                       (asilea-test-tick))
                    (error
                     (list
                      :error
                      (car error-data)
                      (cdr error-data))))
                  finished
                  (length asilea-test-pending)
                  (nreverse asilea-test-events))))
           (asilea-test-cleanup)))"##,
        expect![[
            r#"OK ((:ok t) nil 1 ((:start 1 "measure" (0) ("x") "finished\n" "10") (:sentinel 1) (:complete 1 "finished\n") (:start 2 "measure" (0) ("x") "finished\n" "0") (:sentinel 2)))"#
        ]],
    )
}

pub(super) fn engine_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asilea_one_step_run_reports_initial_candidate_finishes_and_does_not_call_accept_callback(),
        asilea_multi_step_run_uses_accepted_state_for_neighbors_and_accepts_only_better_scores(),
        asilea_nonzero_process_status_skips_parse_and_report_but_consumes_step(),
        asilea_nil_and_false_energy_parses_skip_candidates_while_zero_is_valid(),
        asilea_callback_errors_are_demoted_and_annealing_continues_to_completion(),
        asilea_accepted_solution_callback_error_is_demoted_after_state_and_energy_update(),
        asilea_finished_callback_error_is_demoted_without_escaping_last_sentinel(),
        asilea_concurrent_jobs_have_independent_states_and_call_finished_once_after_last_job(),
        asilea_temperature_terminated_run_cools_until_final_temperature_inclusively(),
        asilea_run_captures_configuration_callbacks_randomness_and_starting_directory(),
        asilea_synchronous_run_drives_pending_processes_until_wrapped_finished_callback(),
        asilea_sentinel_internal_error_finishes_job_then_resignals_original_condition(),
    ]
}
