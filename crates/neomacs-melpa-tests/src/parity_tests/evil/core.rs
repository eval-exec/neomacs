use expect_test::expect;

use super::ParityBatchCase;

fn evil_public_defaults_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_public_defaults_match_the_pinned_release",
        r##"(list
               evil-auto-indent
               evil-shift-width
               evil-shift-round
               evil-indent-convert-tabs
               evil-start-of-line
               evil-repeat-move-cursor
               evil-cross-lines
               evil-backspace-join-lines
               evil-move-cursor-back
               evil-move-beyond-eol
               evil-respect-visual-line-mode
               evil-track-eol
               evil-mode-line-format
               evil-bigword
               evil-want-fine-undo
               evil-regexp-search
               evil-search-wrap
               evil-auto-balance-windows
               evil-split-window-below
               evil-vsplit-window-right
               evil-esc-delay
               evil-intercept-esc
               evil-kill-on-visual-paste
               evil-want-change-word-to-end
               evil-want-Y-yank-to-eol
               evil-disable-insert-state-bindings
               evil-echo-state
               evil-toggle-key
               evil-default-state
               evil-magic
               evil-ex-search-case
               evil-ex-substitute-global
               evil-mode)"##,
        expect![[
            r#"OK (t 4 t t nil t nil t t nil nil t before "^ \11\15\n" nil t t t nil nil 0.01 always t t nil nil t "C-z" normal t smart nil nil)"#
        ]],
    )
}

fn evil_builtin_states_publish_complete_mode_keymap_and_tag_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_builtin_states_publish_complete_mode_keymap_and_tag_properties",
        r##"(mapcar
               (lambda (state)
                 (list
                  state
                  (evil-state-property state :mode)
                  (evil-state-property state :local)
                  (keymapp (evil-state-property state :keymap t))
                  (keymapp (evil-state-property state :local-keymap t))
                  (let ((tag (evil-state-property state :tag t)))
                    (setq tag
                          (if (functionp tag)
                              (funcall tag)
                            tag))
                    (and tag
                         (substring-no-properties tag)))))
               '(normal insert visual replace operator motion emacs))"##,
        expect![[
            r#"OK ((normal evil-normal-state-minor-mode evil-normal-state-local-minor-mode t nil " <N> ") (insert evil-insert-state-minor-mode evil-insert-state-local-minor-mode t nil " <I> ") (visual evil-visual-state-minor-mode evil-visual-state-local-minor-mode t nil nil) (replace evil-replace-state-minor-mode evil-replace-state-local-minor-mode t nil " <R> ") (operator evil-operator-state-minor-mode evil-operator-state-local-minor-mode t nil " <O> ") (motion evil-motion-state-minor-mode evil-motion-state-local-minor-mode t nil " <M> ") (emacs evil-emacs-state-minor-mode evil-emacs-state-local-minor-mode t nil " <E> "))"#
        ]],
    )
}

