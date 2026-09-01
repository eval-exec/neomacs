use expect_test::expect;

use super::ParityBatchCase;

fn which_key_public_defaults_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_public_defaults_match_the_pinned_release",
        r##"(list
               which-key-idle-delay
               which-key-idle-secondary-delay
               which-key-max-description-length
               which-key-min-column-description-width
               which-key-add-column-padding
               which-key-unicode-correction
               which-key-dont-use-unicode
               which-key-separator
               which-key-ellipsis
               which-key-prefix-prefix
               which-key-compute-remaps
               which-key-allow-multiple-replacements
               which-key-show-docstrings
               which-key-buffer-name
               which-key-show-prefix
               which-key-popup-type
               which-key-side-window-location
               which-key-side-window-slot
               which-key-side-window-max-width
               which-key-side-window-max-height
               which-key-frame-max-width
               which-key-frame-max-height
               which-key-show-remaining-keys
               which-key-sort-order
               which-key-sort-uppercase-first
               which-key-paging-key
               which-key-use-C-h-commands
               which-key-preserve-window-configuration
               which-key-persistent-popup
               which-key-hide-alt-key-translations
               which-key-show-transient-maps
               which-key-lighter
               which-key-inhibit
               which-key-mode)"##,
        expect![[
            r#"OK (1.0 nil 27 0 0 3 t " : " ".." "+" nil nil nil " *which-key*" echo side-window bottom 0 0.333 0.25 60 20 nil which-key-key-order t "<f5>" t nil nil t nil " WK" nil nil)"#
        ]],
    )
}

fn which_key_setup_commands_apply_their_complete_configuration_profiles() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_setup_commands_apply_their_complete_configuration_profiles",
        r##"(let ((echo-keystrokes 9)
                    (which-key-idle-delay 1.0)
                    (which-key-echo-keystrokes 0.02))
               (which-key-setup-side-window-right)
               (let ((right
                      (list which-key-popup-type
                            which-key-side-window-location
                            which-key-show-prefix
                            echo-keystrokes)))
                 (which-key-setup-side-window-right-bottom)
                 (let ((right-bottom
                        (list which-key-popup-type
                              which-key-side-window-location
                              which-key-show-prefix
                              echo-keystrokes)))
                   (which-key-setup-side-window-bottom)
                   (let ((bottom
                          (list which-key-popup-type
                                which-key-side-window-location
                                which-key-show-prefix
                                echo-keystrokes)))
                     (setq echo-keystrokes 8
                           which-key-echo-keystrokes 0.02)
                     (which-key-setup-minibuffer)
                     (list right
                           right-bottom
                           bottom
                           (list which-key-popup-type
                                 which-key-side-window-location
                                 which-key-show-prefix
                                 echo-keystrokes))))))"##,
        expect![
            "OK ((side-window right top 9) (side-window (right bottom) top 9) (side-window bottom echo 0.02) (minibuffer bottom left 0.02))"
        ],
    )
}

fn which_key_echo_keystroke_setup_covers_long_short_and_nil_delays() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_echo_keystroke_setup_covers_long_short_and_nil_delays",
        r##"(list
               (let ((echo-keystrokes 3)
                     (which-key-idle-delay 1.0)
                     (which-key-echo-keystrokes 0.02))
                 (which-key--setup-echo-keystrokes)
                 (list echo-keystrokes which-key-echo-keystrokes))
               (let ((echo-keystrokes 3)
                     (which-key-idle-delay 0.01)
                     (which-key-echo-keystrokes 0.02))
                 (which-key--setup-echo-keystrokes)
                 (list echo-keystrokes which-key-echo-keystrokes))
               (let ((echo-keystrokes nil)
                     (which-key-idle-delay 1.0)
                     (which-key-echo-keystrokes 0.02))
                 (which-key--setup-echo-keystrokes)
                 (list echo-keystrokes which-key-echo-keystrokes)))"##,
        expect!["OK ((0.02 0.02) (0.0025 0.0025) (nil 0.02))"],
    )
}

