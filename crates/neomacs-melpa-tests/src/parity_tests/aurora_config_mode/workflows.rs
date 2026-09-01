use expect_test::expect;

use super::ParityBatchCase;

fn aurora_config_mode_practical_font_lock_failure_does_not_block_inspect_and_diff()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_practical_font_lock_failure_does_not_block_inspect_and_diff",
        r##"(with-temp-buffer
          (setq
           buffer-file-name
           (expand-file-name
            "tmp/aurora-config/payments.aurora"
            default-directory))
          (insert
           "payments = Job(\n"
           "    name='payments',\n"
           "    task=Task(processes=[Service(), Process()]))\n")
          (set-buffer-modified-p nil)
          (set-auto-mode)
          (let ((answers
                 '("cluster/payments/prod/payments"
                   "cluster/payments/stage/payments"))
                prompts
                compilations)
            (cl-letf
                (((symbol-function 'read-string)
                  (lambda (prompt initial)
                    (push
                     (list prompt initial)
                     prompts)
                    (pop answers)))
                 ((symbol-function 'compile)
                  (lambda (command)
                    (push
                     (list command compile-command)
                     compilations)
                    (list
                     :compiled
                     command))))
              (list
               (aurora-config-test-buffer-state)
               (aurora-config-test-error-data
                (lambda ()
                  (font-lock-ensure)))
               (aurora-config-test-face-runs)
               (call-interactively
                (key-binding
                 (kbd "C-c a i")))
               (call-interactively
                (key-binding
                 (kbd "C-c a d")))
               (nreverse prompts)
               (nreverse compilations)
               aurora-config-last-job-path
               answers
               (buffer-string)
               (buffer-modified-p)))))"##,
        expect![[
            r#"OK ((aurora-config-mode "Aurora" python-mode "payments = Job(\n    name='payments',\n    task=Task(processes=[Service(), Process()]))\n" nil nil t 6 aurora-config-inspect aurora-config-diff) (:error wrong-type-argument (listp font-lock-type-face)) nil (:compiled "aurora inspect cluster/payments/prod/payments payments.aurora") (:compiled "aurora diff cluster/payments/stage/payments payments.aurora") (("Job path as 'cluster/role/env/job': " "smf1/") ("Job path as 'cluster/role/env/job': " "cluster/payments/prod/payments")) (("aurora inspect cluster/payments/prod/payments payments.aurora" "aurora inspect cluster/payments/prod/payments payments.aurora") ("aurora diff cluster/payments/stage/payments payments.aurora" "aurora diff cluster/payments/stage/payments payments.aurora")) "cluster/payments/stage/payments" nil "payments = Job(\n    name='payments',\n    task=Task(processes=[Service(), Process()]))\n" nil)"#
        ]],
    )
    .fresh_process()
}

fn aurora_config_mode_two_live_configuration_buffers_keep_independent_jobpaths_and_basenames()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_two_live_configuration_buffers_keep_independent_jobpaths_and_basenames",
        r##"(let ((first
                (generate-new-buffer
                 " *aurora-first*"))
               (second
                (generate-new-buffer
                 " *aurora-second*"))
               commands)
          (unwind-protect
              (cl-letf
                  (((symbol-function 'compile)
                    (lambda (command)
                      (push command commands)
                      command)))
                (with-current-buffer first
                  (setq
                   buffer-file-name
                   (expand-file-name
                    "tmp/aurora-config/api.aurora"
                    default-directory))
                  (aurora-config-mode)
                  (setq
                   aurora-config-last-job-path
                   "west/api/prod/api")
                  (aurora-config-inspect
                   aurora-config-last-job-path))
                (with-current-buffer second
                  (setq
                   buffer-file-name
                   (expand-file-name
                    "tmp/aurora-config/worker.mesos"
                    default-directory))
                  (aurora-config-mode)
                  (setq
                   aurora-config-last-job-path
                   "east/worker/stage/worker")
                  (aurora-config-diff
                   aurora-config-last-job-path))
                (list
                 (with-current-buffer first
                   (list
                    aurora-config-last-job-path
                    (local-variable-p
                     'aurora-config-last-job-path)
                    major-mode))
                 (with-current-buffer second
                   (list
                    aurora-config-last-job-path
                    (local-variable-p
                     'aurora-config-last-job-path)
                    major-mode))
                 (nreverse commands)
                 (default-value
                  'aurora-config-last-job-path)))
            (kill-buffer first)
            (kill-buffer second)))"##,
        expect![[
            r#"OK (("west/api/prod/api" t aurora-config-mode) ("east/worker/stage/worker" t aurora-config-mode) ("aurora inspect west/api/prod/api api.aurora" "aurora diff east/worker/stage/worker worker.mesos") "smf1/")"#
        ]],
    )
    .fresh_process()
}

fn aurora_config_mode_incremental_edit_repeats_font_lock_failure_without_losing_content()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_incremental_edit_repeats_font_lock_failure_without_losing_content",
        r##"(with-temp-buffer
          (insert
           "def make_job(name):\n"
           "    return Job(name=name)\n")
          (aurora-config-mode)
          (let ((first-result
                 (aurora-config-test-error-data
                  (lambda ()
                    (font-lock-ensure))))
                (before
                 (aurora-config-test-face-runs)))
            (goto-char
             (point-max))
            (insert
             "\nservice = Service(processes=[JVMProcess(), Process()])\n"
             "schema = Struct(name=String, replicas=Integer)\n")
            (font-lock-flush)
            (list
             first-result
             before
             (aurora-config-test-error-data
              (lambda ()
                (font-lock-ensure)))
             (aurora-config-test-face-runs)
             (buffer-string)
             (buffer-modified-p))))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (listp font-lock-type-face)) nil (:error wrong-type-argument (listp font-lock-type-face)) nil "def make_job(name):\n    return Job(name=name)\n\nservice = Service(processes=[JVMProcess(), Process()])\nschema = Struct(name=String, replicas=Integer)\n" t)"#
        ]],
    )
}

