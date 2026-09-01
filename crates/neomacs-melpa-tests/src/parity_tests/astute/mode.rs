use expect_test::expect;

use super::ParityBatchCase;

fn astute_mode_enable_installs_buffer_local_keywords_lighter_and_font_lock_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_mode_enable_installs_buffer_local_keywords_lighter_and_font_lock_state",
        r##"(with-temp-buffer
         (insert
          "\"quoted\" -- text")
         (text-mode)
         (let ((before
                (list
                 astute-mode
                 astute--keywords
                 (local-variable-p
                  'astute--keywords))))
           (astute-mode 1)
           (font-lock-ensure)
           (list
            before
            astute-mode
            (local-variable-p 'astute-mode)
            (local-variable-p
             'astute--keywords)
            (length astute--keywords)
            (assq
             'astute-mode
             minor-mode-alist)
            (and
             (memq
              astute--keywords
              font-lock-keywords)
             t)
            (astute-test-display-map))))"##,
        expect![[
            r#"OK ((nil nil nil) t t t 8 (astute-mode astute-lighter) t ((0 34 "“") (7 34 "”") (9 45 "–") (10 45 "–")))"#
        ]],
    )
}

fn astute_mode_disable_removes_typographic_displays_but_preserves_unrelated_display_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_mode_disable_removes_typographic_displays_but_preserves_unrelated_display_properties",
        r##"(with-temp-buffer
         (insert
          "x \"quoted\" -- tail")
         (text-mode)
         (add-text-properties
          18
          19
          '(display "★"
            astute-test-owner external))
         (set-buffer-modified-p nil)
         (astute-mode 1)
         (font-lock-ensure)
         (let ((enabled-map
                (astute-test-display-map))
               (enabled-keywords
                astute--keywords))
           (astute-mode -1)
           (list
            enabled-map
            astute-mode
            astute--keywords
            (memq
             enabled-keywords
             font-lock-keywords)
            (astute-test-display-map)
            (get-text-property
             18
             'astute-test-owner)
            (buffer-substring-no-properties
             (point-min)
             (point-max))
            (buffer-modified-p))))"##,
        expect![[
            r#"OK (((2 34 "“") (9 34 "”") (11 45 "–") (12 45 "–") (17 108 "★")) nil nil nil ((17 108 "★")) external "x \"quoted\" -- tail" nil)"#
        ]],
    )
}

fn astute_mode_disable_exposes_exact_cleanup_behavior_for_display_at_buffer_start()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_mode_disable_exposes_exact_cleanup_behavior_for_display_at_buffer_start",
        r##"(with-temp-buffer
         (insert
          "\"start\" and \"middle\" -- done")
         (text-mode)
         (astute-mode 1)
         (font-lock-ensure)
         (let ((before
                (astute-test-display-map)))
           (astute-mode -1)
           (list
            before
            (astute-test-display-map)
            (get-text-property
             (point-min)
             'display)
            (get-text-property
             (1-
              (point-max))
             'display))))"##,
        expect![[
            r#"OK (((0 34 "“") (6 34 "”") (12 34 "“") (19 34 "”") (21 45 "–") (22 45 "–")) ((0 34 "“")) "“" nil)"#
        ]],
    )
}

fn astute_mode_disable_widens_for_cleanup_then_restores_the_original_narrowing() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_mode_disable_widens_for_cleanup_then_restores_the_original_narrowing",
        r##"(with-temp-buffer
         (insert
          "\"outside\" -- left\n\"inside\" --- center\n'outside' -- right")
         (text-mode)
         (astute-mode 1)
         (font-lock-ensure)
         (let ((before
                (astute-test-display-map)))
           (goto-char 20)
           (narrow-to-region
            20
            39)
           (let ((narrowed
                  (list
                   (point-min)
                   (point-max))))
             (astute-mode -1)
             (list
              before
              narrowed
              (list
               (point-min)
               (point-max))
              (save-restriction
                (widen)
                (astute-test-display-map))
              (buffer-substring-no-properties
               (point-min)
               (point-max))))))"##,
        expect![[
            r#"OK (((0 34 "“") (8 34 "”") (10 45 "–") (11 45 "–") (18 34 "“") (25 34 "”") (27 45 "—") (28 45 "—") (29 45 "—") (38 39 "‘") (46 39 "’") (48 45 "–") (49 45 "–")) (20 39) (20 39) ((0 34 "“")) "inside\" --- center\n")"#
        ]],
    )
}

fn astute_mode_reenable_rebuilds_keywords_after_live_transform_configuration_change()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_mode_reenable_rebuilds_keywords_after_live_transform_configuration_change",
        r##"(with-temp-buffer
         (insert
          "'single' \"double\" a--b a---b")
         (text-mode)
         (setq-local
          astute-transform-list
          '(single-quote
            double-quote
            en-dash
            em-dash))
         (astute-mode 1)
         (font-lock-ensure)
         (let ((first-keywords
                astute--keywords)
               (first-map
                (astute-test-display-map)))
           (setq-local
            astute-transform-list
            '(em-dash))
           (astute-mode 1)
           (font-lock-ensure)
           (list
            (length first-keywords)
            first-map
            (length astute--keywords)
            (eq
             first-keywords
             astute--keywords)
            (memq
             first-keywords
             font-lock-keywords)
            (astute-test-display-map))))"##,
        expect![[
            r#"OK (8 ((0 39 "‘") (7 39 "’") (9 34 "“") (16 34 "”") (19 45 "–") (20 45 "–") (24 45 "—") (25 45 "—") (26 45 "—")) 1 nil nil ((0 39 "‘") (7 39 "’") (9 34 "“") (16 34 "”") (19 45 "–") (20 45 "–") (24 45 "—") (25 45 "—") (26 45 "—")))"#
        ]],
    )
}

