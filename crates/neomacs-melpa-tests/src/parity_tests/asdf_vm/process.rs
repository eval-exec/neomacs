use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_process_defaults_use_buffer_file_parent_then_default_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_process_defaults_use_buffer_file_parent_then_default_directory",
        r##"(let ((asdf-vm-process-executable
                    "/fixture/asdf")
                   (asdf-vm-process-executable-arguments
                    '("--color" "never"))
                   (default-directory
                    "/fallback/work/"))
               (list
                (with-temp-buffer
                  (setq buffer-file-name
                        "/project/src/main.ex")
                  (asdf-vm--make-process-defaults))
                (with-temp-buffer
                  (asdf-vm--make-process-defaults))))"##,
        expect![[
            r#"OK ((:executable "/fixture/asdf" :executable-arguments #1=("--color" "never") :directory "/project/src/") (:executable "/fixture/asdf" :executable-arguments #1# :directory "/fallback/work/"))"#
        ]],
    )
}

fn asdf_vm_call_args_merge_supported_overrides_and_ignore_dispatch_only_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_call_args_merge_supported_overrides_and_ignore_dispatch_only_keys",
        r##"(let ((asdf-vm-process-executable
                    "/default/asdf")
                   (asdf-vm-process-executable-arguments
                    '("--default"))
                   (default-directory
                    "/default/work/"))
               (asdf-vm--call-args
                '(:name "explicit"
                  :name-prefix "prefix"
                  :executable "/custom/asdf"
                  :executable-arguments ("--custom")
                  :command (plugin add)
                  :command-arguments ("ruby" "url")
                  :directory "/custom/work/"
                  :buffer-name "*ignored-input*"
                  :blocking t
                  :output t
                  :success-codes (0 7)
                  :unknown "ignored")
                "*selected-buffer*"))"##,
        expect![[
            r#"OK (:executable "/custom/asdf" :executable-arguments ("--custom") :directory "/custom/work/" :buffer-name "*ignored-input*" :name "explicit" :name-prefix "prefix" :command (plugin add) :command-arguments ("ruby" "url"))"#
        ]],
    )
}

fn asdf_vm_process_name_formatting_covers_explicit_nested_and_empty_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_process_name_formatting_covers_explicit_nested_and_empty_commands",
        r##"(mapcar
               (lambda (arguments)
                 (apply
                  #'asdf-vm-process--format-name
                  arguments))
               '(("explicit" "asdf" current)
                 (nil "asdf" current)
                 (nil "asdf" (plugin add))
                 (nil "asdf" nil)
                 ("" "asdf" current)))"##,
        expect![[r#"OK ("explicit" "asdf[current]" "asdf[(plugin add)]" "asdf" "")"#]],
    )
}

fn asdf_vm_make_process_constructs_exact_async_process_plist_and_working_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_make_process_constructs_exact_async_process_plist_and_working_directory",
        r##"(let ((asdf-vm-process-stderr-buffer-name
                    "*fixture-stderr*")
                   calls)
               (cl-letf
                   (((symbol-function
                      'make-process)
                     (lambda (&rest arguments)
                       (push
                        (list
                         arguments
                         default-directory
                         (buffer-name
                          (current-buffer)))
                        calls)
                       :process)))
                 (list
                  (asdf-vm-process--make-process
                   :executable "/fixture/asdf"
                   :executable-arguments
                   '("--global" "資料")
                   :command '(plugin add)
                   :command-arguments
                   '("ruby" "https://example/ruby.git")
                   :directory "/work/project/"
                   :buffer-name "*fixture-output*"
                   :name-prefix "manager")
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:process (((:name "manager[(plugin add)]" :buffer (:buffer "*fixture-output*") :sentinel asdf-vm-process--sentinel :command ("/fixture/asdf" "--global" "資料" "plugin" "add" "ruby" "https://example/ruby.git") :stderr "*fixture-stderr*") "/work/project/" "*fixture-output*")))"#
        ]],
    )
}

