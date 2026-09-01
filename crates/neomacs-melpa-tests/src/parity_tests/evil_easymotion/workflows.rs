use expect_test::expect;

use super::ParityBatchCase;

fn stock_keymap_uses_the_exact_live_labels_to_choose_a_word() -> ParityBatchCase {
    let elisp_form = r####"
(progn
 (neomacs-eem-test-setup)
 (neomacs-eem-test-with-buffer
 "stock-jump"
 "deploy canary release safely to production\n"
 (lambda (_buffer)
   (let ((evilem-keys '(?a ?s ?d ?f ?g))
         (avy-single-candidate-jump nil)
         (avy-ring (make-ring 20))
         (neomacs-eem-test-label-snapshots nil)
         (avy-translate-char-function #'neomacs-eem-test-observe-input)
         (avy-last-candidates nil))
     (execute-kbd-macro (kbd "SPC w d"))
     (list :line (line-number-at-pos)
           :column (current-column)
           :text (neomacs-eem-test-current-line)
           :evil-state evil-state
           :candidates (neomacs-eem-test-candidates)
           :labels (nreverse neomacs-eem-test-label-snapshots)
           :ring (neomacs-eem-test-ring)
           :overlays (neomacs-eem-test-overlay-count))))))
"####;
    let expected = expect![[
        r#"OK (:line 1 :column 22 :text "deploy canary release safely to production" :evil-state normal :candidates ((:buffer " *evil-easymotion-stock-jump*" :position 8 :line 1 :column 7 :text "deploy canary release safely to production") (:buffer " *evil-easymotion-stock-jump*" :position 15 :line 1 :column 14 :text "deploy canary release safely to production") (:buffer " *evil-easymotion-stock-jump*" :position 23 :line 1 :column 22 :text "deploy canary release safely to production") (:buffer " *evil-easymotion-stock-jump*" :position 30 :line 1 :column 29 :text "deploy canary release safely to production") (:buffer " *evil-easymotion-stock-jump*" :position 33 :line 1 :column 32 :text "deploy canary release safely to production") (:buffer " *evil-easymotion-stock-jump*" :position 43 :line 1 :column 42 :text "deploy canary release safely to production")) :labels ((:key 100 :labels ((:buffer " *evil-easymotion-stock-jump*" :position 8 :rendered "a") (:buffer " *evil-easymotion-stock-jump*" :position 15 :rendered "s") (:buffer " *evil-easymotion-stock-jump*" :position 23 :rendered "d") (:buffer " *evil-easymotion-stock-jump*" :position 30 :rendered "f") (:buffer " *evil-easymotion-stock-jump*" :position 33 :rendered "ga") (:buffer " *evil-easymotion-stock-jump*" :position 43 :rendered "gs\n")))) :ring ((:position 1 :buffer " *evil-easymotion-stock-jump*" :selected t)) :overlays 0)"#
    ]];
    ParityBatchCase::value(
        "stock_keymap_uses_the_exact_live_labels_to_choose_a_word",
        elisp_form,
        expected,
    )
}

fn evil_delete_operator_composes_with_an_exclusive_word_easymotion() -> ParityBatchCase {
    let elisp_form = r####"
(progn
 (neomacs-eem-test-setup)
 (neomacs-eem-test-with-buffer
 "operator-delete"
 "draft release candidate now"
 (lambda (_buffer)
   (let ((evilem-keys '(?a ?s ?d ?f))
         (avy-single-candidate-jump nil)
         (avy-ring (make-ring 20))
         (neomacs-eem-test-candidates-before-action nil)
         (avy-pre-action #'neomacs-eem-test-observe-pre-action)
         (neomacs-eem-test-label-snapshots nil)
         (avy-translate-char-function #'neomacs-eem-test-observe-input)
         (kill-ring nil)
         (kill-ring-yank-pointer nil))
     (execute-kbd-macro (kbd "d SPC w s"))
     (list :text (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :line (line-number-at-pos)
           :column (current-column)
           :evil-state evil-state
           :register (current-kill 0)
           :candidates neomacs-eem-test-candidates-before-action
           :labels (nreverse neomacs-eem-test-label-snapshots)
           :ring (neomacs-eem-test-ring)
           :overlays (neomacs-eem-test-overlay-count))))))
"####;
    let expected = expect![[
        r#"OK (:text "candidate now" :point 1 :line 1 :column 0 :evil-state normal :register "draft release " :candidates ((:buffer " *evil-easymotion-operator-delete*" :position 7 :line 1 :column 6 :text "draft release candidate now") (:buffer " *evil-easymotion-operator-delete*" :position 15 :line 1 :column 14 :text "draft release candidate now") (:buffer " *evil-easymotion-operator-delete*" :position 25 :line 1 :column 24 :text "draft release candidate now") (:buffer " *evil-easymotion-operator-delete*" :position 28 :line 1 :column 27 :text "draft release candidate now")) :labels ((:key 115 :labels ((:buffer " *evil-easymotion-operator-delete*" :position 7 :rendered "a") (:buffer " *evil-easymotion-operator-delete*" :position 15 :rendered "s") (:buffer " *evil-easymotion-operator-delete*" :position 25 :rendered "d") (:buffer " *evil-easymotion-operator-delete*" :position 28 :rendered "f")))) :ring ((:position 1 :buffer " *evil-easymotion-operator-delete*" :selected t)) :overlays 0)"#
    ]];
    ParityBatchCase::value(
        "evil_delete_operator_composes_with_an_exclusive_word_easymotion",
        elisp_form,
        expected,
    )
}

fn line_dispatch_teleports_an_entire_unicode_runbook_step() -> ParityBatchCase {
    let elisp_form = r####"
(progn
 (neomacs-eem-test-setup)
 (neomacs-eem-test-with-buffer
 "line-dispatch"
 "📋 Plan rollout\nλ compile assets\n🚀 deploy café\n✅ verify health\n"
 (lambda (_buffer)
   (let ((evilem-keys '(?a ?s ?d))
         (avy-single-candidate-jump nil)
         (avy-ring (make-ring 20))
         (neomacs-eem-test-candidates-before-action nil)
         (avy-pre-action #'neomacs-eem-test-observe-pre-action)
         (neomacs-eem-test-label-snapshots nil)
         (avy-translate-char-function #'neomacs-eem-test-observe-input)
         (kill-ring nil)
         (kill-ring-yank-pointer nil))
     (execute-kbd-macro (kbd "SPC j t s"))
     (list :text (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :line (line-number-at-pos)
           :column (current-column)
           :evil-state evil-state
           :register (current-kill 0)
           :candidates neomacs-eem-test-candidates-before-action
           :labels (nreverse neomacs-eem-test-label-snapshots)
           :ring (neomacs-eem-test-ring)
           :overlays (neomacs-eem-test-overlay-count))))))
"####;
    let expected = expect![[
        r#"OK (:text "🚀 deploy café\n📋 Plan rollout\nλ compile assets\n✅ verify health\n" :point 1 :line 1 :column 0 :evil-state normal :register #("🚀 deploy café\n" 0 14 (yank-handler (evil-yank-line-handler nil t))) :candidates ((:buffer " *evil-easymotion-line-dispatch*" :position 16 :line 2 :column 0 :text "λ compile assets") (:buffer " *evil-easymotion-line-dispatch*" :position 33 :line 3 :column 0 :text "🚀 deploy café") (:buffer " *evil-easymotion-line-dispatch*" :position 47 :line 4 :column 0 :text "✅ verify health")) :labels ((:key 116 :labels ((:buffer " *evil-easymotion-line-dispatch*" :position 16 :rendered "a") (:buffer " *evil-easymotion-line-dispatch*" :position 33 :rendered "s ") (:buffer " *evil-easymotion-line-dispatch*" :position 47 :rendered "d "))) (:key 115 :labels ((:buffer " *evil-easymotion-line-dispatch*" :position 16 :rendered "a") (:buffer " *evil-easymotion-line-dispatch*" :position 33 :rendered "s ") (:buffer " *evil-easymotion-line-dispatch*" :position 47 :rendered "d ")))) :ring ((:position 1 :buffer " *evil-easymotion-line-dispatch*" :selected t)) :overlays 0)"#
    ]];
    ParityBatchCase::value(
        "line_dispatch_teleports_an_entire_unicode_runbook_step",
        elisp_form,
        expected,
    )
}

fn custom_scoped_motion_skips_archived_invisible_tickets() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-eem-test-setup)
  (neomacs-eem-test-ticket-workflow #'neomacs-eem-test-visible-ticket
                                    "visible-ticket"))
"####;
    let expected = expect![[
        r#"OK (:point 30 :line 1 :column 19 :ticket "TICKET-200" :candidates ((:buffer " *evil-easymotion-visible-ticket*" :position 30 :line 1 :column 19 :text "Archived TICKET-100 | Active TICKET-200 | Next TICKET-300") (:buffer " *evil-easymotion-visible-ticket*" :position 48 :line 1 :column 37 :text "Archived TICKET-100 | Active TICKET-200 | Next TICKET-300")) :labels ((:key 97 :labels ((:buffer " *evil-easymotion-visible-ticket*" :position 30 :rendered "a") (:buffer " *evil-easymotion-visible-ticket*" :position 48 :rendered "s")))) :hooks ((:pre 1) (:post 30)) :overlays 0)"#
    ]];
    ParityBatchCase::value(
        "custom_scoped_motion_skips_archived_invisible_tickets",
        elisp_form,
        expected,
    )
}

fn include_invisible_option_exposes_the_pinned_package_failure() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-eem-test-setup)
  (neomacs-eem-test-ticket-workflow #'neomacs-eem-test-any-ticket
                                    "including-hidden-ticket"))
"####;
    let expected = expect!["ERR (void-function t)"];
    ParityBatchCase::signal(
        "include_invisible_option_exposes_the_pinned_package_failure",
        elisp_form,
        expected,
    )
}

fn all_windows_motion_selects_an_alert_in_another_window() -> ParityBatchCase {
    let elisp_form = r####"
(progn
 (neomacs-eem-test-setup)
 (let ((operations (generate-new-buffer " *evil-easymotion-operations*"))
      (audit (generate-new-buffer " *evil-easymotion-audit*")))
  (unwind-protect
      (save-window-excursion
        (delete-other-windows)
        (with-current-buffer operations
          (insert "Operations\nALERT queue stalled\nReady\n")
          (goto-char (point-min))
          (evil-local-mode 1)
          (evil-normal-state))
        (with-current-buffer audit
          (insert "Audit\nALERT invalid token\nResolved\n")
          (goto-char (point-min))
          (evil-local-mode 1)
          (evil-normal-state))
        (let* ((operations-window (selected-window))
               (audit-window (split-window-right)))
          (set-window-buffer operations-window operations)
          (set-window-buffer audit-window audit)
          (select-window operations-window)
          (set-buffer operations)
          (goto-char (point-min))
          (let ((evilem-keys '(?a ?s))
                (avy-single-candidate-jump nil)
                (avy-ring (make-ring 20))
                (neomacs-eem-test-hook-trace nil)
                (neomacs-eem-test-label-snapshots nil)
                (avy-translate-char-function #'neomacs-eem-test-observe-input)
                (avy-last-candidates nil))
            (execute-kbd-macro (kbd "C-c a s"))
            (list :selected-buffer (buffer-name)
                  :selected-audit (eq (current-buffer) audit)
                  :selected-window (eq (selected-window) audit-window)
                  :line (line-number-at-pos)
                  :column (current-column)
                  :text (neomacs-eem-test-current-line)
                  :evil-state evil-state
                  :candidates (neomacs-eem-test-candidates)
                  :labels (nreverse neomacs-eem-test-label-snapshots)
                  :hooks (nreverse neomacs-eem-test-hook-trace)
                  :ring (neomacs-eem-test-ring)
                  :overlays (neomacs-eem-test-overlay-count operations audit)))))
    (dolist (buffer (list operations audit))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer))))))
"####;
    let expected = expect![[
        r#"OK (:selected-buffer " *evil-easymotion-audit*" :selected-audit t :selected-window t :line 2 :column 0 :text "ALERT invalid token" :evil-state normal :candidates ((:buffer " *evil-easymotion-operations*" :position 12 :line 2 :column 0 :text "ALERT queue stalled") (:buffer " *evil-easymotion-audit*" :position 7 :line 2 :column 0 :text "ALERT invalid token")) :labels ((:key 115 :labels ((:buffer " *evil-easymotion-audit*" :position 7 :rendered "s") (:buffer " *evil-easymotion-operations*" :position 12 :rendered "a")))) :hooks ((:pre " *evil-easymotion-operations*" 1) (:post " *evil-easymotion-audit*" 7)) :ring ((:position 1 :buffer " *evil-easymotion-operations*" :selected nil)) :overlays 0)"#
    ]];
    ParityBatchCase::value(
        "all_windows_motion_selects_an_alert_in_another_window",
        elisp_form,
        expected,
    )
}

fn escape_cancels_selection_without_moving_or_leaking_overlays() -> ParityBatchCase {
    let elisp_form = r####"
(progn
 (neomacs-eem-test-setup)
 (neomacs-eem-test-with-buffer
 "cancel"
 "Deploy canary\nDeploy staging\nDeploy production\n"
 (lambda (_buffer)
   (forward-line 1)
   (move-to-column 3)
   (let ((before (list :point (point)
                       :line (line-number-at-pos)
                       :column (current-column)
                       :text (buffer-string)))
         (evilem-keys '(?a ?s ?d))
         (avy-single-candidate-jump nil)
         (avy-background t)
         (avy-ring (make-ring 20))
         (neomacs-eem-test-label-snapshots nil)
         (avy-translate-char-function #'neomacs-eem-test-observe-input)
         (avy-last-candidates nil))
     (execute-kbd-macro (vconcat (kbd "SPC j") [escape]))
     (list :before before
           :after (list :point (point)
                        :line (line-number-at-pos)
                        :column (current-column)
                        :text (buffer-string))
           :evil-state evil-state
           :candidates (neomacs-eem-test-candidates)
           :labels (nreverse neomacs-eem-test-label-snapshots)
           :ring (neomacs-eem-test-ring)
           :backgrounds (length avy--overlays-back)
           :leading (length avy--overlays-lead)
           :overlays (neomacs-eem-test-overlay-count))))))
"####;
    let expected = expect![[
        r#"OK (:before (:point 18 :line 2 :column 3 :text "Deploy canary\nDeploy staging\nDeploy production\n") :after (:point 18 :line 2 :column 3 :text "Deploy canary\nDeploy staging\nDeploy production\n") :evil-state normal :candidates ((:buffer " *evil-easymotion-cancel*" :position 33 :line 3 :column 3 :text "Deploy production")) :labels ((:key 27 :labels ((:buffer " *evil-easymotion-cancel*" :position 33 :rendered "a")))) :ring nil :backgrounds 0 :leading 0 :overlays 0)"#
    ]];
    ParityBatchCase::value(
        "escape_cancels_selection_without_moving_or_leaking_overlays",
        elisp_form,
        expected,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        stock_keymap_uses_the_exact_live_labels_to_choose_a_word(),
        evil_delete_operator_composes_with_an_exclusive_word_easymotion(),
        line_dispatch_teleports_an_entire_unicode_runbook_step(),
        custom_scoped_motion_skips_archived_invisible_tickets(),
        include_invisible_option_exposes_the_pinned_package_failure(),
        all_windows_motion_selects_an_alert_in_another_window(),
        escape_cancels_selection_without_moving_or_leaking_overlays(),
    ]
}
