use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_FUNCTIONAL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'dash-functional)

(defconst neomacs-dash-functional-test-builds
  '((:id "API-104" :environment " Prod " :artifact "api.tar" :bytes (420 580 1000))
    (:id "WEB-205" :environment " STAGE" :artifact "web.tar" :bytes (800 200))
    (:id "DOC-306" :environment "prod " :artifact "docs.tar" :bytes (120 80 50))))

(defun neomacs-dash-functional-test-normalize-environment (environment)
  "Normalize a deployment ENVIRONMENT from a legacy consumer."
  (downcase (string-trim environment)))

(defun neomacs-dash-functional-test-dependencies (task)
  "Return TASK's direct release dependencies."
  (cdr (assq task
             '((deploy package smoke)
               (package compile)
               (smoke integration)
               (compile lint)
               (integration lint)))))

(defun neomacs-dash-functional-test-expand-plan (tasks)
  "Add all direct dependencies of TASKS, retaining stable order."
  (-uniq (append tasks
                 (-mapcat #'neomacs-dash-functional-test-dependencies
                          tasks))))
"####;

fn legacy_feature_registration_records_the_shim_and_its_dash_requirement() -> ParityBatchCase {
    let elisp_form = r####"
(let ((history-entry
       (cl-find-if
        (lambda (entry)
          (member '(provide . dash-functional) (cdr entry)))
        load-history)))
  (list :require-result (require 'dash-functional)
        :features
        (list :dash (and (featurep 'dash) t)
              :dash-functional (and (featurep 'dash-functional) t))
        :source (and history-entry
                     (file-name-nondirectory (car history-entry)))
        :history
        (list :requires-dash
              (and (member '(require . dash) (cdr history-entry)) t)
              :provides-shim
              (and (member '(provide . dash-functional)
                           (cdr history-entry))
                   t))
        :partial-is-gnu-apply-partially
        (eq (symbol-function '-partial)
            (symbol-function 'apply-partially))
        :representative-api
        (mapcar #'fboundp
                '(-rpartial -juxt -compose -applify -on -flip
                  -rotate-args -const -not -orfn -andfn -iteratefn
                  -counter -fixfn -prodfn))))
"####;
    let expected = expect![[
        r#"OK (:require-result dash-functional :features (:dash t :dash-functional t) :source "dash-functional.el" :history (:requires-dash t :provides-shim t) :partial-is-gnu-apply-partially nil :representative-api (t t t t t t t t t t t t t t t))"#
    ]];
    ParityBatchCase::value(
        "legacy_feature_registration_records_the_shim_and_its_dash_requirement",
        elisp_form,
        expected,
    )
}

fn composed_build_report_normalizes_fields_and_computes_complete_artifact_summaries()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((environment
        (-compose #'neomacs-dash-functional-test-normalize-environment
                  (-rpartial #'plist-get :environment)))
       (total-bytes
        (-compose #'-sum (-rpartial #'plist-get :bytes)))
       (build-summary
        (-juxt (-rpartial #'plist-get :id)
               environment
               (-rpartial #'plist-get :artifact)
               total-bytes))
       (format-route (-applify (-partial #'format "%s/%s/%s:%d"))))
  (list
   :rows
   (mapcar
    (lambda (build)
      (let ((summary (funcall build-summary build)))
        (list :summary summary
              :route (funcall format-route summary))))
    neomacs-dash-functional-test-builds)
   :source neomacs-dash-functional-test-builds
   :empty-compose
   (list (funcall (-compose))
         (funcall (-compose) 'unchanged 'ignored))))
"####;
    let expected = expect![[
        r#"OK (:rows ((:summary ("API-104" "prod" "api.tar" 2000) :route "API-104/prod/api.tar:2000") (:summary ("WEB-205" "stage" "web.tar" 1000) :route "WEB-205/stage/web.tar:1000") (:summary ("DOC-306" "prod" "docs.tar" 250) :route "DOC-306/prod/docs.tar:250")) :source ((:id "API-104" :environment " Prod " :artifact "api.tar" :bytes (420 580 1000)) (:id "WEB-205" :environment " STAGE" :artifact "web.tar" :bytes (800 200)) (:id "DOC-306" :environment "prod " :artifact "docs.tar" :bytes (120 80 50))) :empty-compose (nil unchanged))"#
    ]];
    ParityBatchCase::value(
        "composed_build_report_normalizes_fields_and_computes_complete_artifact_summaries",
        elisp_form,
        expected,
    )
}

fn validation_and_risk_ranking_short_circuit_expensive_release_checks() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((releases
        '((:name "api" :status ready :risk 2 :approvals (ops security))
          (:name "web" :status blocked :risk 9 :approvals (ops))
          (:name "worker" :status ready :risk 7 :approvals nil)
          (:name "docs" :status emergency :risk 5 :approvals (editor))))
       events
       (schema-valid
        (lambda (release)
          (push (list :schema (plist-get release :name)) events)
          (memq (plist-get release :status) '(ready emergency))))
       (approved
        (lambda (release)
          (push (list :approval (plist-get release :name)) events)
          (consp (plist-get release :approvals))))
       (deployable (-andfn schema-valid approved))
       (manual-review
        (-orfn (lambda (release)
                 (eq (plist-get release :status) 'emergency))
               (-not deployable)))
       (accepted (-filter deployable releases))
       (ranked (-sort (-on #'> (lambda (release)
                                (plist-get release :risk)))
                      accepted)))
  (list :accepted (--map (plist-get it :name) accepted)
        :ranked (--map (list (plist-get it :name)
                             (plist-get it :risk))
                       ranked)
        :manual-review (--map (plist-get it :name)
                              (-filter manual-review releases))
        :check-events (nreverse events)
        :source releases))
"####;
    let expected = expect![[
        r#"OK (:accepted ("api" "docs") :ranked (("docs" 5) ("api" 2)) :manual-review ("web" "worker" "docs") :check-events ((:schema "api") (:approval "api") (:schema "web") (:schema "worker") (:approval "worker") (:schema "docs") (:approval "docs") (:schema "api") (:approval "api") (:schema "web") (:schema "worker") (:approval "worker")) :source ((:name "api" :status ready :risk 2 :approvals (ops security)) (:name "web" :status blocked :risk 9 :approvals (ops)) (:name "worker" :status ready :risk 7 :approvals nil) (:name "docs" :status emergency :risk 5 :approvals (editor))))"#
    ]];
    ParityBatchCase::value(
        "validation_and_risk_ranking_short_circuit_expensive_release_checks",
        elisp_form,
        expected,
    )
}

fn callback_adapters_reorder_arguments_without_rewriting_the_legacy_handler() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((handler
        (lambda (event context acknowledgement)
          (list :event event
                :environment (plist-get context :environment)
                :ack acknowledgement)))
       (context '(:environment stage :operator "Ada"))
       ;; One producer invokes ACK, CONTEXT, EVENT: complete reversal.
       (reverse-adapter (-flip handler))
       ;; Another invokes CONTEXT, ACK, EVENT: rotate EVENT to the front.
       (rotate-adapter (-rotate-args 1 handler))
       ;; A synchronous producer has no acknowledgement argument, so it is
       ;; specialized when the adapter is constructed.
       (sync-adapter (-cut funcall handler <> <> "synchronous")))
  (list :reverse
        (funcall reverse-adapter "queued" context 'artifact-uploaded)
        :rotate
        (funcall rotate-adapter context "accepted" 'release-approved)
        :specialized
        (funcall sync-adapter 'rollback-requested context)
        :handler-still-direct
        (funcall handler 'health-check context "healthy")))
"####;
    let expected = expect![[
        r#"OK (:reverse (:event artifact-uploaded :environment stage :ack "queued") :rotate (:event release-approved :environment stage :ack "accepted") :specialized (:event rollback-requested :environment stage :ack "synchronous") :handler-still-direct (:event health-check :environment stage :ack "healthy"))"#
    ]];
    ParityBatchCase::value(
        "callback_adapters_reorder_arguments_without_rewriting_the_legacy_handler",
        elisp_form,
        expected,
    )
}

fn dependency_planning_converges_to_a_fixpoint_and_reports_a_bounded_nonconvergence()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((resolve-plan
        (-fixfn #'neomacs-dash-functional-test-expand-plan))
       (three-rounds
        (-iteratefn #'neomacs-dash-functional-test-expand-plan 3))
       (halt-calls 0)
       (oscillating
        (-fixfn #'not #'equal
                (lambda (_value)
                  (setq halt-calls (1+ halt-calls))
                  (= halt-calls 4)))))
  (list :rounds
        (mapcar
         (lambda (round)
           (funcall (-iteratefn
                     #'neomacs-dash-functional-test-expand-plan round)
                    '(deploy)))
         '(0 1 2 3 4 5))
        :three-rounds (funcall three-rounds '(deploy))
        :fixpoint (funcall resolve-plan '(deploy))
        :bounded-oscillation (funcall oscillating nil)
        :halt-calls halt-calls))
"####;
    let expected = expect![
        "OK (:rounds ((deploy) (deploy package smoke) (deploy package smoke compile integration) (deploy package smoke compile integration lint) (deploy package smoke compile integration lint) (deploy package smoke compile integration lint)) :three-rounds (deploy package smoke compile integration lint) :fixpoint (deploy package smoke compile integration lint) :bounded-oscillation (halted . t) :halt-calls 4)"
    ];
    ParityBatchCase::value(
        "dependency_planning_converges_to_a_fixpoint_and_reports_a_bounded_nonconvergence",
        elisp_form,
        expected,
    )
}

fn independent_counters_and_constant_callbacks_allocate_predictable_job_metadata() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((release-ids (-counter 100 106 2))
      (retry-ids (-counter 1 nil 1))
      (default-state (-const 'queued)))
  (list :release-ids
        (mapcar (lambda (_) (funcall release-ids)) '(a b c d e))
        :retry-ids
        (mapcar (lambda (_) (funcall retry-ids)) '(first second third))
        :release-after-exhaustion (funcall release-ids)
        :retry-next (funcall retry-ids)
        :states (mapcar default-state '(api web worker docs))
        :constant-ignores-context
        (funcall default-state :environment 'prod :risk 9)))
"####;
    let expected = expect![
        "OK (:release-ids (100 102 104 nil nil) :retry-ids (1 2 3) :release-after-exhaustion nil :retry-next 4 :states (queued queued queued queued) :constant-ignores-context queued)"
    ];
    ParityBatchCase::value(
        "independent_counters_and_constant_callbacks_allocate_predictable_job_metadata",
        elisp_form,
        expected,
    )
}

fn product_transformers_parse_a_build_matrix_field_by_field() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((normalize-name (-compose #'upcase #'string-trim))
       (normalize-environment
        (-compose #'intern #'downcase #'string-trim))
       (normalize-retries (-partial #'+ 1))
       (parse-row
       (-prodfn normalize-name normalize-environment normalize-retries))
       (rows
        '((" api " "PROD" 2)
          (" worker" " stage " 0)
          ("docs " "DEV" 4))))
  (let ((parsed (mapcar parse-row rows)))
    (list :parsed parsed
          :labels
          (mapcar
           (-compose (-applify (-partial #'format "%s@%s retry=%d"))
                     (-prodfn #'identity #'symbol-name #'identity))
           parsed)
          :source rows)))
"####;
    let expected = expect![[
        r#"OK (:parsed (("API" prod 3) ("WORKER" stage 1) ("DOCS" dev 5)) :labels ("API@prod retry=3" "WORKER@stage retry=1" "DOCS@dev retry=5") :source ((" api " "PROD" 2) (" worker" " stage " 0) ("docs " "DEV" 4)))"#
    ]];
    ParityBatchCase::value(
        "product_transformers_parse_a_build_matrix_field_by_field",
        elisp_form,
        expected,
    )
}

fn reloading_the_obsolete_shim_emits_one_actionable_warning_and_restores_the_feature()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((library (locate-library "dash-functional"))
      warnings
      messages)
  (require 'bytecomp)
  (unload-feature 'dash-functional t)
  (cl-letf (((symbol-function 'byte-compile-warn)
             (lambda (format-string &rest arguments)
               (push (apply #'format-message format-string arguments)
                     warnings)))
            ((symbol-function 'message)
             (lambda (format-string &rest arguments)
               (push (and format-string
                          (apply #'format-message format-string arguments))
                     messages))))
    (load library nil t t))
  (list :library (file-name-nondirectory library)
        :warnings (nreverse warnings)
        :messages (nreverse messages)
        :feature (and (featurep 'dash-functional) t)
        :dash-still-loaded (and (featurep 'dash) t)
        :api-still-works
        (funcall (-compose #'1+ (-partial #'+ 5)) 7)))
"####;
    let expected = expect![[
        r#"OK (:library "dash-functional.el" :warnings ("Package dash-functional is obsolete; use dash 2.18.0 instead") :messages nil :feature t :dash-still-loaded t :api-still-works 13)"#
    ]];
    ParityBatchCase::value(
        "reloading_the_obsolete_shim_emits_one_actionable_warning_and_restores_the_feature",
        elisp_form,
        expected,
    )
}

#[test]
fn dash_functional_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(DASH_FUNCTIONAL_MELPA_PIN, "dash-functional.el")
            .expect("prepare revision-pinned Dash Functional source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "dash-functional-package-batch",
        "Dash Functional",
        &[
            legacy_feature_registration_records_the_shim_and_its_dash_requirement(),
            composed_build_report_normalizes_fields_and_computes_complete_artifact_summaries(),
            validation_and_risk_ranking_short_circuit_expensive_release_checks(),
            callback_adapters_reorder_arguments_without_rewriting_the_legacy_handler(),
            dependency_planning_converges_to_a_fixpoint_and_reports_a_bounded_nonconvergence(),
            independent_counters_and_constant_callbacks_allocate_predictable_job_metadata(),
            product_transformers_parse_a_build_matrix_field_by_field(),
            reloading_the_obsolete_shim_emits_one_actionable_warning_and_restores_the_feature(),
        ],
    );
}