fn asdf_vm_call_runs_deterministic_external_executable_with_exact_order_directory_and_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_call_runs_deterministic_external_executable_with_exact_order_directory_and_output",
        r##"(let* ((directory
                     (file-name-as-directory
                      (asdf-vm-test-path
                       "process-real/work")))
                    (executable
                     (asdf-vm-test-make-executable
                      "asdf-fixture"
                      (concat
                       "printf 'PWD=<%s>\\n' \"$PWD\"\n"
                       "for argument in \"$@\"; do "
                       "printf 'ARG=<%s>\\n' \"$argument\"; "
                       "done")))
                    (asdf-vm-process-executable
                     executable)
                    (asdf-vm-process-executable-arguments
                     '("--global" "資料 λ")))
               (make-directory directory t)
               (asdf-vm-call
                :command '(plugin add)
                :command-arguments
                '("ruby"
                  "https://example/ruby repo.git")
                :directory directory
                :output t))"##,
        expect![[
            r#"OK "PWD=<[ORACLE-SANDBOX]/process-real/work>\nARG=<--global>\nARG=<資料 λ>\nARG=<plugin>\nARG=<add>\nARG=<ruby>\nARG=<https://example/ruby repo.git>\n""#
        ]],
    )
}

fn asdf_vm_sync_call_returns_nil_without_output_and_ignores_external_exit_status() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_sync_call_returns_nil_without_output_and_ignores_external_exit_status",
        r##"(let ((executable
                    (asdf-vm-test-make-executable
                     "failing-asdf"
                     (concat
                      "printf 'failure payload\\n'\n"
                      "exit 7"))))
               (list
                (asdf-vm-call
                 :executable executable
                 :command 'version
                 :blocking t)
                (asdf-vm-call
                 :executable executable
                 :command 'version
                 :output nil)))"##,
        expect!["OK (nil nil)"],
    )
}

fn asdf_vm_call_dispatch_uses_presence_of_blocking_or_output_even_when_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_call_dispatch_uses_presence_of_blocking_or_output_even_when_nil",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm--sync-call)
                     (lambda (&rest arguments)
                       (push
                        (cons :sync arguments)
                        calls)
                       :sync))
                    ((symbol-function
                      'asdf-vm--async-call)
                     (lambda (&rest arguments)
                       (push
                        (cons :async arguments)
                        calls)
                       :async)))
                 (list
                  (asdf-vm-call
                   :command 'one)
                  (asdf-vm-call
                   :command 'two
                   :blocking nil)
                  (asdf-vm-call
                   :command 'three
                   :output nil)
                  (asdf-vm-call
                   :command 'four
                   :blocking nil
                   :output nil)
                  (nreverse calls))))"##,
        expect![
            "OK (:async :sync :sync :sync ((:async :command one) (:sync :command two :blocking nil) (:sync :command three :output nil) (:sync :command four :blocking nil :output nil)))"
        ],
    )
}

fn asdf_vm_async_call_starts_immediately_or_enqueues_complete_call_args_fifo() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_async_call_starts_immediately_or_enqueues_complete_call_args_fifo",
        r##"(let ((asdf-vm-process-executable
                    "/fixture/asdf")
                   (asdf-vm-process-executable-arguments
                    '("--base"))
                   (default-directory
                    "/work/default/")
                   (asdf-vm-process--call-queue
                    '((:command old)))
                   running
                   calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-process--buffer-process-running-p)
                     (lambda (buffer-name)
                       (push
                        (list
                         :running buffer-name running)
                        calls)
                       running))
                    ((symbol-function
                      'asdf-vm-process--make-process)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :make arguments)
                        calls)
                       :started)))
                 (let ((first
                        (asdf-vm--async-call
                         :command 'current
                         :command-arguments
                         '("ruby"))))
                   (setq running t)
                   (let ((second
                          (asdf-vm--async-call
                           :command '(plugin update)
                           :command-arguments
                           '("--all")
                           :directory
                           "/work/custom/")))
                     (list
                      first
                      second
                      asdf-vm-process--call-queue
                      (nreverse calls))))))"##,
        expect![[
            r#"OK (:started #1=((:command old) (:executable "/fixture/asdf" :executable-arguments #2=("--base") :directory "/work/custom/" :buffer-name "*asdf-vm*" :command (plugin update) :command-arguments ("--all"))) #1# ((:running "*asdf-vm*" nil) (:make :executable "/fixture/asdf" :executable-arguments #2# :directory "/work/default/" :buffer-name "*asdf-vm*" :command current :command-arguments ("ruby")) (:running "*asdf-vm*" t)))"#
        ]],
    )
}

