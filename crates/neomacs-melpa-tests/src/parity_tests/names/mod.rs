use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, NAMES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const NAMES_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const NAMES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'names)
"##;

fn names_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NAMES_MELPA_PIN, "names.el")
        .expect("prepare pinned Names source below ./tmp")
        .with_prelude(NAMES_TEST_PRELUDE)
        .with_timeout(NAMES_TEST_TIMEOUT)
}

fn independent_modules_can_reuse_short_names_without_cross_talk() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (define-namespace npt-catalog-
    (defvar records nil)
    (defun normalize (sku)
      (downcase (string-trim sku)))
    (defun add (sku quantity)
      (setq records
            (cons (cons (normalize sku) quantity) records))
      (copy-tree records))
    (defun snapshot ()
      (sort (copy-tree records)
            (lambda (left right) (string< (car left) (car right))))))
  (define-namespace npt-orders-
    (defvar records nil)
    (defun normalize (customer)
      (upcase (string-trim customer)))
    (defun add (customer sku)
      (setq records
            (append records (list (list (normalize customer) sku))))
      (copy-tree records))
    (defun snapshot () (copy-tree records)))
  (setq npt-catalog-records nil
        npt-orders-records nil)
  (list
   :catalog-events
   (list (npt-catalog-add " SKU-B " 2)
         (npt-catalog-add "Sku-A" 5))
   :order-events
   (list (npt-orders-add " alice " "sku-a")
         (npt-orders-add "Bob" "sku-b"))
   :catalog (npt-catalog-snapshot)
   :orders (npt-orders-snapshot)
   :cells
   (list (boundp 'npt-catalog-records)
         (boundp 'npt-orders-records)
         (fboundp 'npt-catalog-add)
         (fboundp 'npt-orders-add))))
"##;
    let expect = expect![[
        r####"OK (:catalog-events ((("sku-b" . 2)) (("sku-a" . 5) ("sku-b" . 2))) :order-events ((("ALICE" "sku-a")) (("ALICE" "sku-a") ("BOB" "sku-b"))) :catalog (("sku-a" . 5) ("sku-b" . 2)) :orders (("ALICE" "sku-a") ("BOB" "sku-b")) :cells (t t t t))"####
    ]];
    ParityBatchCase::value(
        "independent_modules_can_reuse_short_names_without_cross_talk",
        elisp_form,
        expect,
    )
}

fn split_declarations_reuse_prior_namespace_cells_and_protect_global_calls() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (defun names-parity-slug (text)
    (replace-regexp-in-string "[[:space:]]+" "-" (downcase text)))
  (define-namespace npt-report-
    (defvar factor 3)
    (defun length (records)
      (* factor (::length records))))
  (define-namespace npt-report-
    :global
    :no-let-vars
    (defun summarize (title records)
      (let ((factor "request-local"))
        (list :title (::names-parity-slug title)
              :ordinary-count (::length records)
              :weighted-count (length records)
              :local-factor factor))))
  (setq npt-report-factor 4)
  (list
   :summary (npt-report-summarize "Quarterly Sales" '(a b c))
   :function-cell (npt-report-length '(north south))
   :variable-cell npt-report-factor
   :global-length-unchanged (length '(1 2 3 4))))
"##;
    let expect = expect![[
        r####"OK (:summary (:title "quarterly-sales" :ordinary-count 3 :weighted-count 12 :local-factor "request-local") :function-cell 8 :variable-cell 4 :global-length-unchanged 4)"####
    ]];
    ParityBatchCase::value(
        "split_declarations_reuse_prior_namespace_cells_and_protect_global_calls",
        elisp_form,
        expect,
    )
}

fn macro_pipeline_preserves_local_bindings_and_evaluates_backquoted_forms() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (define-namespace npt-pipeline-
    (defvar audit nil)
    (defmacro with-stage (name &rest body)
      (declare (indent 1) (debug (form body)))
      `(let ((started (length npt-pipeline-audit)))
         (push (list :start ,name) npt-pipeline-audit)
         (prog1 (progn ,@body)
           (push (list :finish ,name :previous-events started)
                 npt-pipeline-audit))))
    (defun normalize (row)
      (upcase (string-trim row)))
    (defun run (rows)
      (with-stage "normalize-import"
        (mapcar
         (lambda (row)
           (condition-case error-data
               (let* ((normalized (normalize row))
                      (columns (split-string normalized "[[:space:]]+" t)))
                 (list :source row
                       :normalized normalized
                       :columns columns))
             (error
              (list :source row
                    :error (car error-data)))))
         rows)))
    (defun audit-log () (reverse audit)))
  (setq npt-pipeline-audit nil)
  (let ((records (npt-pipeline-run '(" alpha one " nil "Beta Two"))))
    (list :records records
          :audit (npt-pipeline-audit-log)
          :macro (macrop 'npt-pipeline-with-stage)
          :debug-spec (get 'npt-pipeline-with-stage 'edebug-form-spec))))
"##;
    let expect = expect![[
        r####"OK (:records ((:source " alpha one " :normalized "ALPHA ONE" :columns ("ALPHA" "ONE")) (:source nil :error wrong-type-argument) (:source "Beta Two" :normalized "BETA TWO" :columns ("BETA" "TWO"))) :audit ((:start "normalize-import") (:finish "normalize-import" :previous-events 0)) :macro t :debug-spec (form body))"####
    ]];
    ParityBatchCase::value(
        "macro_pipeline_preserves_local_bindings_and_evaluates_backquoted_forms",
        elisp_form,
        expect,
    )
}

fn cl_defun_keyword_api_builds_records_with_namespaced_mutable_state() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (define-namespace npt-api-
    (defvar issued 100)
    (cl-defun build-record (&key id (status 'queued) labels)
      (let* ((serial (cl-incf issued))
             (resolved-id (or id (format "job-%d" serial))))
        (list :id resolved-id
              :status status
              :labels (sort (copy-sequence labels) #'string<)
              :serial serial)))
    (defun build-batch (specifications)
      (mapcar (lambda (specification)
                (apply #'build-record specification))
              specifications)))
  (setq npt-api-issued 100)
  (list
   :records
   (npt-api-build-batch
    '((:id "release" :labels ("urgent" "backend"))
      (:status running :labels ("worker"))
      (:id "docs" :status done :labels nil)))
   :issued npt-api-issued))
"##;
    let expect = expect![[
        r####"OK (:records ((:id "release" :status queued :labels ("backend" "urgent") :serial 101) (:id "job-102" :status running :labels ("worker") :serial 102) (:id "docs" :status done :labels nil :serial 103)) :issued 103)"####
    ]];
    ParityBatchCase::value(
        "cl_defun_keyword_api_builds_records_with_namespaced_mutable_state",
        elisp_form,
        expect,
    )
}

fn derived_mode_definition_creates_namespaced_mode_state_map_and_hook() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (define-namespace npt-notes-
    (defvar activations 0)
    (define-derived-mode notes-mode text-mode "NPT-Notes"
      "Edit deterministic namespaced notes."
      (setq-local comment-start "# ")
      (setq-local tab-width 3)
      (setq activations (1+ activations))))
  (setq npt-notes-activations 0)
  (let (hook-events)
    (add-hook 'npt-notes-notes-mode-hook
              (lambda ()
                (push (list major-mode comment-start tab-width) hook-events)))
    (with-temp-buffer
      (insert "title\n# detail\n")
      (npt-notes-notes-mode)
      (list
       :buffer (buffer-string)
       :major-mode major-mode
       :mode-name mode-name
       :derived (and (derived-mode-p 'text-mode) t)
       :comment-start comment-start
       :tab-width tab-width
       :hook-events (nreverse hook-events)
       :activations npt-notes-activations
       :generated
       (list (fboundp 'npt-notes-notes-mode)
             (boundp 'npt-notes-notes-mode-hook)
             (boundp 'npt-notes-notes-mode-map)
             (keymapp npt-notes-notes-mode-map))))))
"##;
    let expect = expect![[
        r####"OK (:buffer "title\n# detail\n" :major-mode npt-notes-notes-mode :mode-name "NPT-Notes" :derived t :comment-start "# " :tab-width 3 :hook-events ((npt-notes-notes-mode "# " 3)) :activations 1 :generated (t t t t))"####
    ]];
    ParityBatchCase::value(
        "derived_mode_definition_creates_namespaced_mode_state_map_and_hook",
        elisp_form,
        expect,
    )
}

fn package_metadata_generates_version_command_group_and_custom_defaults() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (define-namespace npt-config-
    :version "2.4.1"
    :package npt-dashboard
    :group applications
    (defcustom refresh-interval 15
      "Seconds between dashboard refreshes."
      :type 'integer)
    (defcustom display-style 'compact
      "Dashboard presentation style."
      :type '(choice (const compact) (const detailed)))
    (defun settings ()
      (list refresh-interval display-style)))
  (setq npt-config-refresh-interval 30
        npt-config-display-style 'detailed)
  (list
   :version (npt-config-version)
   :settings (npt-config-settings)
   :group
   (list (mapcar #'car (get 'npt-dashboard 'custom-group))
         (get 'npt-dashboard 'group-documentation))
   :refresh
   (list (and (custom-variable-p 'npt-config-refresh-interval) t)
         (eval (car (get 'npt-config-refresh-interval 'standard-value)))
         (get 'npt-config-refresh-interval 'custom-type))
   :style
   (list (and (custom-variable-p 'npt-config-display-style) t)
         (eval (car (get 'npt-config-display-style 'standard-value)))
         (get 'npt-config-display-style 'custom-type))))
"##;
    let expect = expect![[
        r####"OK (:version "2.4.1" :settings (30 detailed) :group ((npt-config-refresh-interval npt-config-display-style) "Customization group for npt-dashboard.") :refresh (t 15 integer) :style (t compact (choice (const compact) (const detailed))))"####
    ]];
    ParityBatchCase::value(
        "package_metadata_generates_version_command_group_and_custom_defaults",
        elisp_form,
        expect,
    )
}

#[test]
fn names_package_batch() {
    let cases = vec![
        independent_modules_can_reuse_short_names_without_cross_talk(),
        split_declarations_reuse_prior_namespace_cells_and_protect_global_calls(),
        macro_pipeline_preserves_local_bindings_and_evaluates_backquoted_forms(),
        cl_defun_keyword_api_builds_records_with_namespaced_mutable_state(),
        derived_mode_definition_creates_namespaced_mode_state_map_and_hook(),
        package_metadata_generates_version_command_group_and_custom_defaults(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Names parity test");
    assert_oracle_batch_cases(names_oracle(), test_name, "names_parity", &cases);
}
