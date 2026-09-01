use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_default_engine_installs_local_hooks_and_activates_advices() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_default_engine_installs_local_hooks_and_activates_advices",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (let ((auto-indent-on-save-file nil)
               (auto-indent-untabify-on-save-file nil)
               (auto-indent-engine nil))
           (auto-indent-mode 1)
           (list
            auto-indent-mode
            (memq 'auto-indent-mode-pre-command-hook
                  pre-command-hook)
            (memq 'auto-indent-mode-post-command-hook
                  post-command-hook)
            (memq 'auto-indent-mode-post-command-hook-last
                  post-command-hook)
            (memq 'auto-indent-mode-post-command-hook
                  after-save-hook)
            (mapcar
             #'auto-indent-test-advice-state
             '(delete-char kill-line kill-region
               backward-delete-char-untabify
               move-beginning-of-line)))))"##,
        expect![
            "OK (t (auto-indent-mode-pre-command-hook eldoc-pre-command-refresh-echo-area t) (auto-indent-mode-post-command-hook eldoc-schedule-timer t . #1=(auto-indent-mode-post-command-hook-last)) #1# (auto-indent-mode-post-command-hook t) ((delete-char t t) (kill-line t t) (kill-region t t) (backward-delete-char-untabify t t) (move-beginning-of-line t t)))"
        ],
    )
}

fn auto_indent_mode_disable_removes_primary_hooks_and_preserves_source_quirks() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_disable_removes_primary_hooks_and_preserves_source_quirks",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (let ((auto-indent-on-save-file t)
               (auto-indent-untabify-on-save-file t)
               (auto-indent-on-visit-file t)
               (auto-indent-engine nil))
           (auto-indent-mode 1)
           (let ((enabled
                  (list
                   (memq 'auto-indent-file-when-save
                         write-contents-hooks)
                   (memq 'auto-indent-file-when-visit
                         find-file-hook)
                   (memq 'auto-indent-mode-post-command-hook
                         post-command-hook)
                   (memq 'auto-indent-mode-post-command-hook-last
                         post-command-hook))))
             (auto-indent-mode -1)
             (list
              enabled
              auto-indent-mode
              (memq 'auto-indent-file-when-save
                    write-contents-hooks)
              (memq 'auto-indent-file-when-visit
                    find-file-hook)
              (memq 'auto-indent-mode-post-command-hook
                    post-command-hook)
              (memq 'auto-indent-mode-post-command-hook-last
                    post-command-hook)
              (memq 'auto-indent-mode-pre-command-hook
                    pre-command-hook)))))"##,
        expect![
            "OK ((#2=(auto-indent-file-when-save) (auto-indent-file-when-visit url-handlers-set-buffer-mode vc-refresh-state epa-file-find-file-hook) (auto-indent-mode-post-command-hook eldoc-schedule-timer t . #1=(auto-indent-mode-post-command-hook-last)) #1#) nil #2# nil nil #1# nil)"
        ],
    )
}

fn auto_indent_mode_keys_engine_remaps_editing_commands_without_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_keys_engine_remaps_editing_commands_without_hooks",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (let ((auto-indent-engine 'keys)
               (auto-indent-newline-function
                'newline-and-indent))
           (auto-indent-mode 1)
           (list
            auto-indent-mode
            (key-binding (kbd "RET"))
            (key-binding [remap delete-char])
            (key-binding [remap kill-line])
            (memq 'auto-indent-mode-pre-command-hook
                  pre-command-hook)
            (memq 'auto-indent-mode-post-command-hook
                  post-command-hook))
           ))"##,
        expect!["OK (t newline-and-indent auto-indent-delete-char auto-indent-kill-line nil nil)"],
    )
}

fn auto_indent_mode_on_respects_disabled_major_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_on_respects_disabled_major_modes",
        r##"(mapcar
         (lambda (mode)
           (with-temp-buffer
             (funcall mode)
             (let ((auto-indent-disabled-modes-list
                    '(text-mode fundamental-mode)))
               (auto-indent-mode -1)
               (auto-indent-mode-on)
               (list mode major-mode auto-indent-mode))))
         '(fundamental-mode text-mode emacs-lisp-mode))"##,
        expect![
            "OK ((fundamental-mode fundamental-mode nil) (text-mode text-mode nil) (emacs-lisp-mode emacs-lisp-mode t))"
        ],
    )
}

fn auto_indent_global_mode_enables_eligible_existing_buffers_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_global_mode_enables_eligible_existing_buffers_only",
        r##"(let ((code
                                (generate-new-buffer
                                 " *auto-indent-code*"))
             (text
              (generate-new-buffer
               " *auto-indent-text*"))
             (auto-indent-disabled-modes-list
              '(text-mode fundamental-mode)))
         (unwind-protect
             (progn
               (with-current-buffer code
                 (emacs-lisp-mode))
               (with-current-buffer text
                 (text-mode))
               (auto-indent-global-mode 1)
               (let ((enabled
                      (list
                       auto-indent-global-mode
                       (with-current-buffer code
                         auto-indent-mode)
                       (with-current-buffer text
                         auto-indent-mode))))
                 (auto-indent-global-mode -1)
                 (list
                  enabled
                  auto-indent-global-mode
                  (with-current-buffer code
                    auto-indent-mode)
                  (with-current-buffer text
                    auto-indent-mode))))
           (when auto-indent-global-mode
             (auto-indent-global-mode -1))
           (kill-buffer code)
           (kill-buffer text)))"##,
        expect!["OK ((t t nil) nil nil nil)"],
    )
}

