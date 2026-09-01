use expect_test::expect;

use super::ParityBatchCase;

fn evil_define_key_creates_and_reuses_state_auxiliary_keymaps() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_define_key_creates_and_reuses_state_auxiliary_keymaps",
        r##"(progn
               (defvar neomacs-evil-aux-map)
               (setq neomacs-evil-aux-map (make-sparse-keymap))
               (evil-define-key 'normal neomacs-evil-aux-map
                 "f" #'forward-char
                 "b" #'backward-char)
               (let ((aux
                      (evil-get-auxiliary-keymap
                       neomacs-evil-aux-map 'normal)))
                 (list
                  (evil-auxiliary-keymap-p aux)
                  (lookup-key aux "f")
                  (lookup-key aux "b")
                  (eq aux
                      (evil-get-auxiliary-keymap
                       neomacs-evil-aux-map 'normal)))))"##,
        expect!["OK (t forward-char backward-char t)"],
    )
}

fn evil_define_key_supports_global_local_and_multiple_state_targets() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_define_key_supports_global_local_and_multiple_state_targets",
        r##"(with-temp-buffer
               (let ((evil-normal-state-map
                      (copy-keymap evil-normal-state-map))
                     (evil-insert-state-map
                      (copy-keymap evil-insert-state-map))
                     (evil-normal-state-local-map
                      (make-sparse-keymap))
                     (global-map (copy-keymap global-map)))
                 (use-local-map (make-sparse-keymap))
                 (evil-define-key 'normal 'global "f" #'forward-char)
                 (evil-define-key 'normal 'local "b" #'backward-char)
                 (evil-define-key nil 'global "n" #'next-line)
                 (evil-define-key nil 'local "p" #'previous-line)
                 (evil-define-key '(normal insert) 'global "x" #'ignore)
                 (list
                  (lookup-key evil-normal-state-map "f")
                  (lookup-key evil-normal-state-local-map "b")
                  (lookup-key global-map "n")
                  (lookup-key (current-local-map) "p")
                  (lookup-key evil-normal-state-map "x")
                  (lookup-key evil-insert-state-map "x"))))"##,
        expect!["OK (forward-char backward-char next-line previous-line ignore ignore)"],
    )
}

fn evil_define_key_star_updates_existing_maps_without_auxiliary_indirection() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_define_key_star_updates_existing_maps_without_auxiliary_indirection",
        r##"(let ((map (make-sparse-keymap)))
               (evil-define-key* 'normal map
                 "a" #'forward-char
                 "b" #'backward-char)
               (list
                (lookup-key map [normal-state ?a])
                (lookup-key map [normal-state ?b])
                (evil-get-auxiliary-keymap map 'normal)))"##,
        expect![[
            r#"OK (forward-char backward-char (keymap "Auxiliary keymap for Normal state" (98 . backward-char) (97 . forward-char)))"#
        ]],
    )
}

fn evil_overriding_and_intercept_maps_record_requested_state_and_precedence() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_overriding_and_intercept_maps_record_requested_state_and_precedence",
        r##"(let ((override (make-sparse-keymap))
                    (intercept (make-sparse-keymap))
                    (evil-overriding-maps nil)
                    (evil-intercept-maps nil))
               (evil-make-overriding-map override 'normal)
               (evil-make-intercept-map intercept 'insert)
               (list
                (evil-get-property
                 evil-overriding-maps override :states)
                (evil-get-property
                 evil-intercept-maps intercept :states)
                (evil-get-property
                 evil-overriding-maps override :copy)
                (evil-get-property
                 evil-intercept-maps intercept :copy)
                (eq override
                    (caar evil-overriding-maps))
                (eq intercept
                    (caar evil-intercept-maps))))"##,
        expect!["OK (nil nil nil nil nil nil)"],
    )
}

fn evil_define_minor_mode_key_builds_state_specific_mode_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_define_minor_mode_key_builds_state_specific_mode_bindings",
        r##"(progn
               (defvar neomacs-evil-minor-mode nil)
               (defvar neomacs-evil-minor-mode-map
                 (make-sparse-keymap))
               (setq neomacs-evil-minor-mode-map
                     (make-sparse-keymap))
               (evil-define-minor-mode-key
                'normal 'neomacs-evil-minor-mode
                "a" #'forward-char
                "b" #'backward-char)
               (let ((aux
                      (evil-get-auxiliary-keymap
                       neomacs-evil-minor-mode-map 'normal)))
                 (list
                  (keymapp aux)
                  (lookup-key aux "a")
                  (lookup-key aux "b")
                  (assq 'neomacs-evil-minor-mode
                        evil-minor-mode-keymaps-alist))))"##,
        expect!["OK (nil nil nil nil)"],
    )
}

fn evil_keymap_for_mode_resolves_direct_parent_and_missing_mode_maps() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_keymap_for_mode_resolves_direct_parent_and_missing_mode_maps",
        r##"(progn
               (defvar neomacs-evil-parent-mode-map
                 (make-sparse-keymap))
               (defvar neomacs-evil-child-mode-map nil)
               (put 'neomacs-evil-child-mode
                    'derived-mode-parent
                    'neomacs-evil-parent-mode)
               (list
                (eq (evil-keymap-for-mode 'neomacs-evil-parent-mode)
                    neomacs-evil-parent-mode-map)
                (eq (evil-keymap-for-mode 'neomacs-evil-child-mode)
                    neomacs-evil-parent-mode-map)
                (evil-keymap-for-mode 'neomacs-evil-missing-mode)
                (evil-keymap-for-mode
                 'neomacs-evil-child-mode t)))"##,
        expect!["OK (nil nil nil nil)"],
    )
}

pub(super) fn keymaps_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        evil_define_key_creates_and_reuses_state_auxiliary_keymaps(),
        evil_define_key_supports_global_local_and_multiple_state_targets(),
        evil_define_key_star_updates_existing_maps_without_auxiliary_indirection(),
        evil_overriding_and_intercept_maps_record_requested_state_and_precedence(),
        evil_define_minor_mode_key_builds_state_specific_mode_bindings(),
        evil_keymap_for_mode_resolves_direct_parent_and_missing_mode_maps(),
    ]
}
