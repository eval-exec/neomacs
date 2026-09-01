use expect_test::expect;

use super::ParityBatchCase;

fn mode_switches_install_only_requested_feedback_without_changing_edits() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (neomacs-evil-goggles-test-configure-visible-faces)
      (let ((evil-goggles-enable-yank nil)
            (evil-goggles-enable-delete t)
            (evil-goggles-enable-paste nil)
            (evil-goggles-pulse nil)
            (evil-goggles-blocking-duration 0.125))
        (evil-goggles-mode 1)
        (let ((enabled
               (list
                :mode evil-goggles-mode
                :yank
                (and (advice-member-p 'evil-goggles--generic-async-advice
                                      'evil-yank)
                     t)
                :delete
                (and (advice-member-p 'evil-goggles--generic-blocking-advice
                                      'evil-delete)
                     t)
                :paste
                (and (advice-member-p 'evil-goggles--paste-advice
                                      'evil-paste-after)
                     t))))
          (with-temp-buffer
            (save-window-excursion
              (switch-to-buffer (current-buffer))
              (insert "draft release\nstable release\n")
              (goto-char (point-min))
              (evil-local-mode 1)
              (evil-normal-state)
              (neomacs-evil-goggles-test-keys "y y")
              (neomacs-evil-goggles-test-keys "d d")
              (let ((workflow
                     (list
                      :buffer (buffer-string)
                      :register (substring-no-properties (current-kill 0))
                      :point (point)
                      :state evil-state
                      :events neomacs-evil-goggles-test-events
                      :live (neomacs-evil-goggles-test--live-summary))))
                (evil-goggles-mode -1)
                (list
                 :enabled enabled
                 :workflow workflow
                 :disabled
                 (list
                  :mode evil-goggles-mode
                  :delete
                  (and (advice-member-p
                        'evil-goggles--generic-blocking-advice 'evil-delete)
                       t)))))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:enabled (:mode t :yank nil :delete t :paste nil) :workflow (:buffer "stable release\n" :register "draft release\n" :point 1 :state normal :events ((:blocking :duration 0.125 :this-command evil-delete :real-this-command evil-delete :overlays ((:range (1 15) :text "draft release\n" :face evil-goggles-delete-face :priority 9999 :selected-window t :insert-behind t)))) :live (:overlays nil :timer nil :pre-command-cleanup nil)) :disabled (:mode nil :delete nil))"#
    ]];
    ParityBatchCase::value(
        "mode_switches_install_only_requested_feedback_without_changing_edits",
        elisp_form,
        expect,
    )
}

fn yank_then_linewise_paste_tracks_real_text_until_the_next_command() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (neomacs-evil-goggles-test-configure-visible-faces)
      (let ((evil-goggles-pulse nil)
            (evil-goggles-async-duration 0.75))
        (evil-goggles-mode 1)
        (with-temp-buffer
          (save-window-excursion
            (switch-to-buffer (current-buffer))
            (insert "deploy α\nverify β\n")
            (goto-char (point-min))
            (evil-local-mode 1)
            (evil-normal-state)
            (neomacs-evil-goggles-test-keys "y y")
            (let ((after-yank
                   (list
                    :buffer (buffer-string)
                    :register (substring-no-properties (current-kill 0))
                    :point (point)
                    :live (neomacs-evil-goggles-test--live-summary))))
              (neomacs-evil-goggles-test-keys "p")
              (let ((after-paste
                     (list
                      :buffer (buffer-string)
                      :point (point)
                      :markers
                      (list (evil-get-marker ?\[)
                            (evil-get-marker ?\]))
                      :live (neomacs-evil-goggles-test--live-summary))))
                (neomacs-evil-goggles-test-keys "j")
                (list
                 :after-yank after-yank
                 :after-paste after-paste
                 :after-next-command
                 (list :point (point)
                       :live (neomacs-evil-goggles-test--live-summary))
                 :events neomacs-evil-goggles-test-events)))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:after-yank (:buffer "deploy α\nverify β\n" :register "deploy α\n" :point 1 :live (:overlays ((:range (1 10) :text "deploy α\n" :face evil-goggles-yank-face :priority 9999 :selected-window t :insert-behind t)) :timer t :pre-command-cleanup nil)) :after-paste (:buffer "deploy α\ndeploy α\nverify β\n" :point 10 :markers (10 18) :live (:overlays ((:range (10 19) :text "deploy α\n" :face evil-goggles-paste-face :priority 9999 :selected-window t :insert-behind t)) :timer t :pre-command-cleanup nil)) :after-next-command (:point 19 :live (:overlays nil :timer nil :pre-command-cleanup nil)) :events ((:timer :duration 0.75 :repeat nil :function evil-goggles--vanish :arguments nil :cleanup-hook t :this-command evil-yank :real-this-command evil-yank :overlays ((:range (1 10) :text "deploy α\n" :face evil-goggles-yank-face :priority 9999 :selected-window t :insert-behind t))) (:timer :duration 0.75 :repeat nil :function evil-goggles--vanish :arguments nil :cleanup-hook t :this-command evil-paste-after :real-this-command evil-paste-after :overlays ((:range (10 19) :text "deploy α\n" :face evil-goggles-paste-face :priority 9999 :selected-window t :insert-behind t)))))"#
    ]];
    ParityBatchCase::value(
        "yank_then_linewise_paste_tracks_real_text_until_the_next_command",
        elisp_form,
        expect,
    )
}

fn delete_and_change_preview_the_exact_text_that_real_operators_replace() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (neomacs-evil-goggles-test-configure-visible-faces)
      (let ((evil-goggles-pulse nil)
            (evil-goggles-blocking-duration 0.125))
        (evil-goggles-mode 1)
        (with-temp-buffer
          (save-window-excursion
            (switch-to-buffer (current-buffer))
            (insert "draft release now\nkeep stable\n")
            (goto-char (point-min))
            (evil-local-mode 1)
            (evil-normal-state)
            (neomacs-evil-goggles-test-keys "d w")
            (let ((after-delete
                   (list :buffer (buffer-string)
                         :point (point)
                         :register
                         (substring-no-properties (current-kill 0)))))
              (neomacs-evil-goggles-test-keys "c w s h i p p e d <escape>")
              (list
               :after-delete after-delete
               :after-change
               (list :buffer (buffer-string)
                     :point (point)
                     :state evil-state
                     :register (substring-no-properties (current-kill 0))
                     :small-delete
                     (substring-no-properties (evil-get-register ?-)))
               :events neomacs-evil-goggles-test-events
               :live (neomacs-evil-goggles-test--live-summary)))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:after-delete (:buffer "release now\nkeep stable\n" :point 1 :register "draft ") :after-change (:buffer "shipped now\nkeep stable\n" :point 7 :state normal :register "release" :small-delete "release") :events ((:blocking :duration 0.125 :this-command evil-delete :real-this-command evil-delete :overlays ((:range (1 7) :text "draft " :face evil-goggles-delete-face :priority 9999 :selected-window t :insert-behind t))) (:blocking :duration 0.125 :this-command evil-change :real-this-command evil-change :overlays ((:range (1 8) :text "release" :face evil-goggles-change-face :priority 9999 :selected-window t :insert-behind t)))) :live (:overlays nil :timer nil :pre-command-cleanup nil))"#
    ]];
    ParityBatchCase::value(
        "delete_and_change_preview_the_exact_text_that_real_operators_replace",
        elisp_form,
        expect,
    )
}

