use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_practical_lisp_highlight_navigate_and_rename_workflow_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_practical_lisp_highlight_navigate_and_rename_workflow_matches",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (emacs-lisp-mode)
                             (insert
                              "(defun calculate-total (values)\n  (let ((total 0))\n    (dolist (value values total)\n      (setq total (+ total value)))))")
                             (font-lock-ensure)
                             (search-backward
                              "total")
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight-now)
                             (let ((initial
                                    (list
                                     (point)
                                     (mapcar
                                      (lambda (overlay)
                                        (list
                                         (overlay-start
                                          overlay)
                                         (overlay-end
                                          overlay)
                                         (overlay-get
                                          overlay
                                          'face)))
                                      (append
                                       ahs-current-overlay
                                       ahs-overlay-list)))))
                               (ahs-forward)
                               (ahs-edit-mode t)
                               (goto-char
                                (overlay-end
                                 (ahs-current-overlay-window)))
                               (insert "-sum")
                               (ahs-edit-post-command-hook-function)
                               (ahs-edit-mode nil)
                               (list
                                initial
                                (buffer-string)
                                (point)
                                ahs-edit-mode-enable
                                (auto-highlight-symbol-test-overlays)))))"##,
        expect![[
            r#"OK ((106 ((106 111 ahs-plugin-whole-buffer-face) (106 111 ahs-face) (97 102 ahs-face) (78 83 ahs-face) (42 47 ahs-face))) #("(defun calculate-total (values)\n  (let ((total-sum 0))\n    (dolist (value values total-sum)\n      (setq total-sum (+ total-sum value)))))" 1 6 (face font-lock-keyword-face) 7 22 (face font-lock-function-name-face) 35 38 (face font-lock-keyword-face) 41 46 (fontified t) 50 60 (fontified t) 60 66 (fontified t face font-lock-keyword-face) 66 81 (fontified t) 90 99 (fontified t) 99 103 (fontified t face font-lock-keyword-face) 103 104 (fontified t) 113 117 (fontified t)) 51 nil nil)"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_practical_face_rules_highlight_code_but_ignore_comments_and_strings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_practical_face_rules_highlight_code_but_ignore_comments_and_strings",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (emacs-lisp-mode)
                             (insert
                              "(let ((token 1))\n  ;; token in comment\n  (message \"token in string\")\n  (+ token token))")
                             (font-lock-ensure)
                             (goto-char 9)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight-now)
                             (list
                              (buffer-string)
                              (auto-highlight-symbol-test-overlays)
                              (mapcar
                               (lambda (needle)
                                 (save-excursion
                                   (goto-char
                                    (point-min))
                                   (search-forward
                                    needle)
                                   (list
                                    needle
                                    (get-text-property
                                     (match-beginning 0)
                                     'face)
                                    (length
                                     (seq-filter
                                      (lambda (overlay)
                                        (overlay-get
                                         overlay
                                         'ahs-symbol))
                                      (overlays-at
                                       (match-beginning 0)))))))
                               '("token in comment"
                                 "token in string")))))"##,
        expect![[
            r#"OK (#("(let ((token 1))\n  ;; token in comment\n  (message \"token in string\")\n  (+ token token))" 1 4 (face font-lock-keyword-face) 7 12 (fontified t) 19 22 (face font-lock-comment-delimiter-face) 22 39 (face font-lock-comment-face) 50 67 (face font-lock-string-face) 74 85 (fontified t)) ((8 13 current ahs-plugin-whole-buffer-face 1000 t t) (8 13 others ahs-face nil t t) (75 80 others ahs-face nil t t) (81 86 others ahs-face nil t t)) (("token in comment" font-lock-comment-face 0) ("token in string" font-lock-string-face 0)))"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_two_buffers_keep_ranges_overlays_edits_and_mode_state_independent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_two_buffers_keep_ranges_overlays_edits_and_mode_state_independent",
        r##"(save-window-excursion
                           (let ((first
                                  (generate-new-buffer
                                   " *ahs-workflow-first*"))
                                 (second
                                  (generate-new-buffer
                                   " *ahs-workflow-second*")))
                             (unwind-protect
                                 (progn
                                   (switch-to-buffer first)
                                   (insert
                                    "alpha alpha")
                                   (goto-char 2)
                                   (auto-highlight-symbol-mode 1)
                                   (setq
                                    ahs-current-range
                                    ahs-range-whole-buffer
                                    ahs-selected-window
                                    (selected-window))
                                   (ahs-highlight-now)
                                   (switch-to-buffer second)
                                   (insert
                                    "beta beta beta")
                                   (goto-char 2)
                                   (auto-highlight-symbol-mode 1)
                                   (setq
                                    ahs-current-range
                                    ahs-range-display
                                    ahs-selected-window
                                    (selected-window))
                                   (ahs-highlight-now)
                                   (let ((before
                                          (mapcar
                                           (lambda (buffer)
                                             (with-current-buffer buffer
                                               (list
                                                (buffer-string)
                                                ahs-current-range
                                                (length
                                                 ahs-current-overlay)
                                                (length
                                                 ahs-overlay-list))))
                                           (list first second))))
                                     (switch-to-buffer first)
                                     (ahs-edit-mode t)
                                     (goto-char
                                      (overlay-end
                                       (ahs-current-overlay-window)))
                                     (insert "-edited")
                                     (ahs-edit-post-command-hook-function)
                                     (switch-to-buffer second)
                                     (let ((after-edit
                                            (mapcar
                                             (lambda (buffer)
                                               (with-current-buffer buffer
                                                 (list
                                                  (buffer-string)
                                                  ahs-edit-mode-enable
                                                  (length
                                                   ahs-current-overlay)
                                                  (length
                                                   ahs-overlay-list))))
                                             (list first second))))
                                     (with-current-buffer first
                                       (auto-highlight-symbol-mode
                                        -1))
                                     (list
                                      before
                                      after-edit
                                      (with-current-buffer first
                                        (auto-highlight-symbol-test-mode-state))
                                      (with-current-buffer second
                                        (auto-highlight-symbol-test-mode-state))))))
                               (kill-buffer first)
                               (kill-buffer second))))"##,
        expect![[
            r#"OK (((#("alpha alpha" 0 11 (fontified t)) #1=((name . "whole buffer") (lighter . "HSA") (face . ahs-plugin-whole-buffer-face) (start . point-min) (end . point-max)) 1 2) (#("beta beta beta" 0 14 (fontified t)) #2=((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) 1 3)) ((#("alpha-edited alpha-edited" 0 5 (fontified t) 12 13 (fontified t)) t 0 0) (#("beta beta beta" 0 14 (fontified t)) nil 1 3)) (nil #1# " HSA" nil (ahs-start-timer t) (ahs-start-timer t) 0 0) (t #2# " HS" nil (ahs-start-timer t) (ahs-start-timer t) 1 3))"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_definition_plugin_limits_real_highlighting_to_current_function()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_definition_plugin_limits_real_highlighting_to_current_function",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (emacs-lisp-mode)
                             (insert
                              "(defun first (item)\n  (+ item item))\n\n(defun second (item)\n  (* item item))")
                             (font-lock-ensure)
                             (goto-char
                              (point-min))
                             (search-forward
                              "item")
                             (backward-char)
                             (auto-highlight-symbol-mode 1)
                             (ahs-change-range
                              'ahs-range-beginning-of-defun
                              t)
                             (setq
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight-now)
                             (let ((first-function
                                    (auto-highlight-symbol-test-overlays)))
                               (search-forward
                                "second")
                               (search-forward
                                "item")
                               (ahs-highlight-now)
                               (list
                                first-function
                                (point)
                                (auto-highlight-symbol-test-overlays)
                                ahs-plugin-bod-start
                                ahs-plugin-bod-end))))"##,
        expect![
            "OK (((15 19 current ahs-plugin-bod-face 1000 t t) (15 19 others ahs-face nil t t) (26 30 others ahs-face nil t t) (31 35 others ahs-face nil t t)) 58 ((54 58 current ahs-plugin-bod-face 1000 t t) (54 58 others ahs-face nil t t) (65 69 others ahs-face nil t t) (70 74 others ahs-face nil t t)) 39 76)"
        ],
    )
    .fresh_process()
}

fn auto_highlight_symbol_case_insensitive_highlight_and_navigation_preserve_case_variants()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_case_insensitive_highlight_and_navigation_preserve_case_variants",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "Value value VALUE")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-case-fold-search
                              t
                             ahs-selected-window
                              (selected-window))
                             (ahs-highlight-now)
                             (let ((initial
                                    (auto-highlight-symbol-test-overlays))
                                   positions)
                               (dotimes (_ 4)
                                 (push
                                  (list
                                   (point)
                                   (buffer-substring-no-properties
                                    (overlay-start
                                     (ahs-current-overlay-window))
                                    (overlay-end
                                     (ahs-current-overlay-window))))
                                  positions)
                                 (ahs-forward))
                               (list
                                (buffer-string)
                                initial
                                (nreverse positions)))))"##,
        expect![[
            r#"OK (#("Value value VALUE" 0 17 (fontified t)) ((1 6 current ahs-plugin-whole-buffer-face 1000 t t) (1 6 others ahs-face nil t t) (7 12 others ahs-face nil t t) (13 18 others ahs-face nil t t)) ((2 "Value") (8 "value") (14 "VALUE") (2 "Value")))"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_window_switch_rehighlight_workflow_uses_each_window_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_window_switch_rehighlight_workflow_uses_each_window_point",
        r##"(save-window-excursion
                           (let ((buffer
                                  (generate-new-buffer
                                   " *ahs-window-workflow*")))
                             (unwind-protect
                                 (progn
                                   (switch-to-buffer buffer)
                                   (insert
                                    "alpha alpha\nbeta beta")
                                   (let ((first
                                          (selected-window))
                                         (second
                                          (split-window-right)))
                                     (set-window-buffer
                                      second
                                      buffer)
                                     (select-window first)
                                     (goto-char 2)
                                     (auto-highlight-symbol-mode 1)
                                     (setq
                                      ahs-current-range
                                      ahs-range-whole-buffer
                                      ahs-highlight-all-windows
                                      nil)
                                     (ahs-idle-function)
                                     (select-window second)
                                     (goto-char 15)
                                     (ahs-idle-function)
                                     (list
                                      (mapcar
                                       (lambda (window)
                                         (ht-get
                                          ahs-window-map
                                          window))
                                       (list first second))
                                      (auto-highlight-symbol-test-overlays)
                                      (length
                                       ahs-current-overlay)
                                      (length
                                       ahs-overlay-list))))
                               (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((("alpha" 1 6) ("beta" 13 17)) ((1 6 current ahs-plugin-whole-buffer-face 1000 t nil) (1 6 others ahs-face nil t nil) (7 12 others ahs-face nil t nil) (13 17 current ahs-plugin-whole-buffer-face 1000 t t) (13 17 others ahs-face nil t t) (18 22 others ahs-face nil t t)) 2 4)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_practical_lisp_highlight_navigate_and_rename_workflow_matches(),
        auto_highlight_symbol_practical_face_rules_highlight_code_but_ignore_comments_and_strings(),
        auto_highlight_symbol_two_buffers_keep_ranges_overlays_edits_and_mode_state_independent(),
        auto_highlight_symbol_definition_plugin_limits_real_highlighting_to_current_function(),
        auto_highlight_symbol_case_insensitive_highlight_and_navigation_preserve_case_variants(),
        auto_highlight_symbol_window_switch_rehighlight_workflow_uses_each_window_point(),
    ]
}
