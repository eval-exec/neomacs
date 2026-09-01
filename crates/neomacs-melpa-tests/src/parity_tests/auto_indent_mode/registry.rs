use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_registers_feature_modes_and_public_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_registers_feature_modes_and_public_commands",
        r##"(list
         (featurep 'auto-indent-mode)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (commandp symbol)))
          '(auto-indent-mode
            auto-indent-global-mode
            auto-indent-mode-on
            auto-indent-eol-newline
            auto-indent-eol-char-newline
            auto-indent-whole-buffer
            auto-indent-delete-char
            auto-indent-delete-backward-char
            auto-indent-kill-line
            auto-indent-kill-region
            auto-indent-kill-ring-save
            auto-indent-cua-copy-region
            auto-indent-deactivate-advices)))"##,
        expect![
            "OK (t ((auto-indent-mode t t) (auto-indent-global-mode t t) (auto-indent-mode-on t t) (auto-indent-eol-newline t t) (auto-indent-eol-char-newline t t) (auto-indent-whole-buffer t t) (auto-indent-delete-char t t) (auto-indent-delete-backward-char t t) (auto-indent-kill-line t t) (auto-indent-kill-region t t) (auto-indent-kill-ring-save t t) (auto-indent-cua-copy-region t t) (auto-indent-deactivate-advices t t)))"
        ],
    )
}

fn auto_indent_mode_core_custom_defaults_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_core_custom_defaults_are_exact",
        r##"(mapcar
         (lambda (symbol)
           (list symbol
                 (default-value symbol)
                 (get symbol 'custom-type)
                 (get symbol 'custom-group)))
         '(auto-indent-home-is-beginning-of-indent
           auto-indent-current-pairs
           auto-indent-next-pair
           auto-indent-next-pair-throttle
           auto-indent-indent-style
           auto-indent-on-yank-or-paste
           auto-indent-mode-untabify-on-yank-or-paste
           auto-indent-on-visit-file
           auto-indent-on-save-file
           auto-indent-untabify-on-save-file
           auto-indent-newline-function
           auto-indent-backward-delete-char-behavior
           auto-indent-engine
           auto-indent-assign-indent-level
           auto-indent-block-close))"##,
        expect![[
            r#"OK ((auto-indent-home-is-beginning-of-indent t boolean nil) (auto-indent-current-pairs t boolean nil) (auto-indent-next-pair nil boolean nil) (auto-indent-next-pair-throttle 1 number nil) (auto-indent-indent-style moderate (choice (const 'aggressive :tag "Indent Aggressively") (const 'moderate :tag "Indent Aggressively outside of repository, and conservatively inside a repository.") (const 'conservative :tag "Indent Conservatively")) nil) (auto-indent-on-yank-or-paste t boolean nil) (auto-indent-mode-untabify-on-yank-or-paste t (choice (const :tag "Do not tabify or untabify" nil) (const :tag "Untabify region on paste" t) (const :tag "Tabify region on paste" tabify)) nil) (auto-indent-on-visit-file nil boolean nil) (auto-indent-on-save-file nil boolean nil) (auto-indent-untabify-on-save-file t (choice (const :tag "Do not tabify or untabify file" nil) (const :tag "Untabify file on visit" t) (const :tag "Tabify region on visit" tabify)) nil) (auto-indent-newline-function reindent-then-newline-and-indent (choice (const :tag "Reindent the current line, insert the newline then indent the current line." reindent-then-newline-and-indent) (const :tag "Insert newline then indent current line" 'newline-and-indent)) nil) (auto-indent-backward-delete-char-behavior all (choice (const untabify) (const hungry) (const all) (const nil)) nil) (auto-indent-engine nil (choice (const :tag "default" nil) (const :tag "Keymaps" keys)) nil) (auto-indent-assign-indent-level 2 integer nil) (auto-indent-block-close t boolean nil))"#
        ]],
    )
    .fresh_process()
}