fn counted_shift_and_counted_join_preserve_real_operator_results_and_hint_ranges() -> ParityBatchCase
{
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (neomacs-evil-goggles-test-configure-visible-faces)
      (let ((evil-goggles-pulse nil)
            (evil-goggles-async-duration 0.75)
            (evil-goggles-blocking-duration 0.125)
            (evil-shift-width 2))
        (evil-goggles-mode 1)
        (with-temp-buffer
          (save-window-excursion
            (switch-to-buffer (current-buffer))
            (insert "root\nchild\nleaf\ntrailer\n")
            (goto-char (point-min))
            (evil-local-mode 1)
            (evil-normal-state)
            (neomacs-evil-goggles-test-keys "2 > >")
            (let ((after-shift
                   (list :buffer (buffer-string)
                         :point (point)
                         :state evil-state
                         :live (neomacs-evil-goggles-test--live-summary))))
              (goto-char (point-min))
              (neomacs-evil-goggles-test-keys "2 J")
              (list
               :after-shift after-shift
               :after-join
               (list :buffer (buffer-string)
                     :point (point)
                     :state evil-state
                     :live (neomacs-evil-goggles-test--live-summary))
               :events neomacs-evil-goggles-test-events))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:after-shift (:buffer "  root\n  child\nleaf\ntrailer\n" :point 1 :state normal :live (:overlays ((:range (1 16) :text "  root\n  child\n" :face evil-goggles-shift-face :priority 9999 :selected-window t :insert-behind t)) :timer t :pre-command-cleanup nil)) :after-join (:buffer "  root child\nleaf\ntrailer\n" :point 7 :state normal :live (:overlays nil :timer nil :pre-command-cleanup nil)) :events ((:timer :duration 0.75 :repeat nil :function evil-goggles--vanish :arguments nil :cleanup-hook t :this-command evil-shift-right :real-this-command evil-shift-right :overlays ((:range (1 12) :text "root\nchild\n" :face evil-goggles-shift-face :priority 9999 :selected-window t :insert-behind t))) (:blocking :duration 0.125 :this-command evil-join :real-this-command evil-join :overlays ((:range (1 16) :text "  root\n  child\n" :face evil-goggles-join-face :priority 9999 :selected-window t :insert-behind t)))))"#
    ]];
    ParityBatchCase::value(
        "counted_shift_and_counted_join_preserve_real_operator_results_and_hint_ranges",
        elisp_form,
        expect,
    )
}

