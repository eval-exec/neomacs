use expect_test::expect;

use super::ParityBatchCase;

fn auto_dim_other_buffers_practical_two_buffer_editing_workflow_tracks_selection_content_and_remaps()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_practical_two_buffer_editing_workflow_tracks_selection_content_and_remaps",
        r##"(save-window-excursion
          (let ((notes
                 (generate-new-buffer
                  " *adob-notes*"))
                (code
                 (generate-new-buffer
                  " *adob-code*")))
            (unwind-protect
                (progn
                  (with-current-buffer notes
                    (insert
                     "project notes\n"
                     "- first task\n"))
                  (with-current-buffer code
                    (emacs-lisp-mode)
                    (insert
                     "(defun fixture ()\n"
                     "  :ready)\n"))
                  (delete-other-windows)
                  (let* ((notes-window
                          (selected-window))
                         (code-window
                          (split-window-below)))
                    (set-window-buffer
                     notes-window
                     notes)
                    (set-window-buffer
                     code-window
                     code)
                    (select-window notes-window)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)
                             (mode-line
                              . (nil . bold))))
                          (adob--has-fringes nil))
                      (auto-dim-other-buffers-mode 1)
                      (let ((notes-selected
                             (adob-test-window-summary)))
                        (select-window code-window)
                        (goto-char
                         (point-max))
                        (insert
                         "\n(message \"edited\")")
                        (adob--update)
                        (list
                         notes-selected
                         (adob-test-window-summary)
                         (with-current-buffer notes
                           (buffer-string))
                         (with-current-buffer code
                           (buffer-string))
                         (adob-test-remap-summary
                          notes)
                         (adob-test-remap-summary
                          code))))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p notes)
                (kill-buffer notes))
              (when (buffer-live-p code)
                (kill-buffer code)))))"##,
        expect![[
            r#"OK (((t " *adob-notes*" nil) (nil " *adob-code*" t)) ((t " *adob-code*" nil) (nil " *adob-notes*" t)) "project notes\n- first task\n" "(defun fixture ()\n  :ready)\n\n(message \"edited\")" (t 2 (default mode-line) ((mode-line ((:filtered (:window adob--dim nil) bold))) (default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) (t 2 (default mode-line) ((mode-line ((:filtered (:window adob--dim nil) bold))) (default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

fn auto_dim_other_buffers_practical_same_buffer_two_view_workflow_dims_only_unselected_view()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_practical_same_buffer_two_view_workflow_dims_only_unselected_view",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-two-views*")))
            (unwind-protect
                (progn
                  (with-current-buffer buffer
                    (dotimes (line 8)
                      (insert
                       (format
                        "line-%d\n"
                        line))))
                  (delete-other-windows)
                  (let* ((upper
                          (selected-window))
                         (lower
                          (split-window-below)))
                    (set-window-buffer upper buffer)
                    (set-window-buffer lower buffer)
                    (set-window-point upper 1)
                    (set-window-point lower 22)
                    (select-window upper)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil))
                      (auto-dim-other-buffers-mode 1)
                      (let ((upper-selected
                             (list
                              (adob-test-window-summary)
                              (mapcar
                               #'window-point
                               (window-list nil 'n)))))
                        (select-window lower)
                        (adob--update)
                        (list
                         upper-selected
                         (adob-test-window-summary)
                         (mapcar
                          #'window-point
                          (window-list nil 'n))
                         (adob-test-remap-summary
                          buffer))))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((((t " *adob-two-views*" nil) (nil " *adob-two-views*" t)) (1 22)) ((t " *adob-two-views*" nil) (nil " *adob-two-views*" t)) (22 1) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

fn auto_dim_other_buffers_practical_special_buffer_exemption_stays_lit_across_window_switches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_practical_special_buffer_exemption_stays_lit_across_window_switches",
        r##"(save-window-excursion
          (let ((editing
                 (generate-new-buffer
                  " *adob-editing*"))
                (special
                 (generate-new-buffer
                  " *adob-special*")))
            (unwind-protect
                (progn
                  (with-current-buffer special
                    (insert
                     "read-only status")
                    (special-mode))
                  (delete-other-windows)
                  (let* ((editing-window
                          (selected-window))
                         (special-window
                          (split-window-below)))
                    (set-window-buffer
                     editing-window
                     editing)
                    (set-window-buffer
                     special-window
                     special)
                    (select-window editing-window)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (auto-dim-other-buffers-never-dim-buffer-functions
                           '(adob-test-never-dim-by-name))
                          (adob-test-never-dim-names
                           (list
                            (buffer-name special)))
                          (adob--has-fringes nil))
                      (auto-dim-other-buffers-mode 1)
                      (let ((editing-selected
                             (list
                              (adob-test-window-summary)
                              (adob-test-remap-summary
                               editing)
                              (adob-test-remap-summary
                               special))))
                        (select-window special-window)
                        (adob--update)
                        (list
                         editing-selected
                         (adob-test-window-summary)
                         (adob-test-remap-summary
                          editing)
                         (adob-test-remap-summary
                          special))))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p editing)
                (kill-buffer editing))
              (when (buffer-live-p special)
                (kill-buffer special)))))"##,
        expect![[
            r#"OK ((((t " *adob-editing*" nil) (nil " *adob-special*" nil)) (t 1 (default) ((default (#1=(:filtered (:window adob--dim t) auto-dim-other-buffers))))) (nil 0 nil nil)) ((t " *adob-special*" nil) (nil " *adob-editing*" t)) (t 1 (default) ((default (#1#)))) (nil 0 nil nil))"#
        ]],
    )
}

fn auto_dim_other_buffers_live_customize_changes_real_dim_and_highlight_remapping_specs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_live_customize_changes_real_dim_and_highlight_remapping_specs",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-custom-live*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((auto-dim-other-buffers-affected-faces
                         '((default
                            . auto-dim-other-buffers)))
                        (adob--has-fringes nil))
                    (auto-dim-other-buffers-mode 1)
                    (let ((before
                           (adob-test-remap-summary
                            buffer)))
                      (customize-set-variable
                       'auto-dim-other-buffers-affected-faces
                       '((default
                          . (nil . bold))
                         (mode-line
                          . (auto-dim-other-buffers
                             . mode-line-active))))
                      (list
                       before
                       auto-dim-other-buffers-affected-faces
                       (adob-test-remap-summary
                        buffer)))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![
            "OK ((t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) ((default nil . bold) (mode-line auto-dim-other-buffers . mode-line-active)) (t 2 (default mode-line) ((mode-line nil) (default ((:filtered (:window adob--dim nil) bold))))))"
        ],
    )
}

