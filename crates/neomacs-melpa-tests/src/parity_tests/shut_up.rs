use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SHUT_UP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SHUT_UP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const SHUT_UP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'shut-up)

(defun neomacs-shut-up-test-read-file (file)
  "Return FILE's contents without text properties."
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-substring-no-properties (point-min) (point-max))))
"##;

fn shut_up_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SHUT_UP_MELPA_PIN, "shut-up.el")
        .expect("prepare revision-pinned Shut Up source below ./tmp")
        .with_prelude(SHUT_UP_TEST_PRELUDE)
        .with_timeout(SHUT_UP_TEST_TIMEOUT)
}

fn noisy_build_step_is_captured_in_order_without_changing_its_result() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((standard-output (current-buffer))
        captured result sink)
    (princ "visible-before")
    (setq result
          (shut-up
            (setq sink shut-up-sink)
            (message "Indexing %d files" 3)
            (princ "progress=50%;")
            (prin1 '(:artifact "app.tar" :size 42))
            (terpri)
            (write-char ?✓)
            (setq captured (shut-up-current-output))
            '(:status ready :files 3)))
    (princ "|visible-after")
    (list :result result
          :captured captured
          :visible (buffer-string)
          :sink-cleaned (not (buffer-live-p sink)))))
"##;
    let expected = expect![[
        r####"OK (:result (:status ready :files 3) :captured "Indexing 3 files\nprogress=50%;(:artifact \"app.tar\" :size 42)\n✓" :visible "visible-before|visible-after" :sink-cleaned t)"####
    ]];
    ParityBatchCase::value(
        "noisy_build_step_is_captured_in_order_without_changing_its_result",
        elisp_form,
        expected,
    )
}

