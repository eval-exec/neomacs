use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PFUTURE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PFUTURE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PFUTURE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'pfuture)

(defvar neomacs-pfuture-test-callback-record nil)

(defun neomacs-pfuture-test-wait (process &optional predicate)
  "Wait for PROCESS and optional PREDICATE, failing after five seconds."
  (let ((deadline (+ (float-time) 5)))
    (while (and (< (float-time) deadline)
                (or (process-live-p process)
                    (and predicate (not (funcall predicate)))))
      (accept-process-output process 0.05))
    (accept-process-output process 0.01)
    (when (or (process-live-p process)
              (and predicate (not (funcall predicate))))
      (error "Timed out waiting for pfuture process %s" (process-name process)))))

(defun neomacs-pfuture-test-uppercase-filter (process text)
  "Append an uppercase version of TEXT to PROCESS' configured buffer."
  (with-current-buffer (process-get process 'buffer)
    (goto-char (point-max))
    (insert (upcase text))))

(defun neomacs-pfuture-test-record-callback (process status buffer)
  "Record PROCESS, STATUS, and BUFFER delivered to a named callback."
  (setq neomacs-pfuture-test-callback-record
        (list :status (string-trim-right status)
              :exit (process-exit-status process)
              :output (pfuture-output-from-buffer buffer)
              :buffer-live (buffer-live-p buffer))))

(defun neomacs-pfuture-test-owned-process-p (process)
  "Return non-nil when PROCESS belongs to this Pfuture corpus."
  (let ((name (process-name process)))
    (or (string-match-p "Pfuture" name)
        (string-match-p "Process Future" name)
        (string-prefix-p "release-" name))))

(defun neomacs-pfuture-test-reset ()
  "Remove processes and implementation buffers left by a parity case."
  (dolist (process (process-list))
    (when (neomacs-pfuture-test-owned-process-p process)
      (ignore-errors (delete-process process))))
  (sit-for 0.01)
  (dolist (buffer (buffer-list))
    (when (or (string-prefix-p " Pfuture" (buffer-name buffer))
              (string-prefix-p " *pfuture" (buffer-name buffer))
              (string-prefix-p " release-pfuture" (buffer-name buffer)))
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (setq pfuture--dummy-buffer nil
        neomacs-pfuture-test-callback-record nil))

(defun neomacs-pfuture-test-with-reset (function)
  "Run FUNCTION without leaking Pfuture processes into the next case."
  (neomacs-pfuture-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-pfuture-test-reset)))
"###;

fn future_separates_stdout_stderr_and_caches_a_failed_command_result() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let* ((future
           (pfuture-new
            "sh" "-c"
            "printf 'release-out'; printf 'release-err' >&2; exit 7"))
          (output-buffer (process-get future 'buffer))
          (stdout (pfuture-await-to-finish future))
          (stderr (pfuture-stderr future))
          (cached (pfuture-result future)))
     (list :status (process-status future)
           :exit (process-exit-status future)
           :stdout stdout
           :stderr stderr
           :cached cached
           :same-result-object (eq stdout cached)
           :output-buffer-live (buffer-live-p output-buffer)
           :stderr-process (process-get future 'stderr-process)))))
"###;
    let expected = expect![[
        r#"OK (:status exit :exit 7 :stdout "release-out" :stderr "release-err" :cached "release-out" :same-result-object t :output-buffer-live nil :stderr-process (:process " Process Future stderr" closed))"#
    ]];
    ParityBatchCase::value(
        "future_separates_stdout_stderr_and_caches_a_failed_command_result",
        elisp_form,
        expected,
    )
}

fn live_future_exposes_partial_output_before_awaiting_the_final_result() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let ((future
          (pfuture-new
           "sh" "-c" "printf 'queued'; sleep 0.15; printf ':ready'")))
     (let ((deadline (+ (float-time) 3)))
       (while (and (< (float-time) deadline)
                   (not (string= (pfuture-result future) "queued")))
         (accept-process-output future 0.02)))
     (let ((partial (pfuture-result future))
           (live-before-finish (and (process-live-p future) t))
           (short-await (pfuture-await future :timeout 0.01)))
       (let ((final (pfuture-await-to-finish future)))
         (list :partial partial
               :live live-before-finish
               :short-await short-await
               :final final
               :cached (pfuture-result future)
               :status (process-status future)))))))
"###;
    let expected = expect![[
        r#"OK (:partial "queued" :live t :short-await "queued" :final "queued:ready" :cached "queued:ready" :status exit)"#
    ]];
    ParityBatchCase::value(
        "live_future_exposes_partial_output_before_awaiting_the_final_result",
        elisp_form,
        expected,
    )
}

fn independent_futures_run_concurrently_and_can_be_awaited_in_creation_order() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let* ((api
           (pfuture-new "sh" "-c" "sleep 0.08; printf 'api-ready'"))
          (worker
           (pfuture-new "sh" "-c" "sleep 0.08; printf 'worker-ready'"))
          (both-live
           (and (process-live-p api) (process-live-p worker) t))
          (api-output (pfuture-await api :timeout 2 :just-this-one nil))
          (worker-output
           (pfuture-await-to-finish worker)))
     (unless (memq (process-status api) '(exit signal failed))
       (pfuture-await-to-finish api))
     (list :both-live both-live
           :api api-output
           :worker worker-output
           :statuses (list (process-status api) (process-status worker))
           :exits (list (process-exit-status api)
                        (process-exit-status worker))))))
