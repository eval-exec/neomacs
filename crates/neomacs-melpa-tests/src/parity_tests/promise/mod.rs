use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PROMISE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PROMISE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PROMISE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'promise)

(promise-rejection-tracking-disable)

(defvar promise-test-jobs nil)

(defun promise-test-asap (task)
  (setq promise-test-jobs
        (append promise-test-jobs (list task)))
  task)

(defun promise-test-drain ()
  (let ((count 0))
    (while promise-test-jobs
      (when (> (cl-incf count) 1000)
        (error "Promise test scheduler exceeded 1000 jobs"))
      (let ((task (pop promise-test-jobs)))
        (funcall task)))
    count))

(defun promise-test-state-name (state)
  (aref [pending fulfilled rejected adopted] state))

(defun promise-test-summary (promise)
  (let ((terminal promise))
    (while (= (promise-_state terminal) 3)
      (setq terminal (promise-_value terminal)))
    (list
     :state
     (promise-test-state-name
      (promise-_state promise))
     :settled-state
     (promise-test-state-name
      (promise-_state terminal))
     :value
     (copy-tree (promise-_value terminal))
     :deferred-state
     (promise-_deferred-state promise))))
"##;

fn promise_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PROMISE_MELPA_PIN, "promise.el")
        .expect("prepare pinned promise source below ./tmp")
        .with_prelude(PROMISE_TEST_PRELUDE)
        .with_timeout(PROMISE_TEST_TIMEOUT)
}

