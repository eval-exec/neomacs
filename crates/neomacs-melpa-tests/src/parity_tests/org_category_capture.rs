use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_CATEGORY_CAPTURE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'org)
(require 'org-capture)
(require 'org-category-capture)

(defun neomacs-occ-test-heading-record ()
  "Return the current Org heading's practical planning fields."
  (list :level (org-current-level)
        :heading (org-get-heading t t t t)
        :category (occ-get-heading-category)
        :todo (org-get-todo-state)
        :owner (org-entry-get (point) "OWNER")))

(defun neomacs-occ-test-capture-error (function)
  "Return FUNCTION's value or its exact error data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-occ-test-plain-keyed-values (values)
  "Remove incidental display properties from category keys in VALUES."
  (mapcar (lambda (entry)
            (cons (substring-no-properties (car entry)) (cdr entry)))
          values))

(defclass neomacs-occ-test-strategy (occ-strategy)
  ((buffer :initarg :buffer)))

(cl-defmethod occ-get-capture-marker
  ((strategy neomacs-occ-test-strategy) context)
  "Find or create CONTEXT's category in STRATEGY's target buffer."
  (with-current-buffer (oref strategy buffer)
    (occ-goto-or-insert-category-heading (oref context category))
    (point-marker)))

(cl-defmethod occ-target-entry-p
  ((_strategy neomacs-occ-test-strategy) _context)
  "Insert practical test captures below their category heading."
  t)

(defvar neomacs-occ-test-capture-text nil)

(defun neomacs-occ-test-place-template (&rest _arguments)
  "Place one capture using Org's real entry placement implementation."
  (goto-char (org-capture-get :pos))
  (setq-local outline-level 'org-outline-level)
  (org-capture-place-entry)
  (insert neomacs-occ-test-capture-text))

(defun neomacs-occ-test-run-capture (buffer category text)
  "Capture TEXT below CATEGORY in BUFFER through `occ-capture'."
  (let ((neomacs-occ-test-capture-text text))
    (with-current-buffer buffer
      (occ-capture
       (make-instance
        'occ-context
        :category category
        :options nil
        :strategy (make-instance 'neomacs-occ-test-strategy :buffer buffer)
        :template "* TODO %?\n")))))
"###;

fn package_registration_exposes_context_strategy_and_capture_surface() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((descriptor (cadr (assq 'org-category-capture package-alist)))
       (context (make-instance
                 'occ-context
                 :category "Release"
                 :template "* TODO %?"
                 :options '(:immediate-finish t)
                 :strategy (make-instance 'neomacs-occ-test-strategy
                                          :buffer (current-buffer))))
       (template (occ-build-capture-template
                  context :character "r" :heading "Release task")))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'org-category-capture) t))
   :classes
   (list (child-of-class-p 'occ-context 'eieio-default-superclass)
         (child-of-class-p 'neomacs-occ-test-strategy 'occ-strategy))
   :surface
   (mapcar #'fboundp
           '(occ-capture occ-build-capture-template
             occ-capture-edit-at-marker occ-capture-goto-marker
             occ-get-category-heading-location
             occ-goto-or-insert-category-heading
             occ-get-value-by-category occ-map-entries-for-category))
   :custom occ-auto-insert-category-heading
   :template
   (list :key (nth 0 template)
         :description (nth 1 template)
         :type (nth 2 template)
         :target-kind (car (nth 3 template))
         :target-callable (functionp (cadr (nth 3 template)))
         :body (nth 4 template)
         :options (nthcdr 5 template))))
"###;
    let expected = expect![[
        r#"OK (:package (:name org-category-capture :version "20260127.711" :requirements ((org (9 0 0)) (emacs (24))) :feature t) :classes (t t) :surface (t t t t t t t t) :custom nil :template (:key "r" :description "Release task" :type entry :target-kind function :target-callable t :body "* TODO %?" :options (:immediate-finish t)))"#
    ]];
    ParityBatchCase::value(
        "package_registration_exposes_context_strategy_and_capture_surface",
        elisp_form,
        expected,
    )
}

fn planning_dashboard_indexes_top_level_categories_and_explicit_property_overrides()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Delivery portfolio\n"
          "* Platform\n"
          ":PROPERTIES:\n:OWNER: Alice\n:END:\n"
          "** TODO Upgrade database\n"
          "* Internal codename\n"
          ":PROPERTIES:\n:CATEGORY: Release 2026.08\n:OWNER: Bob\n:END:\n"
          "** TODO Publish binaries\n"
          "* Documentation\n"
          "** DONE Refresh guide\n")
  (list
   :index
   (occ-get-value-by-category
    :property-fn (lambda () (org-entry-get (point) "OWNER")))
   :records
   (org-map-entries #'neomacs-occ-test-heading-record nil nil
                    (occ-level-filter 1))
   :categories (mapcar #'car (occ-get-value-by-category))))
"###;
    let expected = expect![[
        r#"OK (:index (("Platform" . "Alice") ("Release 2026.08" . "Bob") ("Documentation")) :records ((:level 1 :heading "Platform" :category "Platform" :todo nil :owner "Alice") (:level 1 :heading "Internal codename" :category "Release 2026.08" :todo nil :owner "Bob") (:level 1 :heading "Documentation" :category "Documentation" :todo nil :owner nil)) :categories ("Platform" "Release 2026.08" "Documentation"))"#
    ]];
    ParityBatchCase::value(
        "planning_dashboard_indexes_top_level_categories_and_explicit_property_overrides",
        elisp_form,
        expected,
    )
}

fn category_lookup_prefers_properties_and_creation_is_idempotent() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (org-mode)
  (insert "* Display name\n"
          ":PROPERTIES:\n:CATEGORY: api-team\n:END:\n"
          "** TODO Existing task\n"
          "* api-team\n"
          "* Operations\n")
  (let ((property-location (occ-get-category-heading-location "api-team")))
    (goto-char (point-min))
    (occ-goto-or-insert-category-heading "api-team")
    (let ((first-point (point))
          (before (buffer-string)))
      (occ-goto-or-insert-category-heading "api-team")
      (let ((idempotent (equal before (buffer-string))))
        (goto-char (point-max))
        (occ-goto-or-insert-category-heading
         "release train"
         :build-heading (lambda (category)
                          (format "Release: %s" category)))
        (let ((created-point (point)))
        (list
         :property-location property-location
         :selected-heading
         (save-excursion (goto-char first-point) (org-get-heading t t t t))
         :idempotent idempotent
         :created-heading
         (save-excursion (goto-char created-point) (org-get-heading t t t t))
         :created-category
         (save-excursion (goto-char created-point) (org-get-category))
         :buffer (buffer-string)))))))
"###;
    let expected = expect![[
        r#"OK (:property-location 1 :selected-heading "Display name" :idempotent t :created-heading "Release: release train" :created-category "release train" :buffer "* Display name\n:PROPERTIES:\n:CATEGORY: api-team\n:END:\n** TODO Existing task\n* api-team\n* Operations\n* Release: release train\n:PROPERTIES:\n:CATEGORY: release train\n:END:\n")"#
    ]];
    ParityBatchCase::value(
        "category_lookup_prefers_properties_and_creation_is_idempotent",
        elisp_form,
        expected,
    )
}

fn portfolio_subtree_reports_only_direct_teams_and_maps_each_team_workflow() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (org-mode)
  (insert "* Portfolio\n"
          "** API team\n:PROPERTIES:\n:CATEGORY: api\n:OWNER: Ana\n:END:\n"
          "*** TODO Deploy canary\n"
          "*** DONE Rotate keys\n"
          "** Web team\n:PROPERTIES:\n:OWNER: Wei\n:END:\n"
          "*** TODO Publish assets\n"
          "* Outside\n** TODO Not in portfolio\n")
  (goto-char (org-find-exact-headline-in-buffer "Portfolio" nil t))
  (let ((teams
         (occ-get-value-by-category
          :goto-subtree (lambda () nil)
          :property-fn (lambda () (org-entry-get (point) "OWNER"))))
        (api-work
         (occ-map-entries-for-category
          "api" #'neomacs-occ-test-heading-record
          :goto-subheading (lambda () nil))))
    (list :teams teams :api-work api-work
          :missing (occ-map-entries-for-category "mobile" #'org-get-heading))))
"###;
    let expected = expect![[
        r#"OK (:teams (("api" . "Ana") ("Web team" . "Wei")) :api-work ((:level 2 :heading "API team" :category "api" :todo nil :owner "Ana") (:level 3 :heading "Deploy canary" :category "TODO Deploy canary" :todo "TODO" :owner nil) (:level 3 :heading "Rotate keys" :category "DONE Rotate keys" :todo "DONE" :owner nil)) :missing nil)"#
    ]];
    ParityBatchCase::value(
        "portfolio_subtree_reports_only_direct_teams_and_maps_each_team_workflow",
        elisp_form,
        expected,
    )
}

fn filepath_helpers_read_properties_and_evaluated_values_from_a_real_org_file() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (make-temp-file "neomacs-occ-file-" t))
       (file (expand-file-name "portfolio.org" root))
       result buffer)
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "* Platform\n"
                  ":PROPERTIES:\n:OWNER: Alice\n:WEIGHT: 7\n:END:\n"
                  "* Release\n"
                  ":PROPERTIES:\n:CATEGORY: ship\n:OWNER: Bob\n"
                  ":WEIGHT: (:risk high :score 11)\n:END:\n"))
        (setq result
              (list
               :owners
               (neomacs-occ-test-plain-keyed-values
                (occ-get-property-by-category-from-filepath file "OWNER"))
               :weights
               (neomacs-occ-test-plain-keyed-values
                (occ-read-property-by-category-from-filepath file "WEIGHT"))
               :categories
               (mapcar #'substring-no-properties
                       (occ-get-categories-from-filepath file))))
        (setq buffer (get-file-buffer file))
        (append result
                (list :buffer-opened (and (buffer-live-p buffer) t)
                      :mode (and buffer (buffer-local-value
                                         'major-mode buffer)))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))
    (delete-directory root t)))
"###;
    let expected = expect![[
        r#"OK (:owners (("Platform" . "Alice") ("ship" . "Bob")) :weights (("Platform" . 7) ("ship" :risk high :score 11)) :categories ("Platform" "ship") :buffer-opened t :mode org-mode)"#
    ]];
    ParityBatchCase::value(
        "filepath_helpers_read_properties_and_evaluated_values_from_a_real_org_file",
        elisp_form,
        expected,
    )
}

fn context_templates_delegate_markers_and_capture_entry_policy_to_the_strategy() -> ParityBatchCase
{
    let elisp_form = r###"
(let ((target (generate-new-buffer " *occ-target*"))
      (origin (current-buffer))
      capture-call edit-state goto-state)
  (unwind-protect
      (progn
        (with-current-buffer target
          (org-mode)
          (insert "* Existing\n"))
        (let* ((strategy (make-instance 'neomacs-occ-test-strategy
                                        :buffer target))
               (context (make-instance
                         'occ-context
                         :category "Release"
                         :template "* TODO Ship %?\n"
                         :options '(:prepend t :immediate-finish t)
                         :strategy strategy)))
          (cl-letf (((symbol-function 'org-capture)
                     (lambda (goto keys)
                       (let ((template (car org-capture-templates)))
                         (setq capture-call
                               (list :goto goto :keys keys
                                     :key (nth 0 template)
                                     :description (nth 1 template)
                                     :type (nth 2 template)
                                     :target (car (nth 3 template))
                                     :body (nth 4 template)
                                     :options (nthcdr 5 template)))))))
            (occ-capture context))
          (with-current-buffer origin
            (occ-capture-edit-at-marker context)
            (setq edit-state
                  (list :target (eq (current-buffer) target)
                        :heading (org-get-heading t t t t)
                        :category (org-get-category))))
          (with-current-buffer origin
            (occ-capture-goto-marker context)
            (setq goto-state
                  (list :target (eq (current-buffer) target)
                        :heading (org-get-heading t t t t))))
          (list :capture capture-call
                :target-entry (and (occ-target-entry-p strategy context) t)
                :edit edit-state
                :goto goto-state
                :target-buffer
                (with-current-buffer target (buffer-string)))))
    (when (buffer-live-p target) (kill-buffer target))))
"###;
    let expected = expect![[
        r#"OK (:capture (:goto nil :keys "p" :key "p" :description "Category TODO" :type entry :target function :body "* TODO Ship %?\n" :options (:prepend t :immediate-finish t)) :target-entry t :edit (:target t :heading "Release" :category "Release") :goto (:target t :heading "Release") :target-buffer "* Existing\n* Release\n:PROPERTIES:\n:CATEGORY: Release\n:END:\n")"#
    ]];
    ParityBatchCase::value(
        "context_templates_delegate_markers_and_capture_entry_policy_to_the_strategy",
        elisp_form,
        expected,
    )
}

fn real_org_capture_places_repeated_existing_and_new_category_todos() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (org-mode)
  (insert "* Platform\n"
          "* Internal name\n"
          ":PROPERTIES:\n:CATEGORY: Release\n:END:\n"
          "* Documentation\n")
  (let ((target (current-buffer))
        (org-adapt-indentation 1))
    (cl-letf (((symbol-function 'org-capture-place-template)
               #'neomacs-occ-test-place-template)
              ((symbol-function 'org-capture-narrow)
               (lambda (&rest _) nil)))
      (neomacs-occ-test-run-capture target "Platform" "Prepare migration")
      (neomacs-occ-test-run-capture target "Release" "Publish binaries")
      (neomacs-occ-test-run-capture target "Platform" "Verify rollback")
      (neomacs-occ-test-run-capture target "Security" "Rotate signing key"))
    (with-current-buffer target
      (list
       :buffer (buffer-string)
       :platform
       (occ-map-entries-for-category "Platform"
                                     #'neomacs-occ-test-heading-record)
       :release
       (occ-map-entries-for-category "Release"
                                     #'neomacs-occ-test-heading-record)
       :security
       (occ-map-entries-for-category "Security"
                                     #'neomacs-occ-test-heading-record)))))
"###;
    let expected = expect![[
        r#"OK (:buffer "* Platform\n** TODO Prepare migration\n** TODO Verify rollback\n* Internal name\n:PROPERTIES:\n:CATEGORY: Release\n:END:\n** TODO Publish binaries\n* Documentation\n* Security\n  :PROPERTIES:\n  :CATEGORY: Security\n  :END:\n** TODO Rotate signing key\n" :platform ((:level 1 :heading "Platform" :category "Platform" :todo nil :owner nil) (:level 2 :heading "Prepare migration" :category "TODO Prepare migration" :todo "TODO" :owner nil) (:level 2 :heading "Verify rollback" :category "TODO Verify rollback" :todo "TODO" :owner nil)) :release ((:level 1 :heading "Internal name" :category "Release" :todo nil :owner nil) (:level 2 :heading "Publish binaries" :category "TODO Publish binaries" :todo "TODO" :owner nil)) :security ((:level 1 :heading "Security" :category "Security" :todo nil :owner nil) (:level 2 :heading "Rotate signing key" :category "TODO Rotate signing key" :todo "TODO" :owner nil)))"#
    ]];
    ParityBatchCase::value(
        "real_org_capture_places_repeated_existing_and_new_category_todos",
        elisp_form,
        expected,
    )
}

fn property_insertion_subheading_layout_special_names_and_non_org_errors_are_exact()
-> ParityBatchCase {
    let elisp_form = r###"
(let (org-result errors)
  (with-temp-buffer
    (org-mode)
    (insert "* project-with-dashes\n"
            "* project_with_underscores\n"
            "* project.with.dots\n"
            "* Parent\n:PROPERTIES:\n:OWNER: Ops\n:END:\nBody\n")
    (let ((occ-auto-insert-category-heading t))
      (goto-char (org-find-exact-headline-in-buffer
                  "project-with-dashes" nil t))
      (occ-get-heading-category))
    (goto-char (org-find-exact-headline-in-buffer "Parent" nil t))
    (occ-end-of-properties)
    (let ((end-column (current-column)))
      (occ-insert-subheading)
      (insert "Child runbook")
      (setq org-result
            (list
             :locations
             (mapcar (lambda (name)
                       (and (occ-get-category-heading-location name) t))
                     '("project-with-dashes"
                       "project_with_underscores"
                       "project.with.dots"))
             :end-column end-column
             :buffer (buffer-string)))))
  (with-temp-buffer
    (fundamental-mode)
    (setq errors
          (list
           (neomacs-occ-test-capture-error
            (lambda () (occ-get-category-heading-location "x")))
           (neomacs-occ-test-capture-error
            (lambda () (occ-get-heading-category))))))
  (list :org org-result :errors errors))
"###;
    let expected = expect![[
        r#"OK (:org (:locations (t t t) :end-column 5 :buffer "* project-with-dashes\n:PROPERTIES:\n:CATEGORY: project-with-dashes\n:END:\n* project_with_underscores\n* project.with.dots\n* Parent\n:PROPERTIES:\n:OWNER: Ops\n:END:\n** Child runbook\nBody\n") :errors ((:error error :data ("Can’t get category heading in non org-mode file") :message "Can’t get category heading in non org-mode file") (:ok nil)))"#
    ]];
    ParityBatchCase::value(
        "property_insertion_subheading_layout_special_names_and_non_org_errors_are_exact",
        elisp_form,
        expected,
    )
}

#[test]
fn org_category_capture_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(ORG_CATEGORY_CAPTURE_MELPA_PIN, "org-category-capture.el")
            .expect("prepare revision-pinned Org Category Capture source below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "org-category-capture-package-batch",
        "Org Category Capture",
        &[
            package_registration_exposes_context_strategy_and_capture_surface(),
            planning_dashboard_indexes_top_level_categories_and_explicit_property_overrides(),
            category_lookup_prefers_properties_and_creation_is_idempotent(),
            portfolio_subtree_reports_only_direct_teams_and_maps_each_team_workflow(),
            filepath_helpers_read_properties_and_evaluated_values_from_a_real_org_file(),
            context_templates_delegate_markers_and_capture_entry_policy_to_the_strategy(),
            real_org_capture_places_repeated_existing_and_new_category_todos(),
            property_insertion_subheading_layout_special_names_and_non_org_errors_are_exact(),
        ],
    );
}
