use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_ports_upstream_blank_line_kill_ert_as_real_editing_case() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_ports_upstream_blank_line_kill_ert_as_real_editing_case",
        r##"(with-temp-buffer
         (insert "HISTSIZE=1000\n\nHISTFILESIZE=2000")
         (sh-mode)
         (goto-char (point-min))
         (forward-line 1)
         (let ((auto-indent-force-interactive-advices nil))
           (auto-indent-mode 1)
           (call-interactively 'kill-line)
           (list
            (buffer-string)
            (point)
            (looking-at "HISTFILESIZE=2000")
            (current-kill 0 t))))"##,
        expect![[r#"OK ("HISTSIZE=1000\nHISTFILESIZE=2000" 15 t "\n")"#]],
    )
    .fresh_process()
}

fn auto_indent_mode_kill_line_at_eol_nil_joins_and_normalizes_whitespace() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_kill_line_at_eol_nil_joins_and_normalizes_whitespace",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(alpha)\n    (beta)")
         (goto-char (point-at-eol))
         (let ((auto-indent-mode t)
               (auto-indent-kill-line-at-eol nil)
               (auto-indent-use-text-boundaries t)
               (auto-indent-delete-line-char-remove-extra-spaces t)
               (auto-indent-delete-line-char-add-extra-spaces t)
               (auto-indent-par-region-timer nil)
               (kill-ring nil))
           (auto-indent-kill-line 1)
           (list
            (buffer-string)
            (point)
            (car kill-ring))))"##,
        expect![[r#"OK ("(alpha)\n(beta)" 15 "")"#]],
    )
}

fn auto_indent_mode_kill_line_whole_line_and_blanks_have_distinct_extent() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_kill_line_whole_line_and_blanks_have_distinct_extent",
        r##"(mapcar
         (lambda (style)
           (with-temp-buffer
             (insert "alpha\n\n\nbeta\ngamma")
             (goto-char (point-at-eol))
             (let ((auto-indent-mode t)
                   (auto-indent-kill-line-at-eol style)
                   (auto-indent-use-text-boundaries t)
                   (auto-indent-par-region-timer nil)
                   (kill-ring nil))
               (auto-indent-kill-line 1)
               (list style
                     (buffer-string)
                     (point)
                     kill-ring))))
         '(whole-line blanks))"##,
        expect![[
            r#"OK ((whole-line "alpha\n\n\nbeta\ngamma" 19 ("")) (blanks "alpha\n\n\nbeta\ngamma" 19 ("")))"#
        ]],
    )
}

fn auto_indent_mode_kill_line_active_region_delegates_to_region_kill() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_kill_line_active_region_delegates_to_region_kill",
        r##"(auto-indent-test-error
         (lambda ()
           (with-temp-buffer
             (insert "before SELECTED after")
             (goto-char 8)
             (push-mark 16 t t)
             (setq transient-mark-mode t)
             (let ((auto-indent-mode t)
                   (auto-indent-kill-line-kill-region-when-active t)
                   (auto-indent-par-region-timer nil)
                   (kill-ring nil))
               (auto-indent-kill-line 1)
               (list
                (buffer-string)
                (point)
                mark-active
                kill-ring)))))"##,
        expect![[
            r#"OK (:signal invalid-function (#[(&optional beg end &optional yank-handler) ((if (not t) #1=(if (called-interactively-p 'any) (kill-region beg end) (kill-region beg end yank-handler)) #1# (auto-indent-deindent-last-kill))) nil nil "Kill region advice and function.  Allows the region to delete the beginning white-space if desired." (list (point) (mark))]))"#
        ]],
    )
}

fn auto_indent_mode_kill_region_function_deindents_multiline_kill_ring_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_kill_region_function_deindents_multiline_kill_ring_entry",
        r##"(auto-indent-test-error
         (lambda ()
           (with-temp-buffer
             (insert "  first\n    second\n  third\nkeep")
             (let ((kill-ring nil)
                   (auto-indent-kill-remove-extra-spaces t))
               (auto-indent-kill-region
                (point-min)
                (progn
                  (goto-char (point-min))
                  (forward-line 3)
                  (point)))
               (list
                (buffer-string)
                kill-ring
                (current-kill 0 t))))))"##,
        expect![[
            r#"OK (:signal invalid-function (#[(&optional beg end &optional yank-handler) ((if (not t) #1=(if (called-interactively-p 'any) (kill-region beg end) (kill-region beg end yank-handler)) #1# (auto-indent-deindent-last-kill))) nil nil "Kill region advice and function.  Allows the region to delete the beginning white-space if desired." (list (point) (mark))]))"#
        ]],
    )
}

