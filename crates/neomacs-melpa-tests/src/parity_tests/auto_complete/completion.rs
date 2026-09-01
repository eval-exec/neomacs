use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_real_menu_filters_candidates_and_completes_selected_item() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_real_menu_filters_candidates_and_completes_selected_item",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-expand-on-auto-complete nil)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "format"
                                      "forward-char"
                                      "function"
                                      "message")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "fo")
                                    (auto-complete)
                                    (let ((initial
                                           (list
                                            (buffer-string)
                                            ac-prefix
                                            (mapcar
                                             #'substring-no-properties
                                             ac-candidates)
                                            (popup-live-p ac-menu)
                                            (substring-no-properties
                                             (ac-selected-candidate)))))
                                      (ac-next)
                                      (let ((selected
                                             (substring-no-properties
                                              (ac-selected-candidate))))
                                        (ac-complete)
                                        (list
                                         initial
                                         selected
                                         (buffer-string)
                                         ac-completing
                                         ac-menu
                                         ac-prefix))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK (("fo\n\n\n\n\n\n\n\n\n\n\n" "fo" ("format" "forward-char") t "format") "forward-char" "forward-char" nil nil nil)"#
        ]],
    )
}

fn auto_complete_single_candidate_expands_immediately_without_leaving_menu_or_session()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_single_candidate_expands_immediately_without_leaving_menu_or_session",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "production"
                                      "preview"
                                      "publish")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "prod")
                                    (let ((started
                                           (auto-complete)))
                                      (list
                                       started
                                       (buffer-string)
                                       ac-menu
                                       ac-completing
                                       ac-prefix
                                       ac-last-completion)))
                                (auto-complete-mode -1)))))"##,
        expect![[r#"OK (t "production" nil nil nil ((:marker nil nil) . "production"))"#]],
    )
}

fn auto_complete_common_part_expands_then_stop_keeps_edit_and_removes_transient_popup_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_common_part_expands_then_stop_keeps_edit_and_removes_transient_popup_state",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "deploy-prod"
                                      "deploy-preview"
                                      "delete")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "de")
                                    (auto-complete)
                                    (let ((expanded
                                           (list
                                            (buffer-substring-no-properties
                                             (point-min)
                                             (line-end-position))
                                            ac-prefix
                                            ac-common-part
                                            ac-whole-common-part
                                            (mapcar
                                             #'substring-no-properties
                                             ac-candidates))))
                                      (ac-stop)
                                      (list
                                       expanded
                                       (buffer-string)
                                       ac-menu
                                       ac-completing
                                       ac-prefix)))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK (("de" "de" "de" "de" ("deploy-prod" "deploy-preview" "delete")) "de" nil nil nil)"#
        ]],
    )
}

fn auto_complete_incremental_update_narrows_candidates_then_no_match_aborts_cleanly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_incremental_update_narrows_candidates_then_no_match_aborts_cleanly",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "format"
                                      "forward"
                                      "foreach")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "fo")
                                    (auto-complete)
                                    (insert "r")
                                    (setq ac-prefix
                                          (buffer-substring-no-properties
                                           ac-point
                                           (point)))
                                    (ac-update t)
                                    (let ((narrowed
                                           (list
                                            ac-prefix
                                            (mapcar
                                             #'substring-no-properties
                                             ac-candidates)
                                            ac-completing)))
                                      (insert "z")
                                      (setq ac-prefix
                                            (buffer-substring-no-properties
                                             ac-point
                                             (point)))
                                      (ac-update t)
                                      (let ((no-match
                                             (list
                                              (buffer-substring-no-properties
                                               (point-min)
                                               (line-end-position))
                                              ac-prefix
                                              ac-candidates
                                              ac-completing
                                              (popup-live-p
                                               ac-menu))))
                                        (ac-abort)
                                        (list
                                         narrowed
                                         no-match
                                         (list
                                          ac-prefix
                                          ac-candidates
                                          ac-completing
                                          ac-menu)))))
                                (auto-complete-mode -1)))))"##,
        expect![[r#"OK (("forr" nil nil) ("forrz" "forrz" nil nil t) (nil nil nil nil))"#]],
    )
}

