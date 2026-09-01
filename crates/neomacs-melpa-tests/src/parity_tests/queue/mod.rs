use std::time::Duration;

use expect_test::expect;

use crate::{CachedPackageOracle, QUEUE_GNU_ELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const QUEUE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const QUEUE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'queue)
(require 'generator)

(defun queue-test-drain (queue)
  (let (items)
    (while (not (queue-empty queue))
      (push (queue-dequeue queue) items))
    (nreverse items)))

(defun queue-test-summary (queue)
  (list
   :length (queue-length queue)
   :first (copy-tree (queue-first queue))
   :last (copy-tree (queue-last queue))
   :all (copy-tree (queue-all queue))))
"##;

fn queue_oracle() -> CachedPackageOracle {
    CachedPackageOracle::new_from_gnu_elpa(QUEUE_GNU_ELPA_PIN, "queue.el")
        .expect("prepare pinned Queue source below ./tmp")
        .with_prelude(QUEUE_TEST_PRELUDE)
        .with_timeout(QUEUE_TEST_TIMEOUT)
}

fn deployment_scheduler_preserves_fifo_order_and_reports_operational_boundaries() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((jobs (make-queue)))
  (queue-enqueue
   jobs '(:id compile :owner build :minutes 4))
  (queue-append
   jobs '(:id unit-tests :owner qa :minutes 7))
  (queue-enqueue
   jobs '(:id package :owner release :minutes 2))
  (let ((before (queue-test-summary jobs))
        (started (queue-dequeue jobs)))
    (queue-enqueue
     jobs '(:id publish :owner release :minutes 1))
    (list
     :before before
     :started started
     :second-id (plist-get (queue-nth jobs 0) :id)
     :third-id (plist-get (queue-nth jobs 1) :id)
     :missing (queue-nth jobs 20)
     :remaining (queue-test-drain jobs)
     :after (queue-test-summary jobs))))
"##;
    let expect = expect![[
        r##"OK (:before (:length 3 :first (:id compile :owner build :minutes 4) :last (:id package :owner release :minutes 2) :all ((:id compile :owner build :minutes 4) (:id unit-tests :owner qa :minutes 7) (:id package :owner release :minutes 2))) :started (:id compile :owner build :minutes 4) :second-id unit-tests :third-id package :missing nil :remaining ((:id unit-tests :owner qa :minutes 7) (:id package :owner release :minutes 2) (:id publish :owner release :minutes 1)) :after (:length 0 :first nil :last nil :all nil))"##
    ]];
    ParityBatchCase::value(
        "deployment_scheduler_preserves_fifo_order_and_reports_operational_boundaries",
        elisp_form,
        expect,
    )
}

fn urgent_work_preempts_normal_work_without_disturbing_existing_fifo_order() -> ParityBatchCase {
    let elisp_form = r##"
(let ((work (queue-create)))
  (dolist (ticket
           '((normal-101 . "refresh docs")
             (normal-102 . "publish package")
             (normal-103 . "notify users")))
    (queue-append work ticket))
  (queue-prepend work '(urgent-500 . "rollback release"))
  (queue-prepend work '(incident-9 . "disable deployment"))
  (let ((dispatch (queue-test-drain work)))
    (list
     :dispatch dispatch
     :ids (mapcar #'car dispatch)
     :descriptions (mapcar #'cdr dispatch)
     :queue-after (queue-test-summary work))))
"##;
    let expect = expect![[
        r##"OK (:dispatch ((incident-9 . "disable deployment") (urgent-500 . "rollback release") (normal-101 . "refresh docs") (normal-102 . "publish package") (normal-103 . "notify users")) :ids (incident-9 urgent-500 normal-101 normal-102 normal-103) :descriptions ("disable deployment" "rollback release" "refresh docs" "publish package" "notify users") :queue-after (:length 0 :first nil :last nil :all nil))"##
    ]];
    ParityBatchCase::value(
        "urgent_work_preempts_normal_work_without_disturbing_existing_fifo_order",
        elisp_form,
        expect,
    )
}

fn single_item_transitions_clear_both_boundaries_and_allow_safe_reuse() -> ParityBatchCase {
    let elisp_form = r##"
(let ((events (queue-create))
      timeline)
  (push (list 'created (queue-test-summary events)
              :dequeue (queue-dequeue events))
        timeline)
  (queue-enqueue events '(:event checkout :revision "abc123"))
  (push (list 'one-item (queue-test-summary events)) timeline)
  (push (list 'consumed (queue-dequeue events)
              :state (queue-test-summary events))
        timeline)
  (queue-enqueue events '(:event rebuild :revision "def456"))
  (queue-clear events)
  (push (list 'cleared (queue-test-summary events)) timeline)
  (queue-prepend events '(:event recovered :revision "def456"))
  (push (list 'reused (queue-test-summary events)
              :consumed (queue-dequeue events)
              :final (queue-test-summary events))
        timeline)
  (nreverse timeline))
"##;
    let expect = expect![[
        r##"OK ((created (:length 0 :first nil :last nil :all nil) :dequeue nil) (one-item (:length 1 :first (:event checkout :revision "abc123") :last (:event checkout :revision "abc123") :all ((:event checkout :revision "abc123")))) (consumed (:event checkout :revision "abc123") :state (:length 0 :first nil :last nil :all nil)) (cleared (:length 0 :first nil :last nil :all nil)) (reused (:length 1 :first (:event recovered :revision "def456") :last (:event recovered :revision "def456") :all ((:event recovered :revision "def456"))) :consumed (:event recovered :revision "def456") :final (:length 0 :first nil :last nil :all nil)))"##
    ]];
    ParityBatchCase::value(
        "single_item_transitions_clear_both_boundaries_and_allow_safe_reuse",
        elisp_form,
        expect,
    )
}

fn copied_run_queue_has_independent_order_but_intentionally_shared_job_payloads() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((build (list :id 'build :state 'ready :attempt 1))
       (tests (list :id 'tests :state 'ready :attempt 1))
       (primary (queue-create)))
  (queue-enqueue primary build)
  (queue-enqueue primary tests)
  (let ((simulation (queue-copy primary)))
    (queue-prepend
     primary (list :id 'incident :state 'urgent :attempt 1))
    (let ((simulated-build (queue-dequeue simulation)))
      (plist-put simulated-build :attempt 2))
    (plist-put (queue-first simulation) :state 'retry)
    (queue-enqueue
     simulation (list :id 'publish :state 'blocked :attempt 0))
    (list
     :primary (queue-test-summary primary)
     :simulation (queue-test-summary simulation)
     :shared-build-attempt (plist-get (queue-nth primary 1) :attempt)
     :shared-test-state (plist-get (queue-last primary) :state)
     :same-test-object (eq (queue-last primary)
                           (queue-first simulation)))))
"##;
    let expect = expect![[
        r##"OK (:primary (:length 3 :first (:id incident :state urgent :attempt 1) :last (:id tests :state retry :attempt 1) :all ((:id incident :state urgent :attempt 1) (:id build :state ready :attempt 2) (:id tests :state retry :attempt 1))) :simulation (:length 2 :first (:id tests :state retry :attempt 1) :last (:id publish :state blocked :attempt 0) :all ((:id tests :state retry :attempt 1) (:id publish :state blocked :attempt 0))) :shared-build-attempt 2 :shared-test-state retry :same-test-object t)"##
    ]];
    ParityBatchCase::value(
        "copied_run_queue_has_independent_order_but_intentionally_shared_job_payloads",
        elisp_form,
        expect,
    )
}

fn iterator_observes_tail_work_added_after_yield_but_not_new_priority_head() -> ParityBatchCase {
    let elisp_form = r##"
(let ((deliveries (queue-create)))
  (queue-enqueue deliveries 'download)
  (queue-enqueue deliveries 'verify)
  (let* ((iterator (queue-iter deliveries))
         (first (iter-next iterator))
         rest
         end-condition)
    (queue-enqueue deliveries 'install)
    (queue-prepend deliveries 'emergency-stop)
    (condition-case error-data
        (while t
          (push (iter-next iterator) rest))
      (iter-end-of-sequence
       (setq end-condition (car error-data))))
    (list
     :first first
     :rest (nreverse rest)
     :termination end-condition
     :queue-unchanged (queue-test-summary deliveries))))
"##;
    let expect = expect![[
        "OK (:first download :rest (verify install) :termination iter-end-of-sequence :queue-unchanged (:length 4 :first emergency-stop :last install :all (emergency-stop download verify install)))"
    ]];
    ParityBatchCase::value(
        "iterator_observes_tail_work_added_after_yield_but_not_new_priority_head",
        elisp_form,
        expect,
    )
}

fn breadth_first_dependency_planner_preserves_levels_and_suppresses_duplicate_work()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((graph
        '((release build test)
          (build compile package)
          (test unit integration)
          (integration package)
          (compile)
          (package)
          (unit)))
       (pending (queue-create))
       (seen (make-hash-table :test #'eq))
       plan
       skipped)
  (queue-enqueue pending '(release . 0))
  (while (not (queue-empty pending))
    (pcase-let* ((`(,task . ,depth) (queue-dequeue pending)))
      (if (gethash task seen)
          (push task skipped)
        (puthash task t seen)
        (push (list task depth) plan)
        (dolist (dependency (cdr (assq task graph)))
          (queue-enqueue pending (cons dependency (1+ depth)))))))
  (list
   :plan (nreverse plan)
   :skipped-duplicates (nreverse skipped)
   :planned-count (hash-table-count seen)
   :pending (queue-test-summary pending)))
"##;
    let expect = expect![[
        "OK (:plan ((release 0) (build 1) (test 1) (compile 2) (package 2) (unit 2) (integration 2)) :skipped-duplicates (package) :planned-count 7 :pending (:length 0 :first nil :last nil :all nil))"
    ]];
    ParityBatchCase::value(
        "breadth_first_dependency_planner_preserves_levels_and_suppresses_duplicate_work",
        elisp_form,
        expect,
    )
}

fn event_loop_processes_retry_and_follow_up_work_added_during_dispatch() -> ParityBatchCase {
    let elisp_form = r##"
(let ((events (queue-create))
      processed
      outcomes)
  (queue-enqueue events '(:id build :attempt 1))
  (queue-enqueue events '(:id docs :attempt 1))
  (while (not (queue-empty events))
    (let* ((event (queue-dequeue events))
           (id (plist-get event :id))
           (attempt (plist-get event :attempt)))
      (push (list id attempt) processed)
      (pcase (cons id attempt)
        (`(build . 1)
         (push '(build retrying) outcomes)
         (queue-prepend events '(:id build :attempt 2)))
        (`(build . 2)
         (push '(build passed) outcomes))
        (`(docs . 1)
         (push '(docs generated) outcomes)
         (queue-enqueue events '(:id publish :attempt 1)))
        (`(publish . 1)
         (push '(publish completed) outcomes)))))
  (list
   :processed (nreverse processed)
   :outcomes (nreverse outcomes)
   :queue-after (queue-test-summary events)))
"##;
    let expect = expect![[
        "OK (:processed ((build 1) (build 2) (docs 1) (publish 1)) :outcomes ((build retrying) (build passed) (docs generated) (publish completed)) :queue-after (:length 0 :first nil :last nil :all nil))"
    ]];
    ParityBatchCase::value(
        "event_loop_processes_retry_and_follow_up_work_added_during_dispatch",
        elisp_form,
        expect,
    )
}

fn retained_live_view_tracks_appends_while_snapshot_and_new_head_semantics_remain_distinct()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((audit (queue-create)))
  (queue-enqueue audit '(created . 10))
  (queue-enqueue audit '(validated . 20))
  (let ((live-view (queue-all audit))
        (snapshot (copy-tree (queue-all audit))))
    (queue-enqueue audit '(published . 30))
    (queue-prepend audit '(incident . 5))
    (list
     :retained-live-view (copy-tree live-view)
     :independent-snapshot (copy-tree snapshot)
     :current-view (copy-tree (queue-all audit))
     :indexes
     (mapcar
      (lambda (index) (copy-tree (queue-nth audit index)))
      '(0 1 2 3 4))
     :boundaries
     (list (copy-tree (queue-first audit))
           (copy-tree (queue-last audit))))))
"##;
    let expect = expect![[
        "OK (:retained-live-view ((created . 10) (validated . 20) (published . 30)) :independent-snapshot ((created . 10) (validated . 20)) :current-view ((incident . 5) (created . 10) (validated . 20) (published . 30)) :indexes ((incident . 5) (created . 10) (validated . 20) (published . 30) nil) :boundaries ((incident . 5) (published . 30)))"
    ]];
    ParityBatchCase::value(
        "retained_live_view_tracks_appends_while_snapshot_and_new_head_semantics_remain_distinct",
        elisp_form,
        expect,
    )
}

#[test]
fn queue_package_batch() {
    let cases = vec![
        deployment_scheduler_preserves_fifo_order_and_reports_operational_boundaries(),
        urgent_work_preempts_normal_work_without_disturbing_existing_fifo_order(),
        single_item_transitions_clear_both_boundaries_and_allow_safe_reuse(),
        copied_run_queue_has_independent_order_but_intentionally_shared_job_payloads(),
        iterator_observes_tail_work_added_after_yield_but_not_new_priority_head(),
        breadth_first_dependency_planner_preserves_levels_and_suppresses_duplicate_work(),
        event_loop_processes_retry_and_follow_up_work_added_during_dispatch(),
        retained_live_view_tracks_appends_while_snapshot_and_new_head_semantics_remain_distinct(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed queue parity test");
    assert_oracle_batch_cases(queue_oracle(), test_name, "queue_parity", &cases);
}
