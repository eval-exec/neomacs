use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_mode_enable_disable_installs_and_removes_buffer_local_hooks_and_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_mode_enable_disable_installs_and_removes_buffer_local_hooks_and_state",
        r##"(with-temp-buffer
                          (let ((ac-use-comphist nil)
                                (ac-stop-flymake-on-completing
                                 nil)
                                (auto-complete-test-hook-calls
                                 0)
                                (auto-complete-mode-hook
                                 '((lambda ()
                                     (setq
                                      auto-complete-test-hook-calls
                                      (1+
                                       auto-complete-test-hook-calls))))))
                            (let ((before
                                   (list
                                    auto-complete-mode
                                    (local-variable-p
                                     'pre-command-hook)
                                    (local-variable-p
                                     'post-command-hook)
                                    (local-variable-p
                                     'after-save-hook))))
                              (auto-complete-mode 1)
                              (let ((enabled
                                     (list
                                      auto-complete-mode
                                      auto-complete-test-hook-calls
                                      (memq
                                       'ac-handle-pre-command
                                       pre-command-hook)
                                      (memq
                                       'ac-handle-post-command
                                       post-command-hook)
                                      (memq
                                       'ac-clear-variables-after-save
                                       after-save-hook)
                                      (keymapp
                                       (current-local-map)))))
                                (auto-complete-mode -1)
                                (list
                                 before
                                 enabled
                                 (list
                                  auto-complete-mode
                                  auto-complete-test-hook-calls
                                  (memq
                                   'ac-handle-pre-command
                                   pre-command-hook)
                                  (memq
                                   'ac-handle-post-command
                                   post-command-hook)
                                  (memq
                                   'ac-clear-variables-after-save
                                   after-save-hook)
                                  ac-completing
                                  ac-menu
                                  ac-prefix))))))"##,
        expect![
            "OK ((nil nil nil nil) (t 2 (ac-handle-pre-command t) (ac-handle-post-command t) (ac-clear-variables-after-save t) nil) (nil 3 nil nil nil nil nil nil))"
        ],
    )
}

fn auto_complete_trigger_command_classifier_handles_builtin_custom_electric_and_excluded_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_trigger_command_classifier_handles_builtin_custom_electric_and_excluded_commands",
        r##"(let ((ac-trigger-commands
                                '(self-insert-command
                                  fixture-trigger))
                               (ac-non-trigger-commands
                                '(fixture-blocked
                                  electric-buffer-list)))
                           (mapcar
                            (lambda (command)
                              (list
                               command
                               (ac-trigger-command-p
                                command)
                               (ac-compatible-package-command-p
                                command)))
                            '(self-insert-command
                              fixture-trigger
                              fixture-blocked
                              my-self-insert-command
                              electric-pair-post-self-insert-function
                              electric-buffer-list
                              ac-source-command
                              package-install
                              "not-a-symbol"
                              nil)))"##,
        expect![[
            r#"OK ((self-insert-command (self-insert-command . #1=(fixture-trigger)) nil) (fixture-trigger #1# nil) (fixture-blocked nil nil) (my-self-insert-command 3 nil) (electric-pair-post-self-insert-function 0 nil) (electric-buffer-list nil nil) (ac-source-command nil 0) (package-install nil nil) ("not-a-symbol" nil nil) (nil nil nil))"#
        ]],
    )
}

