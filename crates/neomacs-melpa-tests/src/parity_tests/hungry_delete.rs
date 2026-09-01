use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HUNGRY_DELETE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HUNGRY_DELETE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HUNGRY_DELETE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'hungry-delete)

(define-derived-mode neomacs-hungry-delete-test-mode text-mode
  "Hungry-Delete-Test"
  "Major mode for realistic Hungry Delete parity workflows.")

(define-derived-mode neomacs-hungry-delete-test-excluded-mode text-mode
  "Hungry-Delete-Excluded-Test"
  "Major mode excluded from global Hungry Delete activation.")

(defun neomacs-hungry-delete-test-insert-marked (text)
  "Insert TEXT and leave point at its unique | marker."
  (insert text)
  (goto-char (point-min))
  (unless (search-forward "|" nil t)
    (error "Hungry Delete fixture lacks a point marker"))
  (delete-char -1))

(defun neomacs-hungry-delete-test-state ()
  "Return the user-visible editing state of the current buffer."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :column (current-column)
        :modified (buffer-modified-p)))

(defun neomacs-hungry-delete-test-capture-signal (function)
  "Run FUNCTION and return complete stable signal information."
  (condition-case error-data
      (progn (funcall function) 'no-signal)
    (error
     (list :symbol (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn hungry_delete_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HUNGRY_DELETE_MELPA_PIN, "hungry-delete.el")
        .expect("prepare revision-pinned Hungry Delete source below ./tmp")
        .with_prelude(HUNGRY_DELETE_TEST_PRELUDE)
        .with_timeout(HUNGRY_DELETE_TEST_TIMEOUT)
}

fn multiline_and_continued_whitespace_cleanup_preserves_the_payload() -> ParityBatchCase {
    let elisp_form = r###"
(cl-labels
    ((run (text command)
       (with-temp-buffer
         (neomacs-hungry-delete-test-mode)
         (neomacs-hungry-delete-test-insert-marked text)
         (let ((current-prefix-arg nil))
           (funcall command 1))
         (neomacs-hungry-delete-test-state))))
  (list
   :continued-forward
   (run (concat "api|  " (string ?\\) "\n\t ready")
        #'hungry-delete-forward)
   :continued-backward
   (run (concat "api  " (string ?\\) "\n\t |ready")
        #'hungry-delete-backward)
   :multiline-forward
   (run "build| \t\n\r\f\v  deploy" #'hungry-delete-forward)
   :multiline-backward
   (run "build \t\n\r\f\v  |deploy" #'hungry-delete-backward)))
"###;
    let expected = expect![[
        r###"OK (:continued-forward (:text "apiready" :point 4 :column 3 :modified t) :continued-backward (:text "apiready" :point 4 :column 3 :modified t) :multiline-forward (:text "builddeploy" :point 6 :column 5 :modified t) :multiline-backward (:text "builddeploy" :point 6 :column 5 :modified t))"###
    ]];
    ParityBatchCase::value(
        "multiline_and_continued_whitespace_cleanup_preserves_the_payload",
        elisp_form,
        expected,
    )
}

fn reluctant_joining_keeps_one_word_separator_but_removes_a_single_gap() -> ParityBatchCase {
    let elisp_form = r###"
(let ((hungry-delete-join-reluctantly t))
  (cl-labels
      ((run (text command)
         (with-temp-buffer
           (neomacs-hungry-delete-test-mode)
           (neomacs-hungry-delete-test-insert-marked text)
           (let ((current-prefix-arg nil))
             (funcall command 1))
           (neomacs-hungry-delete-test-state))))
    (list
     :forward-many (run "release|     candidate" #'hungry-delete-forward)
     :backward-many (run "release     |candidate" #'hungry-delete-backward)
     :single-gap (run "release| candidate" #'hungry-delete-forward)
     :punctuated (run "release:|   candidate" #'hungry-delete-forward)
     :buffer-edge (run "|   candidate" #'hungry-delete-forward))))
"###;
    let expected = expect![[
        r###"OK (:forward-many (:text "release candidate" :point 8 :column 7 :modified t) :backward-many (:text "release candidate" :point 9 :column 8 :modified t) :single-gap (:text "releasecandidate" :point 8 :column 7 :modified t) :punctuated (:text "release: candidate" :point 9 :column 8 :modified t) :buffer-edge (:text "candidate" :point 1 :column 0 :modified t))"###
    ]];
    ParityBatchCase::value(
        "reluctant_joining_keeps_one_word_separator_but_removes_a_single_gap",
        elisp_form,
        expected,
    )
}

fn custom_skip_characters_clean_repeated_manifest_delimiters() -> ParityBatchCase {
    let elisp_form = r###"
(cl-labels
    ((run (text skip command)
       (with-temp-buffer
         (neomacs-hungry-delete-test-mode)
         (neomacs-hungry-delete-test-insert-marked text)
         (let ((hungry-delete-chars-to-skip skip)
               (current-prefix-arg nil))
           (funcall command 1))
         (neomacs-hungry-delete-test-state))))
  (list
   :default (run "primary|, , ,canary" " \t\n\r\f\v"
                 #'hungry-delete-forward)
   :custom-forward (run "primary|, , ,canary" " \t,"
                        #'hungry-delete-forward)
   :custom-backward (run "primary, , ,|canary" " \t,"
                         #'hungry-delete-backward)))
"###;
    let expected = expect![[
        r###"OK (:default (:text "primary , ,canary" :point 8 :column 7 :modified t) :custom-forward (:text "primarycanary" :point 8 :column 7 :modified t) :custom-backward (:text "primarycanary" :point 8 :column 7 :modified t))"###
    ]];
    ParityBatchCase::value(
        "custom_skip_characters_clean_repeated_manifest_delimiters",
        elisp_form,
        expected,
    )
}

fn active_regions_are_deleted_or_killed_as_one_edit() -> ParityBatchCase {
    let elisp_form = r###"
(cl-labels
    ((select (needle include-trailing-space)
       (goto-char (point-min))
       (search-forward needle)
       (let ((begin (match-beginning 0))
             (end (+ (match-end 0) (if include-trailing-space 1 0))))
         (set-mark end)
         (goto-char begin)
         (activate-mark))))
  (let (deleted killed)
    (with-temp-buffer
      (neomacs-hungry-delete-test-mode)
      (insert "deploy staging-only production")
      (let ((transient-mark-mode t)
            (delete-active-region t))
        (select "staging-only" t)
        (hungry-delete-forward 1)
        (setq deleted
              (append (neomacs-hungry-delete-test-state)
                      (list :mark-active mark-active)))))
    (let ((kill-ring nil)
          (kill-ring-yank-pointer nil)
          (last-command nil))
      (with-temp-buffer
        (neomacs-hungry-delete-test-mode)
        (insert "rollback canary production")
        (let ((transient-mark-mode t)
              (delete-active-region 'kill))
          (select "canary" t)
          (hungry-delete-backward 1 t)
          (setq killed
                (append (neomacs-hungry-delete-test-state)
                        (list :kill (current-kill 0 t)
                              :mark-active mark-active))))))
    (list :deleted deleted :killed killed)))
"###;
    let expected = expect![[
        r###"OK (:deleted (:text "deploy production" :point 8 :column 7 :modified t :mark-active t) :killed (:text "rollback production" :point 10 :column 9 :modified t :kill "canary " :mark-active t))"###
    ]];
    ParityBatchCase::value(
        "active_regions_are_deleted_or_killed_as_one_edit",
        elisp_form,
        expected,
    )
}

fn explicit_prefixes_delete_exact_counts_and_can_save_the_text() -> ParityBatchCase {
    let elisp_form = r###"
(let (forward backward)
  (let ((kill-ring nil)
        (kill-ring-yank-pointer nil)
        (last-command nil))
    (with-temp-buffer
      (neomacs-hungry-delete-test-mode)
      (neomacs-hungry-delete-test-insert-marked
       "release:|staging   ready")
      (let ((current-prefix-arg '(4)))
        (hungry-delete-forward 4 t))
      (setq forward
            (append (neomacs-hungry-delete-test-state)
                    (list :kill (current-kill 0 t))))))
  (with-temp-buffer
    (neomacs-hungry-delete-test-mode)
    (neomacs-hungry-delete-test-insert-marked
     "release-v2|   ready")
    (let ((current-prefix-arg '(3)))
      (hungry-delete-backward 3))
    (setq backward (neomacs-hungry-delete-test-state)))
  (list :forward forward :backward backward))
"###;
    let expected = expect![[
        r###"OK (:forward (:text "release:ing   ready" :point 9 :column 8 :modified t :kill "stag") :backward (:text "release   ready" :point 8 :column 7 :modified t))"###
    ]];
    ParityBatchCase::value(
        "explicit_prefixes_delete_exact_counts_and_can_save_the_text",
        elisp_form,
        expected,
    )
}

fn overwrite_mode_preserves_columns_inside_fixed_width_records() -> ParityBatchCase {
    let elisp_form = r###"
(cl-labels
    ((run (text)
       (with-temp-buffer
         (neomacs-hungry-delete-test-mode)
         (neomacs-hungry-delete-test-insert-marked text)
         (overwrite-mode 1)
         (let ((current-prefix-arg nil))
           (hungry-delete-backward 1))
         (append (neomacs-hungry-delete-test-state)
                 (list :overwrite (and overwrite-mode t))))))
  (list :middle-of-record (run "slot:Q|ready")
        :end-of-record (run "slot:Q|")))
"###;
    let expected = expect![[
        r###"OK (:middle-of-record (:text "slot: ready" :point 6 :column 5 :modified t :overwrite t) :end-of-record (:text "slot:" :point 6 :column 5 :modified t :overwrite t))"###
    ]];
    ParityBatchCase::value(
        "overwrite_mode_preserves_columns_inside_fixed_width_records",
        elisp_form,
        expected,
    )
}

fn protected_text_boundaries_leave_the_read_only_spacing_intact() -> ParityBatchCase {
    let elisp_form = r###"
(let (forward backward)
  (with-temp-buffer
    (neomacs-hungry-delete-test-mode)
    (neomacs-hungry-delete-test-insert-marked "owner|   LOCKED")
    (let ((edit-point (point)))
      (search-forward "LOCKED")
      (add-text-properties (match-beginning 0) (match-end 0)
                           '(read-only t))
      (goto-char edit-point)
      (let ((current-prefix-arg nil))
        (hungry-delete-forward 1))
      (setq forward
            (append (neomacs-hungry-delete-test-state)
                    (list :locked-start
                          (text-property-any (point-min) (point-max)
                                             'read-only t))))))
  (with-temp-buffer
    (neomacs-hungry-delete-test-mode)
    (neomacs-hungry-delete-test-insert-marked "LOCKED   |owner")
    (let ((edit-point (point)))
      (goto-char (point-min))
      (search-forward "LOCKED")
      (add-text-properties (point) (+ (point) 2) '(read-only t))
      (goto-char edit-point)
      (let ((current-prefix-arg nil))
        (hungry-delete-backward 1))
      (setq backward
            (append (neomacs-hungry-delete-test-state)
                    (list :protected-spaces
                          (cl-count-if
                           (lambda (position)
                             (get-text-property position 'read-only))
                           (number-sequence (point-min)
                                            (1- (point-max)))))))))
  (list :forward forward :backward backward))
"###;
    let expected = expect![[
        r###"OK (:forward (:text "owner LOCKED" :point 6 :column 5 :modified t :locked-start 7) :backward (:text "LOCKED  owner" :point 9 :column 8 :modified t :protected-spaces 2))"###
    ]];
    ParityBatchCase::value(
        "protected_text_boundaries_leave_the_read_only_spacing_intact",
        elisp_form,
        expected,
    )
}

fn local_and_global_modes_install_remappings_and_honor_exclusions() -> ParityBatchCase {
    let elisp_form = r###"
(let ((eligible (generate-new-buffer " *hungry-delete-eligible*"))
      (excluded (generate-new-buffer " *hungry-delete-excluded*"))
      (hungry-delete-except-modes
       '(neomacs-hungry-delete-test-excluded-mode))
      enabled disabled)
  (unwind-protect
      (progn
        (global-hungry-delete-mode -1)
        (with-current-buffer eligible
          (neomacs-hungry-delete-test-mode))
        (with-current-buffer excluded
          (neomacs-hungry-delete-test-excluded-mode))
        (global-hungry-delete-mode 1)
        (setq enabled
              (list
               :global global-hungry-delete-mode
               :eligible
               (with-current-buffer eligible
                 (list :mode hungry-delete-mode
                       :forward
                       (command-remapping 'delete-forward-char)
                       :backward
                       (command-remapping 'delete-backward-char)))
               :excluded
               (with-current-buffer excluded
                 (list :mode hungry-delete-mode
                       :forward
                       (command-remapping 'delete-forward-char)))))
        (global-hungry-delete-mode -1)
        (setq disabled
              (list
               :global global-hungry-delete-mode
               :eligible
               (with-current-buffer eligible
                 (list :mode hungry-delete-mode
                       :forward
                       (command-remapping 'delete-forward-char)))
               :manual-excluded
               (with-current-buffer excluded
                 (turn-on-hungry-delete-mode)
                 hungry-delete-mode)))
        (list :enabled enabled :disabled disabled))
    (global-hungry-delete-mode -1)
    (kill-buffer eligible)
    (kill-buffer excluded)))
"###;
    let expected = expect![[
        r###"OK (:enabled (:global t :eligible (:mode t :forward hungry-delete-forward :backward hungry-delete-backward) :excluded (:mode nil :forward nil)) :disabled (:global nil :eligible (:mode nil :forward nil) :manual-excluded nil))"###
    ]];
    ParityBatchCase::value(
        "local_and_global_modes_install_remappings_and_honor_exclusions",
        elisp_form,
        expected,
    )
}

fn buffer_edges_and_invalid_counts_report_the_same_editing_contract() -> ParityBatchCase {
    let elisp_form = r###"
(let (trailing leading end-signal beginning-signal type-signal)
  (with-temp-buffer
    (neomacs-hungry-delete-test-mode)
    (neomacs-hungry-delete-test-insert-marked "payload|   ")
    (hungry-delete-forward 1)
    (setq trailing (neomacs-hungry-delete-test-state)))
  (with-temp-buffer
    (neomacs-hungry-delete-test-mode)
    (neomacs-hungry-delete-test-insert-marked "   |payload")
    (hungry-delete-backward 1)
    (setq leading (neomacs-hungry-delete-test-state)))
  (with-temp-buffer
    (insert "payload")
    (goto-char (point-max))
    (setq end-signal
          (neomacs-hungry-delete-test-capture-signal
           (lambda () (hungry-delete-forward 1)))))
  (with-temp-buffer
    (insert "payload")
    (goto-char (point-min))
    (setq beginning-signal
          (neomacs-hungry-delete-test-capture-signal
           (lambda () (hungry-delete-backward 1)))))
  (setq type-signal
        (neomacs-hungry-delete-test-capture-signal
         (lambda () (hungry-delete-forward 'four))))
  (list :trailing trailing
        :leading leading
        :end end-signal
        :beginning beginning-signal
        :type type-signal))
"###;
    let expected = expect![[
        r###"OK (:trailing (:text "payload" :point 8 :column 7 :modified t) :leading (:text "payload" :point 1 :column 0 :modified t) :end (:symbol end-of-buffer :data nil :message "End of buffer") :beginning (:symbol beginning-of-buffer :data nil :message "Beginning of buffer") :type (:symbol wrong-type-argument :data (integerp four) :message "Wrong type argument: integerp, four"))"###
    ]];
    ParityBatchCase::value(
        "buffer_edges_and_invalid_counts_report_the_same_editing_contract",
        elisp_form,
        expected,
    )
}

#[test]
fn hungry_delete_package_batch() {
    let cases = vec![
        multiline_and_continued_whitespace_cleanup_preserves_the_payload(),
        reluctant_joining_keeps_one_word_separator_but_removes_a_single_gap(),
        custom_skip_characters_clean_repeated_manifest_delimiters(),
        active_regions_are_deleted_or_killed_as_one_edit(),
        explicit_prefixes_delete_exact_counts_and_can_save_the_text(),
        overwrite_mode_preserves_columns_inside_fixed_width_records(),
        protected_text_boundaries_leave_the_read_only_spacing_intact(),
        local_and_global_modes_install_remappings_and_honor_exclusions(),
        buffer_edges_and_invalid_counts_report_the_same_editing_contract(),
    ];
    assert_oracle_batch_cases(
        hungry_delete_oracle(),
        "hungry-delete-package-batch",
        "Hungry Delete",
        &cases,
    );
}