fn auto_complete_ret_executes_candidate_action_after_inserting_associated_display_name()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_ret_executes_candidate_action_after_inserting_associated_display_name",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (auto-complete-test-actions
                                   nil)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      (cons
                                       "deploy-production"
                                       'production-id)
                                      (cons
                                       "deploy-preview"
                                       'preview-id))
                                     (action
                                      . (lambda ()
                                          (push
                                           (list
                                            (buffer-string)
                                            ac-selected-candidate)
                                           auto-complete-test-actions)))))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "deploy-")
                                    (auto-complete)
                                    (let ((candidate-data
                                           (mapcar
                                            (lambda (candidate)
                                              (list
                                               (substring-no-properties
                                                candidate)
                                               (popup-item-value
                                                candidate)
                                               (popup-item-property
                                                candidate
                                                'action)))
                                            ac-candidates)))
                                      (ac-next)
                                      (let ((completed
                                             (ac-complete)))
                                        (list
                                         candidate-data
                                         (substring-no-properties
                                          completed)
                                         (buffer-string)
                                         auto-complete-test-actions
                                         ac-menu))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK ((("deploy-production" production-id #1=(lambda nil (push (list (buffer-string) ac-selected-candidate) auto-complete-test-actions))) ("deploy-preview" preview-id #1#)) "deploy-preview" "deploy-preview" (("deploy-preview" nil)) nil)"#
        ]],
    )
}

fn auto_complete_expansion_replaces_matching_right_hand_symbol_tail_but_preserves_unrelated_suffix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_expansion_replaces_matching_right_hand_symbol_tail_but_preserves_unrelated_suffix",
        r##"(mapcar
                          (lambda (case)
                            (with-temp-buffer
                              (insert (car case))
                              (goto-char (cadr case))
                              (setq ac-point
                                    (caddr case)
                                    ac-prefix
                                    (buffer-substring-no-properties
                                     ac-point
                                     (point))
                                    ac-selected-candidate
                                    nil)
                              (let ((boundary
                                     (ac-extend-region-to-delete
                                      (cadddr case))))
                                (ac-expand-string
                                 (cadddr case))
                                (list
                                 case
                                 boundary
                                 (buffer-string)
                                 (point)
                                 ac-prefix
                                 ac-selected-candidate))))
                          '(("forard-char"
                             4
                             1
                             "forward-char")
                            ("for-other"
                             4
                             1
                             "forward-char")
                            ("prefix prod suffix"
                             12
                             8
                             "production")
                            ("λambd"
                             4
                             1
                             "λambda")))"##,
        expect![[
            r#"OK ((("forard-char" 4 1 "forward-char") 12 "forward-char" 13 "forward-char" "forward-char") (("for-other" 4 1 "forward-char") 4 "forward-char-other" 13 "forward-char" "forward-char") (("prefix prod suffix" 12 8 "production") 12 "prefix production suffix" 18 "production" "production") (("λambd" 4 1 "λambda") 4 "λambdabd" 7 "λambda" "λambda"))"#
        ]],
    )
}

fn auto_complete_expansion_is_one_undoable_edit_and_repeated_expansion_removes_internal_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_expansion_is_one_undoable_edit_and_repeated_expansion_removes_internal_boundaries",
        r##"(with-temp-buffer
                          (buffer-enable-undo)
                          (insert "fo")
                          (setq
                           buffer-undo-list nil
                           ac-point (point-min)
                           ac-prefix "fo"
                           ac-selected-candidate nil)
                          (ac-expand-string
                           "format")
                          (let ((after-first
                                 (list
                                  (buffer-string)
                                  ac-prefix
                                  (copy-tree
                                   buffer-undo-list))))
                            (ac-expand-string
                             "forward-char"
                             t)
                            (let ((after-second
                                   (list
                                    (buffer-string)
                                    ac-prefix
                                    (copy-tree
                                     buffer-undo-list))))
                              (undo)
                              (list
                               after-first
                               after-second
                               (buffer-string)
                               (copy-tree
                                buffer-undo-list)))))"##,
        expect![[
            r#"OK (("format" "format" (nil (1 . 7) ("fo" . -1) 3)) ("forward-char" "forward-char" (nil (1 . 13) ("fo" . -1) 3)) "fo" ((1 . 3) ("forward-char" . 1) nil (1 . 13) ("fo" . -1) 3))"#
        ]],
    )
}