fn ineligible_edits_still_execute_without_single_character_whitespace_visual_or_inhibited_hints()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (neomacs-evil-goggles-test-configure-visible-faces)
      (let ((evil-goggles-pulse nil)
            (button-action nil))
        (evil-goggles-mode 1)
        (with-temp-buffer
          (save-window-excursion
            (switch-to-buffer (current-buffer))
            (insert "AB\n   \nvisual words\n")
            (insert-text-button
             "button action"
             'action (lambda (_button) (setq button-action 'opened)))
            (insert "\nkeep\n")
            (goto-char (point-min))
            (evil-local-mode 1)
            (evil-normal-state)
            (neomacs-evil-goggles-test-keys "x")
            (let ((single-character
                   (buffer-substring-no-properties (point-min) (point-max))))
              (goto-char (point-min))
              (forward-line 1)
              (neomacs-evil-goggles-test-keys "d $")
              (let ((whitespace
                     (buffer-substring-no-properties
                      (point-min) (point-max))))
                (goto-char (point-min))
                (forward-line 2)
                (neomacs-evil-goggles-test-keys "v e d")
                (let ((visual
                       (buffer-substring-no-properties
                        (point-min) (point-max))))
                  (goto-char (point-min))
                  (forward-line 3)
                  ;; Evil's RET motion activates the button and sets
                  ;; `evil-inhibit-operator', so the pending delete is aborted.
                  (neomacs-evil-goggles-test-keys "d <return>")
                  (list
                   :single-character single-character
                   :whitespace whitespace
                   :visual visual
                   :inhibited
                   (buffer-substring-no-properties (point-min) (point-max))
                   :button-action button-action
                   :button-remains (and (button-at (point)) t)
                   :state evil-state
                   :point (point)
                   :events neomacs-evil-goggles-test-events
                   :live (neomacs-evil-goggles-test--live-summary)))))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:single-character "B\n   \nvisual words\nbutton action\nkeep\n" :whitespace "B\n\nvisual words\nbutton action\nkeep\n" :visual "B\n\n words\nbutton action\nkeep\n" :inhibited "B\n\n words\nbutton action\nkeep\n" :button-action opened :button-remains t :state normal :point 11 :events nil :live (:overlays nil :timer nil :pre-command-cleanup nil))"#
    ]];
    ParityBatchCase::value(
        "ineligible_edits_still_execute_without_single_character_whitespace_visual_or_inhibited_hints",
        elisp_form,
        expect,
    )
}

fn local_mark_feedback_tracks_real_evil_markers_and_cleans_before_navigation() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (neomacs-evil-goggles-test-configure-visible-faces)
      (let ((evil-goggles-pulse nil)
            (evil-goggles-async-duration 0.75))
        (evil-goggles-mode 1)
        (with-temp-buffer
          (save-window-excursion
            (switch-to-buffer (current-buffer))
            (insert "deploy release\nverify release\nship release\n")
            (goto-char (point-min))
            (evil-local-mode 1)
            (evil-normal-state)
            (neomacs-evil-goggles-test-keys "m a")
            (let ((first
                   (list :marker (evil-get-marker ?a)
                         :point (point)
                         :live (neomacs-evil-goggles-test--live-summary))))
              (neomacs-evil-goggles-test-keys "j")
              (let ((after-navigation
                     (list :point (point)
                           :live (neomacs-evil-goggles-test--live-summary))))
                (neomacs-evil-goggles-test-keys "m b")
                (let ((second
                       (list :markers
                             (list
                              (evil-get-marker ?a)
                              (evil-get-marker ?b))
                             :point (point)
                             :live (neomacs-evil-goggles-test--live-summary))))
                  (neomacs-evil-goggles-test-keys "j")
                  (list
                   :first first
                   :after-navigation after-navigation
                   :second second
                   :final
                   (list :point (point)
                         :live (neomacs-evil-goggles-test--live-summary))
                   :events neomacs-evil-goggles-test-events))))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:first (:marker 1 :point 1 :live (:overlays ((:range (1 16) :text "deploy release\n" :face evil-goggles-set-marker-face :priority 9999 :selected-window t :insert-behind t)) :timer t :pre-command-cleanup nil)) :after-navigation (:point 16 :live (:overlays nil :timer nil :pre-command-cleanup nil)) :second (:markers (1 16) :point 16 :live (:overlays ((:range (16 31) :text "verify release\n" :face evil-goggles-set-marker-face :priority 9999 :selected-window t :insert-behind t)) :timer t :pre-command-cleanup nil)) :final (:point 31 :live (:overlays nil :timer nil :pre-command-cleanup nil)) :events ((:timer :duration 0.75 :repeat nil :function evil-goggles--vanish :arguments nil :cleanup-hook t :this-command evil-set-marker :real-this-command evil-set-marker :overlays ((:range (1 16) :text "deploy release\n" :face evil-goggles-set-marker-face :priority 9999 :selected-window t :insert-behind t))) (:timer :duration 0.75 :repeat nil :function evil-goggles--vanish :arguments nil :cleanup-hook t :this-command evil-set-marker :real-this-command evil-set-marker :overlays ((:range (16 31) :text "verify release\n" :face evil-goggles-set-marker-face :priority 9999 :selected-window t :insert-behind t)))))"#
    ]];
    ParityBatchCase::value(
        "local_mark_feedback_tracks_real_evil_markers_and_cleans_before_navigation",
        elisp_form,
        expect,
    )
}

fn public_diff_face_presets_drive_pulsed_yank_feedback() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-evil-goggles-test-with-state
 (lambda ()
  (unwind-protect
    (progn
      (neomacs-evil-goggles-test-reset)
      (evil-goggles-use-diff-faces)
      (let ((diff-inheritance
             (list
              (face-attribute 'evil-goggles-delete-face :inherit nil 'default)
              (face-attribute 'evil-goggles-change-face :inherit nil 'default)
              (face-attribute 'evil-goggles-paste-face :inherit nil 'default)
              (face-attribute 'evil-goggles-yank-face :inherit nil 'default))))
        (evil-goggles-use-diff-refine-faces)
        (let ((refine-inheritance
               (list
                (face-attribute 'evil-goggles-delete-face :inherit nil 'default)
                (face-attribute 'evil-goggles-change-face :inherit nil 'default)
                (face-attribute 'evil-goggles-paste-face :inherit nil 'default)
                (face-attribute 'evil-goggles-yank-face :inherit nil 'default))))
          (let ((evil-goggles-pulse t)
                (evil-goggles-async-duration 0.75))
            (evil-goggles-mode 1)
            (with-temp-buffer
              (save-window-excursion
                (switch-to-buffer (current-buffer))
                (insert "release candidate\nstable\n")
                (goto-char (point-min))
                (evil-local-mode 1)
                (evil-normal-state)
                (neomacs-evil-goggles-test-keys "y y")
                (list
                 :diff-inheritance diff-inheritance
                 :refine-inheritance refine-inheritance
                 :register (substring-no-properties (current-kill 0))
                 :live (neomacs-evil-goggles-test--live-summary)
                 :events neomacs-evil-goggles-test-events)))))))
   (neomacs-evil-goggles-test-reset))))
"##;
    let expect = expect![[
        r#"OK (:diff-inheritance (diff-removed diff-removed diff-added diff-changed) :refine-inheritance (diff-refine-removed diff-refine-removed diff-refine-added diff-refine-changed) :register "release candidate\n" :live (:overlays ((:range (1 19) :text "release candidate\n" :face nil :priority 9999 :selected-window t :insert-behind t)) :timer t :pre-command-cleanup nil) :events ((:pulse :face evil-goggles--pulse-face :background nil :target (:range (1 19) :text "release candidate\n" :face nil :priority 9999 :selected-window t :insert-behind t) :this-command evil-yank :real-this-command evil-yank :overlays ((:range (1 19) :text "release candidate\n" :face nil :priority 9999 :selected-window t :insert-behind t))) (:timer :duration 0.75 :repeat nil :function evil-goggles--vanish :arguments nil :cleanup-hook t :this-command evil-yank :real-this-command evil-yank :overlays ((:range (1 19) :text "release candidate\n" :face nil :priority 9999 :selected-window t :insert-behind t)))))"#
    ]];
    ParityBatchCase::value(
        "public_diff_face_presets_drive_pulsed_yank_feedback",
        elisp_form,
        expect,
    )
    // `evil-goggles-use-diff-faces' deliberately writes the persistent user
    // theme.  Quarantine that public customization workflow instead of
    // pretending to reverse GNU Emacs' Custom internals in a shared process.
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_switches_install_only_requested_feedback_without_changing_edits(),
        yank_then_linewise_paste_tracks_real_text_until_the_next_command(),
        delete_and_change_preview_the_exact_text_that_real_operators_replace(),
        counted_shift_and_counted_join_preserve_real_operator_results_and_hint_ranges(),
        ineligible_edits_still_execute_without_single_character_whitespace_visual_or_inhibited_hints(),
        local_mark_feedback_tracks_real_evil_markers_and_cleans_before_navigation(),
        public_diff_face_presets_drive_pulsed_yank_feedback(),
    ]
}
