use expect_test::expect;

use super::ParityBatchCase;

fn anju_window_under_mouse_passes_exact_pointer_coordinates_to_window_lookup() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_window_under_mouse_passes_exact_pointer_coordinates_to_window_lookup",
        r##"(let (calls)
         (cl-letf (((symbol-function 'mouse-position)
                    (lambda ()
                      '(frame-token 37 . 91)))
                   ((symbol-function 'window-at)
                    (lambda (x y frame)
                      (push (list x y frame) calls)
                      'window-token)))
           (list
            (anju-window-under-mouse)
            (nreverse calls))))"##,
        expect!["OK (window-token ((37 91 frame-token)))"],
    )
}

fn anju_mode_line_popup_commands_forward_real_menu_and_mouse_event_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_mode_line_popup_commands_forward_real_menu_and_mouse_event_shapes",
        r##"(let ((anju-mode-line-buffer-list-function
                (lambda ()
                  '(["alpha" switch-to-buffer]
                    ["beta" switch-to-buffer-other-window])))
               calls)
         (cl-letf (((symbol-function 'popup-menu)
                    (lambda (menu &optional position prefix)
                      (push (list menu position prefix) calls)
                      'selected)))
           (list
            (anju-popup-buffer-menu
             '(down-mouse-1 (frame-token 4 (20 . 3) 8)))
            (anju-popup-window-management-menu
             '(down-mouse-3 (frame-token 7 (11 . 2) 9)))
            (nreverse calls))))"##,
        expect![[
            r#"OK (selected selected (((["alpha" switch-to-buffer] ["beta" switch-to-buffer-other-window]) (down-mouse-1 (frame-token 4 (20 . 3) 8)) nil) ((keymap (× menu-item "×" mouse-delete-window :visible (not (one-window-p t)) :help "Delete window") (Split\ → menu-item "Split →" split-window-horizontally :help "Split right") (Split\ ↓ menu-item "Split ↓" split-window-vertically :help "Split below") (Swap menu-item "Swap" (keymap "Swap" (↑ menu-item "↑" windmove-swap-states-up :visible (window-in-direction 'above) :help "Swap window up") (↓ menu-item "↓" windmove-swap-states-down :visible (window-in-direction 'below) :help "Swap window down") (← menu-item "←" windmove-swap-states-left :visible (window-in-direction 'left) :help "Swap window left") (→ menu-item "→" windmove-swap-states-right :visible (window-in-direction 'right) :help "Swap window right")) :visible (and (eq (selected-window) (anju-window-under-mouse)) (not (one-window-p t))))) (down-mouse-3 (frame-token 7 (11 . 2) 9)) nil)))"#
        ]],
    )
}

fn anju_mode_line_buffer_menu_composes_filtered_buffers_and_window_actions() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_mode_line_buffer_menu_composes_filtered_buffers_and_window_actions",
        r##"(let* ((root
                  (file-name-as-directory
                   (expand-file-name
                    "mode-line-menu"
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                 (buffers
                  (list
                   (anju-test-buffer "alpha.md" #'markdown-mode root)
                   (anju-test-buffer "beta.org" #'org-mode root)
                   (anju-test-buffer "*Help*" #'help-mode root)))
                 (anju-buffer-list-filter-functions
                  '((anju-buffer-list-plain-filter . 2)
                    (anju-buffer-list-help-filter . 1))))
         (unwind-protect
             (progn
               (switch-to-buffer (car buffers))
               (let ((entries
                      (cl-letf
                          (((symbol-function 'anju-window-under-mouse)
                            (lambda () (selected-window))))
                        (anju-buffer-list-menu-items))))
                 (let ((labels-and-properties
                        (mapcar
                         (lambda (entry)
                           (if (vectorp entry)
                               (list
                                (aref entry 0)
                                (append (seq-drop entry 2) nil))
                             entry))
                         entries))
                       (before (buffer-name)))
                   (funcall (aref (car entries) 1))
                   (list
                    labels-and-properties
                    before
                    (buffer-name)))))
           (anju-test-kill-buffers buffers)))"##,
        expect![[
            r#"OK ((("beta.org" (:visible t)) ("*Help*" (:visible t)) "--" ("Set Selected" (:visible (not (and (eq (selected-window) (anju-window-under-mouse)))) :help "Set window at point as selected")) ("← Previous" (:visible (and (eq (selected-window) (anju-window-under-mouse))) :help "Previous Buffer")) ("→ Next" (:visible (and (eq (selected-window) (anju-window-under-mouse))) :help "Next buffer")) ("≣ List All Buffers" (:visible (and (eq (selected-window) (anju-window-under-mouse))) :help "List all buffers"))) "alpha.md" "beta.org")"#
        ]],
    )
}

fn anju_mode_line_bindings_replace_the_three_user_facing_mouse_gestures() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_mode_line_bindings_replace_the_three_user_facing_mouse_gestures",
        r##"(let* ((buffer-key
                  (kbd "<mode-line> <mouse-1>"))
                 (double-key
                  (kbd "<mode-line> <double-mouse-1>"))
                 (menu-key
                  (kbd "<mode-line> <down-mouse-3>"))
                 (old-buffer
                  (lookup-key
                   mode-line-buffer-identification-keymap
                   buffer-key))
                 (old-double
                  (lookup-key (current-global-map) double-key))
                 (old-menu
                  (lookup-key (current-global-map) menu-key)))
         (unwind-protect
             (progn
               (anju-mode-line--set-bindings)
               (list
                (lookup-key
                 mode-line-buffer-identification-keymap
                 buffer-key)
                (lookup-key (current-global-map) double-key)
                (lookup-key (current-global-map) menu-key)))
           (define-key
            mode-line-buffer-identification-keymap
            buffer-key old-buffer)
           (define-key (current-global-map) double-key old-double)
           (define-key (current-global-map) menu-key old-menu)))"##,
        expect![
            "OK (anju-popup-buffer-menu anju-toggle-one-window anju-popup-window-management-menu)"
        ],
    )
}

fn anju_window_management_menu_preserves_commands_predicates_and_help() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_window_management_menu_preserves_commands_predicates_and_help",
        r##"(list
         (anju-test-menu-entries anju-window-management-menu)
         (let ((swap-menu
                (lookup-key anju-window-management-menu [Swap])))
           (anju-test-menu-entries swap-menu)))"##,
        expect![[
            r#"OK (((× "×" mouse-delete-window :enable nil :visible (not (one-window-p t)) :style nil :selected nil :help "Delete window") (Split\ → "Split →" split-window-horizontally :enable nil :visible nil :style nil :selected nil :help "Split right") (Split\ ↓ "Split ↓" split-window-vertically :enable nil :visible nil :style nil :selected nil :help "Split below") (Swap "Swap" <submenu> :enable nil :visible (and (eq (selected-window) (anju-window-under-mouse)) (not (one-window-p t))) :style nil :selected nil :help nil)) ((↑ "↑" windmove-swap-states-up :enable nil :visible (window-in-direction 'above) :style nil :selected nil :help "Swap window up") (↓ "↓" windmove-swap-states-down :enable nil :visible (window-in-direction 'below) :style nil :selected nil :help "Swap window down") (← "←" windmove-swap-states-left :enable nil :visible (window-in-direction 'left) :style nil :selected nil :help "Swap window left") (→ "→" windmove-swap-states-right :enable nil :visible (window-in-direction 'right) :style nil :selected nil :help "Swap window right")))"#
        ]],
    )
}

pub(super) fn mode_line_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anju_window_under_mouse_passes_exact_pointer_coordinates_to_window_lookup(),
        anju_mode_line_popup_commands_forward_real_menu_and_mouse_event_shapes(),
        anju_mode_line_buffer_menu_composes_filtered_buffers_and_window_actions(),
        anju_mode_line_bindings_replace_the_three_user_facing_mouse_gestures(),
        anju_window_management_menu_preserves_commands_predicates_and_help(),
    ]
}