"###;
    let expected = expect![[
        r#"OK (:both-live t :api "api-ready" :worker "worker-ready" :statuses (exit exit) :exits (0 0))"#
    ]];
    ParityBatchCase::value(
        "independent_futures_run_concurrently_and_can_be_awaited_in_creation_order",
        elisp_form,
        expected,
    )
}

fn callback_workflow_routes_success_and_failure_with_complete_output() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let (success-record error-record success-statuses error-statuses)
     (let* ((status-recorder
             (lambda (process status _buffer)
               (if (= 0 (process-exit-status process))
                   (push (string-trim-right status) success-statuses)
                 (push (string-trim-right status) error-statuses))))
            (success
             (pfuture-callback ["sh" "-c" "printf 'deployed'"]
               :name "release-success"
               :on-status-change status-recorder
               :on-success
               (setq success-record
                     (list :branch 'success
                           :status (string-trim-right status)
                           :output (pfuture-callback-output)
                           :buffer-live (buffer-live-p pfuture-buffer)))
               :on-error (setq success-record 'wrong-branch)))
            (failure
             (pfuture-callback
                 ["sh" "-c"
                  "printf 'validation-out'; printf 'validation-err' >&2; exit 9"]
               :name "release-failure"
               :on-status-change status-recorder
               :on-success (setq error-record 'wrong-branch)
               :on-error
               (setq error-record
                     (list :branch 'error
                           :status (string-trim-right status)
                           :output (pfuture-callback-output)
                           :exit (process-exit-status process)
                           :buffer-live (buffer-live-p pfuture-buffer))))))
       (neomacs-pfuture-test-wait success (lambda () success-record))
       (neomacs-pfuture-test-wait failure (lambda () error-record))
       (list :success success-record
             :failure error-record
             :status-events
             (list (nreverse success-statuses) (nreverse error-statuses))
             :buffers-live-after-callback
             (list (buffer-live-p (process-get success 'buffer))
                   (buffer-live-p (process-get failure 'buffer))))))))
"###;
    let expected = expect![[
        r#"OK (:success (:branch success :status "finished" :output "deployed" :buffer-live t) :failure (:branch error :status "exited abnormally with code 9" :output "validation-outvalidation-err" :exit 9 :buffer-live t) :status-events (("finished") ("exited abnormally with code 9")) :buffers-live-after-callback (nil nil))"#
    ]];
    ParityBatchCase::value(
        "callback_workflow_routes_success_and_failure_with_complete_output",
        elisp_form,
        expected,
    )
}

fn named_callback_receives_process_status_and_output_buffer() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (setq neomacs-pfuture-test-callback-record nil)
   (let ((process
          (pfuture-callback ["sh" "-c" "printf 'named-callback-output'"]
            :name "release-named-callback"
            :on-success #'neomacs-pfuture-test-record-callback
            :on-error (setq neomacs-pfuture-test-callback-record
                            'wrong-branch))))
     (neomacs-pfuture-test-wait
      process (lambda () neomacs-pfuture-test-callback-record))
     (list :record neomacs-pfuture-test-callback-record
           :buffer-live-after
           (buffer-live-p (process-get process 'buffer))))))
"###;
    let expected = expect![[
        r#"OK (:record (:status "finished" :exit 0 :output "named-callback-output" :buffer-live t) :buffer-live-after nil)"#
    ]];
    ParityBatchCase::value(
        "named_callback_receives_process_status_and_output_buffer",
        elisp_form,
        expected,
    )
}

fn callback_honors_working_directory_custom_buffer_and_filter() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let* ((root (file-name-as-directory
                 (file-truename (make-temp-file "pfuture-directory-" t))))
          (buffer (get-buffer-create " release-pfuture-custom"))
          record process)
     (unwind-protect
         (progn
           (with-temp-file (expand-file-name "release.txt" root)
             (insert "candidate-v2"))
           (setq process
                 (pfuture-callback
                     ["sh" "-c" "printf '%s|' \"$PWD\"; cat release.txt"]
                   :directory root
                   :name "release-directory-callback"
                   :buffer buffer
                   :filter #'neomacs-pfuture-test-uppercase-filter
                   :on-success
                   (setq record
                         (list :status (string-trim-right status)
                               :output (pfuture-callback-output)))
                   :on-error (setq record 'wrong-branch)))
           (neomacs-pfuture-test-wait process (lambda () record))
           (list
            :record
            (list :status (plist-get record :status)
                  :output
                  (string-replace
                   (upcase (directory-file-name root))
                   "<ROOT>"
                   (plist-get record :output)))
            :buffer-live (buffer-live-p buffer)
            :buffer-output
            (string-replace
             (upcase (directory-file-name root))
             "<ROOT>"
             (substring-no-properties
              (with-current-buffer buffer (buffer-string))))))
       (when (buffer-live-p buffer) (kill-buffer buffer))
       (delete-directory root t)))))
"###;
    let expected = expect![[
        r#"OK (:record (:status "finished" :output "<ROOT>|CANDIDATE-V2") :buffer-live t :buffer-output "<ROOT>|CANDIDATE-V2")"#
    ]];
    ParityBatchCase::value(
        "callback_honors_working_directory_custom_buffer_and_filter",
        elisp_form,
        expected,
    )
}

fn command_start_failure_is_atomic_and_does_not_leave_future_resources() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let* ((before-processes
           (seq-count #'neomacs-pfuture-test-owned-process-p (process-list)))
          signal)
     (setq signal
           (condition-case error-data
               (progn
                 (pfuture-new "neomacs-pfuture-command-that-does-not-exist")
                 'unexpected-success)
             (error
              (list (car error-data)
                    (error-message-string error-data)))))
     (sit-for 0.02)
     (list
      :signal signal
      :owned-process-delta
      (- (seq-count #'neomacs-pfuture-test-owned-process-p (process-list))
         before-processes)
      :output-buffers
      (seq-count
       (lambda (buffer)
         (string-prefix-p " Pfuture-Buffer" (buffer-name buffer)))
       (buffer-list))
      :dummy-buffer-live (buffer-live-p pfuture--dummy-buffer)))))
"###;
    let expected = expect![[
        r#"OK (:signal (file-missing "Searching for program: No such file or directory, neomacs-pfuture-command-that-does-not-exist") :owned-process-delta 0 :output-buffers 1 :dummy-buffer-live t)"#
    ]];
    ParityBatchCase::value(
        "command_start_failure_is_atomic_and_does_not_leave_future_resources",
        elisp_form,
        expected,
    )
}

fn many_completed_futures_retain_results_without_stdout_or_pipe_leaks() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-pfuture-test-with-reset
 (lambda ()
   (let ((futures
          (mapcar
           (lambda (index)
             (pfuture-new
              "sh" "-c" "printf '%s' \"$1\"" "pfuture"
              (number-to-string index)))
           (number-sequence 1 16))))
     (let ((results (mapcar #'pfuture-await-to-finish futures)))
       (sit-for 0.05)
       (list
        :results results
        :cached (mapcar #'pfuture-result futures)
        :statuses (delete-dups (mapcar #'process-status futures))
        :stdout-buffers-live
        (seq-count
         (lambda (future)
           (buffer-live-p (process-get future 'buffer)))
         futures)
        :stderr-processes-live
        (seq-count
         (lambda (future)
           (when-let ((stderr (process-get future 'stderr-process)))
             (process-live-p stderr)))
         futures))))))
"###;
    let expected = expect![[
        r#"OK (:results ("1" "2" "3" "4" "5" "6" "7" "8" "9" "10" "11" "12" "13" "14" "15" "16") :cached ("1" "2" "3" "4" "5" "6" "7" "8" "9" "10" "11" "12" "13" "14" "15" "16") :statuses (exit) :stdout-buffers-live 0 :stderr-processes-live 0)"#
    ]];
    ParityBatchCase::value(
        "many_completed_futures_retain_results_without_stdout_or_pipe_leaks",
        elisp_form,
        expected,
    )
}

#[test]
fn pfuture_package_batch() {
    let cases = vec![
        future_separates_stdout_stderr_and_caches_a_failed_command_result(),
        live_future_exposes_partial_output_before_awaiting_the_final_result(),
        independent_futures_run_concurrently_and_can_be_awaited_in_creation_order(),
        callback_workflow_routes_success_and_failure_with_complete_output(),
        named_callback_receives_process_status_and_output_buffer(),
        callback_honors_working_directory_custom_buffer_and_filter(),
        command_start_failure_is_atomic_and_does_not_leave_future_resources(),
        many_completed_futures_retain_results_without_stdout_or_pipe_leaks(),
    ];
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(PFUTURE_MELPA_PIN, "pfuture.el")
            .expect("prepare revision-pinned Pfuture source below ./tmp")
            .with_prelude(PFUTURE_TEST_PRELUDE)
            .with_timeout(PFUTURE_TEST_TIMEOUT),
        "pfuture-package-batch",
        "Pfuture",
        &cases,
    );
}
