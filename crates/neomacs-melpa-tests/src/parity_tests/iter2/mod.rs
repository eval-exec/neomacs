use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ITER2_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const ITER2_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ITER2_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'iter2)

(defun iter2-test-drain (iterator &optional responses already-started)
  (let ((started already-started)
        values return-value done)
    (while (not done)
      (condition-case outcome
          (push
           (if started
               (let ((response (pop responses)))
                 (iter2-next iterator response))
             (setq started t)
             (iter2-next iterator))
           values)
        (iter-end-of-sequence
         (setq return-value (cdr outcome)
               done t))))
    (list :values (nreverse values) :return return-value)))

(iter2-defun iter2-test-job-runner (jobs)
  (let ((completed nil)
        (skipped nil)
        (retries 0))
    (dolist (job jobs)
      (let* ((id (plist-get job :id))
             (decision (iter-yield
                        (list :job id :attempt 1
                              :payload (plist-get job :payload)))))
        (when (and (consp decision) (eq (car decision) :retry))
          (setq retries (1+ retries)
                decision
                (iter-yield
                 (list :job id :attempt 2 :reason (cadr decision)))))
        (if (eq decision 'skip)
            (push id skipped)
          (push id completed))))
    (list :completed (nreverse completed)
          :skipped (nreverse skipped)
          :retries retries)))

(iter2-defun iter2-test-page (page rows)
  (dolist (row rows)
    (iter-yield (list :page page :row row)))
  (length rows))

(iter2-defun iter2-test-export (pages)
  (let ((row-count 0))
    (iter-yield '(:export-started csv))
    (dolist (page pages)
      (setq row-count
            (+ row-count
               (iter-yield-from
                (iter2-test-page (car page) (cdr page))))))
    (list :exported row-count)))

(iter2-defun iter2-test-resource-stream (resources logger)
  (unwind-protect
      (progn
        (funcall logger :session-open)
        (dolist (resource resources)
          (unwind-protect
              (progn
                (funcall logger (list :open resource))
                (iter-yield resource)
                (funcall logger (list :processed resource)))
            (funcall logger (list :close resource))))
        :complete)
    (funcall logger :session-close)))

(iter2-defun iter2-test-log-records (contents)
  (with-temp-buffer
    (insert contents)
    (goto-char (point-min))
    (save-match-data
      (while (re-search-forward
              "^\\(INFO\\|WARN\\) order=\\([0-9]+\\) state=\\([a-z]+\\)$"
              nil t)
        (iter-yield
         (list :line (line-number-at-pos)
               :level (match-string-no-properties 1)
               :order (string-to-number (match-string-no-properties 2))
               :state (intern (match-string-no-properties 3)))))))
  :scan-complete)

(iter2-defun iter2-test-deploy-plan (services)
  (catch 'abort
    (let ((deployed nil)
          (retries 0))
      (dolist (service services)
        (let ((decision
               (condition-case problem
                   (iter-yield (list :approve service))
                 (file-error
                  (setq retries (1+ retries))
                  (iter-yield
                   (list :retry service :reason (cadr problem))))
                 (user-error
                  (throw 'abort
                         (list :aborted service :reason (cadr problem)))))))
          (when (eq decision 'approve)
            (push service deployed))))
      (list :deployed (nreverse deployed) :retries retries))))

(iter2-tracing-defun iter2-test-traced-import (records)
  (dolist (record records)
    (iter-yield (list :validated record)))
  :import-complete)
"##;

fn iter2_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ITER2_MELPA_PIN, "iter2.el")
        .expect("prepare pinned iter2 source below ./tmp")
        .with_prelude(ITER2_TEST_PRELUDE)
        .with_timeout(ITER2_TEST_TIMEOUT)
}

fn resumable_job_runner_accepts_feedback_and_keeps_iterator_state_independent() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((jobs '((:id "JOB-417" :payload (:order 417 :amount 1299))
               (:id "JOB-418" :payload (:order 418 :amount 8450))
               (:id "JOB-419" :payload (:order 419 :amount 3200))))
       (primary (iter2-test-job-runner jobs))
       (secondary (iter2-test-job-runner (cdr jobs)))
       interleaved)
  (push (list :primary (iter2-next primary)) interleaved)
  (push (list :secondary (iter-next secondary)) interleaved)
  (push (list :primary (iter2-next primary '(:retry "timeout"))) interleaved)
  (push (list :secondary (iter-next secondary 'approve)) interleaved)
  (copy-tree
   (list
    :interleaved (nreverse interleaved)
    :primary-rest
    (iter2-test-drain primary '(approve approve skip) t)
    :secondary-rest
    (iter2-test-drain secondary '(approve) t))))
"##;
    let expect = expect![[
        r##"OK (:interleaved ((:primary (:job "JOB-417" :attempt 1 :payload (:order 417 :amount 1299))) (:secondary (:job "JOB-418" :attempt 1 :payload (:order 418 :amount 8450))) (:primary (:job "JOB-417" :attempt 2 :reason "timeout")) (:secondary (:job "JOB-419" :attempt 1 :payload (:order 419 :amount 3200)))) :primary-rest (:values ((:job "JOB-418" :attempt 1 :payload (:order 418 :amount 8450)) (:job "JOB-419" :attempt 1 :payload (:order 419 :amount 3200))) :return (:completed ("JOB-417" "JOB-418") :skipped ("JOB-419") :retries 1)) :secondary-rest (:values nil :return (:completed ("JOB-418" "JOB-419") :skipped nil :retries 0)))"##
    ]];
    ParityBatchCase::value(
        "resumable_job_runner_accepts_feedback_and_keeps_iterator_state_independent",
        elisp_form,
        expect,
    )
}

fn composed_export_delegates_pages_and_integrates_with_iter_do_and_cl_loop() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((pages '((1 . (("ORD-417" 1299) ("ORD-418" 8450)))
                (2 . (("ORD-419" 3200)))))
       (iter-do-values nil)
       (iter-do-return
        (iter-do (record (iter2-test-export pages))
          (push record iter-do-values)))
       (loop-values
        (cl-loop for record iter-by (iter2-test-export pages)
                 collect record))
       (lambda-result
        (iter2-test-drain
         (funcall
          (iter2-lambda (rows)
            (dolist (row rows)
              (iter-yield
               (list (car row) (* (cadr row) 2))))
            :priced)
          '(("ORD-417" 1299) ("ORD-419" 3200))))))
  (copy-tree
   (list
    :iter-do (list (nreverse iter-do-values) iter-do-return)
    :cl-loop loop-values
    :lambda lambda-result)))
"##;
    let expect = expect![[
        r##"OK (:iter-do (((:export-started csv) (:page 1 :row ("ORD-417" 1299)) (:page 1 :row ("ORD-418" 8450)) (:page 2 :row ("ORD-419" 3200))) (:exported 3)) :cl-loop ((:export-started csv) (:page 1 :row ("ORD-417" 1299)) (:page 1 :row ("ORD-418" 8450)) (:page 2 :row ("ORD-419" 3200))) :lambda (:values (("ORD-417" 2598) ("ORD-419" 6400)) :return :priced))"##
    ]];
    ParityBatchCase::value(
        "composed_export_delegates_pages_and_integrates_with_iter_do_and_cl_loop",
        elisp_form,
        expect,
    )
}

fn closing_resource_stream_runs_only_the_active_nested_cleanups() -> ParityBatchCase {
    let elisp_form = r##"
(let (early-events complete-events unopened-events)
  (let ((iterator
         (iter2-test-resource-stream
          '(orders inventory billing)
          (lambda (event) (push event early-events)))))
    (list
     (iter2-next iterator)
     (iter2-next iterator)
     (progn (iter-close iterator) :closed)))
  (let ((iterator
         (iter2-test-resource-stream
          '(orders inventory)
          (lambda (event) (push event complete-events)))))
    (setq complete-events
          (cons (iter2-test-drain iterator) complete-events)))
  (let ((iterator
         (iter2-test-resource-stream
          '(unused)
          (lambda (event) (push event unopened-events)))))
    (iter-close iterator))
  (list
   :early (nreverse early-events)
   :complete (cons (car complete-events)
                   (nreverse (cdr complete-events)))
   :unopened unopened-events))
"##;
    let expect = expect![[
        r##"OK (:early (:session-open (:open orders) (:processed orders) (:close orders) (:open inventory) (:close inventory) :session-close) :complete ((:values (orders inventory) :return :complete) :session-open (:open orders) (:processed orders) (:close orders) (:open inventory) (:processed inventory) (:close inventory) :session-close) :unopened nil)"##
    ]];
    ParityBatchCase::value(
        "closing_resource_stream_runs_only_the_active_nested_cleanups",
        elisp_form,
        expect,
    )
}

fn log_scanner_preserves_generator_buffer_point_and_callers_match_data() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert "caller-99")
  (goto-char (point-min))
  (re-search-forward "caller-\\([0-9]+\\)")
  (let* ((caller (current-buffer))
         (iterator
          (iter2-test-log-records
           (concat
            "INFO order=417 state=queued\n"
            "debug: heartbeat\n"
            "WARN order=418 state=retry\n"
            "INFO order=419 state=complete\n")))
         observations done return-value)
    (while (not done)
      (condition-case outcome
          (let ((record (iter2-next iterator)))
            (push
             (list record
                   (eq (current-buffer) caller)
                   (match-string-no-properties 1))
             observations)
            (goto-char (point-min))
            (re-search-forward "caller-\\([0-9]+\\)"))
        (iter-end-of-sequence
         (setq return-value (cdr outcome)
               done t))))
    (list
     :records (nreverse observations)
     :return return-value
     :caller (list (buffer-string) (point) (match-string-no-properties 1)))))
"##;
    let expect = expect![[
        r##"OK (:records (((:line 1 :level "INFO" :order 417 :state queued) t "99") ((:line 3 :level "WARN" :order 418 :state retry) t "99") ((:line 4 :level "INFO" :order 419 :state complete) t "99")) :return :scan-complete :caller ("caller-99" 10 "99"))"##
    ]];
    ParityBatchCase::value(
        "log_scanner_preserves_generator_buffer_point_and_callers_match_data",
        elisp_form,
        expect,
    )
}

fn injected_failures_retry_or_abort_inside_the_suspended_deployment() -> ParityBatchCase {
    let elisp_form = r##"
(let ((retrying (iter2-test-deploy-plan '(api worker)))
      (aborting (iter2-test-deploy-plan '(api worker)))
      retry-events abort-events retry-return abort-return)
  (push (iter2-next retrying) retry-events)
  (push (iter2-next retrying (signal 'file-error '("network unavailable")))
        retry-events)
  (push (iter2-next retrying 'approve) retry-events)
  (condition-case outcome
      (iter2-next retrying 'approve)
    (iter-end-of-sequence (setq retry-return (cdr outcome))))
  (push (iter2-next aborting) abort-events)
  (condition-case outcome
      (iter2-next aborting (user-error "change window closed"))
    (iter-end-of-sequence (setq abort-return (cdr outcome))))
  (list
   :retry (list (nreverse retry-events) retry-return)
   :abort (list (nreverse abort-events) abort-return)))
"##;
    let expect = expect![[
        r##"OK (:retry (((:approve api) (:retry api :reason "network unavailable") (:approve worker)) (:deployed (api worker) :retries 1)) :abort (((:approve api)) (:aborted api :reason "change window closed")))"##
    ]];
    ParityBatchCase::value(
        "injected_failures_retry_or_abort_inside_the_suspended_deployment",
        elisp_form,
        expect,
    )
}

fn tracing_generators_report_yields_without_changing_results() -> ParityBatchCase {
    let elisp_form = r##"
(let (trace-yields (trace-invocations 0))
  (let* ((iter2-tracing-function
          (lambda (format-string &rest arguments)
            (cond
             ((string-match-p "invoking" format-string)
              (setq trace-invocations (1+ trace-invocations)))
             ((string-match-p "yielding" format-string)
              (push (list :yield (copy-tree (nth 1 arguments)))
                    trace-yields)))))
         (named
          (iter2-test-drain
           (iter2-test-traced-import '("ORD-417" "ORD-418"))
           '(accepted accepted)))
         (anonymous
          (iter2-test-drain
           (funcall
            (iter2-tracing-lambda (values)
              (dolist (value values)
                (iter-yield (upcase value)))
              :normalized)
            '("queued" "complete"))
           '(continue continue))))
    (copy-tree
     (list
      :named named
      :anonymous anonymous
      :responses '(accepted accepted continue continue)
      :trace (list :invocations trace-invocations
                   :yields (nreverse trace-yields))))))
"##;
    let expect = expect![[
        r##"OK (:named (:values ((:validated "ORD-417") (:validated "ORD-418")) :return :import-complete) :anonymous (:values ("QUEUED" "COMPLETE") :return :normalized) :responses (accepted accepted continue continue) :trace (:invocations 20 :yields ((:yield (:validated "ORD-417")) (:yield (:validated "ORD-418")) (:yield "QUEUED") (:yield "COMPLETE"))))"##
    ]];
    ParityBatchCase::value(
        "tracing_generators_report_yields_without_changing_results",
        elisp_form,
        expect,
    )
}

#[test]
fn iter2_package_batch() {
    let cases = vec![
        resumable_job_runner_accepts_feedback_and_keeps_iterator_state_independent(),
        composed_export_delegates_pages_and_integrates_with_iter_do_and_cl_loop(),
        closing_resource_stream_runs_only_the_active_nested_cleanups(),
        log_scanner_preserves_generator_buffer_point_and_callers_match_data(),
        injected_failures_retry_or_abort_inside_the_suspended_deployment(),
        tracing_generators_report_yields_without_changing_results(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed iter2 parity test");
    assert_oracle_batch_cases(iter2_oracle(), test_name, "iter2_parity", &cases);
}