fn evil_local_mode_enable_and_disable_manage_state_maps_and_buffer_local_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "evil_local_mode_enable_and_disable_manage_state_maps_and_buffer_local_state",
        r##"(with-temp-buffer
               (let ((before
                      (list evil-local-mode evil-state
                            (local-variable-p 'evil-mode-map-alist))))
                 (evil-local-mode 1)
                 (let ((enabled
                        (list
                         evil-local-mode
                         evil-state
                         (local-variable-p 'evil-mode-map-alist)
                         (local-variable-p 'evil-normal-state-local-map)
                         (keymapp evil-normal-state-local-map)
                         (memq 'evil-mode-map-alist
                               emulation-mode-map-alists))))
                   (evil-local-mode -1)
                   (list
                    before
                    enabled
                    (list evil-local-mode evil-state
                          (evil-normal-state-p)
                          (evil-insert-state-p))))))"##,
        expect!["OK ((nil nil nil) (t normal t t t (evil-mode-map-alist)) (nil nil nil nil))"],
    )
}

fn evil_state_transitions_track_previous_state_and_mode_line_tag() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_state_transitions_track_previous_state_and_mode_line_tag",
        r##"(with-temp-buffer
               (evil-local-mode 1)
               (let (states)
                 (dolist (state '(normal visual emacs replace normal))
                   (evil-change-state state)
                   (push
                    (list evil-state evil-previous-state
                          (substring-no-properties
                           (format "%s" evil-mode-line-tag)))
                    states))
                 (evil-change-to-previous-state)
                 (push (list evil-state evil-previous-state) states)
                 (nreverse states)))"##,
        expect![[
            r#"OK ((normal nil " <N> ") (visual normal "nil") (emacs visual " <E> ") (replace emacs " <R> ") (normal replace " <N> ") (replace emacs))"#
        ]],
    )
}

fn evil_initial_state_resolution_honors_mode_inheritance_and_buffer_regexps() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_initial_state_resolution_honors_mode_inheritance_and_buffer_regexps",
        r##"(let ((evil-default-state 'normal)
                    (evil-emacs-state-modes '(special-mode))
                    (evil-insert-state-modes '(text-mode))
                    (evil-motion-state-modes '(help-mode))
                    (evil-buffer-regexps
                     '(("\\` \\*neo-special" . emacs)
                       ("neo-motion\\*\\'" . motion))))
               (list
                (evil-initial-state 'special-mode)
                (evil-initial-state 'text-mode)
                (evil-initial-state 'help-mode)
                (evil-initial-state 'fundamental-mode)
                (evil-initial-state-for-buffer-name
                 " *neo-special-buffer*" 'normal)
                (evil-initial-state-for-buffer-name
                 "*neo-motion*" 'normal)
                (evil-initial-state-for-buffer-name
                 "*ordinary*" 'insert)))"##,
        expect!["OK (emacs insert motion nil emacs motion insert)"],
    )
}

fn evil_set_initial_state_updates_existing_modes_and_new_mode_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_set_initial_state_updates_existing_modes_and_new_mode_entries",
        r##"(let ((evil-emacs-state-modes '(alpha-mode shared-mode))
                    (evil-insert-state-modes '(beta-mode))
                    (evil-motion-state-modes '(gamma-mode))
                    (evil-normal-state-modes nil))
               (evil-set-initial-state 'shared-mode 'insert)
               (evil-set-initial-state 'new-mode 'motion)
               (evil-set-initial-state 'alpha-mode 'normal)
               (list
                evil-emacs-state-modes
                evil-insert-state-modes
                evil-motion-state-modes
                evil-normal-state-modes
                (evil-initial-state 'shared-mode)
                (evil-initial-state 'new-mode)
                (evil-initial-state 'alpha-mode)))"##,
        expect![
            "OK (nil (shared-mode beta-mode) (new-mode gamma-mode) (alpha-mode) insert motion normal)"
        ],
    )
}

fn evil_define_state_creates_commands_predicates_maps_and_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_define_state_creates_commands_predicates_maps_and_properties",
        r##"(progn
               (evil-define-state neomacs-parity
                 "Neomacs parity state."
                 :tag " <N> "
                 :message "-- NEOMACS --"
                 :cursor box
                 :enable (motion))
               (list
                (commandp 'evil-neomacs-parity-state)
                (fboundp 'evil-neomacs-parity-state-p)
                (keymapp evil-neomacs-parity-state-map)
                (evil-state-property 'neomacs-parity :tag)
                (evil-state-property 'neomacs-parity :message)
                (evil-state-property 'neomacs-parity :cursor)
                (evil-state-property 'neomacs-parity :enable)
                (with-temp-buffer
                  (evil-local-mode 1)
                  (evil-neomacs-parity-state)
                  (list
                   evil-state
                   (evil-neomacs-parity-state-p)
                   evil-motion-state-minor-mode))))"##,
        expect![
            "OK (t t t evil-neomacs-parity-state-tag evil-neomacs-parity-state-message evil-neomacs-parity-state-cursor (motion) (neomacs-parity t t))"
        ],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        evil_public_defaults_match_the_pinned_release(),
        evil_builtin_states_publish_complete_mode_keymap_and_tag_properties(),
        evil_local_mode_enable_and_disable_manage_state_maps_and_buffer_local_state(),
        evil_state_transitions_track_previous_state_and_mode_line_tag(),
        evil_initial_state_resolution_honors_mode_inheritance_and_buffer_regexps(),
        evil_set_initial_state_updates_existing_modes_and_new_mode_entries(),
        evil_define_state_creates_commands_predicates_maps_and_properties(),
    ]
}
