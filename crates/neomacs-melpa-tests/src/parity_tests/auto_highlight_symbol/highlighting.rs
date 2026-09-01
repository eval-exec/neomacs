use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_whole_buffer_highlight_builds_current_other_and_definition_overlays()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_whole_buffer_highlight_builds_current_other_and_definition_overlays",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (emacs-lisp-mode)
                             (insert
                              "(defun alpha (alpha)\n  (let ((alpha 1))\n    (+ alpha alpha)))")
                             (font-lock-ensure)
                             (goto-char 8)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (list
                              (ahs-highlight-p)
                              (ahs-highlight
                               "alpha"
                               8
                               13)
                              (auto-highlight-symbol-test-overlays)
                              ahs-start-point
                              (length ahs-current-overlay)
                              (length ahs-overlay-list))))"##,
        expect![[
            r#"OK ((#("alpha" 0 5 (face font-lock-function-name-face)) 8 13) t ((8 13 current ahs-plugin-whole-buffer-face 1000 t t) (8 13 others ahs-definition-face nil t t) (15 20 others ahs-face nil t t) (31 36 others ahs-face nil t t) (48 53 others ahs-face nil t t) (54 59 others ahs-face nil t t)) 8 1 5)"#
        ]],
    )
}

fn auto_highlight_symbol_unfocused_window_uses_unfocused_faces_but_same_match_ranges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_unfocused_window_uses_unfocused_faces_but_same_match_ranges",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha beta alpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              nil)
                             (ahs-highlight
                              "alpha"
                              1
                              6)
                             (auto-highlight-symbol-test-overlays)))"##,
        expect![
            "OK ((1 6 current ahs-plugin-whole-buffer-face 1000 t t) (1 6 others ahs-face-unfocused nil t t) (12 17 others ahs-face-unfocused nil t t))"
        ],
    )
}

fn auto_highlight_symbol_search_and_light_up_preserve_definition_face_classification()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_search_and_light_up_preserve_definition_face_classification",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              (propertize
                               "value"
                               'face
                               'font-lock-variable-name-face)
                              " value "
                              (propertize
                               "value"
                               'face
                               'font-lock-comment-face)
                              " value")
                             (setq
                              ahs-search-work nil
                              ahs-need-fontify nil
                              ahs-inhibit-face-list
                              '(font-lock-comment-face)
                              ahs-definition-face-list
                              '(font-lock-variable-name-face)
                              ahs-selected-window
                              (selected-window))
                             (ahs-search-symbol
                              "value"
                              (cons
                               (point-min)
                               (point-max)))
                             (let ((work
                                    (mapcar
                                     (lambda (match)
                                       (list
                                        (nth 0 match)
                                        (nth 1 match)
                                        (nth 2 match)
                                        (nth 3 match)))
                                     ahs-search-work)))
                               (ahs-light-up t)
                               (list
                                work
                                (auto-highlight-symbol-test-overlays)))))"##,
        expect![
            "OK (((1 6 font-lock-variable-name-face nil) (7 12 nil nil) (13 18 font-lock-comment-face nil) (19 24 nil nil)) ((1 6 others ahs-definition-face nil t t) (7 12 others ahs-face nil t t) (19 24 others ahs-face nil t t)))"
        ],
    )
}

fn auto_highlight_symbol_unhighlight_keeps_same_symbol_for_allowed_or_matching_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_unhighlight_keeps_same_symbol_for_allowed_or_matching_commands",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha beta")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              1
                              6)
                             (ht-set
                              ahs-window-map
                              (selected-window)
                              (ahs-highlight-p))
                             (mapcar
                              (lambda (case)
                                (setq
                                 this-command
                                 (car case))
                                (goto-char
                                 (cdr case))
                                (let ((before
                                       (length
                                        (auto-highlight-symbol-test-overlays))))
                                  (ahs-unhighlight)
                                  (list
                                   case
                                   before
                                   (length
                                    (auto-highlight-symbol-test-overlays)))))
                              '((ahs-forward . 2)
                                (fixture-command . 2)
                                (fixture-command . 14)))))"##,
        expect![
            "OK (((ahs-forward . 2) 3 3) ((fixture-command . 2) 3 3) ((fixture-command . 14) 3 0))"
        ],
    )
}

fn auto_highlight_symbol_remove_overlay_force_and_window_scope_preserve_other_window_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_remove_overlay_force_and_window_scope_preserve_other_window_entries",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert "alpha alpha")
                             (let* ((current-window
                                     (selected-window))
                                    (foreign-window
                                     (split-window-right))
                                    (current-overlay
                                     (make-overlay 1 6))
                                    (foreign-overlay
                                     (make-overlay 7 12))
                                    (overlays
                                     (list
                                      current-overlay
                                      foreign-overlay)))
                               (overlay-put
                                current-overlay
                                'window
                                current-window)
                               (overlay-put
                                foreign-overlay
                                'window
                                foreign-window)
                               (list
                                (mapcar
                                 #'overlay-start
                                 (ahs-delete-overlays
                                  overlays))
                                (overlay-start
                                 current-overlay)
                                (overlay-start
                                 foreign-overlay)
                                (mapcar
                                 #'overlay-start
                                 (ahs-delete-overlays
                                  overlays
                                  t))
                                (overlay-start
                                 foreign-overlay)))))"##,
        expect!["OK ((7) nil 7 nil nil)"],
    )
}

