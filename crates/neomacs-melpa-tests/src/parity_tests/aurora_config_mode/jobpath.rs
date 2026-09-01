use expect_test::expect;

use super::ParityBatchCase;

fn aurora_config_mode_last_job_path_default_and_automatic_buffer_local_contract_are_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_last_job_path_default_and_automatic_buffer_local_contract_are_exact",
        r##"(list
          (default-value
           'aurora-config-last-job-path)
          (local-variable-if-set-p
           'aurora-config-last-job-path)
          (special-variable-p
           'aurora-config-last-job-path)
          (with-temp-buffer
            (list
             aurora-config-last-job-path
             (local-variable-p
              'aurora-config-last-job-path)
             (progn
               (setq
                aurora-config-last-job-path
                "cluster/role/dev/job-a")
               (list
                aurora-config-last-job-path
                (local-variable-p
                 'aurora-config-last-job-path)))
             (default-value
              'aurora-config-last-job-path)))
          (with-temp-buffer
            (list
             aurora-config-last-job-path
             (local-variable-p
              'aurora-config-last-job-path))))"##,
        expect![[
            r#"OK ("smf1/" t t ("smf1/" nil ("cluster/role/dev/job-a" t) "smf1/") ("smf1/" nil))"#
        ]],
    )
}

fn aurora_config_mode_read_jobpath_passes_exact_prompt_default_updates_local_and_returns_input()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_read_jobpath_passes_exact_prompt_default_updates_local_and_returns_input",
        r##"(with-temp-buffer
          (setq
           aurora-config-last-job-path
           "cluster/role/prod/old")
          (let (calls)
            (cl-letf
                (((symbol-function 'read-string)
                  (lambda (&rest arguments)
                    (push arguments calls)
                    "cluster/role/prod/new")))
              (list
               (aurora-config-read-jobpath)
               aurora-config-last-job-path
               (local-variable-p
                'aurora-config-last-job-path)
               (nreverse calls)
               (default-value
                'aurora-config-last-job-path)))))"##,
        expect![[
            r#"OK ("cluster/role/prod/new" "cluster/role/prod/new" t (("Job path as 'cluster/role/env/job': " "cluster/role/prod/old")) "smf1/")"#
        ]],
    )
}

fn aurora_config_mode_repeated_jobpath_reads_feed_each_answer_into_the_next_prompt()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_repeated_jobpath_reads_feed_each_answer_into_the_next_prompt",
        r##"(with-temp-buffer
          (let ((answers
                 '("west/role/dev/one"
                   "east/role/stage/two"
                   "prod/role/prod/three"))
                calls
                results)
            (cl-letf
                (((symbol-function 'read-string)
                  (lambda (prompt initial)
                    (push
                     (list prompt initial)
                     calls)
                    (pop answers))))
              (dotimes (_ 3)
                (push
                 (aurora-config-read-jobpath)
                 results)))
            (list
             (nreverse results)
             (nreverse calls)
             aurora-config-last-job-path
             answers)))"##,
        expect![[
            r#"OK (("west/role/dev/one" "east/role/stage/two" "prod/role/prod/three") (("Job path as 'cluster/role/env/job': " "smf1/") ("Job path as 'cluster/role/env/job': " "west/role/dev/one") ("Job path as 'cluster/role/env/job': " "east/role/stage/two")) "prod/role/prod/three" nil)"#
        ]],
    )
}

fn aurora_config_mode_jobpath_read_accepts_empty_nil_and_non_string_stub_results_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_jobpath_read_accepts_empty_nil_and_non_string_stub_results_exactly",
        r##"(mapcar
          (lambda (answer)
            (with-temp-buffer
              (setq
               aurora-config-last-job-path
               "before")
              (cl-letf
                  (((symbol-function 'read-string)
                    (lambda (&rest _)
                      answer)))
                (list
                 answer
                 (aurora-config-read-jobpath)
                 aurora-config-last-job-path
                 (local-variable-p
                  'aurora-config-last-job-path)))))
          '("" nil 0 job-symbol
            ("nested" "path")))"##,
        expect![[
            r#"OK (("" "" "" t) (nil nil nil t) (0 0 0 t) (job-symbol job-symbol job-symbol t) (#1=("nested" "path") #1# #1# t))"#
        ]],
    )
}

fn aurora_config_mode_read_jobpath_error_propagates_without_overwriting_previous_buffer_value()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_read_jobpath_error_propagates_without_overwriting_previous_buffer_value",
        r##"(with-temp-buffer
          (setq
           aurora-config-last-job-path
           "stable/path")
          (let (calls)
            (cl-letf
                (((symbol-function 'read-string)
                  (lambda (&rest arguments)
                    (push arguments calls)
                    (error
                     "fixture minibuffer failure"))))
              (list
               (aurora-config-test-error-data
                #'aurora-config-read-jobpath)
               aurora-config-last-job-path
               (local-variable-p
                'aurora-config-last-job-path)
               (nreverse calls)))))"##,
        expect![[
            r#"OK ((:error error ("fixture minibuffer failure")) "stable/path" t (("Job path as 'cluster/role/env/job': " "stable/path")))"#
        ]],
    )
}

fn aurora_config_mode_major_mode_reentry_discards_buffer_job_history_then_uses_global_default()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_major_mode_reentry_discards_buffer_job_history_then_uses_global_default",
        r##"(with-temp-buffer
          (setq-default
           aurora-config-last-job-path
           "global/default")
          (aurora-config-mode)
          (setq
           aurora-config-last-job-path
           "buffer/remembered")
          (let ((before
                 (list
                  aurora-config-last-job-path
                  (local-variable-p
                   'aurora-config-last-job-path))))
            (aurora-config-mode)
            (let ((after-mode
                   (list
                    aurora-config-last-job-path
                    (local-variable-p
                     'aurora-config-last-job-path)))
                  calls)
              (cl-letf
                  (((symbol-function 'read-string)
                    (lambda (prompt initial)
                      (push
                       (list prompt initial)
                       calls)
                      "buffer/new")))
                (list
                 before
                 after-mode
                 (aurora-config-read-jobpath)
                 aurora-config-last-job-path
                 (nreverse calls))))))"##,
        expect![[
            r#"OK (("buffer/remembered" t) ("global/default" nil) "buffer/new" "buffer/new" (("Job path as 'cluster/role/env/job': " "global/default")))"#
        ]],
    )
}

pub(super) fn jobpath_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurora_config_mode_last_job_path_default_and_automatic_buffer_local_contract_are_exact(),
        aurora_config_mode_read_jobpath_passes_exact_prompt_default_updates_local_and_returns_input(
        ),
        aurora_config_mode_repeated_jobpath_reads_feed_each_answer_into_the_next_prompt(),
        aurora_config_mode_jobpath_read_accepts_empty_nil_and_non_string_stub_results_exactly(),
        aurora_config_mode_read_jobpath_error_propagates_without_overwriting_previous_buffer_value(
        ),
        aurora_config_mode_major_mode_reentry_discards_buffer_job_history_then_uses_global_default(
        ),
    ]
}