fn auto_indent_mode_editing_policy_defaults_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_editing_policy_defaults_are_exact",
        r##"(mapcar
         (lambda (symbol)
           (list symbol (default-value symbol)))
         '(auto-indent-delete-line-char-add-extra-spaces
           auto-indent-delete-line-char-add-extra-spaces-prog-mode-regs
           auto-indent-delete-line-char-remove-extra-spaces
           auto-indent-delete-line-char-remove-last-space
           auto-indent-delete-line-char-remove-last-space-prog-mode-regs
           auto-indent-kill-remove-extra-spaces
           auto-indent-kill-line-at-eol
           auto-indent-kill-line-kill-region-when-active
           auto-indent-use-text-boundaries
           auto-indent-disabled-indent-functions
           auto-indent-disabled-modes-on-save
           auto-indent-multiple-indent-modes))"##,
        expect![[
            r#"OK ((auto-indent-delete-line-char-add-extra-spaces t) (auto-indent-delete-line-char-add-extra-spaces-prog-mode-regs (("\\(\\s.\\|\\sw\\)" "\\(\\sw\\|\\s.\\)"))) (auto-indent-delete-line-char-remove-extra-spaces t) (auto-indent-delete-line-char-remove-last-space t) (auto-indent-delete-line-char-remove-last-space-prog-mode-regs (("\\(\\s.\\|\\s-\\)" "\\(\\s\"\\|\\sw\\)") ("\\s(" "\\(\\s(\\|\\s_\\|\\sw\\)") ("\\s)" "\\s)"))) (auto-indent-kill-remove-extra-spaces nil) (auto-indent-kill-line-at-eol nil) (auto-indent-kill-line-kill-region-when-active t) (auto-indent-use-text-boundaries t) (auto-indent-disabled-indent-functions (indent-relative indent-relative-maybe)) (auto-indent-disabled-modes-on-save (ahk-mode)) (auto-indent-multiple-indent-modes (python-mode coffee-mode haskell-mode haml-mode yaml-mode slim-mode scss-mode)))"#
        ]],
    )
    .fresh_process()
}

fn auto_indent_mode_initial_keymap_and_saved_key_specs_are_consistent() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_initial_keymap_and_saved_key_specs_are_consistent",
        r##"(list
         (keymapp auto-indent-mode-map)
         auto-indent-eol-ret-save
         auto-indent-eol-ret-semi-save
         (lookup-key auto-indent-mode-map (kbd "RET"))
         (lookup-key auto-indent-mode-map (kbd "M-RET"))
         (lookup-key auto-indent-mode-map [remap delete-char])
         (lookup-key auto-indent-mode-map [remap kill-line]))"##,
        expect![[r#"OK (t "" "" nil nil 1 1)"#]],
    )
}

fn auto_indent_mode_all_legacy_advices_are_registered_and_initially_inactive_or_active()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_all_legacy_advices_are_registered_and_initially_inactive_or_active",
        r##"(mapcar
         #'auto-indent-test-advice-state
         '(delete-char
           kill-line
           kill-region
           kill-ring-save
           cua-copy-region
           backward-delete-char-untabify
           backward-delete-char
           delete-backward-char
           move-beginning-of-line))"##,
        expect![
            "OK ((delete-char t nil) (kill-line t nil) (kill-region t nil) (kill-ring-save t nil) (cua-copy-region t nil) (backward-delete-char-untabify t nil) (backward-delete-char t nil) (delete-backward-char t nil) (move-beginning-of-line t nil))"
        ],
    )
}

