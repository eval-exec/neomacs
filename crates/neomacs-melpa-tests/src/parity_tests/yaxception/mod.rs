use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, YAXCEPTION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const YAXCEPTION_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const YAXCEPTION_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(define-error 'yaxception-parity-api-error
  "Remote API request failed" 'error)
(define-error 'yaxception-parity-timeout
  "Remote API request timed out" 'yaxception-parity-api-error)
(define-error 'yaxception-parity-deployment-error
  "Deployment failed" 'error)

(defun yaxception-parity-summary (err)
  (list
   :raw (yaxception:get-raw err)
   :text (yaxception:get-text err)
   :data (copy-tree (yaxception:get-data err))
   :request-id (yaxception:get-prop err 'request-id)
   :stage (yaxception:get-prop err :stage)
   :missing (yaxception:get-prop err :missing)))

(defun yaxception-parity-stack-leaf ()
  (replace-regexp-in-string " " "" 'yaxception-invalid-payload))

(defun yaxception-parity-stack-service ()
  (yaxception-parity-stack-leaf))

(defun yaxception-parity-stack-controller ()
  (yaxception-parity-stack-service))
"####;

fn yaxception_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YAXCEPTION_MELPA_PIN, "yaxception.el")
        .expect("prepare pinned Yaxception source below ./tmp")
        .with_prelude(YAXCEPTION_TEST_PRELUDE)
        .with_timeout(YAXCEPTION_TEST_TIMEOUT)
}

fn api_timeout_is_caught_by_parent_with_payload_and_cleanup_timeline() -> ParityBatchCase {
    let elisp_form = r####"
(let (events)
  (let ((outcome
         (yaxception:$
           (yaxception:try
             (push 'request-started events)
             (yaxception:throw
               'yaxception-parity-timeout
               :request-id "req-17"
               :stage :download
               :attempt 3)
             (push 'unreachable events))
           (yaxception:catch 'yaxception-parity-api-error err
             (push 'fallback-selected events)
             (list :status 'cached-response
                   :error (yaxception-parity-summary err)
                   :attempt (yaxception:get-prop err :attempt)))
           (yaxception:catch 'error err
             (push 'generic-handler events)
             (yaxception-parity-summary err))
           (yaxception:finally
             (push 'connection-closed events)))))
    (list :outcome outcome :events (nreverse events))))
"####;
    let expect = expect![[
        r####"OK (:outcome (:status cached-response :error (:raw (yaxception-parity-timeout :request-id "req-17" :stage :download :attempt 3) :text "Remote API request timed out: :request-id, \"req-17\", :stage, :download, :attempt, 3" :data (:request-id "req-17" :stage :download :attempt 3) :request-id "req-17" :stage :download :missing nil) :attempt 3) :events (request-started fallback-selected connection-closed))"####
    ]];
    ParityBatchCase::value(
        "api_timeout_is_caught_by_parent_with_payload_and_cleanup_timeline",
        elisp_form,
        expect,
    )
}

fn nested_transaction_rethrow_preserves_data_and_runs_finally_inside_out() -> ParityBatchCase {
    let elisp_form = r####"
(let (events)
  (let ((outcome
         (yaxception:$
           (yaxception:try
             (push 'outer-open events)
             (yaxception:$
               (yaxception:try
                 (push 'inner-open events)
                 (yaxception:throw
                   'yaxception-parity-timeout
                   :request-id "req-29"
                   :stage :commit))
               (yaxception:catch 'yaxception-parity-timeout inner-error
                 (push 'inner-caught events)
                 (yaxception:throw inner-error))
               (yaxception:finally
                 (push 'inner-rollback events))))
           (yaxception:catch 'yaxception-parity-api-error outer-error
             (push 'outer-caught events)
             (list :status 'aborted
                   :error (yaxception-parity-summary outer-error)))
           (yaxception:finally
             (push 'outer-close events)))))
    (list :outcome outcome :events (nreverse events))))
"####;
    let expect = expect![[
        r####"OK (:outcome (:status aborted :error (:raw (yaxception-parity-timeout :request-id "req-29" :stage :commit) :text "Remote API request timed out: :request-id, \"req-29\", :stage, :commit" :data (:request-id "req-29" :stage :commit) :request-id "req-29" :stage :commit :missing nil)) :events (outer-open inner-open inner-caught inner-rollback outer-caught outer-close))"####
    ]];
    ParityBatchCase::value(
        "nested_transaction_rethrow_preserves_data_and_runs_finally_inside_out",
        elisp_form,
        expect,
    )
}

fn success_and_failure_paths_return_business_values_after_finally() -> ParityBatchCase {
    let elisp_form = r####"
(let (events)
  (let ((success
         (yaxception:$
           (yaxception:try
             (push 'success-start events)
             '(:status published :revision "abc123"))
           (yaxception:catch 'error err
             (list :unexpected (yaxception:get-text err)))
           (yaxception:finally
             (push 'success-cleanup events))))
        (failure
         (yaxception:$
           (yaxception:try
             (push 'failure-start events)
             (aref [compile test package] 8))
           (yaxception:catch 'args-out-of-range err
             (push 'range-handler events)
             (list :status 'retry
                   :error (yaxception:get-text err)
                   :raw (yaxception:get-raw err)))
           (yaxception:catch 'error err
             (push 'generic-handler events)
             (list :status 'failed :error (yaxception:get-text err)))
           (yaxception:finally
             (push 'failure-cleanup events)))))
    (list :success success
          :failure failure
          :events (nreverse events))))
"####;
    let expect = expect![[
        r####"OK (:success (:status published :revision "abc123") :failure (:status retry :error "Args out of range: [compile test package], 8" :raw (args-out-of-range [compile test package] 8)) :events (success-start success-cleanup failure-start range-handler failure-cleanup))"####
    ]];
    ParityBatchCase::value(
        "success_and_failure_paths_return_business_values_after_finally",
        elisp_form,
        expect,
    )
}

fn low_level_failure_is_wrapped_with_deployment_context_for_outer_handler() -> ParityBatchCase {
    let elisp_form = r####"
(let (original events)
  (let ((outcome
         (yaxception:$
           (yaxception:try
             (yaxception:$
               (yaxception:try
                 (push 'decode-start events)
                 (aref [header body] 7))
               (yaxception:catch 'args-out-of-range cause
                 (setq original
                       (list :raw (yaxception:get-raw cause)
                             :text (yaxception:get-text cause)))
                 (push 'wrapped events)
                 (yaxception:throw
                   'yaxception-parity-deployment-error
                   :stage :decode
                   :request-id "deploy-4"
                   :cause "decoder index out of range"))
               (yaxception:finally
                 (push 'decoder-closed events))))
           (yaxception:catch 'yaxception-parity-deployment-error wrapped
             (push 'deployment-handler events)
             (yaxception-parity-summary wrapped))
           (yaxception:finally
             (push 'deployment-cleanup events)))))
    (list :original original
          :outcome outcome
          :events (nreverse events))))
"####;
    let expect = expect![[
        r####"OK (:original (:raw (args-out-of-range [header body] 7) :text "Args out of range: [header body], 7") :outcome (:raw (yaxception-parity-deployment-error :stage :decode :request-id "deploy-4" :cause "decoder index out of range") :text "Deployment failed: :stage, :decode, :request-id, \"deploy-4\", :cause, \"decoder index out of range\"" :data (:stage :decode :request-id "deploy-4" :cause "decoder index out of range") :request-id "deploy-4" :stage :decode :missing nil) :events (decode-start wrapped decoder-closed deployment-handler deployment-cleanup))"####
    ]];
    ParityBatchCase::value(
        "low_level_failure_is_wrapped_with_deployment_context_for_outer_handler",
        elisp_form,
        expect,
    )
}

fn stack_report_preserves_named_application_frames_and_filtering() -> ParityBatchCase {
    let elisp_form = r####"
(let (captured)
  (yaxception:$
    (yaxception:try
      (yaxception-parity-stack-controller))
    (yaxception:catch 'wrong-type-argument err
      (setq captured err)))
  (let* ((known
          '("replace-regexp-in-string"
            "yaxception-parity-stack-leaf"
            "yaxception-parity-stack-service"
            "yaxception-parity-stack-controller"))
         (traces
          (cl-remove-if-not
           (lambda (trace) (member (plist-get trace :name) known))
           (yaxception-expose-stack-traces captured))))
    (list
     :text (yaxception:get-text captured)
     :limited (yaxception:get-stack-trace-string captured :limit 4)
     :application-only
     (yaxception:get-stack-trace-string
      captured
      :filter (lambda (name) (member name known)))
     :frames
     (mapcar
      (lambda (trace)
        (list :name (plist-get trace :name)
              :args (plist-get trace :argstr)))
      traces))))
"####;
    let expect = expect![[
        r####"OK (:text "Wrong type argument: sequencep, yaxception-invalid-payload" :limited "Wrong type argument: sequencep, yaxception-invalid-payload\n  at replace-regexp-in-string(\" \" \"\" yaxception-invalid-payload)\n  at yaxception-parity-stack-leaf()\n  at yaxception-parity-stack-service()\n  at yaxception-parity-stack-controller()" :application-only "Wrong type argument: sequencep, yaxception-invalid-payload\n  at replace-regexp-in-string(\" \" \"\" yaxception-invalid-payload)\n  at yaxception-parity-stack-leaf()\n  at yaxception-parity-stack-service()\n  at yaxception-parity-stack-controller()" :frames ((:name "replace-regexp-in-string" :args "\" \" \"\" yaxception-invalid-payload") (:name "yaxception-parity-stack-leaf" :args "") (:name "yaxception-parity-stack-service" :args "") (:name "yaxception-parity-stack-controller" :args "")))"####
    ]];
    ParityBatchCase::value(
        "stack_report_preserves_named_application_frames_and_filtering",
        elisp_form,
        expect,
    )
}

#[test]
fn yaxception_package_batch() {
    let cases = vec![
        api_timeout_is_caught_by_parent_with_payload_and_cleanup_timeline(),
        nested_transaction_rethrow_preserves_data_and_runs_finally_inside_out(),
        success_and_failure_paths_return_business_values_after_finally(),
        low_level_failure_is_wrapped_with_deployment_context_for_outer_handler(),
        stack_report_preserves_named_application_frames_and_filtering(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Yaxception parity test");
    assert_oracle_batch_cases(yaxception_oracle(), test_name, "yaxception_parity", &cases);
}