fn auto_complete_disabled_faces_block_pre_command_trigger_at_exact_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_disabled_faces_block_pre_command_trigger_at_exact_point",
        r##"(with-temp-buffer
                          (insert
                           (propertize
                            "comment"
                            'face
                            'font-lock-comment-face)
                           " "
                           (propertize
                            "code"
                            'face
                            'font-lock-keyword-face))
                          (let ((ac-disable-faces
                                 '(font-lock-comment-face
                                   font-lock-string-face)))
                            (mapcar
                             (lambda (position)
                               (goto-char position)
                               (list
                                position
                                (get-text-property
                                 (point)
                                 'face)
                                (ac-cursor-on-diable-face-p)
                                (ac-cursor-on-diable-face-p
                                 position)))
                             '(1 4 8 9 10 12))))"##,
        expect![
            "OK ((1 font-lock-comment-face #1=(font-lock-comment-face font-lock-string-face) #1#) (4 font-lock-comment-face #1# #1#) (8 nil nil nil) (9 font-lock-keyword-face nil nil) (10 font-lock-keyword-face nil nil) (12 font-lock-keyword-face nil nil))"
        ],
    )
}

fn auto_complete_pre_and_post_command_hooks_start_update_and_abort_real_completion_session()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pre_and_post_command_hooks_start_update_and_abort_real_completion_session",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-auto-start 2)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "alpha"
                                      "alpine"
                                      "amber")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "al")
                                    (setq this-command
                                          'self-insert-command)
                                    (ac-handle-pre-command)
                                    (let ((after-pre
                                           (list
                                            ac-triggered
                                            ac-completing
                                            ac-prefix)))
                                      (ac-handle-post-command)
                                      (ac-update t)
                                      (let ((after-post
                                             (list
                                              ac-triggered
                                              ac-completing
                                              ac-prefix
                                              (mapcar
                                               #'substring-no-properties
                                               ac-candidates)
                                              (popup-live-p
                                               ac-menu))))
                                        (setq
                                         this-command
                                         'self-insert-command)
                                        (ac-handle-pre-command)
                                        (insert "p")
                                        (ac-handle-post-command)
                                        (ac-update t)
                                        (let ((after-update
                                               (list
                                                (buffer-substring-no-properties
                                                 (point-min)
                                                 (line-end-position))
                                                ac-triggered
                                                ac-completing
                                                ac-prefix
                                                (mapcar
                                                 #'substring-no-properties
                                                 ac-candidates)
                                                (popup-live-p
                                                 ac-menu))))
                                          (setq
                                           this-command
                                           'forward-char)
                                          (ac-handle-pre-command)
                                          (list
                                           after-pre
                                           after-post
                                           after-update
                                           (list
                                            ac-triggered
                                            ac-completing
                                            ac-prefix
                                            ac-candidates
                                            ac-menu))))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK ((t nil nil) (t t "al" ("alpha" "alpine") t) ("alp" t t "alp" ("alpha" "alpine") t) (nil nil nil nil nil))"#
        ]],
    )
}

fn auto_complete_start_distinguishes_manual_command_from_automatic_stop_words() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_start_distinguishes_manual_command_from_automatic_stop_words",
        r##"(with-temp-buffer
                          (insert "end")
                          (let ((auto-complete-mode t)
                                (ac-use-comphist nil)
                                (ac-use-quick-help nil)
                                (ac-stop-words
                                 '("end"))
                                (ac-use-dictionary-as-stop-words
                                 nil)
                                (ac-sources
                                 '(((candidates
                                    list
                                    "ending"
                                    "endless")))))
                            (unwind-protect
                                (let ((automatic
                                       (ac-start
                                        :requires 1
                                        :triggered t)))
                                  (let ((automatic-state
                                         (list
                                          automatic
                                          ac-prefix
                                          ac-point
                                          ac-current-sources)))
                                    (let ((manual
                                           (ac-start
                                            :requires 1
                                            :triggered
                                            'command)))
                                      (list
                                       automatic-state
                                       (list
                                        manual
                                        ac-prefix
                                        ac-point
                                        (length
                                         ac-current-sources))))))
                              (ac-abort))))"##,
        expect![[r#"OK ((nil nil nil nil) (t "end" 1 1))"#]],
    )
}

fn auto_complete_trigger_key_replaces_old_binding_and_restores_fallback_space() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_trigger_key_replaces_old_binding_and_restores_fallback_space",
        r##"(let ((ac-trigger-key nil)
                               (ac-mode-map
                                (make-sparse-keymap)))
                           (ac-set-trigger-key "TAB")
                           (let ((tab
                                  (lookup-key
                                   ac-mode-map
                                   (kbd "TAB"))))
                             (ac-set-trigger-key "C-SPC")
                             (let ((replacement
                                    (list
                                     (lookup-key
                                      ac-mode-map
                                      (kbd "TAB"))
                                     (lookup-key
                                      ac-mode-map
                                      (kbd "C-SPC"))
                                     ac-trigger-key)))
                               (ac-set-trigger-key nil)
                               (list
                                tab
                                replacement
                                (lookup-key
                                 ac-mode-map
                                 (kbd "C-SPC"))
                                ac-trigger-key))))"##,
        expect![[r#"OK (ac-trigger-key-command (nil ac-trigger-key-command "C-SPC") nil nil)"#]],
    )
}

