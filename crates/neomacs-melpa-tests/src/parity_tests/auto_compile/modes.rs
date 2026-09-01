use expect_test::expect;

use super::ParityBatchCase;

fn auto_compile_local_mode_adds_and_removes_only_its_buffer_local_save_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_local_mode_adds_and_removes_only_its_buffer_local_save_hook",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (let (baseline after-enable after-disable)
           (setq baseline
                 (list
                  auto-compile-mode
                  (local-variable-p 'after-save-hook)
                  (memq #'auto-compile-byte-compile
                        after-save-hook)))
           (auto-compile-mode 1)
           (setq after-enable
                 (list
                  auto-compile-mode
                  (local-variable-p 'after-save-hook)
                  (and
                   (memq #'auto-compile-byte-compile
                         after-save-hook)
                   t)
                  (length
                   (seq-filter
                    (lambda (fn)
                      (eq fn
                          #'auto-compile-byte-compile))
                    after-save-hook))))
           (auto-compile-mode -1)
           (setq after-disable
                 (list
                  auto-compile-mode
                  (local-variable-p 'after-save-hook)
                  (memq #'auto-compile-byte-compile
                        after-save-hook)))
           (list baseline after-enable after-disable)))"##,
        expect!["OK ((nil nil nil) (t t t 1) (nil nil nil))"],
    )
}

fn auto_compile_local_mode_rejects_non_elisp_buffers_and_rolls_back_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_local_mode_rejects_non_elisp_buffers_and_rolls_back_state",
        r##"(with-temp-buffer
         (fundamental-mode)
         (list
          major-mode
          (auto-compile-test-error
           (lambda ()
             (auto-compile-mode 1)))
          auto-compile-mode
          (and
           (memq #'auto-compile-byte-compile
                 after-save-hook)
           t)))"##,
        expect![[
            r#"OK (fundamental-mode (:signal user-error ("‘auto-compile-mode’ only makes sense in ‘emacs-lisp-mode’")) nil nil)"#
        ]],
    )
}

fn auto_compile_global_save_mode_updates_existing_eligible_buffers_and_cleans_up() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_global_save_mode_updates_existing_eligible_buffers_and_cleans_up",
        r##"(let ((elisp-buffer
                (generate-new-buffer
                 " *auto-compile-elisp*"))
               (plain-buffer
                (generate-new-buffer
                 " *auto-compile-plain*")))
         (unwind-protect
             (progn
               (with-current-buffer elisp-buffer
                 (emacs-lisp-mode))
               (with-current-buffer plain-buffer
                 (fundamental-mode))
               (auto-compile-on-save-mode 1)
               (let ((enabled
                      (mapcar
                       (lambda (buffer)
                         (with-current-buffer buffer
                           (list
                            major-mode
                            auto-compile-mode
                            (and
                             (memq
                              #'auto-compile-byte-compile
                              after-save-hook)
                             t))))
                       (list elisp-buffer plain-buffer))))
                 (auto-compile-on-save-mode -1)
                 (list
                  enabled
                  auto-compile-on-save-mode
                  (with-current-buffer elisp-buffer
                    (list
                     auto-compile-mode
                     (memq
                      #'auto-compile-byte-compile
                      after-save-hook))))))
           (auto-compile-on-save-mode -1)
           (kill-buffer elisp-buffer)
           (kill-buffer plain-buffer)))"##,
        expect!["OK (((emacs-lisp-mode t t) (fundamental-mode nil nil)) nil (nil nil))"],
    )
}

fn auto_compile_global_turn_on_requires_exact_emacs_lisp_mode_not_merely_derived_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_global_turn_on_requires_exact_emacs_lisp_mode_not_merely_derived_mode",
        r##"(progn
         (define-derived-mode
           auto-compile-test-derived-mode
           emacs-lisp-mode
           "AutoCompileDerived")
         (list
          (with-temp-buffer
            (emacs-lisp-mode)
            (auto-compile-mode--turn-on)
            (list
             major-mode
             (derived-mode-p 'emacs-lisp-mode)
             auto-compile-mode))
          (with-temp-buffer
            (auto-compile-test-derived-mode)
            (auto-compile-mode--turn-on)
            (list
             major-mode
             (derived-mode-p 'emacs-lisp-mode)
             auto-compile-mode))))"##,
        expect![
            "OK ((emacs-lisp-mode emacs-lisp-mode t) (auto-compile-test-derived-mode emacs-lisp-mode nil))"
        ],
    )
}