fn auto_dim_other_buffers_enabled_advice_restores_mapping_after_real_major_mode_change()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_enabled_advice_restores_mapping_after_real_major_mode_change",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-major-mode*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((auto-dim-other-buffers-affected-faces
                         '((default
                            . auto-dim-other-buffers)))
                        (adob--has-fringes nil))
                    (auto-dim-other-buffers-mode 1)
                    (with-current-buffer buffer
                      (setq-local
                       adob-test-major-local
                       :before)
                      (let ((before
                             (adob-test-remap-summary
                              buffer)))
                        (text-mode)
                        (list
                         before
                         major-mode
                         (local-variable-p
                          'adob-test-major-local)
                         (adob-test-remap-summary
                          buffer))))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![
            "OK ((t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) text-mode nil (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"
        ],
    )
}

fn auto_dim_other_buffers_direct_face_assignment_is_stale_until_disable_reenable_rebuilds_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_direct_face_assignment_is_stale_until_disable_reenable_rebuilds_it",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-direct-config*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((auto-dim-other-buffers-affected-faces
                         '((default
                            . auto-dim-other-buffers)))
                        (adob--has-fringes nil))
                    (auto-dim-other-buffers-mode 1)
                    (let ((before
                           (adob-test-remap-summary
                            buffer)))
                      (setq
                       auto-dim-other-buffers-affected-faces
                       '((mode-line
                          . (nil . bold))))
                      (let ((stale
                             (adob-test-remap-summary
                              buffer)))
                        (auto-dim-other-buffers-mode
                         -1)
                        (auto-dim-other-buffers-mode
                         1)
                        (list
                         before
                         stale
                         (adob-test-remap-summary
                          buffer))))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![
            "OK ((t 1 (default) ((default (#1=(:filtered (:window adob--dim t) auto-dim-other-buffers))))) (t 1 (default) ((default (#1#)))) (t 1 (mode-line) ((mode-line ((:filtered (:window adob--dim nil) bold))))))"
        ],
    )
}