fn auto_indent_mode_global_hooks_are_installed_by_source_load() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_global_hooks_are_installed_by_source_load",
        r##"(let ((registrations
                                `((auto-indent-save-par-region-interval
                                   . ,kill-emacs-hook)
                                  (auto-indent-turn-on-org-indent
                                   . ,org-mode-hook)
                                  (auto-indent-minibuffer-hook
                                   . ,minibuffer-setup-hook)
                                  (auto-indent-disable-electric
                                   . ,after-change-major-mode-hook))))
         (list
          (mapcar
           (lambda (registration)
             (let ((function (car registration))
                   (hook (cdr registration)))
               (list
                function
                (not (null (memq function hook)))
                (eq function (car hook))
                (cl-count function hook))))
           registrations)
          (not
           (null
            (advice-member-p
             #'ad-Advice-move-beginning-of-line
             'beginning-of-visual-line)))))"##,
        expect![
            "OK (((auto-indent-save-par-region-interval t t 1) (auto-indent-turn-on-org-indent t t 1) (auto-indent-minibuffer-hook t t 1) (auto-indent-disable-electric t t 1)) t)"
        ],
    )
}

fn auto_indent_mode_load_history_records_selected_full_surface_definitions() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_load_history_records_selected_full_surface_definitions",
        r##"(let* ((entry
                                 (cl-find-if
                                  (lambda (item)
                                    (memq
                                     '(provide . auto-indent-mode)
                                     (cdr item)))
                                  load-history))
              (definitions (cdr entry)))
         (list
          (not (null entry))
          (mapcar
           (lambda (definition)
             (member definition definitions))
           '((defun . auto-indent-is-repository-p)
             (defun . auto-indent-mode)
             (defun . auto-indent-yank-post-command)
             (defun . auto-indent-whole-buffer)
             (defun . auto-indent-delete-char)
             (defun . auto-indent-kill-line)
             (defun . auto-indent-mode-post-command-hook)
             (provide . auto-indent-mode)))))"##,
        expect!["OK (nil (nil nil nil nil nil nil nil nil))"],
    )
}

fn auto_indent_mode_setup_map_rebuilds_exact_configured_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_setup_map_rebuilds_exact_configured_bindings",
        r##"(let ((auto-indent-key-for-end-of-line-then-newline
                                "M-RET")
             (auto-indent-key-for-end-of-line-insert-char-then-newline
              "C-M-RET"))
         (auto-indent-setup-map)
         (list
          (keymapp auto-indent-mode-map)
          (lookup-key auto-indent-mode-map (kbd "M-RET"))
          (lookup-key auto-indent-mode-map (kbd "C-M-RET"))
          auto-indent-eol-ret-save
          auto-indent-eol-ret-semi-save))"##,
        expect![[
            r#"OK (t auto-indent-eol-newline auto-indent-eol-char-newline "M-RET" "C-M-RET")"#
        ]],
    )
}

fn auto_indent_mode_generated_autoloads_expose_commands_without_loading_runtime() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_generated_autoloads_expose_commands_without_loading_runtime",
        r##"(list
         (featurep 'auto-indent-mode)
         (mapcar
          (lambda (symbol)
            (let ((definition (symbol-function symbol)))
              (list symbol
                    (autoloadp definition)
                    (commandp symbol)
                    (and (autoloadp definition)
                         (nth 1 definition)))))
          '(auto-indent-eol-newline
            auto-indent-eol-char-newline
            auto-indent-mode
            auto-indent-mode-on
            auto-indent-global-mode))
         (boundp 'auto-indent-current-pairs))"##,
        expect![[
            r#"OK (nil ((auto-indent-eol-newline t t "auto-indent-mode") (auto-indent-eol-char-newline t t "auto-indent-mode") (auto-indent-mode t t "auto-indent-mode") (auto-indent-mode-on t t "auto-indent-mode") (auto-indent-global-mode t t "auto-indent-mode")) nil)"#
        ]],
    )
}

pub(super) fn registry_auto_indent_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_registers_feature_modes_and_public_commands(),
        auto_indent_mode_core_custom_defaults_are_exact(),
        auto_indent_mode_editing_policy_defaults_are_exact(),
        auto_indent_mode_initial_keymap_and_saved_key_specs_are_consistent(),
        auto_indent_mode_all_legacy_advices_are_registered_and_initially_inactive_or_active(),
        auto_indent_mode_global_hooks_are_installed_by_source_load(),
        auto_indent_mode_load_history_records_selected_full_surface_definitions(),
        auto_indent_mode_setup_map_rebuilds_exact_configured_bindings(),
    ]
}

pub(super) fn registry_auto_indent_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_indent_mode_generated_autoloads_expose_commands_without_loading_runtime()]
}
