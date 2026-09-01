use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WS_BUTLER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ws-butler)

(defun neomacs-ws-butler-test-in-buffer (name initial function)
  (let ((buffer (generate-new-buffer (format "*ws-butler-%s*" name))))
    (unwind-protect
        (with-current-buffer buffer
          (insert initial)
          (set-buffer-modified-p nil)
          (buffer-disable-undo)
          (buffer-enable-undo)
          (ws-butler-mode 1)
          (funcall function))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (ws-butler-mode -1)
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun neomacs-ws-butler-test-changes ()
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((property (get-text-property position 'ws-butler-chg))
             (end (next-single-property-change
                   position 'ws-butler-chg nil (point-max))))
        (when property
          (push (list property position end
                      (buffer-substring-no-properties position end)) runs))
        (setq position end)))
    (nreverse runs)))
"####;

fn touched_lines_and_virtual_space_follow_the_save_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(list
 (neomacs-ws-butler-test-in-buffer
  "touched" "legacy entry   \nrelease pending   \nuntouched note   \n"
  (lambda ()
    (goto-char (point-min)) (forward-line 1) (search-forward "pending")
    (replace-match "ready")
    (let ((tracked (neomacs-ws-butler-test-changes)))
      (ws-butler-before-save)
      (let ((written (buffer-substring-no-properties (point-min) (point-max))))
        (ws-butler-after-save)
        (list :tracked tracked :written written
              :after (buffer-substring-no-properties (point-min) (point-max))
              :properties (neomacs-ws-butler-test-changes)
              :modified (buffer-modified-p))))))
 (neomacs-ws-butler-test-in-buffer
  "virtual" "task: deploy    \nnext: verify\n"
  (lambda ()
    (goto-char (point-min)) (search-forward "deploy") (insert " safely")
    (end-of-line)
    (let ((before (list (point) (current-column))))
      (ws-butler-before-save)
      (let ((written (buffer-substring-no-properties (point-min) (point-max)))
            (trimmed-point (point)))
        (ws-butler-after-save)
        (list :before before :written written :trimmed-point trimmed-point
              :after (buffer-substring-no-properties (point-min) (point-max))
              :point (point) :column (current-column)
              :modified (buffer-modified-p)))))))
"####;
    let expected = expect![[
        r#"OK ((:tracked ((chg 25 30 "ready")) :written "legacy entry   \nrelease ready\nuntouched note   \n" :after "legacy entry   \nrelease ready\nuntouched note   \n" :properties nil :modified nil) (:before (24 23) :written "task: deploy safely\nnext: verify\n" :trimmed-point 20 :after "task: deploy safely    \nnext: verify\n" :point 24 :column 23 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "touched_lines_and_virtual_space_follow_the_save_lifecycle",
        elisp_form,
        expected,
    )
}

fn trim_predicate_protects_whitespace_significant_regions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ws-butler-test-in-buffer
 "predicate" "title: draft   \nliteral: keep-me   \nfooter: draft   \n"
 (lambda ()
   (setq-local ws-butler-trim-predicate
               (lambda (beg _end)
                 (save-excursion (goto-char beg) (not (looking-at "literal:")))))
   (goto-char (point-min))
   (while (search-forward "draft" nil t) (replace-match "ready"))
   (goto-char (point-min)) (forward-line 1) (search-forward "keep-me")
   (replace-match "verbatim")
   (let ((tracked (neomacs-ws-butler-test-changes)))
     (ws-butler-before-save)
     (list :tracked tracked
           :text (buffer-substring-no-properties (point-min) (point-max))
           :coordinate ws-butler-presave-coord))))
"####;
    let expected = expect![[
        r#"OK (:tracked ((chg 8 13 "ready") (chg 26 34 "verbatim") (chg 46 51 "ready")) :text "title: ready\nliteral: verbatim   \nfooter: ready\n" :coordinate (2 17))"#
    ]];
    ParityBatchCase::value(
        "trim_predicate_protects_whitespace_significant_regions",
        elisp_form,
        expected,
    )
}

fn eof_cleanup_respects_final_newline_policy() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((scenario (name initial require-newline edit)
       (neomacs-ws-butler-test-in-buffer
        name initial
        (lambda ()
          (let ((require-final-newline require-newline))
            (goto-char (point-max)) (funcall edit) (ws-butler-before-save)
            (list :text (buffer-substring-no-properties (point-min) (point-max))
                  :point (point)
                  :coordinate ws-butler-presave-coord))))))
  (list
   (scenario "collapse" "payload\n\n  \n" nil (lambda () (insert " ")))
   (scenario "required" "payload" t (lambda () (insert "!")))
   (scenario "optional" "payload\n" nil (lambda () (insert "   ")))))
"####;
    let expected = expect![[
        r#"OK ((:text "payload\n" :point 9 :coordinate (4 1)) (:text "payload!" :point 9 :coordinate (1 8)) (:text "payload\n" :point 9 :coordinate (2 3)))"#
    ]];
    ParityBatchCase::value(
        "eof_cleanup_respects_final_newline_policy",
        elisp_form,
        expected,
    )
}

