use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, IVY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const IVY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const IVY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ivy)

(defvar ivy-test-expression nil)
(defvar ivy-test-result nil)
(defvar ivy-test-history nil)
(defvar ivy-test-action-events nil)
(defvar ivy-test-region-candidates
  '("deploy-development" "deploy-production" "deploy-staging"))

(defun ivy-test-eval-expression ()
  (interactive)
  (setq ivy-test-result (eval ivy-test-expression t)))

(global-set-key (kbd "C-c e") #'ivy-test-eval-expression)

(defun ivy-test-with (expression keys)
  (let ((ivy-test-expression expression)
        (inhibit-message t)
        (origin (current-buffer)))
    (save-window-excursion
      (unwind-protect
          (execute-kbd-macro
           (vconcat (kbd "C-c e") (kbd keys)))
        (switch-to-buffer origin)))
    ivy-test-result))

(defun ivy-test-open-order (candidate)
  (push (list :open (copy-tree candidate)) ivy-test-action-events))

(defun ivy-test-retry-order (candidate)
  (push (list :retry (copy-tree candidate)) ivy-test-action-events))

(defun ivy-test-archive-order (candidate)
  (push (list :archive (copy-tree candidate)) ivy-test-action-events))

(defun ivy-test-bulk-archive-orders (candidates)
  (push (list :bulk-archive (copy-tree candidates)) ivy-test-action-events))

(defun ivy-test-complete-deployment ()
  (interactive)
  (completion-in-region
   (save-excursion (skip-chars-backward "[:word:]-") (point))
   (point)
   ivy-test-region-candidates))

(global-set-key (kbd "C-c d") #'ivy-test-complete-deployment)

(defun ivy-test-complete-in-buffer (initial mode keys)
  (let ((buffer (generate-new-buffer " *ivy-region-workflow*")))
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer buffer)
          (funcall mode)
          (insert initial)
          (execute-kbd-macro (kbd keys))
          (buffer-string))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"##;

fn ivy_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IVY_MELPA_PIN, "ivy.el")
        .expect("prepare pinned Ivy source below ./tmp")
        .with_prelude(IVY_TEST_PRELUDE)
        .with_timeout(IVY_TEST_TIMEOUT)
}

fn interactive_selection_handles_navigation_fuzzy_search_dynamic_sources_and_literal_input()
-> ParityBatchCase {
    let elisp_form = r##"
(list
 :navigated
 (ivy-test-with
  '(ivy-read
    "Order: "
    '("order-417 queued" "order-418 retry" "order-419 complete"))
  "order C-n RET")
 :fuzzy
 (ivy-test-with
  '(let ((ivy-re-builders-alist '((t . ivy--regex-fuzzy))))
     (ivy-read
      "Service: "
      '("orders-api" "payments-worker" "inventory-worker")))
  "pmntwrkr RET")
 :dynamic
 (ivy-test-with
  '(ivy-read
    "Environment: "
    (lambda (input)
      (mapcar (lambda (suffix) (concat input "-" suffix))
              '("blue" "green" "canary")))
    :dynamic-collection t)
  "production C-n RET")
 :literal
 (ivy-test-with
  '(let ((ivy-use-selectable-prompt t))
     (ivy-read "Incident: " '("INC-417" "INC-418")
               :require-match nil))
  "INC-999 C-M-j"))
"##;
    let expect = expect![[
        r##"OK (:navigated "order-418 retry" :fuzzy "payments-worker" :dynamic "production-green" :literal "INC-999")"##
    ]];
    ParityBatchCase::value(
        "interactive_selection_handles_navigation_fuzzy_search_dynamic_sources_and_literal_input",
        elisp_form,
        expect,
    )
}