fn asdf_vm_buffer_process_running_p_handles_missing_buffer_process_and_statuses() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_buffer_process_running_p_handles_missing_buffer_process_and_statuses",
        r##"(let (status)
               (cl-letf
                   (((symbol-function
                      'get-buffer)
                     (lambda (name)
                       (and
                        (not
                         (equal name
                                "missing"))
                        :buffer)))
                    ((symbol-function
                      'get-buffer-process)
                     (lambda (_)
                       (unless
                           (eq status
                               'none)
                         :process)))
                    ((symbol-function
                      'process-status)
                     (lambda (_)
                       status)))
                 (mapcar
                  (lambda (value)
                    (setq status value)
                    (list
                     value
                     (asdf-vm-process--buffer-process-running-p
                      "present")))
                  '(run stop exit signal none))
                 (list
                  (asdf-vm-process--buffer-process-running-p
                   "missing")
                  (mapcar
                   (lambda (value)
                     (setq status value)
                     (list
                      value
                      (asdf-vm-process--buffer-process-running-p
                       "present")))
                   '(run stop exit signal none)))))"##,
        expect!["OK (nil ((run t) (stop nil) (exit nil) (signal nil) (none nil)))"],
    )
}

fn asdf_vm_process_sentinel_preserves_idle_states_signals_errors_and_drains_one_queue_item()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_process_sentinel_preserves_idle_states_signals_errors_and_drains_one_queue_item",
        r##"(let ((asdf-vm-process--call-queue
                    '((:command first
                       :command-arguments
                       ("one"))
                      (:command second)))
                   status
                   calls)
               (cl-letf
                   (((symbol-function
                      'process-status)
                     (lambda (_)
                       status))
                    ((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       :started)))
                 (let ((results
                        (mapcar
                         (lambda (value)
                           (setq status value)
                           (list
                            value
                            (asdf-vm-test-error-data
                             (lambda ()
                               (asdf-vm-process--sentinel
                                :process
                                "fixture event")))))
                         '(run stop signal open nil mystery exit exit))))
                   (list
                    results
                    asdf-vm-process--call-queue
                    (nreverse calls)))))"##,
        expect![[
            r#"OK (((run (:ok nil)) (stop (:ok nil)) (signal (:error asdf-vm-sentinel-nonsense-process-status (signal "fixture event"))) (open (:error asdf-vm-sentinel-nonsense-process-status (open "fixture event"))) (nil (:error asdf-vm-sentinel-missing-process (nil "fixture event"))) (mystery (:error asdf-vm-sentinel-unknown-status (mystery "fixture event"))) (exit (:ok :started)) (exit (:ok :started))) nil ((:call :command first :command-arguments ("one")) (:call :command second)))"#
        ]],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_process_defaults_use_buffer_file_parent_then_default_directory(),
        asdf_vm_call_args_merge_supported_overrides_and_ignore_dispatch_only_keys(),
        asdf_vm_process_name_formatting_covers_explicit_nested_and_empty_commands(),
        asdf_vm_make_process_constructs_exact_async_process_plist_and_working_directory(),
        asdf_vm_call_runs_deterministic_external_executable_with_exact_order_directory_and_output(),
        asdf_vm_sync_call_returns_nil_without_output_and_ignores_external_exit_status(),
        asdf_vm_call_dispatch_uses_presence_of_blocking_or_output_even_when_nil(),
        asdf_vm_async_call_starts_immediately_or_enqueues_complete_call_args_fifo(),
        asdf_vm_buffer_process_running_p_handles_missing_buffer_process_and_statuses(),
        asdf_vm_process_sentinel_preserves_idle_states_signals_errors_and_drains_one_queue_item(),
    ]
}
