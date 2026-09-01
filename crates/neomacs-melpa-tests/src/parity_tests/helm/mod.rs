use std::time::Duration;

use expect_test::expect;

use crate::{
    ASYNC_MELPA_PIN, CachedMelpaOracle, HELM_CORE_MELPA_PIN, HELM_MELPA_PIN, WFNAMES_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HELM_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'helm-mode)
(require 'helm-imenu)
(require 'helm-occur)

(defvar crm-separator ",")

(setq helm-imenu-use-icon nil
      helm-imenu-hide-item-type-name nil
      helm-mm-matching-method 'multi3)

(defvar helm-test-action-log nil)

(defun helm-test-open-order (order)
  (push (list :opened (plist-get order :id)
              :status (plist-get order :status))
        helm-test-action-log)
  (plist-get order :id))

(defun helm-test-copy-order-id (order)
  (format "ORDER-%s" (plist-get order :id)))

(defun helm-test-candidate-shape (candidate)
  (if (consp candidate)
      (list (substring-no-properties (car candidate))
            (copy-tree (cdr candidate)))
    (substring-no-properties candidate)))

(defun helm-test-imenu-shape (candidates)
  (mapcar
   (lambda (candidate)
     (list (substring-no-properties (car candidate))
           (get-text-property 0 'helm-imenu-type (car candidate))
           (line-number-at-pos (cdr candidate))))
   candidates))
"##;

fn helm_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_MELPA_PIN, "helm.el")
        .expect("prepare pinned Helm source below ./tmp")
        .with_melpa_dependency(HELM_CORE_MELPA_PIN)
        .expect("prepare pinned helm-core dependency")
        .with_melpa_dependency(ASYNC_MELPA_PIN)
        .expect("prepare pinned async dependency")
        .with_melpa_dependency(WFNAMES_MELPA_PIN)
        .expect("prepare pinned wfnames dependency")
        .with_prelude(HELM_TEST_PRELUDE)
        .with_timeout(HELM_TEST_TIMEOUT)
}

fn synchronous_source_filters_orders_and_dispatches_the_selected_real_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "synchronous_source_filters_orders_and_dispatches_the_selected_real_value",
        r##"
(let* ((helm-test-action-log nil)
       (orders
        '(("Order 417 | pending | Alice" . (:id 417 :status pending))
          ("Order 418 | fraud-review | Bob" . (:id 418 :status fraud-review))
          ("Order 419 | pending | Carol" . (:id 419 :status pending))
          ("Order 420 | shipped | David" . (:id 420 :status shipped))))
       (actions
        (helm-make-actions
         "Open order" #'helm-test-open-order
         (lambda () (and (> (length orders) 3) "Copy order id"))
         #'helm-test-copy-order-id
         (lambda () nil) #'ignore))
       (source
        (helm-build-sync-source "Orders"
          :candidates orders
          :multimatch t
          :action actions
          :candidate-number-limit 10))
       (helm-pattern "pending !fraud")
       (candidates (helm-get-candidates source))
       (matches
        (helm-match-from-candidates
         candidates
         (helm-match-functions source)
         (assoc-default 'match-part source)
         10 source))
       (selected (cdr (car matches)))
       (opened (funcall (cdr (car actions)) selected))
       (copied (funcall (cdr (cadr actions)) selected)))
  (list
   :source
   (list (assoc-default 'name source)
         (assoc-default 'group source)
         (assoc-default 'multimatch source)
         (mapcar #'car (assoc-default 'action source)))
   :candidates (mapcar #'helm-test-candidate-shape candidates)
   :matches (mapcar #'helm-test-candidate-shape matches)
   :actions (list opened copied (nreverse helm-test-action-log))))
"##,
        expect![[
            r##"OK (:source ("Orders" helm t ("Open order" "Copy order id")) :candidates (("Order 417 | pending | Alice" (:id 417 :status pending)) ("Order 418 | fraud-review | Bob" (:id 418 :status fraud-review)) ("Order 419 | pending | Carol" (:id 419 :status pending)) ("Order 420 | shipped | David" (:id 420 :status shipped))) :matches (("Order 417 | pending | Alice" (:id 417 :status pending)) ("Order 419 | pending | Carol" (:id 419 :status pending))) :actions (417 "ORDER-417" ((:opened 417 :status pending))))"##
        ]],
    )
}

fn multi_pattern_queries_cover_ordered_permuted_negative_exact_and_diacritic_matching()
-> ParityBatchCase {
    ParityBatchCase::value(
        "multi_pattern_queries_cover_ordered_permuted_negative_exact_and_diacritic_matching",
        r##"
(let ((candidates
       '("Order 417 pending São Paulo"
         "Order 418 fraud-review Berlin"
         "Order 419 pending Paris"
         "Order 420 shipped Sao Tome")))
  (list
   :split (helm-mm-split-pattern "pending São\\ Paulo !fraud")
   :multi1
   (let ((helm-mm-matching-method 'multi1))
     (mapcar (lambda (candidate)
               (and (helm-mm-match candidate "Order pending") candidate))
             candidates))
   :multi2
   (let ((helm-mm-matching-method 'multi2))
     (mapcar (lambda (candidate)
               (and (helm-mm-match candidate "pending Paris") candidate))
             candidates))
   :multi3
   (let ((helm-mm-matching-method 'multi3))
     (mapcar (lambda (candidate)
               (and (helm-mm-match candidate "pending !fraud") candidate))
             candidates))
   :exact
   (mapcar (lambda (candidate)
             (and (helm-mm-exact-match candidate "Order 419 pending Paris")
                  candidate))
           candidates)
   :diacritics
   (let ((helm-mm-matching-method 'multi3))
     (mapcar (lambda (candidate)
               (and (helm-mm-3-match-on-diacritics candidate "Sao Paulo")
                    candidate))
             candidates))))
"##,
        expect![[
            r##"OK (:split ("pending" "São\\s-Paulo" "!fraud") :multi1 ("Order 417 pending São Paulo" nil "Order 419 pending Paris" nil) :multi2 (nil nil "Order 419 pending Paris" nil) :multi3 ("Order 417 pending São Paulo" nil "Order 419 pending Paris" nil) :exact (nil nil "Order 419 pending Paris" nil) :diacritics ("Order 417 pending São Paulo" nil nil nil))"##
        ]],
    )
}

fn completion_insertion_handles_annotations_and_real_crm_separator_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_insertion_handles_annotations_and_real_crm_separator_contracts",
        r##"
(list
 :annotated
 (with-temp-buffer
   (insert "ord")
   (let ((choice (concat "order-417"
                         (propertize " " 'display " pending — Alice"))))
     (helm-completion-in-region--insert-result
      choice (point-min) (point) (point) 0)
     (list (buffer-substring-no-properties (point-min) (point-max))
           (point))))
 :crm
 (mapcar
  (lambda (configuration)
    (with-temp-buffer
      (insert "Range: ")
      (let ((crm-separator (nth 0 configuration))
            (helm-crm-default-separator (nth 1 configuration)))
        (helm-completion-in-region--insert-result
         (list "order-417") 1 (point) (point) 0)
        (buffer-substring-no-properties (point-min) (point-max)))))
  (list
   (list "\\.\\.\\.\\?" nil)
   (list (propertize "\\.\\.\\.\\?" 'separator "...") nil)
   (list "[ \t]*:[ \t]*" ","))))
"##,
        expect![[
            r##"OK (:annotated ("order-417" 10) :crm ("order-417" "order-417..." "Range:order-417:"))"##
        ]],
    )
}

fn imenu_candidates_preserve_definition_types_locations_and_display_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_candidates_preserve_definition_types_locations_and_display_paths",
        r##"
(with-temp-buffer
  (insert
   ";;; checkout.el --- Checkout operations\n\n"
   "(defvar checkout-tax-rate 0.20)\n\n"
   "(defun checkout-total (items)\n"
   "  (apply #'+ (mapcar #'cdr items)))\n\n"
   "(defun checkout-submit (order)\n"
   "  (list :submitted order :total (checkout-total order)))\n")
  (emacs-lisp-mode)
  (let* ((helm-current-buffer (current-buffer))
         (helm-cached-imenu-tick nil)
         (helm-cached-imenu-candidates nil)
         (candidates (helm-imenu-candidates (current-buffer)))
         (transformed (helm-imenu-transformer candidates)))
    (list
     :candidates (helm-test-imenu-shape candidates)
     :display
     (mapcar
      (lambda (candidate)
        (list (substring-no-properties (car candidate))
              (line-number-at-pos (cddr candidate))))
      transformed))))
"##,
        expect![[
            r##"OK (:candidates (("checkout-tax-rate" "Variables" 3) ("checkout-total" nil 5) ("checkout-submit" nil 8)) :display (("Variables / checkout-tax-rate" 3) ("Function / checkout-total" 5) ("Function / checkout-submit" 8)))"##
        ]],
    )
}

fn occur_source_numbers_log_lines_transforms_matches_and_expands_symbol_shorthands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "occur_source_numbers_log_lines_transforms_matches_and_expands_symbol_shorthands",
        r##"
(let ((buffer (generate-new-buffer " *helm-order-log*")))
  (unwind-protect
      (with-current-buffer buffer
        (insert
         "INFO commerce-order-created id=417\n"
         "WARN commerce-order-payment-retry id=418\n"
         "INFO commerce-order-shipped id=419\n")
        (setq-local read-symbol-shorthands
                    '(("co-" . "commerce-order-")))
        (let* ((helm-occur-match-shorthands t)
               (source (car (helm-occur-build-sources
                             (list buffer) "Order log")))
               (numbered
                (split-string
                 (substring-no-properties
                  (helm-occur-buffer-substring-with-linums))
                 "\n" t))
               (transformed
                (helm-occur-transformer
                 (list (nth 0 numbered) (nth 2 numbered)) source)))
          (list
           :source
           (list (assoc-default 'name source)
                 (assoc-default 'buffer-name source))
           :numbered numbered
           :transformed
           (mapcar (lambda (candidate)
                     (list (substring-no-properties (car candidate))
                           (cdr candidate)))
                   transformed)
           :patterns
           (list
            (funcall (assoc-default 'pattern-transformer source)
                     "co-shipped")
            (funcall (assoc-default 'pattern-transformer source)
                     "commerce-order-payment")))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"##,
        expect![[
            r##"OK (:source ("Order log" " *helm-order-log*") :numbered ("1 INFO commerce-order-created id=417" "2 WARN commerce-order-payment-retry id=418" "3 INFO commerce-order-shipped id=419" "4 ") :transformed (("1:INFO commerce-order-created id=417" 1) ("3:INFO commerce-order-shipped id=419" 3)) :patterns ("co-shipped" "co-payment"))"##
        ]],
    )
}

#[test]
fn helm_package_batch() {
    let cases = vec![
        synchronous_source_filters_orders_and_dispatches_the_selected_real_value(),
        multi_pattern_queries_cover_ordered_permuted_negative_exact_and_diacritic_matching(),
        completion_insertion_handles_annotations_and_real_crm_separator_contracts(),
        imenu_candidates_preserve_definition_types_locations_and_display_paths(),
        occur_source_numbers_log_lines_transforms_matches_and_expands_symbol_shorthands(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Helm parity test");
    assert_oracle_batch_cases(helm_oracle(), test_name, "helm_parity", &cases);
}