fn order_dashboard_dispatches_default_extra_and_marked_multi_actions_with_full_records()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((orders '(("ORD-417 queued" . (:id 417 :state queued :amount 1299))
                ("ORD-418 retry" . (:id 418 :state retry :amount 8450))
                ("ORD-419 complete" . (:id 419 :state complete :amount 3200)))))
  (setq ivy-test-action-events nil)
  (ivy-set-actions
   'ivy-test-order-dashboard
   '(("r" ivy-test-retry-order "retry")
     ("a" ivy-test-archive-order "archive"
      ivy-test-bulk-archive-orders)))
  (ivy-test-with
   `(ivy-read "Order: " ',orders
              :action #'ivy-test-open-order
              :caller 'ivy-test-order-dashboard)
   "ORD-419 RET")
  (ivy-test-with
   `(ivy-read "Order: " ',orders
              :action #'ivy-test-open-order
              :caller 'ivy-test-order-dashboard)
   "ORD-418 M-o r")
  (ivy-test-with
   `(ivy-read "Order: " ',orders
              :action #'ivy-test-open-order
              :multi-action #'ivy-test-bulk-archive-orders
              :caller 'ivy-test-order-dashboard)
   "M-a M-o a")
  (list
   :actions
   (mapcar (lambda (entry) (list (car entry) (nth 2 entry)))
           (plist-get ivy--actions-list 'ivy-test-order-dashboard))
   :events (nreverse ivy-test-action-events)))
"##;
    let expect = expect![[
        r##"OK (:actions (("r" "retry") ("a" "archive")) :events ((:open ("ORD-419 complete" :id 419 :state complete :amount 3200)) (:retry ("ORD-418 retry" :id 418 :state retry :amount 8450)) (:bulk-archive (("ORD-417 queued" :id 417 :state queued :amount 1299) ("ORD-418 retry" :id 418 :state retry :amount 8450) ("ORD-419 complete" :id 419 :state complete :amount 3200)))))"##
    ]];
    ParityBatchCase::value(
        "order_dashboard_dispatches_default_extra_and_marked_multi_actions_with_full_records",
        elisp_form,
        expect,
    )
}

fn ivy_mode_routes_completing_read_and_preserves_defaults_require_match_and_history()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((original-read completing-read-function)
      (original-region completion-in-region-function)
      (ivy-test-history '("staging" "development"))
      selected-default selected-required selected-custom installed restored)
  (unwind-protect
      (progn
        (ivy-mode 1)
        (setq installed
              (list (eq completing-read-function #'ivy-completing-read)
                    (eq completion-in-region-function
                        #'ivy-completion-in-region)))
        (setq selected-default
              (ivy-test-with
               '(completing-read
                 "Environment: " '("development" "staging" "production")
                 nil t nil 'ivy-test-history "staging")
               "RET"))
        (setq selected-required
              (ivy-test-with
               '(completing-read
                 "Environment: " '("development" "staging" "production")
                 nil t nil 'ivy-test-history)
               "prod RET"))
        (setq selected-custom
              (ivy-test-with
               '(completing-read
                 "Environment: " '("development" "staging" "production")
                 nil nil nil 'ivy-test-history)
               "canary RET")))
    (ivy-mode -1)
    (setq restored
          (list (eq completing-read-function original-read)
                (eq completion-in-region-function original-region))))
  (list
   :installed installed
   :selected (list selected-default selected-required selected-custom)
   :history (mapcar #'substring-no-properties ivy-test-history)
   :restored restored))
"##;
    let expect = expect![[
        r##"OK (:installed (t t) :selected ("staging" "production" "canary") :history ("canary" "production" "staging" "development") :restored (t t))"##
    ]];
    ParityBatchCase::value(
        "ivy_mode_routes_completing_read_and_preserves_defaults_require_match_and_history",
        elisp_form,
        expect,
    )
}

fn dashboard_search_language_combines_ordered_unordered_negative_fuzzy_and_case_rules()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((candidates
       '("order-417 owner:alice status:failed"
         "order-418 owner:bob status:queued"
         "order-419 owner:alice status:archived"
         "payments-worker region:east status:healthy"
         "inventory-worker region:west status:degraded"))
      (ivy-last (make-ivy-state :caller 'ivy-test-dashboard))
      (ivy-sort-functions-alist nil)
      (ivy-sort-matches-functions-alist nil))
  (cl-labels
      ((search (builder input &optional case-rule)
         (let ((ivy--regex-function builder)
               (ivy-case-fold-search (or case-rule 'auto))
               (ivy--old-re nil)
               (ivy--old-cands nil)
               (ivy--index 0))
           (copy-sequence (ivy--filter input candidates)))))
    (list
     :ordered (search #'ivy--regex "owner:alice failed")
     :unordered
     (search #'ivy--regex-ignore-order "status:failed owner:alice")
     :negative
     (search #'ivy--regex-ignore-order "owner:alice !archived")
     :fuzzy (search #'ivy--regex-fuzzy "pmntwrkr")
     :case
     (list (search #'ivy--regex "ORDER-418")
           (search #'ivy--regex "order-418")))))
"##;
    let expect = expect![[
        r##"OK (:ordered ("order-417 owner:alice status:failed") :unordered ("order-417 owner:alice status:failed") :negative ("order-417 owner:alice status:failed") :fuzzy ("payments-worker region:east status:healthy") :case (nil ("order-418 owner:bob status:queued")))"##
    ]];
    ParityBatchCase::value(
        "dashboard_search_language_combines_ordered_unordered_negative_fuzzy_and_case_rules",
        elisp_form,
        expect,
    )
}

fn completion_in_region_replaces_source_text_and_restores_global_completion_hooks()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((original-read completing-read-function)
      (original-region completion-in-region-function)
      deployment elisp-symbol installed)
  (unwind-protect
      (progn
        (ivy-mode 1)
        (setq installed
              (eq completion-in-region-function
                  #'ivy-completion-in-region))
        (setq deployment
              (ivy-test-complete-in-buffer
               "deploy" #'fundamental-mode "C-c d C-n RET"))
        (setq elisp-symbol
              (ivy-test-complete-in-buffer
               " emacs-lisp-mode-h" #'emacs-lisp-mode "C-M-i")))
    (ivy-mode -1))
  (list
   :installed installed
   :deployment deployment
   :elisp-symbol elisp-symbol
   :restored
   (list (eq completing-read-function original-read)
         (eq completion-in-region-function original-region))))
"##;
    let expect = expect![[
        r##"OK (:installed t :deployment "deploy-production" :elisp-symbol " emacs-lisp-mode-hook" :restored (t t))"##
    ]];
    ParityBatchCase::value(
        "completion_in_region_replaces_source_text_and_restores_global_completion_hooks",
        elisp_form,
        expect,
    )
}

fn named_sessions_resume_their_own_input_candidates_and_actions() -> ParityBatchCase {
    let elisp_form = r##"
(let ((ivy-last ivy-last)
      ivy-text ivy--all-candidates ivy--sessions)
  (ivy-test-with
   '(ivy-read "Orders: " '("ORD-417" "ORD-418" "ORD-419")
              :caller 'ivy-test-orders
              :action #'ignore)
   "418 RET")
  (ivy-test-with
   '(ivy-read "Services: " '("api" "worker" "scheduler"))
   "work RET")
  (ivy-test-with
   '(ivy-read "Deployments: " '("blue" "green" "canary")
              :action #'ignore
              :extra-props '(:session ivy-test-deployments))
   "can RET")
  (let ((before (list ivy-text (mapcar #'car ivy--sessions)))
        (orders
         (ivy-test-with
          '(let ((current-prefix-arg '(4))) (ivy-resume))
          "ivy-test-orders RET RET"))
        (deployment
         (ivy-test-with '(ivy-resume 'ivy-test-deployments) "RET")))
    (list
     :before before
     :resumed (list orders deployment)
     :last-input ivy-text)))
"##;
    let expect = expect![[
        r##"OK (:before ("can" (ivy-test-deployments ivy-test-orders)) :resumed ("ORD-418" "canary") :last-input "can")"##
    ]];
    ParityBatchCase::value(
        "named_sessions_resume_their_own_input_candidates_and_actions",
        elisp_form,
        expect,
    )
}

#[test]
fn ivy_package_batch() {
    let cases = vec![
        interactive_selection_handles_navigation_fuzzy_search_dynamic_sources_and_literal_input(),
        order_dashboard_dispatches_default_extra_and_marked_multi_actions_with_full_records(),
        ivy_mode_routes_completing_read_and_preserves_defaults_require_match_and_history(),
        dashboard_search_language_combines_ordered_unordered_negative_fuzzy_and_case_rules(),
        completion_in_region_replaces_source_text_and_restores_global_completion_hooks(),
        named_sessions_resume_their_own_input_candidates_and_actions(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Ivy parity test");
    assert_oracle_batch_cases(ivy_oracle(), test_name, "ivy_parity", &cases);
}