fn auto_complete_menu_navigation_wraps_pages_and_completes_the_visible_selection() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_menu_navigation_wraps_pages_and_completes_the_visible_selection",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-menu-height 3)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "item-1"
                                      "item-2"
                                      "item-3"
                                      "item-4"
                                      "item-5"
                                      "item-6")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "item-")
                                    (auto-complete)
                                    (let ((states
                                           (list
                                            (list
                                             (substring-no-properties
                                              (ac-selected-candidate))
                                             (popup-cursor
                                              ac-menu)
                                             (popup-scroll-top
                                              ac-menu)))))
                                      (ac-next-page)
                                      (push
                                       (list
                                        (substring-no-properties
                                         (ac-selected-candidate))
                                        (popup-cursor
                                         ac-menu)
                                        (popup-scroll-top
                                         ac-menu))
                                       states)
                                      (ac-previous)
                                      (push
                                       (list
                                        (substring-no-properties
                                         (ac-selected-candidate))
                                        (popup-cursor
                                         ac-menu)
                                        (popup-scroll-top
                                         ac-menu))
                                       states)
                                      (ac-previous-page)
                                      (push
                                       (list
                                        (substring-no-properties
                                         (ac-selected-candidate))
                                        (popup-cursor
                                         ac-menu)
                                        (popup-scroll-top
                                         ac-menu))
                                       states)
                                      (ac-complete)
                                      (list
                                       (nreverse states)
                                       (buffer-string)
                                       ac-menu)))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK ((("item-1" 0 0) ("item-3" 2 0) ("item-2" 1 0) ("item-6" 5 3)) "item-6" nil)"#
        ]],
    )
}

fn auto_complete_numbered_selection_command_chooses_requested_candidate_and_finishes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_numbered_selection_command_chooses_requested_candidate_and_finishes",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      "choice-a"
                                      "choice-b"
                                      "choice-c"
                                      "choice-d")))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "choice-")
                                    (auto-complete)
                                    (let ((before
                                           (mapcar
                                            #'substring-no-properties
                                            ac-candidates)))
                                      (ac-complete-select-3)
                                      (list
                                       before
                                       (buffer-string)
                                       ac-menu
                                       ac-completing)))
                                (auto-complete-mode -1)))))"##,
        expect![[r#"OK (("choice-a" "choice-b" "choice-c" "choice-d") "choice-c" nil nil)"#]],
    )
}

