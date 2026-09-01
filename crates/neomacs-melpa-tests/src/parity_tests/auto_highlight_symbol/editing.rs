use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_edit_mode_renames_every_highlighted_occurrence_in_real_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_edit_mode_renames_every_highlighted_occurrence_in_real_code",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (emacs-lisp-mode)
                             (insert
                              "(let ((alpha 1))\n  (+ alpha alpha))")
                             (goto-char 9)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              8
                              13)
                             (ahs-edit-mode t)
                             (let ((before
                                    (list
                                     (buffer-string)
                                     ahs-edit-mode-enable
                                     ahs-mode-line
                                     (auto-highlight-symbol-test-overlays))))
                               (goto-char
                                (overlay-end
                                 (ahs-current-overlay-window)))
                               (insert "-renamed")
                               (ahs-edit-post-command-hook-function)
                               (list
                                before
                                (buffer-string)
                                (point)
                                ahs-edit-mode-enable
                                ahs-mode-line
                                (auto-highlight-symbol-test-overlays)))))"##,
        expect![[
            r#"OK ((#("(let ((alpha 1))\n  (+ alpha alpha))" 7 33 (fontified t)) t " *HSA*" ((8 13 current ahs-edit-mode-face 1000 t t) (8 13 others ahs-face nil t t) (23 28 others ahs-face nil t t) (29 34 others ahs-face nil t t))) #("(let ((alpha-renamed 1))\n  (+ alpha-renamed alpha-renamed))" 7 12 (fontified t) 20 30 (fontified t) 43 44 (fontified t)) 21 t " *HSA*" nil)"#
        ]],
    )
}

fn auto_highlight_symbol_symbol_modification_replaces_shorter_and_longer_targets_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_symbol_modification_replaces_shorter_and_longer_targets_exactly",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "x medium lengthy")
                             (let ((window
                                    (selected-window))
                                   (current
                                    (make-overlay 1 2))
                                   (medium
                                    (make-overlay 3 9))
                                   (lengthy
                                    (make-overlay 10 17)))
                               (dolist
                                   (overlay
                                    (list
                                     current
                                     medium
                                     lengthy))
                                 (overlay-put
                                  overlay
                                  'window
                                  window))
                               (setq
                                ahs-current-overlay
                                (list current)
                                ahs-overlay-list
                                (list medium lengthy))
                               (goto-char 1)
                               (delete-region 1 2)
                               (insert "replacement")
                               (move-overlay
                                current
                                1
                                12)
                               (ahs-symbol-modification)
                               (list
                                (buffer-string)
                                (mapcar
                                 (lambda (overlay)
                                   (list
                                    (overlay-start
                                     overlay)
                                    (overlay-end
                                     overlay)))
                                 (list
                                  current
                                  medium
                                  lengthy))))))"##,
        expect![[r#"OK ("replacement replacement replacement" ((1 12) (13 24) (25 36)))"#]],
    )
}

fn auto_highlight_symbol_modification_hook_tracks_before_after_and_undo_inhibition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_modification_hook_tracks_before_after_and_undo_inhibition",
        r##"(with-temp-buffer
                           (insert "alpha")
                           (let ((overlay
                                  (make-overlay
                                   1
                                   6)))
                             (setq
                              ahs-edit-mode-enable
                              t)
                             (mapcar
                              (lambda (case)
                                (setq
                                 this-command
                                 (car case)
                                 ahs-inhibit-modification
                                 nil
                                 ahs-start-modification
                                 nil)
                                (ahs-modification-hook
                                 overlay
                                 nil
                                 1
                                 6)
                                (let ((before
                                       (list
                                        ahs-inhibit-modification
                                        ahs-start-modification)))
                                  (ahs-modification-hook
                                   overlay
                                   t
                                   1
                                   6
                                   0)
                                  (list
                                   case
                                   before
                                   ahs-inhibit-modification
                                   ahs-start-modification)))
                              '((self-insert-command)
                                (undo)
                                (redo)))))"##,
        expect![
            "OK (((self-insert-command) (nil nil) nil t) ((undo) (#1=(undo . #2=(redo)) nil) #1# t) ((redo) (#2# nil) #2# t))"
        ],
    )
}

fn auto_highlight_symbol_edit_mode_on_off_updates_hooks_faces_lighter_and_user_hooks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_edit_mode_on_off_updates_hooks_faces_lighter_and_user_hooks",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert "alpha alpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window)
                              ahs-edit-mode-on-hook
                              (list
                               (lambda ()
                                 (push
                                  :on
                                  auto-highlight-symbol-test-events)))
                              ahs-edit-mode-off-hook
                              (list
                               (lambda ()
                                 (push
                                  :off
                                  auto-highlight-symbol-test-events))))
                             (ahs-highlight
                              "alpha"
                              1
                              6)
                             (ahs-edit-mode-on)
                             (let ((enabled
                                    (list
                                     ahs-edit-mode-enable
                                     ahs-mode-line
                                     (overlay-get
                                      (ahs-current-overlay-window)
                                      'face)
                                     (memq
                                      'ahs-unhighlight
                                      post-command-hook)
                                     (car buffer-undo-list)
                                     auto-highlight-symbol-test-events)))
                               (ahs-edit-mode-off
                                t
                                t)
                               (list
                                enabled
                                ahs-edit-mode-enable
                                ahs-mode-line
                                (memq
                                 'ahs-unhighlight
                                 post-command-hook)
                                auto-highlight-symbol-test-events
                                (auto-highlight-symbol-test-overlays)))))"##,
        expect![[
            r#"OK ((t " *HSA*" ahs-edit-mode-face nil (apply ahs-clear t) #1=(:on)) nil " HSA" (ahs-unhighlight ahs-start-timer t) (:off . #1#) ((1 6 current ahs-plugin-whole-buffer-face 1000 t t) (1 6 others ahs-face nil t t) (7 12 others ahs-face nil t t)))"#
        ]],
    )
}

