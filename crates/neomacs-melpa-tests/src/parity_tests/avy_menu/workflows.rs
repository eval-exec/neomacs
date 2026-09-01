use expect_test::expect;

use super::ParityBatchCase;

fn selecting_a_release_action_uses_a_multilevel_path_and_restores_the_window() -> ParityBatchCase {
    ParityBatchCase::value(
        "selecting_a_release_action_uses_a_multilevel_path_and_restores_the_window",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "release dashboard")
    (local-set-key (kbd "C-c m") #'neomacs-avy-menu-test-open)
    (setq neomacs-avy-menu-test-spec
          '("Release Operations"
            ("Deployment"
             ("Deploy Canary" . (:action deploy :environment canary))
             ("Deploy Staging" . (:action deploy :environment staging))
             ("Deploy Production" . (:action deploy :environment production)))
            ("Safety"
             "Read-only health checks"
             ""
             ("Rollback Release" . (:action rollback)))))
    (setq neomacs-avy-menu-test-show-pane-header t
          neomacs-avy-menu-test-result nil
          neomacs-avy-menu-test-observed nil)
    (let ((origin (selected-window))
          (avy-keys '(?a ?s))
          (avy-single-candidate-jump nil)
          (avy-pre-action #'neomacs-avy-menu-test-capture))
      (execute-kbd-macro (kbd "C-c m s a"))
      (list :result neomacs-avy-menu-test-result
            :rendered neomacs-avy-menu-test-observed
            :origin-restored (eq (selected-window) origin)
            :origin-text (buffer-string)
            :menu-buffer-live (and (get-buffer " *neomacs-avy-menu*") t)))))
"##,
        expect![[
            r##"OK (:result (:action deploy :environment production) :rendered (:text "Release Operations\n\nDeployment\n\nDeploy Canary\nDeploy Staging\nDeploy Production\n\nSafety\n\nRead-only health checks\n\nRollback Release" :faces (("Release Operations" avy-menu-title) ("Deployment" avy-menu-pane-header) ("Safety" avy-menu-pane-header) ("Read-only health checks" avy-menu-inactive)) :cursor nil :candidates 4 :menu-windows 1) :origin-restored t :origin-text "release dashboard" :menu-buffer-live nil)"##
        ]],
    )
}

fn a_compact_context_menu_hides_headers_but_keeps_inactive_guidance() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_compact_context_menu_hides_headers_but_keeps_inactive_guidance",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "incident 42")
    (local-set-key (kbd "C-c m") #'neomacs-avy-menu-test-open)
    (setq neomacs-avy-menu-test-spec
          '("Incident Actions"
            ("Investigation"
             "Requires an incident owner"
             ("Open Logs" . (:open logs))
             ("Open Trace" . (:open trace)))
            ("Coordination"
             ("Page On-call" . (:notify on-call)))))
    (setq neomacs-avy-menu-test-show-pane-header nil
          neomacs-avy-menu-test-result nil
          neomacs-avy-menu-test-observed nil)
    (let ((avy-keys '(?a ?s ?d))
          (avy-single-candidate-jump nil)
          (avy-pre-action #'neomacs-avy-menu-test-capture))
      (execute-kbd-macro (kbd "C-c m s"))
      (list :result neomacs-avy-menu-test-result
            :rendered neomacs-avy-menu-test-observed
            :menu-buffer-live (and (get-buffer " *neomacs-avy-menu*") t)))))
"##,
        expect![[
            r##"OK (:result (:open trace) :rendered (:text "Incident Actions\n\nRequires an incident owner\nOpen Logs\nOpen Trace\n\nPage On-call" :faces (("Incident Actions" avy-menu-title) ("Requires an incident owner" avy-menu-inactive)) :cursor nil :candidates 3 :menu-windows 1) :menu-buffer-live nil)"##
        ]],
    )
}

fn a_single_available_recovery_action_selects_without_an_extra_key() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_single_available_recovery_action_selects_without_an_extra_key",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (local-set-key (kbd "C-c m") #'neomacs-avy-menu-test-open)
    (setq neomacs-avy-menu-test-spec
          '("Recovery"
            ("Database"
             "Replica is already healthy"
             ("Promote Standby" . (:promote standby)))))
    (setq neomacs-avy-menu-test-show-pane-header t
          neomacs-avy-menu-test-result nil
          neomacs-avy-menu-test-observed nil)
    (let ((avy-single-candidate-jump t)
          (avy-pre-action #'neomacs-avy-menu-test-capture))
      (execute-kbd-macro (kbd "C-c m"))
      (list :result neomacs-avy-menu-test-result
            :rendered neomacs-avy-menu-test-observed
            :menu-buffer-live (and (get-buffer " *neomacs-avy-menu*") t)))))
"##,
        expect![[
            r##"OK (:result (:promote standby) :rendered (:text "Recovery\n\nDatabase\n\nReplica is already healthy\nPromote Standby" :faces (("Recovery" avy-menu-title) ("Database" avy-menu-pane-header) ("Replica is already healthy" avy-menu-inactive)) :cursor nil :candidates 1 :menu-windows 1) :menu-buffer-live nil)"##
        ]],
    )
}

fn cancelling_a_menu_returns_nil_and_kills_the_temporary_ui() -> ParityBatchCase {
    ParityBatchCase::value(
        "cancelling_a_menu_returns_nil_and_kills_the_temporary_ui",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "unchanged workspace")
    (goto-char 7)
    (local-set-key (kbd "C-c m") #'neomacs-avy-menu-test-open)
    (setq neomacs-avy-menu-test-spec
          '("Workspace"
            ("Files"
             ("Save All" . save)
             ("Close Workspace" . close))))
    (setq neomacs-avy-menu-test-show-pane-header t
          neomacs-avy-menu-test-result :not-run
          neomacs-avy-menu-test-observed :not-selected)
    (let ((origin (selected-window))
          (before-point (point))
          (before-text (buffer-string))
          (avy-keys '(?a ?s))
          (avy-single-candidate-jump nil))
      (execute-kbd-macro (vconcat (kbd "C-c m") [escape]))
      (list :result neomacs-avy-menu-test-result
            :selection-observed neomacs-avy-menu-test-observed
            :origin-restored (eq (selected-window) origin)
            :point-restored (= (point) before-point)
            :text-restored (equal (buffer-string) before-text)
            :windows (length (window-list))
            :menu-buffer-live (and (get-buffer " *neomacs-avy-menu*") t)))))
"##,
        expect![[
            r##"OK (:result nil :selection-observed :not-selected :origin-restored t :point-restored t :text-restored t :windows 1 :menu-buffer-live nil)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        selecting_a_release_action_uses_a_multilevel_path_and_restores_the_window(),
        a_compact_context_menu_hides_headers_but_keeps_inactive_guidance(),
        a_single_available_recovery_action_selects_without_an_extra_key(),
        cancelling_a_menu_returns_nil_and_kills_the_temporary_ui(),
    ]
}
