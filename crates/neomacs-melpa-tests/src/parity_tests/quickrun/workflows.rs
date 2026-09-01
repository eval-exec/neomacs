use expect_test::expect;

use super::ParityBatchCase;

fn custom_command_runs_current_buffer_with_unicode_and_public_hook_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_command_runs_current_buffer_with_unicode_and_public_hook_lifecycle",
        r####"
(neomacs-quickrun-test-with-buffer "release.qr" "release Ω\nready\n"
  (let ((quickrun-after-run-hook '(neomacs-quickrun-test-after-run))
        (quickrun-output-only t)
        (quickrun-focus-p nil)
        (quickrun-option-outputter 'quickrun--default-outputter)
        (quickrun-option-cmdkey "parity/cat"))
    (quickrun-add-command "parity/cat"
      '((:command . "cat") (:exec . "%c %s"))
      :mode 'fundamental-mode)
    (quickrun)
    (neomacs-quickrun-test-wait 1)
    (list :output (neomacs-quickrun-test-output)
          :hooks neomacs-quickrun-test-hook-count
          :last quickrun--last-cmd-key
          :temp-cleaned (null quickrun--remove-files))))
"####,
        expect![[
            r#"OK (:output (:text "release Ω\nready\n" :mode quickrun--mode :read-only t :truncate t :process nil) :hooks 1 :last "parity/cat" :temp-cleaned t)"#
        ]],
    )
}

fn region_commands_run_only_selection_and_replace_it_with_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "region_commands_run_only_selection_and_replace_it_with_output",
        r####"
(neomacs-quickrun-test-with-buffer "transform.qr" "before\nalpha beta\nafter\n"
  (quickrun-add-command "parity/upper"
    '((:command . "tr") (:exec . "%c a-z A-Z < %s")))
  (setq-local quickrun-option-cmdkey "parity/upper")
  (let ((quickrun-focus-p nil))
    (goto-char (point-min))
    (forward-line 1)
    (let ((beg (point)))
      (forward-line 1)
      (let ((end (point))
            (quickrun-output-only t))
        (quickrun-region beg end)
        (neomacs-quickrun-test-wait)
        (let ((region-output (neomacs-quickrun-test-output)))
          (set-buffer buffer)
          (goto-char beg)
          (set-mark end)
          (activate-mark)
          (quickrun-replace-region beg end)
          (neomacs-quickrun-test-wait)
          (list :region region-output
                :buffer (buffer-substring-no-properties (point-min) (point-max))
                :point (point) :modified (buffer-modified-p)))))))
"####,
        expect![[
            r#"OK (:region (:text "ALPHA BETA\n" :mode quickrun--mode :read-only t :truncate t :process nil) :buffer "before\nALPHA BETA\nafter\n" :point 19 :modified t)"#
        ]],
    )
}

fn arguments_default_directory_and_stdin_sidecar_reach_the_real_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "arguments_default_directory_and_stdin_sidecar_reach_the_real_process",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "quickrun-args"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "script.qr" root))
       (input (concat source quickrun-input-file-extension)))
  (when (file-exists-p root) (delete-directory root t))
  (make-directory root t)
  (with-temp-file source (insert "ignored source\n"))
  (with-temp-file input (insert "stdin Ω\n"))
  (let ((buffer (find-file-noselect source)))
    (unwind-protect
        (with-current-buffer buffer
          (quickrun-add-command "parity/args"
            `((:command . "sh")
              (:exec . "%c -c 'read line; echo dir=$PWD arg=$1 input=$line' sh %a")
              (:tempfile . nil)
              (:default-directory . ,root)))
          (let ((quickrun-option-cmdkey "parity/args")
                (quickrun-option-args "release-42")
                (quickrun-output-only t))
            (quickrun)
            (neomacs-quickrun-test-wait)
            (list :output (neomacs-quickrun-test-output)
                  :default quickrun-option-default-directory
                  :executed quickrun--executed-file)))
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)
      (neomacs-quickrun-test-reset)
      (delete-directory root t))))
"####,
        expect![[
            r#"OK (:output (:text "dir=[ORACLE-SANDBOX]/quickrun-args arg=release-42 input=stdin Ω\n" :mode quickrun--mode :read-only t :truncate t :process nil) :default nil :executed "script.qr")"#
        ]],
    )
}

fn outputters_copy_success_to_variable_buffer_file_and_null_destinations() -> ParityBatchCase {
    ParityBatchCase::value(
        "outputters_copy_success_to_variable_buffer_file_and_null_destinations",
        r####"
(neomacs-quickrun-test-with-buffer "output.qr" "payload Ω\n"
  (let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (file (expand-file-name "quickrun-output.txt" root))
         (quickrun-output-only t))
    (setq quickrun-test-value nil)
    (quickrun-add-command "parity/output"
      '((:command . "cat") (:exec . "%c %s")))
    (setq-local quickrun-option-cmdkey "parity/output")
    (let ((quickrun-option-outputter
           `(multi variable:quickrun-test-value buffer:*quickrun-copy*
                   ,(intern (concat "file:" file)))))
      (quickrun)
      (neomacs-quickrun-test-wait))
    (list :variable quickrun-test-value
          :buffer (with-current-buffer "*quickrun-copy*" (buffer-string))
          :file (with-temp-buffer (insert-file-contents file) (buffer-string))
          :quickrun-live (and (get-buffer quickrun--buffer-name) t))))
"####,
        expect![[
            r#"OK (:variable "payload Ω\n" :buffer "payload Ω\n" :file "payload Ω\n" :quickrun-live t)"#
        ]],
    )
}

fn failures_skip_success_hook_and_invalid_command_configuration_reports_errors() -> ParityBatchCase
{
    ParityBatchCase::value(
        "failures_skip_success_hook_and_invalid_command_configuration_reports_errors",
        r####"
(neomacs-quickrun-test-with-buffer "failure.qr" "bad\n"
  (let ((quickrun-after-run-hook '(neomacs-quickrun-test-after-run))
        (quickrun-output-only t)
        (quickrun-option-cmdkey "parity/fail"))
    (quickrun-add-command "parity/fail"
      '((:command . "sh") (:exec . "%c -c 'printf failure >&2; exit 7'")))
    (quickrun)
    (neomacs-quickrun-test-wait)
    (let ((failure (neomacs-quickrun-test-output)))
      (list :failure failure :hooks neomacs-quickrun-test-hook-count
            :invalid
            (mapcar
             (lambda (args)
               (condition-case err
                   (list :value (apply #'quickrun-add-command args))
                 (error (list :signal (car err)
                              :message (error-message-string err)))))
             '((nil ((:command . "x")))
               ("missing" nil)
               ("no-command" ((:exec . "x")))
               ("not-registered" ((:command . "x")) :override t)))))))
"####,
        expect![[
            r#"OK (:failure (:text "failure" :mode quickrun--mode :read-only nil :truncate t :process nil) :hooks 0 :invalid ((:signal error :message "Undefined 1st argument ’key’") (:signal error :message "Undefined 2nd argument ’command alist’") (:signal error :message "Not found :command parameter in language alist") (:signal error :message "’not-registered’ is not registered")))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        custom_command_runs_current_buffer_with_unicode_and_public_hook_lifecycle(),
        region_commands_run_only_selection_and_replace_it_with_output(),
        arguments_default_directory_and_stdin_sidecar_reach_the_real_process(),
        outputters_copy_success_to_variable_buffer_file_and_null_destinations(),
        failures_skip_success_hook_and_invalid_command_configuration_reports_errors(),
    ]
}
