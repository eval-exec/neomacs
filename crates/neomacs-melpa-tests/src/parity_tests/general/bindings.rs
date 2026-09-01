use expect_test::expect;

use super::ParityBatchCase;

fn general_public_defaults_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_public_defaults_match_the_pinned_release",
        r##"(list
               general-implicit-kbd
               general-default-prefix
               general-default-non-normal-prefix
               general-default-global-prefix
               general-default-states
               general-non-normal-states
               general-default-keymaps
               general-vim-definer-default
               general-override-auto-enable
               general-use-package-emit-autoloads
               general-describe-update-previous-definition
               (keymapp general-override-mode-map)
               general-override-mode)"##,
        expect![[
            r#"OK (t nil nil nil nil (insert replace emacs hybrid iedit-insert) global nil t t on-change t nil)"#
        ]],
    )
}

fn general_define_key_binds_supported_key_and_definition_shapes_and_records_them() -> ParityBatchCase
{
    ParityBatchCase::value(
        "general_define_key_binds_supported_key_and_definition_shapes_and_records_them",
        r##"(progn
               (defvar
                 neomacs-general-shapes-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-shapes-map
                (make-sparse-keymap))
               (let ((general-keybindings nil))
                 (general-define-key
                  :keymaps
                  'neomacs-general-shapes-map
                  "C-c a" #'forward-char
                  [f8] #'ignore
                  [remap kill-line] #'backward-kill-line
                  "C-c b" "C-x b"
                  "C-c l"
                  (lambda ()
                    (interactive)
                    'lambda-result))
                 (list
                  (lookup-key
                   neomacs-general-shapes-map
                   (kbd "C-c a"))
                  (lookup-key
                   neomacs-general-shapes-map
                   [f8])
                  (lookup-key
                   neomacs-general-shapes-map
                   [remap kill-line])
                  (lookup-key
                   neomacs-general-shapes-map
                   (kbd "C-c b"))
                  (commandp
                   (lookup-key
                    neomacs-general-shapes-map
                    (kbd "C-c l")))
                  general-keybindings)))"##,
        expect![[
            r#"OK (forward-char ignore backward-kill-line "\30b" t ((neomacs-general-shapes-map (nil ("\3a" forward-char nil) ([f8] ignore nil) ([remap kill-line] backward-kill-line nil) ("\3b" "\30b" nil) ("\3l" #[nil ('lambda-result) (t) nil nil nil] nil)))))"#
        ]],
    )
}

fn general_define_key_local_map_is_buffer_local_and_records_local_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_define_key_local_map_is_buffer_local_and_records_local_bindings",
        r##"(let ((general-local-keybindings nil))
               (with-temp-buffer
                 (general-define-key
                  :keymaps 'local
                  "C-c l" #'forward-line)
                 (list
                  general-override-local-mode
                  (lookup-key
                   general-override-local-mode-map
                   (kbd "C-c l"))
                  general-local-keybindings))
               )"##,
        expect![[r#"OK (t forward-line ((nil ("\3l" forward-line nil))))"#]],
    )
}

fn general_define_key_combines_prefix_infix_and_vector_keys_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_define_key_combines_prefix_infix_and_vector_keys_exactly",
        r##"(progn
               (defvar
                 neomacs-general-prefix-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-prefix-map
                (make-sparse-keymap))
               (general-define-key
                :keymaps
                'neomacs-general-prefix-map
                :prefix "C-c"
                :infix "p"
                "a" #'forward-char
                [?b] #'backward-char)
               (list
                (lookup-key
                 neomacs-general-prefix-map
                 (kbd "C-c p a"))
                (lookup-key
                 neomacs-general-prefix-map
                 (kbd "C-c p b"))
                (lookup-key
                 neomacs-general-prefix-map
                 (kbd "C-c a"))
                (lookup-key
                 neomacs-general-prefix-map
                 (kbd "p a"))))"##,
        expect![[r#"OK (forward-char backward-char nil 1)"#]],
    )
}

fn general_define_key_creates_and_reuses_named_prefix_commands_and_maps() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_define_key_creates_and_reuses_named_prefix_commands_and_maps",
        r##"(progn
               (when
                   (fboundp
                    'neomacs-general-prefix-command)
                 (fmakunbound
                  'neomacs-general-prefix-command))
               (when
                   (boundp
                    'neomacs-general-prefix-command-map)
                 (makunbound
                  'neomacs-general-prefix-command-map))
               (defvar
                 neomacs-general-parent-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-parent-map
                (make-sparse-keymap))
               (general-define-key
                :keymaps
                'neomacs-general-parent-map
                :prefix "C-c"
                :prefix-command
                'neomacs-general-prefix-command
                :prefix-map
                'neomacs-general-prefix-command-map
                :prefix-name "Neomacs General"
                "a" #'forward-char)
               (general-define-key
                :keymaps
                'neomacs-general-parent-map
                :prefix "C-c"
                :prefix-command
                'neomacs-general-prefix-command
                :prefix-map
                'neomacs-general-prefix-command-map
                "b" #'backward-char)
               (list
                (fboundp
                 'neomacs-general-prefix-command)
                (keymapp
                 neomacs-general-prefix-command-map)
                (keymap-prompt
                 neomacs-general-prefix-command-map)
                (eq
                 (lookup-key
                  neomacs-general-parent-map
                  (kbd "C-c"))
                 'neomacs-general-prefix-command)
                (lookup-key
                 neomacs-general-parent-map
                 (kbd "C-c a"))
                (lookup-key
                 neomacs-general-parent-map
                 (kbd "C-c b"))))"##,
        expect![[r#"OK (t t "Neomacs General" t forward-char backward-char)"#]],
    )
}

fn general_extended_definitions_cover_ignore_predicate_and_nested_keymap() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_extended_definitions_cover_ignore_predicate_and_nested_keymap",
        r##"(progn
               (defvar
                 neomacs-general-enabled nil)
               (defvar
                 neomacs-general-nested-map
                 (make-sparse-keymap))
               (defvar
                 neomacs-general-extended-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-enabled nil
                neomacs-general-nested-map
                (make-sparse-keymap)
                neomacs-general-extended-map
                (make-sparse-keymap))
               (define-key
                neomacs-general-nested-map
                (kbd "x") #'next-line)
               (general-define-key
                :keymaps
                'neomacs-general-extended-map
                "i" '(:ignore)
                "p"
                '(forward-char
                  :predicate
                  neomacs-general-enabled)
                "n"
                '(:keymap
                  neomacs-general-nested-map
                  :package general))
               (with-temp-buffer
                 (use-local-map
                  neomacs-general-extended-map)
                 (let ((disabled
                        (key-binding (kbd "p"))))
                   (setq neomacs-general-enabled t)
                   (list
                    (lookup-key
                     neomacs-general-extended-map
                     (kbd "i"))
                    disabled
                    (key-binding (kbd "p"))
                    (key-binding (kbd "n x"))
                    (keymapp
                     (lookup-key
                      neomacs-general-extended-map
                      (kbd "n")))))))"##,
        expect![[r#"OK (nil self-insert-command forward-char next-line t)"#]],
    )
}

fn general_keymap_aliases_select_the_exact_target_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_keymap_aliases_select_the_exact_target_map",
        r##"(progn
               (defvar
                 neomacs-general-alias-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-alias-map
                (make-sparse-keymap))
               (let ((general-keymap-aliases
                      (cons
                       '(neomacs-alias
                         .
                         neomacs-general-alias-map)
                       general-keymap-aliases)))
                 (general-define-key
                  :keymaps 'neomacs-alias
                  "C-c a" #'forward-char)
                 (list
                  (lookup-key
                   neomacs-general-alias-map
                   (kbd "C-c a"))
                  (assq
                   'neomacs-alias
                   general-keymap-aliases))))"##,
        expect![[r#"OK (forward-char (neomacs-alias . neomacs-general-alias-map))"#]],
    )
}

fn general_define_key_delays_binding_until_a_named_keymap_exists() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_define_key_delays_binding_until_a_named_keymap_exists",
        r##"(progn
               (when
                   (boundp
                    'neomacs-general-delayed-map)
                 (makunbound
                  'neomacs-general-delayed-map))
               (general-define-key
                :keymaps
                'neomacs-general-delayed-map
                "C-c d" #'forward-char)
               (let ((before
                      (boundp
                       'neomacs-general-delayed-map)))
                 (defvar
                   neomacs-general-delayed-map
                   (make-sparse-keymap))
                 (run-hook-with-args
                  'after-load-functions
                  "neomacs-general-feature.el")
                 (list
                  before
                  (lookup-key
                   neomacs-general-delayed-map
                   (kbd "C-c d")))))"##,
        expect![[r#"OK (nil forward-char)"#]],
    )
}

fn general_override_mode_map_takes_precedence_over_an_active_minor_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_override_mode_map_takes_precedence_over_an_active_minor_mode",
        r##"(progn
               (defvar
                 neomacs-general-minor-map
                 (make-sparse-keymap))
               (define-minor-mode
                 neomacs-general-minor-mode
                 "General parity mode."
                 :keymap
                 neomacs-general-minor-map)
               (setq
                neomacs-general-minor-map
                (make-sparse-keymap))
               (setcdr
                general-override-mode-map nil)
               (define-key
                neomacs-general-minor-map
                (kbd "C-c o")
                #'forward-char)
               (general-define-key
                :keymaps 'override
                "C-c o" #'backward-char)
               (unwind-protect
                   (progn
                     (general-override-mode 1)
                     (with-temp-buffer
                       (neomacs-general-minor-mode 1)
                       (list
                        general-override-mode
                        neomacs-general-minor-mode
                        (lookup-key
                         neomacs-general-minor-map
                         (kbd "C-c o"))
                        (lookup-key
                         general-override-mode-map
                         (kbd "C-c o"))
                        (key-binding
                         (kbd "C-c o")))))
                 (general-override-mode -1)))"##,
        expect![[r#"OK (t t forward-char backward-char backward-char)"#]],
    )
}

fn general_describe_keybindings_formats_recorded_bindings_into_an_org_table() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_describe_keybindings_formats_recorded_bindings_into_an_org_table",
        r##"(progn
               (defvar
                 neomacs-general-report-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-report-map
                (make-sparse-keymap))
               (let ((general-keybindings nil)
                     (general-local-keybindings nil)
                     (general-describe-priority-keymaps
                      '(neomacs-general-report-map)))
                 (define-key
                  neomacs-general-report-map
                  (kbd "C-c a")
                  #'beginning-of-line)
                 (general-define-key
                  :keymaps
                  'neomacs-general-report-map
                  "C-c a" #'forward-char
                  "C-c b" #'backward-char)
                 (unwind-protect
                     (cl-letf
                         (((symbol-function 'org-mode)
                           #'ignore)
                          ((symbol-function
                            'org-at-heading-p)
                           (lambda () nil))
                          ((symbol-function
                            'org-table-align)
                           #'ignore)
                          ((symbol-function
                            'outline-next-heading)
                           (lambda () nil)))
                       (general-describe-keybindings)
                       (with-current-buffer
                           "*General Keybindings*"
                         (buffer-substring-no-properties
                          (point-min)
                          (point-max))))
                   (when
                       (get-buffer
                        "*General Keybindings*")
                     (kill-buffer
                      "*General Keybindings*")))))"##,
        expect![[
            r#"OK "* Neomacs-General-Report-Map Keybindings\n|key|command|previous|\n|-+-|\n|=C-c a=|~forward-char~|~beginning-of-line~|\n|=C-c b=|~backward-char~|~nil~|\n\n* Local Keybindings\n""#
        ]],
    )
}

fn general_define_key_ignores_an_unpaired_trailing_key() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_define_key_ignores_an_unpaired_trailing_key",
        r##"(let ((map
                     (make-sparse-keymap)))
               (list
                (general-define-key
                 :keymaps map
                 "C-c a")
                (lookup-key
                 map (kbd "C-c a"))))"##,
        expect![[r#"OK (nil 1)"#]],
    )
}

pub(super) fn bindings_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        general_public_defaults_match_the_pinned_release(),
        general_define_key_binds_supported_key_and_definition_shapes_and_records_them(),
        general_define_key_local_map_is_buffer_local_and_records_local_bindings(),
        general_define_key_combines_prefix_infix_and_vector_keys_exactly(),
        general_define_key_creates_and_reuses_named_prefix_commands_and_maps(),
        general_extended_definitions_cover_ignore_predicate_and_nested_keymap(),
        general_keymap_aliases_select_the_exact_target_map(),
        general_define_key_delays_binding_until_a_named_keymap_exists(),
        general_override_mode_map_takes_precedence_over_an_active_minor_mode(),
        general_describe_keybindings_formats_recorded_bindings_into_an_org_table(),
        general_define_key_ignores_an_unpaired_trailing_key(),
    ]
}