fn auto_dim_other_buffers_source_reload_while_enabled_keeps_runtime_integrations_and_can_disable_cleanly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_source_reload_while_enabled_keeps_runtime_integrations_and_can_disable_cleanly",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-reload-enabled*"))
                (source
                 (getenv
                  "NEOMACS_PACKAGE_SOURCE")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((auto-dim-other-buffers-affected-faces
                         '((default
                            . auto-dim-other-buffers)))
                        (adob--has-fringes nil))
                    (auto-dim-other-buffers-mode 1)
                    (let ((before
                           (list
                            auto-dim-other-buffers-mode
                            (adob-test-hook-count
                             'adob--rescan-windows
                             'window-configuration-change-hook)
                            (adob-test-hook-count
                             'adob--buffer-list-update-hook
                             'buffer-list-update-hook)
                            (and
                             (advice-member-p
                              #'adob--kill-all-local-variables-advice
                              'kill-all-local-variables)
                             t)
                            (adob-test-remap-summary
                             buffer))))
                      (load source nil t t)
                      (let ((reloaded
                             (list
                              auto-dim-other-buffers-mode
                              (adob-test-hook-count
                               'adob--rescan-windows
                               'window-configuration-change-hook)
                              (adob-test-hook-count
                               'adob--buffer-list-update-hook
                               'buffer-list-update-hook)
                              (and
                               (advice-member-p
                                #'adob--kill-all-local-variables-advice
                                'kill-all-local-variables)
                               t)
                              (adob-test-remap-summary
                               buffer))))
                        (auto-dim-other-buffers-mode
                         -1)
                        (list
                         before
                         reloaded
                         auto-dim-other-buffers-mode
                         (adob-test-remap-summary
                          buffer))))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![
            "OK ((t 1 1 t (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers)))))) (t 1 1 t (t 4 (default fringe org-block org-hide) ((org-hide ((:filtered (:window adob--dim t) auto-dim-other-buffers-hide))) (org-block ((:filtered (:window adob--dim t) auto-dim-other-buffers))) (fringe ((:filtered (:window adob--dim t) auto-dim-other-buffers))) (default ((:filtered (:window adob--dim t) auto-dim-other-buffers)))))) nil (nil 0 nil nil))"
        ],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_practical_two_buffer_editing_workflow_tracks_selection_content_and_remaps(),
        auto_dim_other_buffers_practical_same_buffer_two_view_workflow_dims_only_unselected_view(),
        auto_dim_other_buffers_practical_special_buffer_exemption_stays_lit_across_window_switches(),
        auto_dim_other_buffers_live_customize_changes_real_dim_and_highlight_remapping_specs(),
        auto_dim_other_buffers_enabled_advice_restores_mapping_after_real_major_mode_change(),
        auto_dim_other_buffers_direct_face_assignment_is_stale_until_disable_reenable_rebuilds_it(),
        auto_dim_other_buffers_source_reload_while_enabled_keeps_runtime_integrations_and_can_disable_cleanly(),
    ]
}