fn auto_compile_failed_modified_toggle_changes_option_and_reports_both_transitions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_failed_modified_toggle_changes_option_and_reports_both_transitions",
        r##"(let ((auto-compile-mark-failed-modified nil))
         (auto-compile-toggle-mark-failed-modified)
         (let ((enabled
                (list
                 auto-compile-mark-failed-modified
                 (current-message))))
           (auto-compile-toggle-mark-failed-modified)
           (list
            enabled
            (list
             auto-compile-mark-failed-modified
             (current-message)))))"##,
        expect!["OK ((t nil) (nil nil))"],
    )
}

fn auto_compile_custom_mode_line_setter_repositions_and_removes_control() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_custom_mode_line_setter_repositions_and_removes_control",
        r##"(let ((original-format
                (default-value 'mode-line-format))
               (original-option
                (default-value
                 'auto-compile-use-mode-line)))
         (unwind-protect
             (progn
               (set-default
                'mode-line-format
                '(mode-line-front-space
                  mode-line-modified
                  mode-line-buffer-identification))
               (customize-set-variable
                'auto-compile-use-mode-line
                'mode-line-modified)
               (let ((inserted
                      (copy-tree
                       (default-value
                        'mode-line-format))))
                 (customize-set-variable
                  'auto-compile-use-mode-line
                  nil)
                 (list
                  inserted
                  (default-value 'mode-line-format)
                  (default-value
                   'auto-compile-use-mode-line))))
           (set-default 'mode-line-format original-format)
           (set-default
            'auto-compile-use-mode-line
            original-option)))"##,
        expect![
            "OK ((mode-line-front-space mode-line-modified mode-line-auto-compile mode-line-buffer-identification) (mode-line-front-space mode-line-modified mode-line-buffer-identification) nil)"
        ],
    )
}

fn auto_compile_on_load_mode_has_stable_global_toggle_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_on_load_mode_has_stable_global_toggle_lifecycle",
        r##"(progn
         (auto-compile-on-load-mode -1)
         (let ((disabled
                (list
                 auto-compile-on-load-mode
                 (default-value
                  'auto-compile-on-load-mode)
                 (and
                  (advice-member-p
                   'load@auto-compile
                   'load)
                  t)
                 (and
                  (advice-member-p
                   'require@auto-compile
                   'require)
                  t))))
           (auto-compile-on-load-mode 1)
           (let ((enabled
                  (list
                   auto-compile-on-load-mode
                   (default-value
                    'auto-compile-on-load-mode)
                   auto-compile-on-load-mode-lighter)))
             (auto-compile-on-load-mode -1)
             (list disabled enabled
                   auto-compile-on-load-mode))))"##,
        expect![[r#"OK ((nil nil t t) (t t "") nil)"#]],
    )
}

fn auto_compile_ding_obeys_option_without_leaking_terminal_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_ding_obeys_option_without_leaking_terminal_side_effects",
        r##"(let ((count 0))
         (cl-letf (((symbol-function 'ding)
                    (lambda (&rest _)
                      (setq count (1+ count))
                      'rang)))
           (let ((auto-compile-ding nil))
             (auto-compile-ding))
           (let ((auto-compile-ding t))
             (auto-compile-ding)
             (auto-compile-ding))
           count))"##,
        expect!["OK 2"],
    )
}

fn auto_compile_display_log_signals_when_absent_and_selects_existing_compile_log() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_display_log_signals_when_absent_and_selects_existing_compile_log",
        r##"(let ((existing
                (get-buffer
                 byte-compile-log-buffer)))
         (when existing
           (kill-buffer existing))
         (let ((absent
                (auto-compile-test-error
                 #'auto-compile-display-log))
               (buffer
                (get-buffer-create
                 byte-compile-log-buffer)))
           (with-current-buffer buffer
             (insert "warning one\nwarning two\n"))
           (unwind-protect
               (list
                absent
                (buffer-name
                 (auto-compile-display-log))
                (buffer-name (current-buffer))
                (with-current-buffer buffer
                  (buffer-string)))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((:signal user-error ("Buffer *Compile-Log* doesn’t exist")) "*Compile-Log*" "*Compile-Log*" "warning one\nwarning two\n")"#
        ]],
    )
}

pub(super) fn modes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_compile_local_mode_adds_and_removes_only_its_buffer_local_save_hook(),
        auto_compile_local_mode_rejects_non_elisp_buffers_and_rolls_back_state(),
        auto_compile_global_save_mode_updates_existing_eligible_buffers_and_cleans_up(),
        auto_compile_global_turn_on_requires_exact_emacs_lisp_mode_not_merely_derived_mode(),
        auto_compile_failed_modified_toggle_changes_option_and_reports_both_transitions(),
        auto_compile_custom_mode_line_setter_repositions_and_removes_control(),
        auto_compile_on_load_mode_has_stable_global_toggle_lifecycle(),
        auto_compile_ding_obeys_option_without_leaking_terminal_side_effects(),
        auto_compile_display_log_signals_when_absent_and_selects_existing_compile_log(),
    ]
}