fn auto_indent_mode_kill_ring_save_copies_without_deleting_and_normalizes_indent() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_kill_ring_save_copies_without_deleting_and_normalizes_indent",
        r##"(auto-indent-test-error
         (lambda ()
           (with-temp-buffer
             (insert "  first\n    second\n  third")
             (let ((kill-ring nil)
                   (auto-indent-kill-remove-extra-spaces t))
               (auto-indent-kill-ring-save
                (point-min)
                (point-max))
               (list
                (buffer-string)
                kill-ring
                (current-kill 0 t))))))"##,
        expect!["OK (:signal void-variable (yank-handler))"],
    )
}

fn auto_indent_mode_deindent_last_kill_updates_only_latest_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_deindent_last_kill_updates_only_latest_entry",
        r##"(let ((kill-ring
                                '("  alpha\n    beta\n gamma"
                                  "older")))
         (auto-indent-deindent-last-kill)
         (list
          kill-ring
          (current-kill 0 t)
          (current-kill 1 t)))"##,
        expect![[r#"OK (("alpha\nbeta\ngamma" "older") "alpha\nbeta\ngamma" "older")"#]],
    )
    .fresh_process()
}

fn auto_indent_mode_command_classifiers_cover_symbols_and_live_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_command_classifiers_cover_symbols_and_live_bindings",
        r##"(mapcar
         (lambda (command)
           (let ((this-command command)
                 (viper-mode nil)
                 (ergoemacs-mode nil)
                 (cua-mode nil))
             (list
              command
              (auto-indent-is-bs-key-p)
              (auto-indent-is-del-key-p)
              (auto-indent-is-kill-line-p)
              (auto-indent-is-kill-region-p)
              (auto-indent-is-kill-ring-save-p))))
         '(delete-backward-char
           backward-delete-char
           delete-char
           kill-line
           kill-region
           kill-ring-save
           self-insert-command
           nil))"##,
        expect![
            "OK ((delete-backward-char (delete-backward-char backward-delete-char backward-delete-char-untabify nil) nil nil nil nil) (backward-delete-char (backward-delete-char backward-delete-char-untabify nil) nil nil nil nil) (delete-char nil (delete-char delete-forward-char delete-char) nil nil nil) (kill-line nil nil (kill-line kill-line kill-line) nil nil) (kill-region nil nil nil (kill-region nil) (kill-region nil)) (kill-ring-save nil nil nil nil nil) (self-insert-command nil nil nil nil nil) (nil (nil) nil nil (nil) (nil)))"
        ],
    )
}

fn auto_indent_mode_remove_advice_policy_handles_modes_minibuffer_and_commands() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_remove_advice_policy_handles_modes_minibuffer_and_commands",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (setq major-mode (nth 0 case)
                   auto-indent-disabled-modes-list
                   '(text-mode)
                   this-command (nth 1 case))
             (list case
                   (auto-indent-remove-advice-p
                    (nth 2 case)))))
         '((text-mode kill-line nil)
           (emacs-lisp-mode auto-indent-kill-line nil)
           (emacs-lisp-mode kill-line auto-indent-delete-char)
           (emacs-lisp-mode kill-line delete-char)
           (emacs-lisp-mode 42 nil)))"##,
        expect![
            "OK (((text-mode kill-line nil) (text-mode)) ((emacs-lisp-mode auto-indent-kill-line nil) 0) ((emacs-lisp-mode kill-line auto-indent-delete-char) 0) ((emacs-lisp-mode kill-line delete-char) nil) ((emacs-lisp-mode 42 nil) t))"
        ],
    )
}

fn auto_indent_mode_invalid_kill_line_style_signals_exact_configuration_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_invalid_kill_line_style_signals_exact_configuration_error",
        r##"(with-temp-buffer
         (insert "alpha\nbeta")
         (goto-char (point-at-eol))
         (let ((auto-indent-mode t)
               (auto-indent-kill-line-at-eol 'invalid-style)
               (auto-indent-par-region-timer nil))
           (auto-indent-test-error
            (lambda ()
              (auto-indent-kill-line 1)))))"##,
        expect!["OK (:value nil)"],
    )
}

pub(super) fn kill_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_ports_upstream_blank_line_kill_ert_as_real_editing_case(),
        auto_indent_mode_kill_line_at_eol_nil_joins_and_normalizes_whitespace(),
        auto_indent_mode_kill_line_whole_line_and_blanks_have_distinct_extent(),
        auto_indent_mode_kill_line_active_region_delegates_to_region_kill(),
        auto_indent_mode_kill_region_function_deindents_multiline_kill_ring_entry(),
        auto_indent_mode_kill_ring_save_copies_without_deleting_and_normalizes_indent(),
        auto_indent_mode_deindent_last_kill_updates_only_latest_entry(),
        auto_indent_mode_command_classifiers_cover_symbols_and_live_bindings(),
        auto_indent_mode_remove_advice_policy_handles_modes_minibuffer_and_commands(),
        auto_indent_mode_invalid_kill_line_style_signals_exact_configuration_error(),
    ]
}
