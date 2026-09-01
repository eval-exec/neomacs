use expect_test::expect;

use super::ParityBatchCase;

fn routing_a_manifest_uses_shared_bindings_and_skips_unreachable_work() -> ParityBatchCase {
    ParityBatchCase::value(
        "routing_a_manifest_uses_shared_bindings_and_skips_unreachable_work",
        r##"
(let* ((manifest
        '(:service "billing"
          :deployment
          (:environment production
           :endpoint "https://billing.example.test"
           :token "deploy-token")))
       (broken
        '(:service "checkout"
          :deployment
          (:environment production
           :endpoint "https://checkout.example.test")))
       events)
  (list
   :selected
   (cond-let*
     [[service (plist-get manifest :service)]]
     ([deployment (plist-get manifest :deployment)]
      [environment (plist-get deployment :environment)]
      [endpoint (and (eq environment 'production)
                     (plist-get deployment :endpoint))]
      [token (plist-get deployment :token)]
      (push (list :routed service environment) events)
      (list :service service :endpoint endpoint :token token))
     (t :no-route))
   :rejected
   (cond-let*
     [[service (plist-get broken :service)]]
     ([deployment (plist-get broken :deployment)]
      [token (plist-get deployment :token)]
      [unreachable (progn (push :unreachable events) t)]
      (list service token unreachable))
     (t (list :missing-token service)))
   :events (nreverse events)))
"##,
        expect![[
            r##"OK (:selected (:service "billing" :endpoint "https://billing.example.test" :token "deploy-token") :rejected (:missing-token "checkout") :events ((:routed "billing" production)))"##
        ]],
    )
}

fn parallel_and_sequential_bindings_resolve_configuration_at_the_intended_scope() -> ParityBatchCase
{
    ParityBatchCase::value(
        "parallel_and_sequential_bindings_resolve_configuration_at_the_intended_scope",
        r##"
(let ((region "global"))
  (list
   :parallel
   (cond-let
     ([region "us-east-1"]
      [endpoint (format "%s.api.example.test" region)]
      (list :region region :endpoint endpoint)))
   :sequential
   (cond-let*
     ([region "us-east-1"]
      [endpoint (format "%s.api.example.test" region)]
      (list :region region :endpoint endpoint)))
   :fallback
   (cond-let
     ([token nil]
      [endpoint "unused"]
      (list token endpoint))
     ((string= region "global")
      (list :region region :mode 'fallback)))))
"##,
        expect![[
            r##"OK (:parallel (:region "us-east-1" :endpoint "global.api.example.test") :sequential (:region "us-east-1" :endpoint "us-east-1.api.example.test") :fallback (:region "global" :mode fallback))"##
        ]],
    )
}

fn data_pipelines_normalize_validate_and_short_circuit_real_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "data_pipelines_normalize_validate_and_short_circuit_real_values",
        r##"
(let ((events nil)
      (endpoints '((production . "https://api.example.test")
                   (preview . "https://preview.example.test"))))
  (list
   :normalized
   (cond-let--thread$
    "  API.EXAMPLE.TEST  "
    (string-trim $)
    (downcase $)
    (concat "https://" $ "/health"))
   :validated
   (cond-let--and$
    '(:host "api.example.test" :port 443)
    (plist-get $ :host)
    (progn
      (push (list :validated-host $) events)
      (concat "https://" $)))
   :rejected
   (cond-let--and$
    '(:host nil :port 443)
    (plist-get $ :host)
    (progn (push :must-not-run events) $))
   :selected
   (cond-let--when$ (alist-get 'production endpoints)
     (push (list :selected $) events)
     (concat $ "/deploy"))
   :events (nreverse events)))
"##,
        expect![[
            r##"OK (:normalized "https://api.example.test/health" :validated "https://api.example.test" :rejected nil :selected "https://api.example.test/deploy" :events ((:validated-host "api.example.test") (:selected "https://api.example.test")))"##
        ]],
    )
}

fn authorization_workflows_preserve_parallel_and_sequential_lookup_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "authorization_workflows_preserve_parallel_and_sequential_lookup_rules",
        r##"
(let ((user "anonymous")
      (directory '(("alice" . admin) ("bob" . reader)))
      events)
  (list
   :parallel-if
   (cond-let--if-let
       ((user "alice")
        (label (format "%s-session" user)))
       (list :user user :label label)
     :denied)
   :sequential-if
   (cond-let--if-let*
       ((user "alice")
        (role (alist-get user directory nil nil #'string=)))
       (list :user user :role role)
     :denied)
   :missing-user
   (cond-let--if-let*
       ((user nil)
        (role (progn (push :lookup-must-not-run events) 'admin)))
       (list user role)
     (push :missing-user events)
     :denied)
   :parallel-when
   (cond-let--when-let
       ((user "bob")
        (label (format "%s-session" user)))
     (push (list :parallel user label) events)
     label)
   :sequential-when
   (cond-let--when-let*
       ((user "bob")
        (role (alist-get user directory nil nil #'string=)))
     (push (list :sequential user role) events)
     role)
   :events (nreverse events)))
"##,
        expect![[
            r##"OK (:parallel-if (:user "alice" :label "anonymous-session") :sequential-if (:user "alice" :role admin) :missing-user :denied :parallel-when "anonymous-session" :sequential-when reader :events (:missing-user (:parallel "bob" "anonymous-session") (:sequential "bob" reader)))"##
        ]],
    )
}

fn queue_workers_drain_ready_jobs_and_stop_before_invalid_bodies() -> ParityBatchCase {
    ParityBatchCase::value(
        "queue_workers_drain_ready_jobs_and_stop_before_invalid_bodies",
        r##"
(let ((simple-queue '(compile test package))
      (deployment-queue
       '((:id 101 :status ready)
         (:id 102 :status ready)
         (:id 103 :status cancelled)
         (:id 104 :status ready)))
      simple-processed
      deployment-processed
      events)
  (cond-let--while-let ((job (pop simple-queue)))
    (push job simple-processed))
  (cond-let--while-let*
      ((job (pop deployment-queue))
       (status (plist-get job :status))
       (_ (eq status 'ready)))
    (push (plist-get job :id) deployment-processed)
    (push (list :ran (plist-get job :id)) events))
  (list
   :simple (nreverse simple-processed)
   :simple-remaining simple-queue
   :deployments (nreverse deployment-processed)
   :deployment-remaining
   (mapcar (lambda (job) (plist-get job :id)) deployment-queue)
   :events (nreverse events)))
"##,
        expect![[
            r##"OK (:simple (compile test package) :simple-remaining nil :deployments (101 102) :deployment-remaining (104) :events ((:ran 101) (:ran 102)))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        routing_a_manifest_uses_shared_bindings_and_skips_unreachable_work(),
        parallel_and_sequential_bindings_resolve_configuration_at_the_intended_scope(),
        data_pipelines_normalize_validate_and_short_circuit_real_values(),
        authorization_workflows_preserve_parallel_and_sequential_lookup_rules(),
        queue_workers_drain_ready_jobs_and_stop_before_invalid_bodies(),
    ]
}
