use expect_test::expect;

use super::ParityBatchCase;

fn keyboard_jump_and_candidate_history_navigate_deployment_targets() -> ParityBatchCase {
    ParityBatchCase::value(
        "keyboard_jump_and_candidate_history_navigate_deployment_targets",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "Deploy dashboard\nDeploy logs\nDeploy production")
    (goto-char (point-min))
    (local-set-key (kbd "C-c j") #'neomacs-avy-test-goto-deploy)
    (let ((avy-all-windows nil)
          (avy-case-fold-search nil)
          (avy-keys '(?a ?s ?d))
          (avy-single-candidate-jump nil)
          (avy-ring (make-ring 20))
          (avy-last-candidates nil))
      (execute-kbd-macro (kbd "C-c j s"))
      (let ((selected (list :line (line-number-at-pos)
                            :text (neomacs-avy-test-current-line)
                            :path avy-current-path)))
        (avy-next)
        (let ((next (list :line (line-number-at-pos)
                          :text (neomacs-avy-test-current-line))))
          (avy-prev)
          (list :selected selected
                :next next
                :previous (list :line (line-number-at-pos)
                                :text (neomacs-avy-test-current-line))
                :history (ring-length avy-ring)
                :candidates (length avy-last-candidates)
                :overlays (neomacs-avy-test-overlay-count)))))))
"##,
        expect![[
            r##"OK (:selected (:line 2 :text "Deploy logs" :path "s") :next (:line 3 :text "Deploy production") :previous (:line 2 :text "Deploy logs") :history 1 :candidates 3 :overlays 0)"##
        ]],
    )
}

fn keyboard_jump_switches_to_a_target_in_another_window() -> ParityBatchCase {
    ParityBatchCase::value(
        "keyboard_jump_switches_to_a_target_in_another_window",
        r##"
(let ((operations (generate-new-buffer " *neomacs-avy-operations*"))
      (audit (generate-new-buffer " *neomacs-avy-audit*")))
  (unwind-protect
      (save-window-excursion
        (delete-other-windows)
        (with-current-buffer operations
          (insert "Operations\nX inspect queue\nReady"))
        (with-current-buffer audit
          (insert "Audit\nX inspect incident\nResolved"))
        (let* ((operations-window (selected-window))
               (audit-window (split-window-right)))
          (set-window-buffer operations-window operations)
          (set-window-buffer audit-window audit)
          (set-window-start operations-window 1)
          (set-window-start audit-window 1)
          (select-window operations-window)
          (goto-char (point-min))
          (local-set-key (kbd "C-c j") #'neomacs-avy-test-goto-cross-window)
          (let ((avy-all-windows t)
                (avy-case-fold-search nil)
                (avy-keys '(?a ?s))
                (avy-single-candidate-jump nil)
                (avy-ring (make-ring 20)))
            (execute-kbd-macro (kbd "C-c j a"))
            (let ((origin (ring-ref avy-ring 0)))
              (list :selected-audit (eq (current-buffer) audit)
                    :selected-window (eq (selected-window) audit-window)
                    :line (line-number-at-pos)
                    :text (neomacs-avy-test-current-line)
                    :origin-operations (eq (window-buffer (cdr origin)) operations)
                    :origin-point (car origin)
                    :overlays (neomacs-avy-test-overlay-count))))))
    (when (buffer-live-p operations) (kill-buffer operations))
    (when (buffer-live-p audit) (kill-buffer audit))))
"##,
        expect![[
            r##"OK (:selected-audit t :selected-window t :line 2 :text "X inspect incident" :origin-operations t :origin-point 1 :overlays 0)"##
        ]],
    )
}

fn dispatch_yank_inserts_a_selected_expression_at_the_original_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "dispatch_yank_inserts_a_selected_expression_at_the_original_point",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "(pipeline )\n\n(retry-job payload)\n(release-job payload)")
    (goto-char (point-min))
    (search-forward "pipeline ")
    (local-set-key (kbd "C-c j") #'neomacs-avy-test-yank-release)
    (let ((avy-all-windows nil)
          (avy-case-fold-search nil)
          (avy-keys '(?a ?s))
          (avy-single-candidate-jump nil)
          (avy-dispatch-alist '((?y . avy-action-yank)))
          (avy-ring (make-ring 20))
          (kill-ring nil)
          (kill-ring-yank-pointer nil))
      (execute-kbd-macro (kbd "C-c j y s s"))
      (list :text (buffer-substring-no-properties (point-min) (point-max))
            :copied (current-kill 0)
            :line (line-number-at-pos)
            :history (ring-length avy-ring)
            :overlays (neomacs-avy-test-overlay-count)))))
"##,
        expect![[
            r##"OK (:text "(pipeline (release-job payload))\n\n(retry-job payload)\n(release-job payload)" :copied "(release-job payload)" :line 1 :history 1 :overlays 0)"##
        ]],
    )
}