fn auto_indent_mode_assigns_known_and_derived_indent_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_assigns_known_and_derived_indent_variables",
        r##"(with-temp-buffer
         (setq major-mode 'fixture-mode
               auto-indent-known-indent-level-variables
               '(fixture-offset shared-offset)
               auto-indent-assign-indent-level 6
               auto-indent-assign-indent-level-variables t)
         (set 'fixture-offset 2)
         (set 'shared-offset 4)
         (auto-indent-mode 1)
         (list
          fixture-offset
          shared-offset
          (symbol-value 'fixture-mode-indent-level)
          (local-variable-p 'auto-indent-mode)
          auto-indent-mode))"##,
        expect!["OK (6 6 6 t t)"],
    )
}

fn auto_indent_deactivate_advices_disables_every_available_editing_advice() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_deactivate_advices_disables_every_available_editing_advice",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (auto-indent-mode 1)
         (let ((before
                (mapcar
                 #'auto-indent-test-advice-state
                 '(delete-char kill-line kill-region
                   kill-ring-save
                   backward-delete-char-untabify
                   move-beginning-of-line))))
           (auto-indent-deactivate-advices)
           (list
            before
            (mapcar
             #'auto-indent-test-advice-state
             '(delete-char kill-line kill-region
               kill-ring-save
               backward-delete-char-untabify
               move-beginning-of-line)))))"##,
        expect![
            "OK (((delete-char t t) (kill-line t t) (kill-region t t) (kill-ring-save t t) (backward-delete-char-untabify t t) (move-beginning-of-line t t)) ((delete-char t nil) (kill-line t nil) (kill-region t nil) (kill-ring-save t nil) (backward-delete-char-untabify t nil) (move-beginning-of-line t nil)))"
        ],
    )
}

fn auto_indent_eol_newline_uses_alternate_return_at_physical_line_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_eol_newline_uses_alternate_return_at_physical_line_end",
        r##"(let (calls)
         (fset
          'auto-indent-test-return
          (lambda ()
            (interactive)
            (push (point) calls)
            (insert "\n<return>")))
         (with-temp-buffer
           (insert "first line\nsecond line")
           (goto-char 4)
           (let ((auto-indent-alternate-return-function-for-end-of-line-then-newline
                  'auto-indent-test-return))
             (auto-indent-eol-newline)
             (list
              (buffer-string)
              (point)
              (nreverse calls)))))"##,
        expect![[r#"OK ("first line\n<return>\nsecond line" 20 (11))"#]],
    )
}

fn auto_indent_eol_char_newline_inserts_configured_character_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_eol_char_newline_inserts_configured_character_once",
        r##"(let (calls)
         (fset
          'auto-indent-test-return
          (lambda ()
            (interactive)
            (push (point) calls)
            (insert "\n")))
         (mapcar
          (lambda (text)
            (with-temp-buffer
              (insert text)
              (goto-char (point-min))
              (let ((auto-indent-eol-char ":")
                    (auto-indent-alternate-return-function-for-end-of-line-then-newline
                     'auto-indent-test-return))
                (auto-indent-eol-char-newline)
                (list text
                      (buffer-string)
                      (point)))))
          '("statement" "statement;" "statement;   ")))"##,
        expect![[
            r#"OK (("statement" "statement:\n" 12) ("statement;" "statement;\n" 12) ("statement;   " "statement;   \n" 15))"#
        ]],
    )
}

fn auto_indent_disable_electric_mirrors_mode_state_and_calls_local_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_disable_electric_mirrors_mode_state_and_calls_local_mode",
        r##"(let (calls)
         (cl-letf (((symbol-function 'electric-indent-local-mode)
                    (lambda (argument)
                      (push argument calls))))
           (mapcar
            (lambda (enabled)
              (with-temp-buffer
                (setq auto-indent-mode enabled)
                (auto-indent-disable-electric)
                (list
                 enabled
                 electric-indent-inhibit
                 (nreverse calls))))
            '(nil t t nil))))"##,
        expect!["OK ((nil nil nil) (t t #1=(0 . #2=(0))) (t t #1#) (nil nil #2#))"],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_default_engine_installs_local_hooks_and_activates_advices(),
        auto_indent_mode_disable_removes_primary_hooks_and_preserves_source_quirks(),
        auto_indent_mode_keys_engine_remaps_editing_commands_without_hooks(),
        auto_indent_mode_on_respects_disabled_major_modes(),
        auto_indent_global_mode_enables_eligible_existing_buffers_only(),
        auto_indent_mode_assigns_known_and_derived_indent_variables(),
        auto_indent_deactivate_advices_disables_every_available_editing_advice(),
        auto_indent_eol_newline_uses_alternate_return_at_physical_line_end(),
        auto_indent_eol_char_newline_inserts_configured_character_once(),
        auto_indent_disable_electric_mirrors_mode_state_and_calls_local_mode(),
    ]
}
