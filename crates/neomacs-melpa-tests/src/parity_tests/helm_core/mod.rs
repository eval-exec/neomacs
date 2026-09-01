use std::time::Duration;

use expect_test::expect;

use crate::{ASYNC_MELPA_PIN, CachedMelpaOracle, HELM_CORE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_CORE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HELM_CORE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'helm-core)
(require 'helm-source)
(require 'helm-multi-match)

(defvar helm-core-test-minimum-pattern 2)

(defun helm-core-test-candidate-shape (candidate)
  (list (substring-no-properties (car candidate))
        (copy-tree (cdr candidate))))

(defun helm-core-test-buffer-candidate-shape (candidate)
  (let ((real (get-text-property 0 'helm-realvalue candidate))
        (match-part (get-text-property 0 'match-part candidate)))
    (list (substring-no-properties candidate)
          (copy-tree real)
          (and match-part (substring-no-properties match-part))
          (and match-part
               (equal (get-text-property 0 'helm-realvalue match-part)
                      real)))))
"##;

fn helm_core_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_CORE_MELPA_PIN, "helm-core.el")
        .expect("prepare pinned helm-core source below ./tmp")
        .with_melpa_dependency(ASYNC_MELPA_PIN)
        .expect("prepare pinned async dependency")
        .with_prelude(HELM_CORE_TEST_PRELUDE)
        .with_timeout(HELM_CORE_TEST_TIMEOUT)
}

fn extension_source_configuration_normalizes_dynamic_attributes_and_actions() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((source
        (helm-build-sync-source "Deploy targets"
          :candidates
          '(("api / east" . (:service api :region east))
            ("worker / west" . (:service worker :region west)))
          :requires-pattern 'helm-core-test-minimum-pattern
          :candidate-number-limit 3
          :multimatch t
          :nohighlight t
          :action
          (helm-make-actions
           "Inspect" #'identity
           (lambda () "Restart") #'ignore
           (lambda () nil) #'ignore)))
       (original-candidates (helm-get-attr 'candidates source t)))
  (helm-add-action-to-source "Copy service" #'symbol-name source 1)
  (helm-delete-action-from-source "Inspect" source)
  (helm-set-attr 'candidate-number-limit 5 source)
  (helm-set-attr 'owner (lambda () 'platform-team) source)
  (list
   :source
   (list (helm-get-attr 'name source)
         (helm-get-attr 'group source)
         (helm-get-attr 'requires-pattern source)
         (helm-get-attr 'candidate-number-limit source)
         (helm-get-attr 'owner source t))
   :matching (helm-get-attr 'match source)
   :actions (mapcar #'car (helm-get-attr 'action source 'ignorefn))
   :candidates (mapcar #'helm-core-test-candidate-shape original-candidates)))
"##;
    let expect = expect![[
        r##"OK (:source ("Deploy targets" helm 2 5 platform-team) :matching (helm-mm-exact-match helm-mm-match) :actions ("Copy service" "Restart") :candidates (("api / east" (:service api :region east)) ("worker / west" (:service worker :region west))))"##
    ]];
    ParityBatchCase::value(
        "extension_source_configuration_normalizes_dynamic_attributes_and_actions",
        elisp_form,
        expect,
    )
}

fn in_buffer_source_searches_real_deployment_records_by_match_part() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((helm-current-buffer (current-buffer))
       (helm--candidate-buffer-alist nil)
       (source
        (helm-build-in-buffer-source "Deployments"
          :data
          '(("api | pending | east" . (:id 417 :owner alice))
            ("web | fraud-review | west" . (:id 418 :owner bob))
            ("worker | pending | west" . (:id 419 :owner carol))
            ("api | shipped | east" . (:id 420 :owner david)))
          :get-line #'buffer-substring
          :match-part
          (lambda (candidate)
            (string-trim (nth 1 (split-string candidate "|"))))
          :multimatch t
          :nohighlight t
          :candidate-number-limit 10))
       candidate-buffer)
  (unwind-protect
      (progn
        (setq candidate-buffer
              (helm-apply-functions-from-source
               source (helm-get-attr 'init source)))
        (let ((helm-pattern ""))
          (let ((all
                 (helm-apply-functions-from-source
                  source #'helm-candidates-in-buffer source)))
            (let ((helm-pattern "pending !fraud"))
              (let ((pending
                     (helm-apply-functions-from-source
                      source #'helm-candidates-in-buffer source)))
                (let ((helm-pattern "["))
                  (list
                   :all (mapcar #'helm-core-test-buffer-candidate-shape all)
                   :pending
                   (mapcar #'helm-core-test-buffer-candidate-shape pending)
                   :invalid-regexp
                   (helm-apply-functions-from-source
                    source #'helm-candidates-in-buffer source)
                   :longest
                   (buffer-local-value
                    'helm-candidate-buffer-longest-len
                    candidate-buffer))))))))
    (when (buffer-live-p candidate-buffer)
      (kill-buffer candidate-buffer))))
"##;
    let expect = expect![[
        r##"OK (:all (("api | pending | east" (:id 417 :owner alice) nil nil) ("web | fraud-review | west" (:id 418 :owner bob) nil nil) ("worker | pending | west" (:id 419 :owner carol) nil nil) ("api | shipped | east" (:id 420 :owner david) nil nil)) :pending (("api | pending | east" (:id 417 :owner alice) "pending" t) ("worker | pending | west" (:id 419 :owner carol) "pending" t)) :invalid-regexp nil :longest 25)"##
    ]];
    ParityBatchCase::value(
        "in_buffer_source_searches_real_deployment_records_by_match_part",
        elisp_form,
        expect,
    )
}

fn incident_queue_pipeline_deduplicates_groups_pages_and_rotates_records() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((records
        '(("INC-417 checkout rejected" . (:team payments :severity critical))
          ("INC-418 inventory delayed" . (:team inventory :severity warning))
          ("INC-419 refund timeout" . (:team payments :severity warning))
          ("INC-417 checkout rejected" . (:team payments :severity critical))
          ("INC-420 warehouse offline" . (:team inventory :severity critical))))
       (unique (helm-fast-remove-dups records :test 'equal))
       (selection (nth 2 unique))
       (groups
        (helm-group-candidates-by
         unique (lambda (record) (plist-get (cdr record) :team))
         selection t))
       (rotated (helm-reorganize-sequence-from-elm unique selection))
       (iterator (helm-iter-sub-next-circular unique selection :test #'equal)))
  (list
   :unique (mapcar #'helm-core-test-candidate-shape unique)
   :groups
   (mapcar
    (lambda (group) (mapcar #'helm-core-test-candidate-shape group))
    groups)
   :page (mapcar #'helm-core-test-candidate-shape (helm-take unique 3))
   :vector-page (append (helm-take [queued running passed failed] 2) nil)
   :rotated (mapcar #'helm-core-test-candidate-shape rotated)
   :next-six
   (cl-loop repeat 6
            collect (car (helm-iter-next iterator)))))
"##;
    let expect = expect![[
        r##"OK (:unique (("INC-417 checkout rejected" (:team payments :severity critical)) ("INC-418 inventory delayed" (:team inventory :severity warning)) ("INC-419 refund timeout" (:team payments :severity warning)) ("INC-420 warehouse offline" (:team inventory :severity critical))) :groups ((("INC-417 checkout rejected" (:team payments :severity critical)) ("INC-419 refund timeout" (:team payments :severity warning))) (("INC-418 inventory delayed" (:team inventory :severity warning)) ("INC-420 warehouse offline" (:team inventory :severity critical)))) :page (("INC-417 checkout rejected" (:team payments :severity critical)) ("INC-418 inventory delayed" (:team inventory :severity warning)) ("INC-419 refund timeout" (:team payments :severity warning))) :vector-page (queued running) :rotated (("INC-420 warehouse offline" (:team inventory :severity critical)) ("INC-417 checkout rejected" (:team payments :severity critical)) ("INC-418 inventory delayed" (:team inventory :severity warning)) ("INC-419 refund timeout" (:team payments :severity warning))) :next-six ("INC-420 warehouse offline" "INC-417 checkout rejected" "INC-418 inventory delayed" "INC-419 refund timeout" "INC-420 warehouse offline" "INC-417 checkout rejected"))"##
    ]];
    ParityBatchCase::value(
        "incident_queue_pipeline_deduplicates_groups_pages_and_rotates_records",
        elisp_form,
        expect,
    )
}

fn multiline_incident_previews_preserve_real_records_and_display_width() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((source '((name . "Incident previews") (multiline . 24)))
       (helm-current-source source)
       (candidates
        '(("API checkout rejected\nOrder 417 needs another payment method"
           . (:id 417 :severity critical))
          ("支付 service timeout\nOrder 418 retry queued"
           . (:id 418 :severity warning))))
       (transformed (helm-multiline-transformer candidates source)))
  (list
   :previews (mapcar #'helm-core-test-candidate-shape transformed)
   :labels
   (mapcar
    (lambda (label)
      (list (helm-substring-by-width label 16 "…")
            (string-width (helm-substring-by-width label 16 "…"))))
    '("payments / critical" "支付 / warning" "api"))
   :columns
   (mapcar (lambda (width)
             (helm-substring "東京-orders-417" width))
           '(4 8 12))))
"##;
    let expect = expect![[
        r##"OK (:previews (("API checkout rejected\nOr\n[...]" (:id 417 :severity critical)) ("支付 service timeout\nOrder\n[...]" (:id 418 :severity warning))) :labels (("payments / criti…" 17) ("支付 / warning…  " 17) ("api…             " 17)) :columns ("東京" "東京-ord" "東京-orders-"))"##
    ]];
    ParityBatchCase::value(
        "multiline_incident_previews_preserve_real_records_and_display_width",
        elisp_form,
        expect,
    )
}

fn artifact_path_helpers_parse_archives_wildcards_and_encoded_urls() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((wildcard "**.{json,yaml,toml}")
       (regexp (helm-wildcard-to-regexp wildcard))
       (artifacts
        '("release/order.json" "release/order.yaml"
          "release/order.toml" "release/order.txt")))
  (list
   :sans-extensions
   (mapcar #'helm-file-name-sans-extension
           '("dist/neomacs-1.2.3.tar.zst"
             "reports/order.417.json"
             ".config.local.el"))
   :basenames
   (list
    (helm-basename "/build/neomacs-1.2.3.tar.zst" 2)
    (helm-basename "/build/checkout.el.gz" '(2 . "\\.el\\'"))
    (helm-basename "/build/archive.part.7" t))
   :extensions
   (mapcar #'helm-file-name-extension
           '("invoice.pdf" "invoice.417" "archive.tar.zst" "README"))
   :wildcard
   (list regexp
         (mapcar (lambda (path)
                   (and (string-match-p regexp path) path))
                 artifacts))
   :quoted (helm-quote-whitespace "Q3 orders / São Paulo.csv")
   :decoded
   (helm-url-unhex-string
    "orders%2F2026%20Q3%2FS%C3%A3o%20Paulo%2Bpriority.json")))
"##;
    let expect = expect![[
        r##"OK (:sans-extensions ("dist/neomacs-1" "reports/order" ".config") :basenames ("neomacs-1.2.3" "checkout" "archive.part") :extensions ("pdf" nil "zst" nil) :wildcard (".*\\.\\(json\\|yaml\\|toml\\)$" ("release/order.json" "release/order.yaml" "release/order.toml" nil)) :quoted "Q3\\ orders\\ /\\ São\\ Paulo.csv" :decoded "orders/2026 Q3/São Paulo+priority.json")"##
    ]];
    ParityBatchCase::value(
        "artifact_path_helpers_parse_archives_wildcards_and_encoded_urls",
        elisp_form,
        expect,
    )
}

#[test]
fn helm_core_package_batch() {
    let cases = vec![
        extension_source_configuration_normalizes_dynamic_attributes_and_actions(),
        in_buffer_source_searches_real_deployment_records_by_match_part(),
        incident_queue_pipeline_deduplicates_groups_pages_and_rotates_records(),
        multiline_incident_previews_preserve_real_records_and_display_width(),
        artifact_path_helpers_parse_archives_wildcards_and_encoded_urls(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed helm-core parity test");
    assert_oracle_batch_cases(helm_core_oracle(), test_name, "helm_core_parity", &cases);
}
