use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, HT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ht)

(defun ht-test-key-less-p (left right)
  (string< (prin1-to-string left) (prin1-to-string right)))

(defun ht-test-normalize (value)
  (if (ht-p value)
      (sort
       (ht-map
        (lambda (key item)
          (list key (ht-test-normalize item)))
        value)
       (lambda (left right)
         (ht-test-key-less-p (car left) (car right))))
    (copy-tree value)))

(defun ht-test-case-fold-equal (left right)
  (and (stringp left)
       (stringp right)
       (string-equal (downcase left) (downcase right))))

(defun ht-test-case-fold-hash (key)
  (sxhash-equal (downcase key)))

(define-hash-table-test
  'ht-test-case-fold
  #'ht-test-case-fold-equal
  #'ht-test-case-fold-hash)
"##;

fn ht_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HT_MELPA_PIN, "ht.el")
        .expect("prepare pinned ht source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency")
        .with_prelude(HT_TEST_PRELUDE)
        .with_timeout(HT_TEST_TIMEOUT)
}

fn layered_configuration_merge_preserves_inputs_and_last_writer_precedence() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((defaults
        (ht<-plist
         '(:theme dark :timeout 30 :retry 2 :region "east")))
       (project
        (ht<-alist
         '((:timeout . 45)
           (:features . (audit tracing))
           (:timeout . 99))))
       (user
        (ht (:theme 'light)
            (:retry 5)
            (:notify nil)))
       (merged (ht-merge defaults project user)))
  (list
   :defaults (ht-test-normalize defaults)
   :project (ht-test-normalize project)
   :user (ht-test-normalize user)
   :merged (ht-test-normalize merged)
   :lookups
   (list (ht-get merged :timeout)
         (ht-get merged :retry)
         (ht-get merged :missing 'inherit)
         (ht-contains-p merged :notify)
         (ht-contains-p merged :missing))
   :sizes (mapcar #'ht-size (list defaults project user merged))))
"##;
    let expect = expect![[
        r##"OK (:defaults ((:region "east") (:retry 2) (:theme dark) (:timeout 30)) :project ((:features (audit tracing)) (:timeout 45)) :user ((:notify nil) (:retry 5) (:theme light)) :merged ((:features (audit tracing)) (:notify nil) (:region "east") (:retry 5) (:theme light) (:timeout 45)) :lookups (45 5 inherit t nil) :sizes (4 2 3 6))"##
    ]];
    ParityBatchCase::value(
        "layered_configuration_merge_preserves_inputs_and_last_writer_precedence",
        elisp_form,
        expect,
    )
}

fn nested_service_registry_supports_deep_lookup_assignment_and_counter_updates() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((registry
        (ht
         ("orders"
          (ht (:owner "commerce")
              (:endpoint "/v1/orders")
              (:counters (ht ('success 10) ('failure 2)))))
         ("inventory"
          (ht (:owner "warehouse")
              (:endpoint "/v1/stock")
              (:counters (ht ('success 7)))))))
       (order-counters (ht-get* registry "orders" :counters))
       (owner-assignment
        (setf (ht-get* registry "orders" :owner) "platform")))
  (ht-update-with! order-counters 'success (lambda (count) (+ count 3)))
  (ht-update-with! order-counters 'retries #'1+ 0)
  (ht-update-with! order-counters 'not-created #'1+)
  (ht-set! (ht-get registry "orders") :maintenance nil)
  (list
   :registry (ht-test-normalize registry)
   :assignment owner-assignment
   :deep-lookups
   (list (ht-get* registry "orders" :owner)
         (ht-get* registry "orders" :counters 'success)
         (ht-get* registry "orders" :counters 'retries)
         (ht-get* registry "inventory" :counters 'failure))
   :presence
   (list (ht-contains? (ht-get registry "orders") :maintenance)
         (ht-contains? order-counters 'not-created))))
"##;
    let expect = expect![[
        r##"OK (:registry (("inventory" ((:counters ((success 7))) (:endpoint "/v1/stock") (:owner "warehouse"))) ("orders" ((:counters ((failure 2) (retries 1) (success 13))) (:endpoint "/v1/orders") (:maintenance nil) (:owner "platform")))) :assignment "platform" :deep-lookups ("platform" 13 1 nil) :presence (t nil))"##
    ]];
    ParityBatchCase::value(
        "nested_service_registry_supports_deep_lookup_assignment_and_counter_updates",
        elisp_form,
        expect,
    )
}

fn job_pipeline_selects_transforms_and_prunes_structured_records() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((jobs
        (ht
         ("job-417" '(:status queued :attempts 1 :duration 0))
         ("job-418" '(:status running :attempts 2 :duration 38))
         ("job-419" '(:status failed :attempts 3 :duration 91))
         ("job-420" '(:status passed :attempts 1 :duration 24))
         ("job-421" '(:status queued :attempts 4 :duration 0))))
       (runnable
        (ht-select
         (lambda (_id job)
           (and (eq (plist-get job :status) 'queued)
                (< (plist-get job :attempts) 3)))
         jobs))
       (not-failed
        (ht-reject
         (lambda (_id job) (eq (plist-get job :status) 'failed))
         jobs))
       (dashboard (ht-select-keys jobs '("job-420" "job-417" "missing")))
       (pending (ht-copy jobs))
       (duration-total 0)
       (status-counts (ht-create)))
  (ht-each
   (lambda (_id job)
     (setq duration-total (+ duration-total (plist-get job :duration))))
   jobs)
  (ht-aeach
   (ht-update-with! status-counts (plist-get value :status) #'1+ 0)
   jobs)
  (ht-reject!
   (lambda (_id job) (eq (plist-get job :status) 'passed))
   pending)
  (list
   :runnable (ht-test-normalize runnable)
   :not-failed (ht-test-normalize not-failed)
   :dashboard (ht-test-normalize dashboard)
   :scores
   (sort
    (ht-map
     (lambda (id job)
       (list id (+ (* 100 (plist-get job :attempts))
                   (plist-get job :duration))))
     jobs)
    (lambda (left right) (string< (car left) (car right))))
   :failed
   (ht-find
    (lambda (id _job) (string= id "job-419"))
    jobs)
   :totals (list duration-total (ht-test-normalize status-counts))
   :pending (ht-test-normalize pending)))
"##;
    let expect = expect![[
        r##"OK (:runnable (("job-417" (:status queued :attempts 1 :duration 0))) :not-failed (("job-417" (:status queued :attempts 1 :duration 0)) ("job-418" (:status running :attempts 2 :duration 38)) ("job-420" (:status passed :attempts 1 :duration 24)) ("job-421" (:status queued :attempts 4 :duration 0))) :dashboard (("job-417" (:status queued :attempts 1 :duration 0)) ("job-420" (:status passed :attempts 1 :duration 24))) :scores (("job-417" 100) ("job-418" 238) ("job-419" 391) ("job-420" 124) ("job-421" 400)) :failed ("job-419" (:status failed :attempts 3 :duration 91)) :totals (153 ((failed 1) (passed 1) (queued 2) (running 1))) :pending (("job-417" (:status queued :attempts 1 :duration 0)) ("job-418" (:status running :attempts 2 :duration 38)) ("job-419" (:status failed :attempts 3 :duration 91)) ("job-421" (:status queued :attempts 4 :duration 0))))"##
    ]];
    ParityBatchCase::value(
        "job_pipeline_selects_transforms_and_prunes_structured_records",
        elisp_form,
        expect,
    )
}

fn case_folded_registry_uses_custom_hashing_for_alias_updates_and_removal() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((registry (ht-create 'ht-test-case-fold))
       (set-results
        (list
         (ht-set! registry "API" '(:port 443 :healthy t))
         (ht-set! registry "Worker" '(:port 9443 :healthy nil))
         (ht-set! registry "api" '(:port 8443 :healthy t))))
       (copied (ht-copy registry))
       (remove-result (ht-remove! copied "WORKER"))
       (clear-result (ht-clear! copied)))
  (list
   :test (hash-table-test registry)
   :registry (ht-test-normalize registry)
   :aliases
   (append
    (mapcar #'copy-tree
            (list (ht-get registry "api")
                  (ht-get registry "API")
                  (ht-get registry "worker")))
    (list (ht-size registry)))
   :mutation-results (list set-results remove-result clear-result)
   :cleared (list (ht-empty-p copied) (ht-size copied))))
"##;
    let expect = expect![[
        r##"OK (:test ht-test-case-fold :registry (("API" (:port 8443 :healthy t)) ("Worker" (:port 9443 :healthy nil))) :aliases ((:port 8443 :healthy t) (:port 8443 :healthy t) (:port 9443 :healthy nil) 2) :mutation-results ((nil nil nil) nil nil) :cleared (t 0))"##
    ]];
    ParityBatchCase::value(
        "case_folded_registry_uses_custom_hashing_for_alias_updates_and_removal",
        elisp_form,
        expect,
    )
}

fn shallow_snapshot_and_recursive_equality_model_a_rollback_workflow() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((production
        (ht
         (:mode 'active)
         (:tags (list "stable" "customer-facing"))
         (:services
          (ht ("orders" (ht (:replicas 3) (:region "east")))
              ("worker" (ht (:replicas 2) (:region "west")))))))
       (snapshot (ht-copy production))
       (equivalent
        (ht
         (:services
          (ht ("worker" (ht (:region "west") (:replicas 2)))
              ("orders" (ht (:region "east") (:replicas 3)))))
         (:tags (list "stable" "customer-facing"))
         (:mode 'active)))
       (different
        (ht
         (:mode 'active)
         (:tags (list "stable" "customer-facing"))
         (:services (ht ("orders" (ht (:replicas 4)))))))
       (before-equivalence
        (list (ht-equal-p production equivalent)
              (ht-equal-p production different))))
  (setf (ht-get snapshot :mode) 'maintenance)
  (setcar (ht-get snapshot :tags) "canary")
  (list
   :before-equivalence
   before-equivalence
   :production (ht-test-normalize production)
   :snapshot (ht-test-normalize snapshot)
   :table-independence
   (list (ht-get production :mode)
         (ht-get snapshot :mode))
   :shared-value
   (list (ht-get production :tags)
         (eq (ht-get production :tags) (ht-get snapshot :tags)))))
"##;
    let expect = expect![[
        r##"OK (:before-equivalence (t nil) :production ((:mode active) (:services (("orders" ((:region "east") (:replicas 3))) ("worker" ((:region "west") (:replicas 2))))) (:tags ("canary" "customer-facing"))) :snapshot ((:mode maintenance) (:services (("orders" ((:region "east") (:replicas 3))) ("worker" ((:region "west") (:replicas 2))))) (:tags ("canary" "customer-facing"))) :table-independence (active maintenance) :shared-value (("canary" "customer-facing") t))"##
    ]];
    ParityBatchCase::value(
        "shallow_snapshot_and_recursive_equality_model_a_rollback_workflow",
        elisp_form,
        expect,
    )
}

#[test]
fn ht_package_batch() {
    let cases = vec![
        layered_configuration_merge_preserves_inputs_and_last_writer_precedence(),
        nested_service_registry_supports_deep_lookup_assignment_and_counter_updates(),
        job_pipeline_selects_transforms_and_prunes_structured_records(),
        case_folded_registry_uses_custom_hashing_for_alias_updates_and_removal(),
        shallow_snapshot_and_recursive_equality_model_a_rollback_workflow(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed ht parity test");
    assert_oracle_batch_cases(ht_oracle(), test_name, "ht_parity", &cases);
}