fn astute_mode_configuration_and_keyword_state_are_isolated_between_live_buffers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_mode_configuration_and_keyword_state_are_isolated_between_live_buffers",
        r##"(let ((left
                (generate-new-buffer
                 " *astute-left*"))
               (right
                (generate-new-buffer
                 " *astute-right*")))
         (unwind-protect
             (progn
               (with-current-buffer left
                 (insert
                  "'left' \"left\" a--b")
                 (text-mode)
                 (setq-local
                  astute-transform-list
                  '(single-quote))
                 (astute-mode 1)
                 (font-lock-ensure))
               (with-current-buffer right
                 (insert
                  "'right' \"right\" a--b")
                 (text-mode)
                 (setq-local
                  astute-transform-list
                  '(double-quote
                    en-dash))
                 (astute-mode 1)
                 (font-lock-ensure))
               (list
                (with-current-buffer left
                  (list
                   astute-transform-list
                   (length astute--keywords)
                   (astute-test-display-map)))
                (with-current-buffer right
                  (list
                   astute-transform-list
                   (length astute--keywords)
                   (astute-test-display-map)))
                astute-transform-list
                astute--keywords))
           (kill-buffer left)
           (kill-buffer right)))"##,
        expect![[
            r#"OK (((single-quote) 4 ((0 39 "‘") (5 39 "’"))) ((double-quote en-dash) 3 ((8 34 "“") (14 34 "”") (17 45 "–") (18 45 "–"))) (single-quote double-quote en-dash em-dash) nil)"#
        ]],
    )
}

fn astute_mode_command_toggles_interactively_and_reuses_the_declared_lighter() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_mode_command_toggles_interactively_and_reuses_the_declared_lighter",
        r##"(with-temp-buffer
         (text-mode)
         (let ((command
                (commandp 'astute-mode))
               (interactive
                (interactive-form
                 'astute-mode))
               (lighter-entry
                (assq
                 'astute-mode
                 minor-mode-alist)))
           (call-interactively
            #'astute-mode)
           (let ((after-first
                  (list
                   astute-mode
                   (length astute--keywords))))
             (call-interactively
              #'astute-mode)
             (list
              command
              interactive
              lighter-entry
              astute-lighter
              after-first
              astute-mode
              astute--keywords))))"##,
        expect![[
            r#"OK (t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (astute-mode astute-lighter) " “As”" (t 8) nil nil)"#
        ]],
    )
}

fn astute_custom_lighter_value_is_observed_through_minor_mode_alist_without_redefinition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_custom_lighter_value_is_observed_through_minor_mode_alist_without_redefinition",
        r##"(with-temp-buffer
         (text-mode)
         (let ((astute-lighter
                " [Typography]"))
           (astute-mode 1)
           (list
            astute-lighter
            (assq
             'astute-mode
             minor-mode-alist)
            (eval
             (cadr
              (assq
               'astute-mode
               minor-mode-alist)))
            astute-mode)))"##,
        expect![[r#"OK (" [Typography]" (astute-mode astute-lighter) " [Typography]" t)"#]],
    )
}

fn astute_mode_disable_before_fontification_is_safe_and_clears_registered_keywords()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_mode_disable_before_fontification_is_safe_and_clears_registered_keywords",
        r##"(with-temp-buffer
         (insert
          "\"not yet fontified\" -- text")
         (text-mode)
         (astute-mode 1)
         (let ((registered
                astute--keywords))
           (astute-mode -1)
           (list
            (length registered)
            astute-mode
            astute--keywords
            (memq
             registered
             font-lock-keywords)
            (astute-test-display-map)
            (buffer-substring-no-properties
             (point-min)
             (point-max)))))"##,
        expect![[r#"OK (8 nil nil nil nil "\"not yet fontified\" -- text")"#]],
    )
}

fn astute_mode_hook_observes_completed_enable_and_disable_state_transitions() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_mode_hook_observes_completed_enable_and_disable_state_transitions",
        r##"(with-temp-buffer
         (text-mode)
         (let (events)
           (add-hook
            'astute-mode-hook
            (lambda ()
              (push
               (list
                astute-mode
                (and
                 astute--keywords
                 (length astute--keywords)))
               events))
            nil
            t)
           (astute-mode 1)
           (astute-mode -1)
           (list
            (nreverse events)
            astute-mode
            astute--keywords
            (local-variable-p
             'astute-mode-hook))))"##,
        expect!["OK (((t 8) (nil nil)) nil nil t)"],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        astute_mode_enable_installs_buffer_local_keywords_lighter_and_font_lock_state(),
        astute_mode_disable_removes_typographic_displays_but_preserves_unrelated_display_properties(
        ),
        astute_mode_disable_exposes_exact_cleanup_behavior_for_display_at_buffer_start(),
        astute_mode_disable_widens_for_cleanup_then_restores_the_original_narrowing(),
        astute_mode_reenable_rebuilds_keywords_after_live_transform_configuration_change(),
        astute_mode_configuration_and_keyword_state_are_isolated_between_live_buffers(),
        astute_mode_command_toggles_interactively_and_reuses_the_declared_lighter(),
        astute_custom_lighter_value_is_observed_through_minor_mode_alist_without_redefinition(),
        astute_mode_disable_before_fontification_is_safe_and_clears_registered_keywords(),
        astute_mode_hook_observes_completed_enable_and_disable_state_transitions(),
    ]
}