fn aurora_config_mode_renaming_between_supported_and_unsupported_suffixes_changes_auto_mode_choice()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_renaming_between_supported_and_unsupported_suffixes_changes_auto_mode_choice",
        r##"(with-temp-buffer
          (let (states)
            (dolist
                (name
                 '("service.aurora"
                   "service.txt"
                   "service.mesos"
                   "service.py"
                   "service.AURORA"))
              (fundamental-mode)
              (setq
               buffer-file-name
               (expand-file-name
                name
                default-directory))
              (set-auto-mode)
              (push
               (list
                name
                major-mode
                mode-name
                (derived-mode-p
                 'aurora-config-mode)
                (lookup-key
                 (current-local-map)
                 (kbd "C-c a i")))
               states))
            (nreverse states)))"##,
        expect![[
            r#"OK (("service.aurora" aurora-config-mode "Aurora" aurora-config-mode aurora-config-inspect) ("service.txt" text-mode "Text" nil 1) ("service.mesos" aurora-config-mode "Aurora" aurora-config-mode aurora-config-inspect) ("service.py" python-mode "Python" nil 2) ("service.AURORA" aurora-config-mode "Aurora" aurora-config-mode aurora-config-inspect))"#
        ]],
    )
}

fn aurora_config_mode_compile_command_binding_is_per_call_and_does_not_leak_across_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_compile_command_binding_is_per_call_and_does_not_leak_across_workflow",
        r##"(with-temp-buffer
          (setq
           buffer-file-name
           (expand-file-name
            "tmp/aurora-config/no-leak.aurora"
            default-directory))
          (let ((compile-command
                 "outer compile command")
                observations)
            (cl-letf
                (((symbol-function 'compile)
                  (lambda (command)
                    (push
                     (list
                      command
                      compile-command)
                     observations)
                    (setq compile-command
                          "mutated only in call")
                    :done)))
              (list
               (aurora-config-inspect
                "cluster/role/dev/no-leak")
               compile-command
               (aurora-config-diff
                "cluster/role/prod/no-leak")
               compile-command
               (nreverse observations)))))"##,
        expect![[
            r#"OK (:done "outer compile command" :done "outer compile command" (("aurora inspect cluster/role/dev/no-leak no-leak.aurora" "aurora inspect cluster/role/dev/no-leak no-leak.aurora") ("aurora diff cluster/role/prod/no-leak no-leak.aurora" "aurora diff cluster/role/prod/no-leak no-leak.aurora")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurora_config_mode_practical_font_lock_failure_does_not_block_inspect_and_diff(),
        aurora_config_mode_two_live_configuration_buffers_keep_independent_jobpaths_and_basenames(),
        aurora_config_mode_incremental_edit_repeats_font_lock_failure_without_losing_content(),
        aurora_config_mode_renaming_between_supported_and_unsupported_suffixes_changes_auto_mode_choice(),
        aurora_config_mode_compile_command_binding_is_per_call_and_does_not_leak_across_workflow(),
    ]
}
