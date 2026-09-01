use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DIMINISH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DIMINISH_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const DIMINISH_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'diminish)

(defvar-local neomacs-diminish-test-audit-events nil)

(defun neomacs-diminish-test-record-edit (begin end old-length)
  "Record a user-visible edit made while the audit mode is active."
  (push (list :range (list begin end)
              :old-length old-length
              :text (buffer-substring-no-properties begin end))
        neomacs-diminish-test-audit-events))

(define-minor-mode neomacs-diminish-test-audit-mode
  "Record buffer edits for the Diminish parity workflow."
  :lighter " Audit"
  (if neomacs-diminish-test-audit-mode
      (add-hook 'after-change-functions
                #'neomacs-diminish-test-record-edit nil t)
    (remove-hook 'after-change-functions
                 #'neomacs-diminish-test-record-edit t)))

(define-minor-mode neomacs-diminish-test-sync-mode
  "Represent a live synchronization workflow."
  :lighter " Sync")

(defvar neomacs-diminish-test-status-lighter " Ready")

(define-minor-mode neomacs-diminish-test-status-mode
  "Represent a mode whose lighter follows a status variable."
  :lighter neomacs-diminish-test-status-lighter)

(define-minor-mode neomacs-diminish-test-review-mode
  "Represent a review workflow that can remain undiminished."
  :lighter " Review")

(defvar-local neomacs-diminish-test-pipeline-phase :idle)

(define-minor-mode neomacs-diminish-test-pipeline-mode
  "Represent a pipeline whose lighter is computed from live buffer state."
  :lighter (:eval
            (pcase neomacs-diminish-test-pipeline-phase
              (:idle " Idle")
              (:deploy " Deploy")
              (:release " Release"))))

(defun neomacs-diminish-test-lighter (mode)
  "Return a copy of MODE's documented mode-line specification."
  (copy-tree (cdr (assq mode minor-mode-alist))))

(defmacro neomacs-diminish-test-with-state (&rest body)
  "Run BODY with isolated Diminish and mode-line registration state."
  (declare (indent 0) (debug t))
  `(let ((minor-mode-alist
          (mapcar (lambda (mode)
                    (copy-tree (assq mode minor-mode-alist)))
                  '(neomacs-diminish-test-audit-mode
                    neomacs-diminish-test-sync-mode
                    neomacs-diminish-test-status-mode
                    neomacs-diminish-test-review-mode
                    neomacs-diminish-test-pipeline-mode)))
         (diminished-mode-alist nil)
         (neomacs-diminish-test-status-lighter " Ready"))
     ,@body))
"##;

fn diminish_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DIMINISH_MELPA_PIN, "diminish.el")
        .expect("prepare revision-pinned Diminish source below ./tmp")
        .with_prelude(DIMINISH_TEST_PRELUDE)
        .with_timeout(DIMINISH_TEST_TIMEOUT)
}

fn hiding_a_live_audit_mode_does_not_stop_its_buffer_work() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (with-temp-buffer
    (text-mode)
    (neomacs-diminish-test-audit-mode 1)
    (insert "draft")
    (let ((before (copy-tree (reverse neomacs-diminish-test-audit-events))))
      (diminish 'neomacs-diminish-test-audit-mode)
      (goto-char (point-max))
      (insert " ready")
      (list :buffer (buffer-string)
            :mode-active neomacs-diminish-test-audit-mode
            :lighter (neomacs-diminish-test-lighter
                      'neomacs-diminish-test-audit-mode)
            :events-before-diminish before
            :all-events (reverse neomacs-diminish-test-audit-events)
            :reported-as-if-undiminished (diminished-modes)))))
"##;
    let expected = expect![[
        r####"OK (:buffer "draft ready" :mode-active t :lighter ("") :events-before-diminish ((:range (1 6) :old-length 0 :text "draft")) :all-events ((:range (1 6) :old-length 0 :text "draft") (:range (6 12) :old-length 0 :text " ready")) :reported-as-if-undiminished "Audit")"####
    ]];
    ParityBatchCase::value(
        "hiding_a_live_audit_mode_does_not_stop_its_buffer_work",
        elisp_form,
        expected,
    )
}

fn custom_labels_keep_word_boundaries_or_scrunch_single_status_letters() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (with-temp-buffer
    (neomacs-diminish-test-audit-mode 1)
    (neomacs-diminish-test-sync-mode 1)
    (neomacs-diminish-test-status-mode 1)
    (neomacs-diminish-test-review-mode 1)
    (diminish 'neomacs-diminish-test-audit-mode)
    (diminish 'neomacs-diminish-test-sync-mode "Sy")
    (diminish 'neomacs-diminish-test-status-mode "R")
    (diminish 'neomacs-diminish-test-review-mode " QA")
    (list :active
          (mapcar #'symbol-value
                  '(neomacs-diminish-test-audit-mode
                    neomacs-diminish-test-sync-mode
                    neomacs-diminish-test-status-mode
                    neomacs-diminish-test-review-mode))
          :registered-lighters
          (mapcar #'neomacs-diminish-test-lighter
                  '(neomacs-diminish-test-audit-mode
                    neomacs-diminish-test-sync-mode
                    neomacs-diminish-test-status-mode
                    neomacs-diminish-test-review-mode))
          :reported-original-lighters (diminished-modes))))
"##;
    let expected = expect![[
        r####"OK (:active (t t t t) :registered-lighters (("") (" Sy") ("R") (" QA")) :reported-original-lighters "Audit Sync Ready Review")"####
    ]];
    ParityBatchCase::value(
        "custom_labels_keep_word_boundaries_or_scrunch_single_status_letters",
        elisp_form,
        expected,
    )
}

fn rediminishing_a_dynamic_status_preserves_the_live_original_for_undo() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (with-temp-buffer
    (neomacs-diminish-test-status-mode 1)
    (let ((original
           (neomacs-diminish-test-lighter
            'neomacs-diminish-test-status-mode)))
      (diminish 'neomacs-diminish-test-status-mode "Busy")
      (let ((busy
             (neomacs-diminish-test-lighter
              'neomacs-diminish-test-status-mode)))
        (setq neomacs-diminish-test-status-lighter " Running")
        (diminish 'neomacs-diminish-test-status-mode "R")
        (let ((compact
               (neomacs-diminish-test-lighter
                'neomacs-diminish-test-status-mode)))
          (diminish-undo 'neomacs-diminish-test-status-mode)
          (list :mode-active neomacs-diminish-test-status-mode
                :original original
                :busy busy
                :compact compact
                :restored
                (neomacs-diminish-test-lighter
                 'neomacs-diminish-test-status-mode)
                :reported-current-status (diminished-modes)))))))
"##;
    let expected = expect![[
        r####"OK (:mode-active t :original (neomacs-diminish-test-status-lighter) :busy (" Busy") :compact ("R") :restored (neomacs-diminish-test-status-lighter) :reported-current-status "Running")"####
    ]];
    ParityBatchCase::value(
        "rediminishing_a_dynamic_status_preserves_the_live_original_for_undo",
        elisp_form,
        expected,
    )
}

fn restoring_all_recovers_lighters_without_reenabling_inactive_modes() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (with-temp-buffer
    (neomacs-diminish-test-audit-mode 1)
    (neomacs-diminish-test-sync-mode 1)
    (neomacs-diminish-test-review-mode 1)
    (diminish 'neomacs-diminish-test-audit-mode)
    (diminish 'neomacs-diminish-test-sync-mode "S")
    (diminish 'neomacs-diminish-test-review-mode "Rv")
    (neomacs-diminish-test-sync-mode -1)
    (let ((before
           (mapcar #'neomacs-diminish-test-lighter
                   '(neomacs-diminish-test-audit-mode
                     neomacs-diminish-test-sync-mode
                     neomacs-diminish-test-review-mode)))
          (active-report (diminished-modes)))
      (diminish-undo 'diminished-modes)
      (list :before-restore before
            :active-before-restore
            (list neomacs-diminish-test-audit-mode
                  neomacs-diminish-test-sync-mode
                  neomacs-diminish-test-review-mode)
            :active-report active-report
            :after-restore
            (mapcar #'neomacs-diminish-test-lighter
                    '(neomacs-diminish-test-audit-mode
                      neomacs-diminish-test-sync-mode
                      neomacs-diminish-test-review-mode))
            :active-after-restore
            (list neomacs-diminish-test-audit-mode
                  neomacs-diminish-test-sync-mode
                  neomacs-diminish-test-review-mode)
            :restored-report (diminished-modes)))))
"##;
    let expected = expect![[
        r####"OK (:before-restore (("") ("S") (" Rv")) :active-before-restore (t nil t) :active-report "Audit Review" :after-restore ((" Audit") (" Sync") (" Review")) :active-after-restore (t nil t) :restored-report "Audit Review")"####
    ]];
    ParityBatchCase::value(
        "restoring_all_modes_recovers_every_lighter_without_reenabling_inactive_workflows",
        elisp_form,
        expected,
    )
}

fn early_configuration_waits_for_registration_and_bad_undo_is_explicit() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (with-temp-buffer
    (setq minor-mode-alist
          (assq-delete-all 'neomacs-diminish-test-review-mode
                           minor-mode-alist))
    (diminish 'neomacs-diminish-test-review-mode "Rv")
    (let ((before-registration
           (assq 'neomacs-diminish-test-review-mode minor-mode-alist)))
      (add-minor-mode 'neomacs-diminish-test-review-mode " Review")
      (neomacs-diminish-test-review-mode 1)
      (diminish 'neomacs-diminish-test-review-mode "Rv")
      (diminish-undo 'neomacs-diminish-test-sync-mode)
      (let ((unknown-undo
             (condition-case err
                 (progn
                   (diminish-undo 'neomacs-diminish-test-missing-mode)
                   :unexpected-success)
               (error
                (list (car err) (error-message-string err))))))
        (list :before-registration before-registration
              :mode-active neomacs-diminish-test-review-mode
              :after-registration
              (neomacs-diminish-test-lighter
               'neomacs-diminish-test-review-mode)
              :undiminished-mode-unchanged
              (neomacs-diminish-test-lighter
               'neomacs-diminish-test-sync-mode)
              :unknown-undo unknown-undo
              :active-report (diminished-modes))))))
"##;
    let expected = expect![[
        r####"OK (:before-registration nil :mode-active t :after-registration (" Rv") :undiminished-mode-unchanged (" Sync") :unknown-undo (error "neomacs-diminish-test-missing-mode is not currently registered as a minor mode") :active-report "Review")"####
    ]];
    ParityBatchCase::value(
        "early_configuration_is_a_noop_until_the_mode_registers_and_bad_undo_is_explicit",
        elisp_form,
        expected,
    )
}

fn one_global_diminution_respects_each_buffers_independent_mode_activity() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (let ((deploy-buffer (generate-new-buffer " *diminish-deploy*"))
        (notes-buffer (generate-new-buffer " *diminish-notes*")))
    (unwind-protect
        (progn
          (with-current-buffer deploy-buffer
            (neomacs-diminish-test-sync-mode 1)
            (neomacs-diminish-test-review-mode 1))
          (with-current-buffer notes-buffer
            (neomacs-diminish-test-review-mode 1))
          (diminish 'neomacs-diminish-test-sync-mode "S")
          (list
           :registered-sync-lighter
           (neomacs-diminish-test-lighter
            'neomacs-diminish-test-sync-mode)
           :deploy
           (with-current-buffer deploy-buffer
             (list :sync neomacs-diminish-test-sync-mode
                   :review neomacs-diminish-test-review-mode
                   :reported (diminished-modes)))
           :notes
           (with-current-buffer notes-buffer
             (list :sync neomacs-diminish-test-sync-mode
                   :review neomacs-diminish-test-review-mode
                   :reported (diminished-modes)))))
      (kill-buffer deploy-buffer)
      (kill-buffer notes-buffer))))
"##;
    let expected = expect![[
        r####"OK (:registered-sync-lighter ("S") :deploy (:sync t :review t :reported "Sync Review") :notes (:sync nil :review t :reported "Review"))"####
    ]];
    ParityBatchCase::value(
        "one_global_diminution_respects_each_buffers_independent_mode_activity",
        elisp_form,
        expected,
    )
}

fn dynamic_pipeline_lighter_can_be_hidden_across_phase_changes_and_restored() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-diminish-test-with-state
  (with-temp-buffer
    (setq neomacs-diminish-test-pipeline-phase :deploy)
    (neomacs-diminish-test-pipeline-mode 1)
    (let* ((original
            (neomacs-diminish-test-lighter
             'neomacs-diminish-test-pipeline-mode))
           (deploy-label (eval (cadar original) t)))
      (diminish 'neomacs-diminish-test-pipeline-mode "P")
      (setq neomacs-diminish-test-pipeline-phase :release)
      (let ((hidden
             (neomacs-diminish-test-lighter
              'neomacs-diminish-test-pipeline-mode)))
        (diminish-undo 'neomacs-diminish-test-pipeline-mode)
        (let ((restored
               (neomacs-diminish-test-lighter
                'neomacs-diminish-test-pipeline-mode)))
          (list :mode-active neomacs-diminish-test-pipeline-mode
                :phase neomacs-diminish-test-pipeline-phase
                :original original
                :deploy-label deploy-label
                :hidden hidden
                :restored restored
                :release-label (eval (cadar restored) t)))))))
"##;
    let expected = expect![[
        r####"OK (:mode-active t :phase :release :original ((:eval (pcase neomacs-diminish-test-pipeline-phase (:idle " Idle") (:deploy " Deploy") (:release " Release")))) :deploy-label " Deploy" :hidden ("P") :restored ((:eval (pcase neomacs-diminish-test-pipeline-phase (:idle " Idle") (:deploy " Deploy") (:release " Release")))) :release-label " Release")"####
    ]];
    ParityBatchCase::value(
        "dynamic_pipeline_lighter_can_be_hidden_across_phase_changes_and_restored",
        elisp_form,
        expected,
    )
}

#[test]
fn diminish_package_batch() {
    assert_oracle_batch_cases(
        diminish_oracle(),
        "diminish-package-batch",
        "Diminish",
        &[
            hiding_a_live_audit_mode_does_not_stop_its_buffer_work(),
            custom_labels_keep_word_boundaries_or_scrunch_single_status_letters(),
            rediminishing_a_dynamic_status_preserves_the_live_original_for_undo(),
            restoring_all_recovers_lighters_without_reenabling_inactive_modes(),
            early_configuration_waits_for_registration_and_bad_undo_is_explicit(),
            one_global_diminution_respects_each_buffers_independent_mode_activity(),
            dynamic_pipeline_lighter_can_be_hidden_across_phase_changes_and_restored(),
        ],
    );
}