fn failed_task_propagates_its_error_and_restores_every_output_binding() -> ParityBatchCase {
    let elisp_form = r##"
(let ((original-message (symbol-function 'message))
      (original-output standard-output)
      sink captured failure)
  (condition-case err
      (shut-up
        (setq sink shut-up-sink)
        (message "Verifying artifact")
        (princ "checksum=bad")
        (setq captured (shut-up-current-output))
        (error "Deployment failed: %s" "checksum mismatch"))
    (error
     (setq failure (list (car err) (error-message-string err)))))
  (list :failure failure
        :captured captured
        :message-restored (eq original-message (symbol-function 'message))
        :output-restored (eq original-output standard-output)
        :sink-cleaned (not (buffer-live-p sink))))
"##;
    let expected = expect![[
        r####"OK (:failure (error "Deployment failed: checksum mismatch") :captured "Verifying artifact\nchecksum=bad" :message-restored t :output-restored t :sink-cleaned t)"####
    ]];
    ParityBatchCase::value(
        "failed_task_propagates_its_error_and_restores_every_output_binding",
        elisp_form,
        expected,
    )
}

fn artifact_generation_writes_and_appends_exact_bytes_without_console_noise() -> ParityBatchCase {
    let elisp_form = r##"
(let ((file (expand-file-name "shut-up-release-manifest.txt"
                              temporary-file-directory)))
  (unwind-protect
      (progn
        (when (file-exists-p file)
          (delete-file file))
        (let (captured write-results)
          (with-temp-buffer
            (insert "artifact=app.tar\nsize=42")
            (setq write-results
                  (shut-up
                    (let ((first (write-region (point-min) (point-max) file))
                          (second (write-region "\nsha256=abc123" nil file t)))
                      (setq captured (shut-up-current-output))
                      (list first second)))))
          (list :write-results write-results
                :captured captured
                :contents (neomacs-shut-up-test-read-file file)
                :size (file-attribute-size (file-attributes file)))))
    (when (file-exists-p file)
      (delete-file file))))
"##;
    let expected = expect![[
        r####"OK (:write-results (nil nil) :captured "" :contents "artifact=app.tar\nsize=42\nsha256=abc123" :size 38)"####
    ]];
    ParityBatchCase::value(
        "artifact_generation_writes_and_appends_exact_bytes_without_console_noise",
        elisp_form,
        expected,
    )
}

fn quiet_loading_suppresses_progress_while_capturing_the_library_message() -> ParityBatchCase {
    let elisp_form = r##"
(let ((library (expand-file-name "shut-up-release-helper.el"
                                 temporary-file-directory)))
  (unwind-protect
      (progn
        (write-region
         (concat
          ";;; shut-up-release-helper.el --- fixture -*- lexical-binding: t; -*-\n"
          "(message \"Loading release helper\")\n"
          "(setq neomacs-shut-up-loaded\n"
          "      '(:artifact \"app.tar\" :verified t))\n")
         nil library nil 'no-message)
        (let (captured loaded)
          (setq neomacs-shut-up-loaded nil)
          (setq loaded
                (shut-up
                  (prog1 (load library nil nil t)
                    (setq captured (shut-up-current-output)))))
          (list :loaded loaded
                :state neomacs-shut-up-loaded
                :captured captured)))
    (when (file-exists-p library)
      (delete-file library))))
"##;
    let expected = expect![[
        r####"OK (:loaded t :state (:artifact "app.tar" :verified t) :captured "Loading release helper\n")"####
    ]];
    ParityBatchCase::value(
        "quiet_loading_suppresses_progress_while_capturing_the_library_message",
        elisp_form,
        expected,
    )
}

fn nested_silence_scopes_keep_inner_and_outer_logs_separate() -> ParityBatchCase {
    let elisp_form = r##"
(let (outer-output inner-output result outer-sink inner-sink)
  (setq result
        (shut-up
          (setq outer-sink shut-up-sink)
          (message "outer:start")
          (setq inner-output
                (shut-up
                  (setq inner-sink shut-up-sink)
                  (message "inner:download")
                  (princ "inner:verify")
                  (shut-up-current-output)))
          (message "outer:done")
          (setq outer-output (shut-up-current-output))
          :complete))
  (list :result result
        :outer outer-output
        :inner inner-output
        :distinct-sinks (not (eq outer-sink inner-sink))
        :sinks-cleaned (list (not (buffer-live-p outer-sink))
                             (not (buffer-live-p inner-sink)))))
"##;
    let expected = expect![[
        r####"OK (:result :complete :outer "outer:start\nouter:done\n" :inner "inner:download\ninner:verify" :distinct-sinks t :sinks-cleaned (t t))"####
    ]];
    ParityBatchCase::value(
        "nested_silence_scopes_keep_inner_and_outer_logs_separate",
        elisp_form,
        expected,
    )
}

fn explicit_bypass_uses_the_callers_output_and_is_snapshotted_at_entry() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((standard-output (current-buffer))
        captured bypass-capture)
    (princ "before|")
    (let ((shut-up-ignore nil))
      (shut-up
        (princ "captured")
        (setq shut-up-ignore t)
        (princ "|still-captured")
        (setq captured (shut-up-current-output))))
    (let ((shut-up-ignore t))
      (shut-up
        (princ "bypassed")
        (setq bypass-capture (shut-up-current-output))))
    (princ "|after")
    (list :captured captured
          :bypass-capture bypass-capture
          :callers-output (buffer-string))))
"##;
    let expected = expect![[
        r####"OK (:captured "captured|still-captured" :bypass-capture "" :callers-output "before|bypassed|after")"####
    ]];
    ParityBatchCase::value(
        "explicit_bypass_uses_the_callers_output_and_is_snapshotted_at_entry",
        elisp_form,
        expected,
    )
}

fn noninteractive_startup_disables_vc_file_hooks_and_dired_ls_metadata() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (require 'dired)
  (let ((find-file-hooks '(vc-find-file-hook release-audit-hook))
        (dired-use-ls-dired t))
    (shut-up-silence-emacs)
    (list :find-file-hooks find-file-hooks
          :dired-use-ls-dired dired-use-ls-dired)))
"##;
    let expected =
        expect![[r####"OK (:find-file-hooks (release-audit-hook) :dired-use-ls-dired nil)"####]];
    ParityBatchCase::value(
        "noninteractive_startup_disables_vc_file_hooks_and_dired_ls_metadata",
        elisp_form,
        expected,
    )
}

#[test]
fn shut_up_package_batch() {
    let cases = vec![
        noisy_build_step_is_captured_in_order_without_changing_its_result(),
        failed_task_propagates_its_error_and_restores_every_output_binding(),
        artifact_generation_writes_and_appends_exact_bytes_without_console_noise(),
        quiet_loading_suppresses_progress_while_capturing_the_library_message(),
        nested_silence_scopes_keep_inner_and_outer_logs_separate(),
        explicit_bypass_uses_the_callers_output_and_is_snapshotted_at_entry(),
        noninteractive_startup_disables_vc_file_hooks_and_dired_ls_metadata(),
    ];
    assert_oracle_batch_cases(
        shut_up_oracle(),
        "parity_tests::shut_up::shut_up_package_batch",
        "shut_up_parity",
        &cases,
    );
}