fn auto_highlight_symbol_statistics_count_before_after_displayed_and_hidden_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_statistics_count_before_after_displayed_and_hidden_matches",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha\nalpha\nalpha\nalpha\nalpha")
                             (goto-char 14)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              13
                              18)
                             (let ((hidden
                                    (make-overlay
                                     25
                                     30)))
                               (overlay-put
                                hidden
                                'invisible
                                t)
                               (list
                                (ahs-stat)
                                (ahs-stat-string)
                                (ahs-stat-alert-p
                                 (ahs-stat))))))"##,
        expect![[
            r#"OK ((#("whole buffer" 0 12 (face ahs-plugin-whole-buffer-face)) 5 2 2 4 1) #("Current plugin `whole buffer' matched 5  displayed 4  hidden 1  before 2  after 2." 16 28 (face ahs-plugin-whole-buffer-face)) nil)"#
        ]],
    )
}

fn auto_highlight_symbol_fontify_coalesces_unfontified_search_regions_at_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_fontify_coalesces_unfontified_search_regions_at_boundaries",
        r##"(let ((ahs-search-work
                                '((1 3 nil nil)
                                  (5 7 nil nil)
                                  (9 11 font-lock-keyword-face t)
                                  (13 15 nil nil)
                                  (17 19 nil nil)))
                               calls)
                           (cl-letf
                               (((symbol-function
                                  'jit-lock-fontify-now)
                                 (lambda (beg end)
                                   (push
                                    (list beg end)
                                    calls))))
                             (ahs-fontify)
                             (nreverse calls)))"##,
        expect!["OK ((1 7) (13 19))"],
    )
}

fn auto_highlight_symbol_current_overlay_has_exact_edit_hooks_priority_help_and_evaporation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_current_overlay_has_exact_edit_hooks_priority_help_and_evaporation",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert "alpha")
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window)
                              ahs-overlay-priority
                              4321)
                             (ahs-highlight-current-symbol
                              t
                              1
                              6)
                             (let ((overlay
                                    (car
                                     ahs-current-overlay)))
                               (mapcar
                                (lambda (property)
                                  (list
                                   property
                                   (overlay-get
                                    overlay
                                    property)))
                                '(ahs-symbol
                                  face
                                  priority
                                  evaporate
                                  help-echo
                                  modification-hooks
                                  insert-in-front-hooks
                                  insert-behind-hooks)))))"##,
        expect![[
            r#"OK ((ahs-symbol current) (face ahs-plugin-whole-buffer-face) (priority 4321) (evaporate t) (help-echo (or (ignore-errors (ahs-stat-string)) "")) (modification-hooks (ahs-modification-hook)) (insert-in-front-hooks (ahs-modification-hook)) (insert-behind-hooks (ahs-modification-hook)))"#
        ]],
    )
}

fn auto_highlight_symbol_two_windows_keep_separate_current_overlays_and_window_map_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_two_windows_keep_separate_current_overlays_and_window_map_entries",
        r##"(save-window-excursion
                           (let ((buffer
                                  (generate-new-buffer
                                   " *ahs-two-window*")))
                             (unwind-protect
                                 (progn
                                   (switch-to-buffer buffer)
                                   (insert
                                    "alpha alpha")
                                   (goto-char 2)
                                   (auto-highlight-symbol-mode 1)
                                   (setq
                                    ahs-current-range
                                    ahs-range-whole-buffer)
                                   (let ((first
                                          (selected-window))
                                         (second
                                          (split-window-right)))
                                     (set-window-buffer
                                      second
                                      buffer)
                                     (select-window first)
                                     (ahs--do-hl)
                                     (select-window second)
                                     (goto-char 8)
                                     (ahs--do-hl)
                                     (list
                                      (length
                                       ahs-current-overlay)
                                      (length
                                       ahs-overlay-list)
                                      (mapcar
                                       (lambda (window)
                                         (ht-get
                                          ahs-window-map
                                          window))
                                       (list first second))
                                      (auto-highlight-symbol-test-overlays))))
                               (kill-buffer buffer))))"##,
        expect![[
            r#"OK (2 4 (("alpha" 1 6) (#("alpha" 0 5 (fontified t)) 7 12)) ((1 6 current ahs-plugin-whole-buffer-face 1000 t nil) (1 6 others ahs-face-unfocused nil t t) (1 6 others ahs-face-unfocused nil t nil) (7 12 current ahs-plugin-whole-buffer-face 1000 t t) (7 12 others ahs-face-unfocused nil t t) (7 12 others ahs-face-unfocused nil t nil)))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn highlighting_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_whole_buffer_highlight_builds_current_other_and_definition_overlays(),
        auto_highlight_symbol_unfocused_window_uses_unfocused_faces_but_same_match_ranges(),
        auto_highlight_symbol_search_and_light_up_preserve_definition_face_classification(),
        auto_highlight_symbol_unhighlight_keeps_same_symbol_for_allowed_or_matching_commands(),
        auto_highlight_symbol_remove_overlay_force_and_window_scope_preserve_other_window_entries(),
        auto_highlight_symbol_statistics_count_before_after_displayed_and_hidden_matches(),
        auto_highlight_symbol_fontify_coalesces_unfontified_search_regions_at_boundaries(),
        auto_highlight_symbol_current_overlay_has_exact_edit_hooks_priority_help_and_evaporation(),
        auto_highlight_symbol_two_windows_keep_separate_current_overlays_and_window_map_entries(),
    ]
}