fn line_commands_copy_and_move_selected_release_steps() -> ParityBatchCase {
    ParityBatchCase::value(
        "line_commands_copy_and_move_selected_release_steps",
        r##"
(let ((copy-result
       (with-temp-buffer
         (save-window-excursion
           (switch-to-buffer (current-buffer))
           (insert "Deploy canary\nDeploy staging\nDeploy production\nVerify metrics")
           (goto-char (point-max))
           (beginning-of-line)
           (local-set-key (kbd "C-c c") #'neomacs-avy-test-copy-line)
           (let ((avy-all-windows nil)
                 (avy-keys '(?a ?s ?d ?f))
                 (avy-single-candidate-jump nil)
                 (avy-line-insert-style 'above)
                 (avy-ring (make-ring 20)))
             (execute-kbd-macro (kbd "C-c c s"))
             (list :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :point-line (line-number-at-pos)
                   :history (ring-length avy-ring))))))
      (move-result
       (with-temp-buffer
         (save-window-excursion
           (switch-to-buffer (current-buffer))
           (insert "Deploy canary\nDeploy staging\nDeploy production\nVerify metrics")
           (goto-char (point-min))
           (local-set-key (kbd "C-c m") #'neomacs-avy-test-move-line)
           (let ((avy-all-windows nil)
                 (avy-keys '(?a ?s ?d ?f))
                 (avy-single-candidate-jump nil)
                 (avy-line-insert-style 'above)
                 (avy-ring (make-ring 20))
                 (kill-ring nil)
                 (kill-ring-yank-pointer nil))
             (execute-kbd-macro (kbd "C-c m d"))
             (list :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :point-line (line-number-at-pos)
                   :killed (current-kill 0)
                   :history (ring-length avy-ring)))))))
  (list :copied copy-result :moved move-result))
"##,
        expect![[
            r##"OK (:copied (:text "Deploy canary\nDeploy staging\nDeploy production\nDeploy staging\nVerify metrics" :point-line 4 :history 1) :moved (:text "Deploy production\nDeploy canary\nDeploy staging\nVerify metrics" :point-line 1 :killed "Deploy production\n" :history 1))"##
        ]],
    )
}

fn cancelling_a_jump_restores_point_and_removes_every_overlay() -> ParityBatchCase {
    ParityBatchCase::value(
        "cancelling_a_jump_restores_point_and_removes_every_overlay",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "Deploy canary\nDeploy staging\nDeploy production")
    (goto-char (point-min))
    (forward-line 1)
    (let ((before-point (point))
          (before-text (buffer-string)))
      (local-set-key (kbd "C-c j") #'neomacs-avy-test-goto-deploy)
      (let ((avy-all-windows nil)
            (avy-background t)
            (avy-case-fold-search nil)
            (avy-keys '(?a ?s ?d))
            (avy-single-candidate-jump nil)
            (avy-ring (make-ring 20))
            (avy--overlays-back nil)
            (avy--overlays-lead nil))
        (execute-kbd-macro (vconcat (kbd "C-c j") [escape]))
        (list :point-restored (= (point) before-point)
              :text-restored (equal (buffer-string) before-text)
              :history (ring-length avy-ring)
              :lead-overlays (length avy--overlays-lead)
              :background-overlays (length avy--overlays-back)
              :buffer-overlays (neomacs-avy-test-overlay-count))))))
"##,
        expect![[
            r##"OK (:point-restored t :text-restored t :history 0 :lead-overlays 0 :background-overlays 0 :buffer-overlays 0)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        keyboard_jump_and_candidate_history_navigate_deployment_targets(),
        keyboard_jump_switches_to_a_target_in_another_window(),
        dispatch_yank_inserts_a_selected_expression_at_the_original_point(),
        line_commands_copy_and_move_selected_release_steps(),
        cancelling_a_jump_restores_point_and_removes_every_overlay(),
    ]
}