fn indentation_conversion_obeys_tabs_spaces_and_smart_tabs() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((scenario (name indent-tabs smart-tabs)
       (neomacs-ws-butler-test-in-buffer
        name "        deploy();   \n\tverify();   \n"
        (lambda ()
          (setq-local ws-butler-convert-leading-tabs-or-spaces t)
          (setq-local indent-tabs-mode indent-tabs)
          (setq-local tab-width 8)
          (when smart-tabs (setq-local smart-tabs-mode t))
          (goto-char (point-min)) (search-forward "deploy") (replace-match "DEPLOY")
          (search-forward "verify") (replace-match "VERIFY")
          (ws-butler-before-save)
          (buffer-substring-no-properties (point-min) (point-max))))))
  (list :tabs (scenario "tabs" t nil)
        :spaces (scenario "spaces" nil nil)
        :smart-tabs (scenario "smart" nil t)))
"####;
    let expected = expect![[
        r#"OK (:tabs "\11DEPLOY();\n\11VERIFY();\n" :spaces "        DEPLOY();\n        VERIFY();\n" :smart-tabs "        DEPLOY();\n\11VERIFY();\n")"#
    ]];
    ParityBatchCase::value(
        "indentation_conversion_obeys_tabs_spaces_and_smart_tabs",
        elisp_form,
        expected,
    )
}

fn undo_and_narrowed_saves_keep_change_tracking_consistent() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ws-butler-test-in-buffer
 "undo-narrow" "first old   \nsecond old   \nthird old   \n"
 (lambda ()
   (goto-char (point-min)) (forward-line 1) (search-forward "old")
   (delete-region (match-beginning 0) (match-end 0)) (insert "ready")
   (undo-boundary)
   (let ((after-edit (neomacs-ws-butler-test-changes)))
     (undo)
     (let ((after-undo (neomacs-ws-butler-test-changes)))
       (goto-char (point-min)) (search-forward "first") (replace-match "FIRST")
       (narrow-to-region (line-beginning-position) (line-end-position))
       (let ((restriction (list (point-min) (point-max))))
         (ws-butler-before-save)
         (list :after-edit after-edit :after-undo after-undo
               :restriction restriction :restriction-after (list (point-min) (point-max))
               :text (save-restriction
                       (widen)
                       (buffer-substring-no-properties
                        (point-min) (point-max)))))))))
"####;
    let expected = expect![[
        r#"OK (:after-edit ((chg 21 26 "ready") (delete 26 27 " ")) :after-undo nil :restriction (1 13) :restriction-after (1 10) :text "FIRST old\nsecond old   \nthird old   \n")"#
    ]];
    ParityBatchCase::value(
        "undo_and_narrowed_saves_keep_change_tracking_consistent",
        elisp_form,
        expected,
    )
}

fn mode_lifecycle_and_global_exemptions_are_buffer_local() -> ParityBatchCase {
    let elisp_form = r####"
(let ((hooks
       (neomacs-ws-butler-test-in-buffer
        "hooks" "text\n"
        (lambda ()
          (let ((enabled
                 (list ws-butler-mode
                       (memq #'ws-butler-after-change after-change-functions)
                       (memq #'ws-butler-before-save before-save-hook)
                       (memq #'ws-butler-after-save after-save-hook))))
            (ws-butler-mode -1)
            (list :enabled (mapcar (lambda (value) (not (null value))) enabled)
                  :disabled
                  (list (memq #'ws-butler-after-change after-change-functions)
                        (memq #'ws-butler-before-save before-save-hook)
                        (memq #'ws-butler-after-save after-save-hook))))))))
  (list :hooks hooks
        :text (with-temp-buffer (text-mode) (ws-butler--global-mode-turn-on) ws-butler-mode)
        :special (with-temp-buffer (special-mode) (ws-butler--global-mode-turn-on) ws-butler-mode)
        :derived-exempt
        (with-temp-buffer
          (define-derived-mode neomacs-ws-butler-exempt-mode text-mode "WB Exempt")
          (neomacs-ws-butler-exempt-mode)
          (let ((ws-butler-global-exempt-modes '(text-mode)))
            (ws-butler--global-mode-turn-on))
          ws-butler-mode)))
"####;
    let expected = expect![
        "OK (:hooks (:enabled (t t t t) :disabled (nil nil nil)) :text t :special nil :derived-exempt nil)"
    ];
    ParityBatchCase::value(
        "mode_lifecycle_and_global_exemptions_are_buffer_local",
        elisp_form,
        expected,
    )
}

#[test]
fn ws_butler_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(WS_BUTLER_MELPA_PIN, "ws-butler.el")
            .expect("prepare revision-pinned WS Butler source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "ws-butler-package-batch",
        "WS Butler",
        &[
            touched_lines_and_virtual_space_follow_the_save_lifecycle(),
            trim_predicate_protects_whitespace_significant_regions(),
            eof_cleanup_respects_final_newline_policy(),
            indentation_conversion_obeys_tabs_spaces_and_smart_tabs(),
            undo_and_narrowed_saves_keep_change_tracking_consistent(),
            mode_lifecycle_and_global_exemptions_are_buffer_local(),
        ],
    );
}