fn release_pipeline_chains_transforms_adopts_promises_and_runs_finally_before_publish()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((promise-test-jobs nil)
      events)
  (cl-letf (((symbol-function 'promise--asap)
             #'promise-test-asap))
    (let* ((source
            (promise-resolve
             '(:artifact "neomacs"
               :version 41
               :channel staging)))
           (pipeline
            (promise-chain source
              (thena
               (push
                (list 'validated
                      (plist-get result :version))
                events)
               (plist-put
                (copy-tree result)
                :validated t))
              (then
               (lambda (artifact)
                 (push
                  (list 'packaged
                        (plist-get artifact :artifact))
                  events)
                 (promise-resolve
                  (append artifact
                          '(:archive
                            "neomacs-linux.tar.zst")))))
              (finally
               (lambda ()
                 (push 'cleanup events)
                 'cleanup-complete))
              (thena
               (push
                (list 'published
                      (plist-get result :channel))
                events)
               result)))
           (before (promise-test-summary pipeline))
           (jobs (promise-test-drain)))
      (list
       :before before
       :after (promise-test-summary pipeline)
       :events (nreverse events)
       :jobs jobs))))
"##;
    let expect = expect![[
        r##"OK (:before (:state pending :settled-state pending :value nil :deferred-state 0) :after (:state fulfilled :settled-state fulfilled :value (:artifact "neomacs" :version 41 :channel staging :validated t :archive "neomacs-linux.tar.zst") :deferred-state 0) :events ((validated 41) (packaged "neomacs") cleanup (published staging)) :jobs 5)"##
    ]];
    ParityBatchCase::value(
        "release_pipeline_chains_transforms_adopts_promises_and_runs_finally_before_publish",
        elisp_form,
        expect,
    )
}

fn resolver_settles_once_self_resolution_rejects_and_thrown_callbacks_are_recoverable()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((promise-test-jobs nil)
      events
      self-resolve)
  (cl-letf (((symbol-function 'promise--asap)
             #'promise-test-asap))
    (let* ((first-wins
            (promise-new
             (lambda (resolve reject)
               (push 'resolver-enter events)
               (funcall resolve "artifact-ready")
               (funcall reject "late-rejection")
               (error "late-error"))))
           (recovered
            (promise-chain first-wins
              (thena
               (push (list 'then result) events)
               (error "deployment exploded"))
              (catcha
               (push
                (list 'caught
                      (error-message-string reason))
                events)
               'rollback-complete)
              (finally
               (lambda ()
                 (push 'release-lock events)))))
           (self
            (promise-new
             (lambda (resolve _reject)
               (setq self-resolve resolve)))))
      (funcall self-resolve self)
      (let ((before
             (list
              :first (promise-test-summary first-wins)
              :recovered (promise-test-summary recovered)
              :self (promise-test-summary self))))
        (promise-test-drain)
        (list
         :before before
         :after
         (list
          :first (promise-test-summary first-wins)
          :recovered (promise-test-summary recovered)
          :self (promise-test-summary self))
         :events (nreverse events))))))
"##;
    let expect = expect![[
        r##"OK (:before (:first (:state fulfilled :settled-state fulfilled :value "artifact-ready" :deferred-state 0) :recovered (:state pending :settled-state pending :value nil :deferred-state 0) :self (:state rejected :settled-state rejected :value (wrong-type-argument "A promise cannot be resolved with itself.") :deferred-state 0)) :after (:first (:state fulfilled :settled-state fulfilled :value "artifact-ready" :deferred-state 0) :recovered (:state adopted :settled-state fulfilled :value rollback-complete :deferred-state 0) :self (:state rejected :settled-state rejected :value (wrong-type-argument "A promise cannot be resolved with itself.") :deferred-state 0)) :events (resolver-enter (then "artifact-ready") (caught "deployment exploded") release-lock))"##
    ]];
    ParityBatchCase::value(
        "resolver_settles_once_self_resolution_rejects_and_thrown_callbacks_are_recoverable",
        elisp_form,
        expect,
    )
}

fn all_and_race_coordinate_mixed_values_pending_promises_and_rejections() -> ParityBatchCase {
    let elisp_form = r##"
(let ((promise-test-jobs nil)
      resolve-build
      reject-tests
      resolve-docs)
  (cl-letf (((symbol-function 'promise--asap)
             #'promise-test-asap))
    (let* ((build
            (promise-new
             (lambda (resolve _reject)
               (setq resolve-build resolve))))
           (tests
            (promise-new
             (lambda (_resolve reject)
               (setq reject-tests reject))))
           (docs
            (promise-new
             (lambda (resolve _reject)
               (setq resolve-docs resolve))))
           (all-success
            (promise-all
             (vector build "static" docs)))
           (all-failure
            (promise-all
             (vector build tests docs)))
           (race
            (promise-race
             (vector docs build)))
           (empty (promise-all []))
           timeline)
      (push
       (list 'initial
             (promise-test-summary all-success)
             (promise-test-summary race))
       timeline)
      (funcall resolve-docs "docs-ready")
      (promise-test-drain)
      (push
       (list 'docs
             (promise-test-summary all-success)
             (promise-test-summary race))
       timeline)
      (funcall reject-tests
               '(:suite integration
                 :status failed))
      (promise-test-drain)
      (funcall resolve-build "binary-ready")
      (promise-test-drain)
      (push
       (list 'complete
             (promise-test-summary all-success)
             (promise-test-summary all-failure)
             (promise-test-summary race))
       timeline)
      (list
       :timeline (nreverse timeline)
       :empty (promise-test-summary empty)))))
"##;
    let expect = expect![[
        r##"OK (:timeline ((initial (:state pending :settled-state pending :value nil :deferred-state 0) (:state pending :settled-state pending :value nil :deferred-state 0)) (docs (:state pending :settled-state pending :value nil :deferred-state 0) (:state fulfilled :settled-state fulfilled :value "docs-ready" :deferred-state 0)) (complete (:state fulfilled :settled-state fulfilled :value ["binary-ready" "static" "docs-ready"] :deferred-state 0) (:state rejected :settled-state rejected :value (:suite integration :status failed) :deferred-state 0) (:state fulfilled :settled-state fulfilled :value "docs-ready" :deferred-state 0))) :empty (:state fulfilled :settled-state fulfilled :value [] :deferred-state 0))"##
    ]];
    ParityBatchCase::value(
        "all_and_race_coordinate_mixed_values_pending_promises_and_rejections",
        elisp_form,
        expect,
    )
}

fn foreign_thenables_are_assimilated_and_failures_from_their_then_method_reject() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (defclass promise-test-thenable ()
    ((value :initarg :value)
     (failure :initarg :failure :initform nil)))

  (cl-defmethod promise-then
    ((this promise-test-thenable)
     &optional on-fulfilled on-rejected)
    (let ((failure (oref this failure)))
      (if failure
          (funcall on-rejected failure)
        (funcall on-fulfilled (oref this value)))))

  (let ((promise-test-jobs nil))
    (cl-letf (((symbol-function 'promise--asap)
               #'promise-test-asap))
      (let* ((foreign-success
              (make-instance
               'promise-test-thenable
               :value
               '(:revision "abc123"
                 :verified t)))
             (foreign-failure
              (make-instance
               'promise-test-thenable
               :value nil
               :failure
               '(:service registry
                 :status unavailable)))
             (success
              (promise-resolve foreign-success))
             (failure
              (promise-resolve foreign-failure))
             (mapped
              (promise-then
               success
               (lambda (artifact)
                 (plist-put
                  (copy-tree artifact)
                  :promoted t))))
             (recovered
              (promise-catch
               failure
               (lambda (reason)
                 (list
                  :fallback "cached-index"
                  :cause reason)))))
        (promise-test-drain)
        (list
         :success (promise-test-summary success)
         :failure (promise-test-summary failure)
         :mapped (promise-test-summary mapped)
         :recovered (promise-test-summary recovered)
         :types
         (list
          (promise--type-of foreign-success)
          (promise--type-of success)))))))
"##;
    let expect = expect![[
        r##"OK (:success (:state fulfilled :settled-state fulfilled :value (:revision "abc123" :verified t) :deferred-state 0) :failure (:state rejected :settled-state rejected :value (:service registry :status unavailable) :deferred-state 0) :mapped (:state fulfilled :settled-state fulfilled :value (:revision "abc123" :verified t :promoted t) :deferred-state 0) :recovered (:state fulfilled :settled-state fulfilled :value (:fallback "cached-index" :cause (:service registry :status unavailable)) :deferred-state 0) :types (promise-test-thenable promise-class))"##
    ]];
    ParityBatchCase::value(
        "foreign_thenables_are_assimilated_and_failures_from_their_then_method_reject",
        elisp_form,
        expect,
    )
}

fn timer_utilities_schedule_exact_callbacks_and_propagate_values_or_timeout_reasons()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((promise-test-jobs nil)
      scheduled
      (sequence 0))
  (cl-letf
      (((symbol-function 'promise--asap)
        #'promise-test-asap)
       ((symbol-function 'run-at-time)
        (lambda (time repeat function &rest arguments)
          (let ((timer
                 (list 'timer
                       (cl-incf sequence)
                       time repeat function arguments)))
            (setq scheduled
                  (append scheduled (list timer)))
            timer))))
    (let* ((computed
            (promise:run-at-time
             0.25
             (lambda (artifact revision)
               (list :artifact artifact
                     :revision revision))
             "neomacs" 42))
           (delayed
            (promise:delay 1.5
                           '(:phase publish)))
           (timeout
            (promise:time-out
             3 '(:phase upload
                 :reason deadline))))
      (let ((before
             (mapcar #'promise-test-summary
                     (list computed delayed timeout))))
        (dolist (timer scheduled)
          (apply (nth 4 timer) (nth 5 timer)))
        (promise-test-drain)
        (list
         :scheduled
         (mapcar
          (lambda (timer)
            (list
             :id (nth 1 timer)
             :time (nth 2 timer)
             :repeat (nth 3 timer)
             :arguments (nth 5 timer)))
          scheduled)
         :before before
         :after
         (mapcar #'promise-test-summary
                 (list computed delayed timeout)))))))
"##;
    let expect = expect![[
        r##"OK (:scheduled ((:id 1 :time 0.25 :repeat nil :arguments nil) (:id 2 :time 1.5 :repeat nil :arguments nil) (:id 3 :time 3 :repeat nil :arguments nil)) :before ((:state pending :settled-state pending :value nil :deferred-state 0) (:state pending :settled-state pending :value nil :deferred-state 0) (:state pending :settled-state pending :value nil :deferred-state 0)) :after ((:state fulfilled :settled-state fulfilled :value (:artifact "neomacs" :revision 42) :deferred-state 0) (:state fulfilled :settled-state fulfilled :value (:phase publish) :deferred-state 0) (:state rejected :settled-state rejected :value (:phase upload :reason deadline) :deferred-state 0)))"##
    ]];
    ParityBatchCase::value(
        "timer_utilities_schedule_exact_callbacks_and_propagate_values_or_timeout_reasons",
        elisp_form,
        expect,
    )
}

fn rejection_tracking_reports_late_handlers_and_cancels_reports_for_early_handlers()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((promise-test-jobs nil)
      scheduled
      cancelled
      events
      options
      (sequence 0))
  (cl-letf
      (((symbol-function 'promise--asap)
        #'promise-test-asap)
       ((symbol-function 'run-at-time)
        (lambda (time repeat function &rest arguments)
          (let ((timer
                 (list 'timer
                       (cl-incf sequence)
                       time repeat function arguments)))
            (setq scheduled
                  (append scheduled (list timer)))
            timer)))
       ((symbol-function 'cancel-timer)
        (lambda (timer)
          (push (list (nth 1 timer)
                      (nth 2 timer))
                cancelled))))
    (setq
     options
     (list
      (cons 'all-rejections t)
      (cons
       'on-unhandled
       (lambda (id error)
         (push
          (list 'unhandled id error)
          events)))
      (cons
       'on-handled
       (lambda (id error)
         (push
          (list 'handled id error)
          events)))))
    (unwind-protect
        (let (late early)
          (promise-rejection-tracking-enable options)
          (setq early
                (promise-reject
                 '(error "transient")))
          (promise-catch
           early (lambda (_reason) 'recovered))
          (promise-test-drain)

          (setq late
                (promise-reject
                 '(:deployment release
                   :reason denied)))
          (let ((late-timer (nth 1 scheduled)))
            (apply
             (nth 4 late-timer)
             (nth 5 late-timer)))
          (promise-catch
           late (lambda (_reason) 'acknowledged))
          (promise-test-drain)

          (list
           :events
           (mapcar #'copy-tree
                   (nreverse events))
           :scheduled
           (mapcar
            (lambda (timer)
              (list
               :id (nth 1 timer)
               :delay (nth 2 timer)))
            scheduled)
           :cancelled (nreverse cancelled)
           :late (promise-test-summary late)
           :early (promise-test-summary early)))
      (promise-rejection-tracking-disable))))
"##;
    let expect = expect![[
        r##"OK (:events ((unhandled 0 (:deployment release :reason denied)) (handled 0 (:deployment release :reason denied))) :scheduled ((:id 1 :delay 2) (:id 2 :delay 2)) :cancelled ((1 2)) :late (:state rejected :settled-state rejected :value (:deployment release :reason denied) :deferred-state 0) :early (:state rejected :settled-state rejected :value (error "transient") :deferred-state 0))"##
    ]];
    ParityBatchCase::value(
        "rejection_tracking_reports_late_handlers_and_cancels_reports_for_early_handlers",
        elisp_form,
        expect,
    )
}

fn bounded_workers_limit_active_jobs_preserve_result_order_and_collect_failures() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((promise-test-jobs nil))
  (cl-letf (((symbol-function 'promise--asap)
             #'promise-test-asap))
    (cl-labels
        ((run-workflow
          (collect-failures)
          (let ((active 0)
                (maximum-active 0)
                pending
                started
                finished)
            (let ((aggregate
                   (funcall
                    (if collect-failures
                        #'promise-concurrent-no-reject-immidiately
                      #'promise-concurrent)
                    2 5
                    (lambda (index)
                      (push index started)
                      (setq active (1+ active)
                            maximum-active
                            (max maximum-active active))
                      (promise-new
                       (lambda (resolve reject)
                         (push
                          (list index resolve reject)
                          pending)))))))
              (while (= (promise-_state aggregate) 0)
                (let* ((job
                        (car
                         (sort
                          (copy-sequence pending)
                          (lambda (left right)
                            (< (car left) (car right))))))
                       (index (nth 0 job)))
                  (setq pending (delq job pending)
                        active (1- active))
                  (push index finished)
                  (if (and collect-failures
                           (memq index '(1 3)))
                      (funcall
                       (nth 2 job)
                       (list :job index
                             :reason 'failed))
                    (funcall
                     (nth 1 job)
                     (list :job index
                           :artifact
                           (format "part-%d" index))))
                  (promise-test-drain)))
              (list
               :started (nreverse started)
               :finished (nreverse finished)
               :maximum-active maximum-active
               :aggregate
               (promise-test-summary aggregate))))))
      (list
       :successful (run-workflow nil)
       :collecting-failures
       (run-workflow t)))))
"##;
    let expect = expect![[
        r##"OK (:successful (:started (0 1 2 3 4) :finished (0 1 2 3 4) :maximum-active 2 :aggregate (:state fulfilled :settled-state fulfilled :value [(:job 0 :artifact "part-0") (:job 1 :artifact "part-1") (:job 2 :artifact "part-2") (:job 3 :artifact "part-3") (:job 4 :artifact "part-4")] :deferred-state 0)) :collecting-failures (:started (0 1 2 3 4) :finished (0 1 2 3 4) :maximum-active 2 :aggregate (:state adopted :settled-state rejected :value ([(:job 0 :artifact "part-0") nil (:job 2 :artifact "part-2") nil (:job 4 :artifact "part-4")] ((3 (:job 3 :reason failed)) (1 (:job 1 :reason failed)))) :deferred-state 0)))"##
    ]];
    ParityBatchCase::value(
        "bounded_workers_limit_active_jobs_preserve_result_order_and_collect_failures",
        elisp_form,
        expect,
    )
}

fn shell_commands_resolve_stdout_and_reject_with_exit_event_through_the_real_process_loop()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((success
        (promise-wait
         10
         (promise:make-shell-command
          "printf 'artifact-ready\n'; printf 'notice\n' >&2")))
       (failure
        (promise-wait
         10
         (promise:make-shell-command
          "printf 'partial\n'; printf 'fatal\n' >&2; exit 7"))))
  (list
   :success (promise-test-summary success)
   :success-value (promise-wait-value success)
   :failure (promise-test-summary failure)
   :failure-value
   (condition-case error-data
       (promise-wait-value failure)
     (error error-data))))
"##;
    let expect = expect![[
        r##"OK (:success (:state fulfilled :settled-state fulfilled :value (:fullfilled "artifact-ready\n") :deferred-state 0) :success-value "artifact-ready\n" :failure (:state rejected :settled-state rejected :value (:rejected "exited abnormally with code 7\n") :deferred-state 0) :failure-value (error "Rejected: \"exited abnormally with code 7\\n\""))"##
    ]];
    ParityBatchCase::value(
        "shell_commands_resolve_stdout_and_reject_with_exit_event_through_the_real_process_loop",
        elisp_form,
        expect,
    )
}

#[test]
fn promise_package_batch() {
    let cases = vec![
        release_pipeline_chains_transforms_adopts_promises_and_runs_finally_before_publish(),
        resolver_settles_once_self_resolution_rejects_and_thrown_callbacks_are_recoverable(),
        all_and_race_coordinate_mixed_values_pending_promises_and_rejections(),
        foreign_thenables_are_assimilated_and_failures_from_their_then_method_reject(),
        timer_utilities_schedule_exact_callbacks_and_propagate_values_or_timeout_reasons(),
        rejection_tracking_reports_late_handlers_and_cancels_reports_for_early_handlers(),
        bounded_workers_limit_active_jobs_preserve_result_order_and_collect_failures(),
        shell_commands_resolve_stdout_and_reject_with_exit_event_through_the_real_process_loop(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed promise parity test");
    assert_oracle_batch_cases(promise_oracle(), test_name, "promise_parity", &cases);
}