fn auto_complete_inline_overlay_shows_common_suffix_without_mutating_buffer_and_hides_cleanly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_inline_overlay_shows_common_suffix_without_mutating_buffer_and_hides_cleanly",
        r##"(with-temp-buffer
                          (insert "format following")
                          (goto-char 4)
                          (setq
                           ac-inline nil
                           ac-completing t
                           ac-prefix "for"
                           ac-common-part "forward")
                          (ac-inline-update)
                          (let* ((overlay
                                  (ac-inline-overlay))
                                 (shown
                                  (list
                                   (buffer-string)
                                   (overlay-start
                                    overlay)
                                   (overlay-end
                                    overlay)
                                   (overlay-get
                                    overlay
                                    'string)
                                   (overlay-get
                                    overlay
                                    'display)
                                   (overlay-get
                                    overlay
                                    'after-string)
                                   (overlay-get
                                    overlay
                                    'invisible))))
                            (ac-inline-hide)
                            (let ((hidden
                                   (list
                                    (overlay-start
                                     overlay)
                                    (overlay-end
                                     overlay)
                                    (overlay-get
                                     overlay
                                     'display)
                                    (overlay-get
                                     overlay
                                     'after-string)
                                    (overlay-get
                                     overlay
                                     'invisible))))
                              (ac-inline-delete)
                              (list
                               shown
                               hidden
                               (buffer-string)
                               ac-inline
                               (overlay-buffer
                                overlay)))))"##,
        expect![[
            r#"OK (("format following" 4 8 "ward" #("w" 0 1 (face ac-completion-face)) #("ard" 0 3 (face ac-completion-face)) nil) (1 1 nil nil t) "format following" nil nil)"#
        ]],
    )
}

fn auto_complete_candidate_documentation_flows_through_real_popup_menu_and_last_completion()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_candidate_documentation_flows_through_real_popup_menu_and_last_completion",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-sources
                                   '(((candidates
                                      list
                                      (cons
                                       "render-page"
                                       'render-payload)
                                      (cons
                                       "render-preview"
                                       'preview-payload))
                                     (document
                                      . (lambda (value)
                                          (format
                                           "Documentation for %S"
                                           value)))))))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "render-")
                                    (auto-complete)
                                    (let ((menu-doc
                                           (popup-menu-documentation
                                            ac-menu))
                                          (selected
                                           (ac-selected-candidate)))
                                      (ac-complete)
                                      (list
                                       menu-doc
                                       (popup-item-documentation
                                        selected)
                                       (marker-position
                                        (car
                                         ac-last-completion))
                                       (marker-buffer
                                        (car
                                         ac-last-completion))
                                       (substring-no-properties
                                        (cdr
                                         ac-last-completion))
                                       (buffer-string))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK ("Documentation for render-payload" "Documentation for render-payload" 1 (:buffer nil) "render-page" "render-page")"#
        ]],
    )
}

fn auto_complete_source_initialization_runs_once_per_prefix_start_and_restarts_after_point_changes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_source_initialization_runs_once_per_prefix_start_and_restarts_after_point_changes",
        r##"(with-temp-buffer
                          (insert "al")
                          (setq
                           auto-complete-test-init-calls
                           0)
                          (let ((auto-complete-mode t)
                                (ac-use-comphist nil)
                                (ac-use-quick-help nil)
                                (ac-sources
                                 '(((init
                                    . (lambda ()
                                        (setq
                                         auto-complete-test-init-calls
                                         (1+
                                          auto-complete-test-init-calls))))
                                   (candidates
                                    list
                                    "alpha"
                                    "alpine")))))
                            (unwind-protect
                                (progn
                                  (ac-start
                                   :requires 1
                                   :triggered
                                   'command)
                                  (let ((first
                                         auto-complete-test-init-calls))
                                    (ac-start
                                     :requires 1
                                     :triggered
                                     'command)
                                    (let ((same-point
                                           auto-complete-test-init-calls))
                                      (ac-start
                                       :requires 1
                                       :force-init t
                                       :triggered
                                       'command)
                                      (let ((forced
                                             auto-complete-test-init-calls))
                                        (insert "p")
                                        (ac-start
                                         :requires 1
                                         :triggered
                                         'command)
                                        (list
                                         first
                                         same-point
                                         forced
                                         auto-complete-test-init-calls
                                         ac-prefix)))))
                              (ac-abort))))"##,
        expect![[r#"OK (1 1 2 2 "alp")"#]],
    )
}

fn auto_complete_prefix_overlay_adds_temporary_end_newlines_and_cleanup_restores_exact_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_prefix_overlay_adds_temporary_end_newlines_and_cleanup_restores_exact_buffer",
        r##"(with-temp-buffer
                          (insert "end")
                          (setq
                           ac-point (point-min)
                           ac-prefix-overlay nil)
                          (let ((before
                                 (buffer-string)))
                            (ac-put-prefix-overlay)
                            (let ((during
                                   (list
                                    (buffer-string)
                                    (overlay-start
                                     ac-prefix-overlay)
                                    (overlay-end
                                     ac-prefix-overlay)
                                    (overlay-get
                                     ac-prefix-overlay
                                     'newline)
                                    (keymapp
                                     (overlay-get
                                      ac-prefix-overlay
                                      'keymap)))))
                              (ac-remove-prefix-overlay)
                              (setq ac-prefix-overlay nil)
                              (list
                               before
                               during
                               (buffer-string)
                               (overlays-in
                                (point-min)
                                (point-max))))))"##,
        expect![[r#"OK ("end" ("end\n" 1 5 t t) "end" nil)"#]],
    )
}

pub(super) fn completion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_real_menu_filters_candidates_and_completes_selected_item(),
        auto_complete_single_candidate_expands_immediately_without_leaving_menu_or_session(),
        auto_complete_common_part_expands_then_stop_keeps_edit_and_removes_transient_popup_state(),
        auto_complete_incremental_update_narrows_candidates_then_no_match_aborts_cleanly(),
        auto_complete_ret_executes_candidate_action_after_inserting_associated_display_name(),
        auto_complete_expansion_replaces_matching_right_hand_symbol_tail_but_preserves_unrelated_suffix(),
        auto_complete_expansion_is_one_undoable_edit_and_repeated_expansion_removes_internal_boundaries(),
        auto_complete_menu_navigation_wraps_pages_and_completes_the_visible_selection(),
        auto_complete_numbered_selection_command_chooses_requested_candidate_and_finishes(),
        auto_complete_inline_overlay_shows_common_suffix_without_mutating_buffer_and_hides_cleanly(),
        auto_complete_candidate_documentation_flows_through_real_popup_menu_and_last_completion(),
        auto_complete_source_initialization_runs_once_per_prefix_start_and_restarts_after_point_changes(),
        auto_complete_prefix_overlay_adds_temporary_end_newlines_and_cleanup_restores_exact_buffer(),
    ]
}
