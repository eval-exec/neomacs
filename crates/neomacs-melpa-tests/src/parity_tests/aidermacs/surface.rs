use expect_test::expect;

use super::ParityBatchCase;

fn aidermacs_exact_pin_features_version_and_core_defaults_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_exact_pin_features_version_and_core_defaults_match",
        r##"(list
                      (mapcar #'featurep
                              '(aidermacs aidermacs-backends
                                aidermacs-backend-comint
                                aidermacs-backend-vterm
                                aidermacs-models aidermacs-output))
                      (list
                       aidermacs-program
                       aidermacs-backend
                       aidermacs-default-model
                       aidermacs-default-chat-mode
                       aidermacs-auto-commits
                       aidermacs-watch-files
                       aidermacs-auto-accept-architect
                       aidermacs-show-diff-after-change
                       aidermacs-output-limit)
                      (get 'aidermacs-use-architect-mode 'obsolete-variable)
                      (help-function-arglist #'aidermacs-run t))"##,
        expect![[
            r#"OK ((t t t t t t) (("aider-ce" "aider") comint "sonnet" nil nil nil nil t 10) nil nil)"#
        ]],
    )
}

fn aidermacs_customization_types_and_practical_defaults_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_customization_types_and_practical_defaults_match",
        r##"(mapcar
                      (lambda (symbol)
                        (list symbol
                              (default-value symbol)
                              (get symbol 'custom-type)
                              (get symbol 'custom-group)))
                      '(aidermacs-program
                        aidermacs-backend
                        aidermacs-enable-notifications
                        aidermacs-notify-after-seconds
                        aidermacs-default-chat-mode
                        aidermacs-config-file
                        aidermacs-extra-args
                        aidermacs-global-read-only-files
                        aidermacs-project-read-only-files
                        aidermacs-subtree-only
                        aidermacs-auto-commits
                        aidermacs-watch-files
                        aidermacs-auto-accept-architect
                        aidermacs-exit-kills-buffer
                        aidermacs-output-limit
                        aidermacs-show-diff-after-change
                        aidermacs-default-model
                        aidermacs-architect-model
                        aidermacs-editor-model
                        aidermacs-weak-model
                        aidermacs-litellm-prices-cache-duration
                        aidermacs-comint-multiline-newline-key
                        aidermacs-vterm-multiline-newline-key
                        aidermacs-vterm-use-theme-colors))"##,
        expect![[
            r#"OK ((aidermacs-program ("aider-ce" "aider") (choice string (repeat :tag "Program fallbacks" string)) nil) (aidermacs-backend comint (choice (const :tag "Comint" comint) (const :tag "VTerm" vterm)) nil) (aidermacs-enable-notifications t boolean nil) (aidermacs-notify-after-seconds 120 integer nil) (aidermacs-default-chat-mode nil (choice (const :tag "Code (default)" nil) (const :tag "Code" code) (const :tag "Ask" ask) (const :tag "Architect" architect) (const :tag "Help" help)) nil) (aidermacs-config-file nil (choice (const :tag "None" nil) (file :tag "Config file")) nil) (aidermacs-extra-args nil (repeat string) nil) (aidermacs-global-read-only-files nil (repeat string) nil) (aidermacs-project-read-only-files nil (repeat string) nil) (aidermacs-subtree-only nil boolean nil) (aidermacs-auto-commits nil boolean nil) (aidermacs-watch-files nil boolean nil) (aidermacs-auto-accept-architect nil boolean nil) (aidermacs-exit-kills-buffer nil boolean nil) (aidermacs-output-limit 10 integer nil) (aidermacs-show-diff-after-change t boolean nil) (aidermacs-default-model "sonnet" string nil) (aidermacs-architect-model nil (choice (const :tag "Use default model" nil) (string :tag "Specific model")) nil) (aidermacs-editor-model nil (choice (const :tag "Use default model" nil) (string :tag "Specific model")) nil) (aidermacs-weak-model nil (choice (const :tag "Use default model" nil) (string :tag "Specific model")) nil) (aidermacs-litellm-prices-cache-duration 86400 integer nil) (aidermacs-comint-multiline-newline-key "S-<return>" string nil) (aidermacs-vterm-multiline-newline-key "S-<return>" string nil) (aidermacs-vterm-use-theme-colors t boolean nil))"#
        ]],
    )
}

fn aidermacs_modes_keymaps_markers_and_prompt_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_modes_keymaps_markers_and_prompt_contract_match",
        r##"(let ((keys
                           '("C-c C-n" "C-<return>" "C-c C-c"
                             "C-c C-z" "S-<return>" "RET" "<return>")))
                      (list
                       (mapcar
                        (lambda (key)
                          (list
                           key
                           (lookup-key aidermacs-minor-mode-map (kbd key))
                           (lookup-key aidermacs-comint-mode-map (kbd key))
                           (lookup-key aidermacs-vterm-mode-map (kbd key))))
                        keys)
                       aidermacs-search-marker
                       aidermacs-diff-marker
                       aidermacs-replace-marker
                       aidermacs-fence-marker
                       aidermacs-prompt-regexp
                       aidermacs-question-regexp
                       aidermacs-auto-mode-files
                       (with-temp-buffer
                         (aidermacs-file-diff-selection-mode)
                         (list major-mode mode-name buffer-read-only))
                       (with-temp-buffer
                         (aidermacs-comint-mode)
                         (list major-mode
                               comint-prompt-regexp
                               comint-input-sender
                               (memq #'aidermacs--comint-output-filter
                                     comint-output-filter-functions)))))"##,
        expect![[
            r#"OK ((("C-c C-n" aidermacs-send-line-or-region nil nil) ("C-<return>" aidermacs-send-line-or-region nil nil) ("C-c C-c" aidermacs-send-block-or-region aidermacs-comint-interrupt-subjob aidermacs-vterm-send-C-c) ("C-c C-z" aidermacs-switch-to-buffer nil nil) ("S-<return>" nil comint-accumulate aidermacs-vterm-insert-newline) ("RET" nil nil aidermacs-vterm-send-return) ("<return>" nil nil aidermacs-vterm-send-return)) "<<<<<<< SEARCH" "=======" ">>>>>>> REPLACE" "```" "^[^[:space:]<]*>[[:space:]]+$" "(Y)es/(N)o" (".aider.prompt.org" ".aider.chat.md" ".aider.chat.history.md" ".aider.input.history") (aidermacs-file-diff-selection-mode "Aider Diff Files" t) (aidermacs-comint-mode "^[^[:space:]<]*>[[:space:]]+$" aidermacs-input-sender nil))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn surface_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidermacs_exact_pin_features_version_and_core_defaults_match(),
        aidermacs_customization_types_and_practical_defaults_match(),
        aidermacs_modes_keymaps_markers_and_prompt_contract_match(),
    ]
}