fn auto_complete_after_save_cache_registry_honors_predicates_and_registration_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_after_save_cache_registry_honors_predicates_and_registration_order",
        r##"(let ((ac-clear-variables-after-save
                                nil)
                               (always
                                'always-value)
                               (conditional
                                'conditional-value)
                               (kept
                                'kept-value)
                               (predicate-calls 0))
                           (fset
                            'auto-complete-test-clear-p
                            (lambda ()
                              (setq predicate-calls
                                    (1+ predicate-calls))
                              t))
                           (fset
                            'auto-complete-test-keep-p
                            (lambda ()
                              (setq predicate-calls
                                    (+ predicate-calls 10))
                              nil))
                           (ac-clear-variable-after-save
                            'always)
                           (ac-clear-variable-after-save
                            'conditional
                            'auto-complete-test-clear-p)
                           (ac-clear-variable-after-save
                            'kept
                            'auto-complete-test-keep-p)
                           (let ((registry
                                  ac-clear-variables-after-save))
                             (ac-clear-variables-after-save)
                             (list
                              registry
                              always
                              conditional
                              kept
                              predicate-calls)))"##,
        expect![
            "OK (((kept . auto-complete-test-keep-p) (conditional . auto-complete-test-clear-p) (always)) always-value conditional-value kept-value 11)"
        ],
    )
}

fn auto_complete_periodic_cache_registry_clears_on_exact_minute_multiples() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_periodic_cache_registry_clears_on_exact_minute_multiples",
        r##"(let ((ac-clear-variables-every-minute
                                nil)
                               (ac-minutes-counter 0)
                               (each 'each-0)
                               (every-two 'two-0)
                               (every-three 'three-0)
                               observations)
                           (ac-clear-variable-every-minute
                            'each)
                           (ac-clear-variable-every-minutes
                            'every-two
                            2)
                           (ac-clear-variable-every-minutes
                            'every-three
                            3)
                           (dotimes (index 6)
                             (setq
                              each
                              (intern
                               (format
                                "each-%d"
                                (1+ index)))
                              every-two
                              (intern
                               (format
                                "two-%d"
                                (1+ index)))
                              every-three
                              (intern
                               (format
                                "three-%d"
                                (1+ index))))
                             (ac-clear-variables-every-minute)
                             (push
                              (list
                               ac-minutes-counter
                               each
                               every-two
                               every-three)
                              observations))
                           (list
                            ac-clear-variables-every-minute
                            (nreverse observations)))"##,
        expect![
            "OK (((every-three . 3) (every-two . 2) (each . 1)) ((1 each-1 two-1 three-1) (2 each-2 two-2 three-2) (3 each-3 two-3 three-3) (4 each-4 two-4 three-4) (5 each-5 two-5 three-5) (6 each-6 two-6 three-6)))"
        ],
    )
}

fn auto_complete_cleanup_resets_session_objects_and_records_selected_candidate_history()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_cleanup_resets_session_objects_and_records_selected_candidate_history",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (insert "fo")
                            (let ((ac-use-comphist t)
                                  (ac-comphist
                                   (ac-comphist-make))
                                  (ac-use-quick-help nil)
                                  (auto-complete-mode t)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "format"
                                      "forward-char")))))
                              (unwind-protect
                                  (progn
                                    (ac-start
                                     :requires 1
                                     :triggered
                                     'command)
                                    (setq
                                     ac-selected-candidate
                                     "forward-char"
                                     ac-last-point
                                     (+ ac-point 1)
                                     ac-inline
                                     (list nil))
                                    (let ((before
                                           (list
                                            ac-point
                                            ac-prefix
                                            (length
                                             ac-current-sources))))
                                      (ac-cleanup)
                                      (list
                                       before
                                       (mapcar
                                        (lambda (symbol)
                                          (list
                                           symbol
                                           (symbol-value
                                            symbol)))
                                        '(ac-inline
                                          ac-menu
                                          ac-completing
                                          ac-point
                                          ac-prefix
                                          ac-selected-candidate
                                          ac-candidates
                                          ac-current-sources))
                                       (append
                                        (ac-comphist-get
                                         ac-comphist
                                         "forward-char")
                                        nil))))
                                (ac-cleanup)))))"##,
        expect![[
            r#"OK ((1 "fo" 1) ((ac-inline nil) (ac-menu nil) (ac-completing nil) (ac-point nil) (ac-prefix nil) (ac-selected-candidate nil) (ac-candidates nil) (ac-current-sources nil)) (0 1 0 0 0 0 0 0 0 0 0 0))"#
        ]],
    )
}

fn auto_complete_error_reports_original_condition_disables_mode_and_cleans_session()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_error_reports_original_condition_disables_mode_and_cleans_session",
        r##"(with-temp-buffer
                          (let ((ac-use-comphist nil)
                                (ac-stop-flymake-on-completing
                                 nil))
                            (auto-complete-mode 1)
                            (setq
                             ac-prefix "broken"
                             ac-point (point-min)
                             ac-completing t)
                            (let ((message-log-max t)
                                  (result
                                   (ac-error
                                    '(wrong-type-argument
                                      stringp
                                      42))))
                              (list
                               result
                               auto-complete-mode
                               ac-prefix
                               ac-point
                               ac-completing
                               (current-message)))))"##,
        expect!["OK ((wrong-type-argument stringp 42) nil nil nil nil nil)"],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_mode_enable_disable_installs_and_removes_buffer_local_hooks_and_state(),
        auto_complete_trigger_command_classifier_handles_builtin_custom_electric_and_excluded_commands(),
        auto_complete_disabled_faces_block_pre_command_trigger_at_exact_point(),
        auto_complete_pre_and_post_command_hooks_start_update_and_abort_real_completion_session(),
        auto_complete_start_distinguishes_manual_command_from_automatic_stop_words(),
        auto_complete_trigger_key_replaces_old_binding_and_restores_fallback_space(),
        auto_complete_after_save_cache_registry_honors_predicates_and_registration_order(),
        auto_complete_periodic_cache_registry_clears_on_exact_minute_multiples(),
        auto_complete_cleanup_resets_session_objects_and_records_selected_candidate_history(),
        auto_complete_error_reports_original_condition_disables_mode_and_cleans_session(),
    ]
}