fn which_key_unicode_cleanup_only_replaces_the_default_arrow_separator() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_unicode_cleanup_only_replaces_the_default_arrow_separator",
        r##"(list
               (let ((which-key-separator " → "))
                 (which-key-remove-default-unicode-chars)
                 which-key-separator)
               (let ((which-key-separator " ⇒ "))
                 (which-key-remove-default-unicode-chars)
                 which-key-separator)
               (let ((which-key-separator " : "))
                 (which-key-remove-default-unicode-chars)
                 which-key-separator))"##,
        expect![[r#"OK (" : " " ⇒ " " : ")"#]],
    )
}

fn which_key_mode_enable_and_disable_restore_state_and_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_mode_enable_and_disable_restore_state_and_hooks",
        r##"(let ((which-key-mode nil)
                    (which-key-show-prefix 'echo)
                    (which-key-popup-type 'side-window)
                    (which-key-show-remaining-keys t)
                    (which-key-use-C-h-commands t)
                    (which-key-show-early-on-C-h nil)
                    (which-key--prefix-help-cmd-backup nil)
                    (which-key--echo-keystrokes-backup nil)
                    (prefix-help-command 'describe-prefix-bindings)
                    (echo-keystrokes 4)
                    (pre-command-hook nil)
                    (window-size-change-functions nil)
                    events)
               (cl-letf (((symbol-function 'which-key--start-timer)
                          (lambda (&rest args)
                            (push (cons 'start args) events)))
                         ((symbol-function 'which-key--stop-timer)
                          (lambda ()
                            (push 'stop events))))
                 (which-key-mode 1)
                 (let ((enabled
                        (list
                         which-key-mode
                         echo-keystrokes
                         prefix-help-command
                         (memq #'which-key--lighter-restore pre-command-hook)
                         (memq #'which-key--hide-popup pre-command-hook)
                         (memq #'which-key--hide-popup-on-frame-size-change
                               window-size-change-functions)
                         (copy-tree events))))
                   (which-key-mode -1)
                   (list
                    enabled
                    (list
                     which-key-mode
                     echo-keystrokes
                     prefix-help-command
                     (memq #'which-key--lighter-restore pre-command-hook)
                     (memq #'which-key--hide-popup pre-command-hook)
                     (memq #'which-key--hide-popup-on-frame-size-change
                           window-size-change-functions)
                     events)))))"##,
        expect![[
            r#"OK ((t 0.25 which-key-C-h-dispatch #1=(which-key--lighter-restore) (which-key--hide-popup . #1#) (which-key--hide-popup-on-frame-size-change) ((start))) (nil 4 describe-prefix-bindings nil nil nil (stop (start))))"#
        ]],
    )
}

fn which_key_buffer_initialization_sets_exact_local_display_state_and_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_buffer_initialization_sets_exact_local_display_state_and_hook",
        r##"(let ((which-key--buffer nil)
                    (which-key-buffer-name " *neomacs-which-key-test*")
                    (which-key-init-buffer-hook
                     (list
                      (lambda ()
                        (setq-local neomacs-which-key-hook-ran t)))))
               (unwind-protect
                   (progn
                     (which-key--init-buffer)
                     (with-current-buffer which-key--buffer
                       (list
                        (buffer-name)
                        truncate-lines
                        cursor-type
                        cursor-in-non-selected-windows
                        mode-line-format
                        header-line-format
                        word-wrap
                        show-trailing-whitespace
                        neomacs-which-key-hook-ran)))
                 (when (buffer-live-p which-key--buffer)
                   (kill-buffer which-key--buffer))))"##,
        expect![[r#"OK (" *neomacs-which-key-test*" t nil nil nil nil nil nil t)"#]],
    )
}

pub(super) fn defaults_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        which_key_public_defaults_match_the_pinned_release(),
        which_key_setup_commands_apply_their_complete_configuration_profiles(),
        which_key_echo_keystroke_setup_covers_long_short_and_nil_delays(),
        which_key_unicode_cleanup_only_replaces_the_default_arrow_separator(),
        which_key_mode_enable_and_disable_restore_state_and_hooks(),
        which_key_buffer_initialization_sets_exact_local_display_state_and_hook(),
    ]
}