fn auto_highlight_symbol_edit_mode_condition_reports_disabled_and_read_only_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_edit_mode_condition_reports_disabled_and_read_only_buffers",
        r##"(let ((ahs-suppress-log nil)
                                messages)
                           (cl-letf
                               (((symbol-function
                                  'message)
                                 (lambda (format-string &rest args)
                                   (push
                                    (apply
                                     #'format
                                     format-string
                                     args)
                                    messages))))
                             (mapcar
                              (lambda (case)
                                (with-temp-buffer
                                  (rename-buffer
                                   (format
                                    "fixture-%s"
                                    case)
                                   t)
                                  (setq
                                   auto-highlight-symbol-mode
                                   (not
                                    (eq case 'disabled))
                                   buffer-read-only
                                   (eq case 'read-only))
                                  (list
                                   case
                                   (ahs-edit-mode-condition-p))))
                              '(disabled
                                read-only
                                enabled))
                             (list
                              (nreverse messages))))"##,
        expect![[
            r#"OK (("`auto-highlight-symbol-mode' is not working at current buffer." "Buffer is read-only: `fixture-read-only'"))"#
        ]],
    )
}

fn auto_highlight_symbol_post_command_exits_edit_mode_when_cursor_leaves_current_overlay()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_post_command_exits_edit_mode_when_cursor_leaves_current_overlay",
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
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              1
                              6)
                             (ahs-edit-mode-on)
                             (goto-char 8)
                             (ahs-edit-post-command-hook-function)
                             (list
                              (point)
                              ahs-edit-mode-enable
                              ahs-current-overlay
                              ahs-overlay-list
                              ahs-start-modification
                              ahs-inhibit-modification)))"##,
        expect!["OK (8 nil nil nil nil nil)"],
    )
}

fn auto_highlight_symbol_onekey_edit_temporarily_switches_whole_buffer_then_restores_range()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_onekey_edit_temporarily_switches_whole_buffer_then_restores_range",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha\nalpha\nalpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-display
                              ahs-highlight-all-windows
                              nil
                              ahs-selected-window
                              (selected-window))
                             (ahs-onekey-edit-function
                              'whole-buffer
                              nil)
                             (let ((during
                                    (list
                                     ahs-edit-mode-enable
                                     ahs-onekey-range-store
                                     ahs-current-range
                                     ahs-mode-line
                                     (length
                                      ahs-overlay-list))))
                               (ahs-edit-mode-off
                                t
                                t)
                               (list
                                during
                                ahs-edit-mode-enable
                                ahs-onekey-range-store
                                ahs-current-range
                                ahs-mode-line))))"##,
        expect![[
            r#"OK ((t #1=((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) ((name . "whole buffer") (lighter . "HSA") (face . ahs-plugin-whole-buffer-face) (start . point-min) (end . point-max)) " *HSA*" 3) nil nil #1# " HS")"#
        ]],
    )
}

fn auto_highlight_symbol_onekey_edit_all_windows_path_creates_matches_but_misses_edit_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_onekey_edit_all_windows_path_creates_matches_but_misses_edit_mode",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-display
                              ahs-highlight-all-windows
                              t
                              ahs-selected-window
                              (selected-window))
                             (ahs-onekey-edit-function
                              'whole-buffer
                              nil)
                             (list
                              ahs-edit-mode-enable
                              ahs-onekey-range-store
                              ahs-current-range
                              ahs-mode-line
                              (auto-highlight-symbol-test-overlays))))"##,
        expect![[
            r#"OK (nil nil ((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) " HS" ((1 6 current ahs-plugin-whole-buffer-face 1000 t t) (1 6 others ahs-face nil t t) (7 12 others ahs-face nil t t)))"#
        ]],
    )
}

fn auto_highlight_symbol_edit_command_prefix_selects_temporary_whole_buffer_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_edit_command_prefix_selects_temporary_whole_buffer_workflow",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-display
                              ahs-highlight-all-windows
                              nil
                              ahs-selected-window
                              (selected-window))
                             (ahs-edit-mode
                              t
                              '(4))
                             (list
                              ahs-edit-mode-enable
                              ahs-onekey-range-store
                              ahs-current-range
                              ahs-mode-line
                              (auto-highlight-symbol-test-overlays))))"##,
        expect![[
            r#"OK (t ((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) ((name . "whole buffer") (lighter . "HSA") (face . ahs-plugin-whole-buffer-face) (start . point-min) (end . point-max)) " *HSA*" ((1 6 current ahs-edit-mode-face 1000 t t) (1 6 others ahs-face nil t t) (7 12 others ahs-face nil t t)))"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_edit_mode_renames_every_highlighted_occurrence_in_real_code(),
        auto_highlight_symbol_symbol_modification_replaces_shorter_and_longer_targets_exactly(),
        auto_highlight_symbol_modification_hook_tracks_before_after_and_undo_inhibition(),
        auto_highlight_symbol_edit_mode_on_off_updates_hooks_faces_lighter_and_user_hooks(),
        auto_highlight_symbol_edit_mode_condition_reports_disabled_and_read_only_buffers(),
        auto_highlight_symbol_post_command_exits_edit_mode_when_cursor_leaves_current_overlay(),
        auto_highlight_symbol_onekey_edit_temporarily_switches_whole_buffer_then_restores_range(),
        auto_highlight_symbol_onekey_edit_all_windows_path_creates_matches_but_misses_edit_mode(),
        auto_highlight_symbol_edit_command_prefix_selects_temporary_whole_buffer_workflow(),
    ]
}
