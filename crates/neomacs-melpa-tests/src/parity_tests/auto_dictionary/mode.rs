use expect_test::expect;

use super::ParityBatchCase;

fn auto_dictionary_mode_enable_disable_manages_lighter_timer_and_local_kill_hook() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_mode_enable_disable_manages_lighter_timer_and_local_kill_hook",
        r##"(with-temp-buffer
         (let ((scheduled nil)
               (cancelled nil)
               (ispell-dictionary "english")
               (adict-idle-time 3))
           (cl-letf
               (((symbol-function
                  'run-with-idle-timer)
                 (lambda (&rest args)
                   (setq scheduled
                         (append
                          (butlast args)
                          (list
                           (buffer-name
                            (car
                             (last args))))))
                   'test-timer))
                ((symbol-function
                  'cancel-timer)
                 (lambda (timer)
                   (push timer cancelled))))
             (auto-dictionary-mode 1)
             (let ((enabled
                    (list
                     auto-dictionary-mode
                     adict-lighter
                     adict-timer
                     scheduled
                     (and
                      (memq
                       #'adict--cancel-timer
                       kill-buffer-hook)
                      t)
                     adict-last-check)))
               (auto-dictionary-mode -1)
               (list
                enabled
                (list
                 auto-dictionary-mode
                 (local-variable-p
                  'adict-lighter)
                 (local-variable-p
                  'adict-timer)
                 (local-variable-p
                  'adict-last-check)
                 (memq
                  #'adict--cancel-timer
                  kill-buffer-hook)
                 (nreverse cancelled)))))))"##,
        expect![[
            r#"OK ((t " en" test-timer (3 t adict-guess-dictionary-maybe " *temp*") t :never) (nil nil nil nil nil (test-timer)))"#
        ]],
    )
}

fn auto_dictionary_mode_with_nil_idle_time_enables_without_scheduling() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_mode_with_nil_idle_time_enables_without_scheduling",
        r##"(with-temp-buffer
         (let ((adict-idle-time nil)
               (scheduled 0)
               (ispell-dictionary nil))
           (cl-letf
               (((symbol-function
                  'run-with-idle-timer)
                 (lambda (&rest _)
                   (setq scheduled
                         (1+ scheduled)))))
             (auto-dictionary-mode 1)
             (list
              auto-dictionary-mode
              adict-timer
              scheduled
              adict-lighter
              (and
               (memq
                #'adict--cancel-timer
                kill-buffer-hook)
               t)))))"##,
        expect![[r#"OK (t nil 0 " ??" t)"#]],
    )
}

fn auto_dictionary_mode_reuses_existing_buffer_timer_without_duplicate_schedule() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_mode_reuses_existing_buffer_timer_without_duplicate_schedule",
        r##"(with-temp-buffer
         (let ((adict-timer 'existing-timer)
               (scheduled 0)
               (cancelled nil))
           (cl-letf
               (((symbol-function
                  'run-with-idle-timer)
                 (lambda (&rest _)
                   (setq scheduled
                         (1+ scheduled))))
                ((symbol-function
                  'cancel-timer)
                 (lambda (timer)
                   (push timer cancelled))))
             (auto-dictionary-mode 1)
             (auto-dictionary-mode 1)
             (auto-dictionary-mode -1)
             (list
              scheduled
              (nreverse cancelled)
              (local-variable-p
               'adict-timer)))))"##,
        expect!["OK (0 (existing-timer) nil)"],
    )
}

fn auto_dictionary_cancel_timer_is_idempotent_and_kills_local_binding() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_cancel_timer_is_idempotent_and_kills_local_binding",
        r##"(with-temp-buffer
         (let ((cancelled nil))
           (setq-local adict-timer
                       'buffer-timer)
           (cl-letf
               (((symbol-function
                  'cancel-timer)
                 (lambda (timer)
                   (push timer cancelled))))
             (adict--cancel-timer)
             (adict--cancel-timer)
             (list
              (nreverse cancelled)
              (local-variable-p
               'adict-timer)
              adict-timer))))"##,
        expect!["OK ((buffer-timer) nil nil)"],
    )
}

fn auto_dictionary_lighter_shortens_long_names_and_preserves_short_codes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_lighter_shortens_long_names_and_preserves_short_codes",
        r##"(list
         (mapcar
          #'adict--shorten-dict
          '("en" "eng" "english"
            "de_DE" "" "日本語"))
         (with-temp-buffer
           (let ((ispell-local-dictionary
                  "american")
                 (ispell-dictionary "de"))
             (adict-update-lighter)
             adict-lighter))
         (with-temp-buffer
           (let ((ispell-local-dictionary nil)
                 (ispell-dictionary nil))
             (adict-update-lighter)
             adict-lighter)))"##,
        expect![[r#"OK (("en" "eng" "en" "de" "" "日本語") " am" " ??")"#]],
    )
}

fn auto_dictionary_next_guess_tick_uses_never_sentinel_size_and_fractional_threshold()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_next_guess_tick_uses_never_sentinel_size_and_fractional_threshold",
        r##"(list
         (with-temp-buffer
           (insert "1234567890")
           (let ((adict-last-check :never)
                 (adict-change-threshold 0.2))
             (adict--next-guess-tick)))
         (with-temp-buffer
           (insert "1234567890")
           (let ((adict-last-check 40)
                 (adict-change-threshold 0.2))
             (adict--next-guess-tick)))
         (with-temp-buffer
           (insert "12345")
           (let ((adict-last-check 7)
                 (adict-change-threshold 0.125))
             (adict--next-guess-tick))))"##,
        expect!["OK (0 42.0 7.625)"],
    )
}

fn auto_dictionary_timer_callback_requires_same_buffer_and_sufficient_modification()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_timer_callback_requires_same_buffer_and_sufficient_modification",
        r##"(let ((target
                (generate-new-buffer
                 " *adict-target*"))
               (other
                (generate-new-buffer
                 " *adict-other*"))
               (calls nil))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'adict-guess-dictionary)
                   (lambda (&optional idle)
                     (push idle calls)
                     'guessed)))
               (with-current-buffer target
                 (setq-local
                  adict-last-check
                  (buffer-modified-tick))
                 (let ((adict-change-threshold
                        0))
                   (adict-guess-dictionary-maybe
                    target)
                   (insert "changed enough")
                   (adict-guess-dictionary-maybe
                    target)
                   (adict-guess-dictionary-maybe
                    other)))
               (with-current-buffer other
                 (adict-guess-dictionary-maybe
                  target))
               (nreverse calls))
           (kill-buffer target)
           (kill-buffer other)))"##,
        expect!["OK (t)"],
    )
}

fn auto_dictionary_valid_manual_change_calls_ispell_hook_lighter_and_cancels_timer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_valid_manual_change_calls_ispell_hook_lighter_and_cancels_timer",
        r##"(with-temp-buffer
         (let* ((ispell-local-dictionary nil)
               (ispell-dictionary "en")
               (changes nil)
               (hooks nil)
               (cancelled nil)
               (adict-change-dictionary-hook
                (list
                 (lambda ()
                   (push
                    (list
                     'hook
                     ispell-local-dictionary)
                    hooks)))))
           (setq-local adict-timer
                       'active-timer)
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (push lang changes)
                   (setq
                    ispell-local-dictionary
                    lang)))
                ((symbol-function
                  'cancel-timer)
                 (lambda (timer)
                   (push timer cancelled))))
             (let ((result
                    (adict-change-dictionary
                     "de")))
               (list
                result
                changes
                (nreverse hooks)
                adict-lighter
                (nreverse cancelled)
                (local-variable-p
                 'adict-timer)
                (current-message))))))"##,
        expect![[r#"OK (adict-timer ("de") ((hook "de")) " de" (active-timer) nil nil)"#]],
    )
}

fn auto_dictionary_invalid_manual_change_signals_before_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_invalid_manual_change_signals_before_side_effects",
        r##"(with-temp-buffer
         (let ((changes nil)
               (hooks 0)
               (adict-test-valid-dictionaries
                '("en" "de"))
               (adict-change-dictionary-hook
                (list
                 (lambda ()
                   (setq hooks
                         (1+ hooks))))))
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (push lang changes))))
             (list
              (adict-test-error
               (lambda ()
                 (adict-change-dictionary
                  "missing")))
              changes
              hooks
              adict-lighter))))"##,
        expect![[r#"OK ((:signal error ("Dictionary \"missing\" not found")) nil 0 nil)"#]],
    )
}

fn auto_dictionary_nil_manual_change_delegates_interactively_then_updates_hook() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_nil_manual_change_delegates_interactively_then_updates_hook",
        r##"(with-temp-buffer
         (let* ((interactive-calls nil)
               (hook-calls 0)
               (ispell-local-dictionary nil)
               (adict-change-dictionary-hook
                (list
                 (lambda ()
                   (setq hook-calls
                         (1+ hook-calls))))))
           (cl-letf
               (((symbol-function
                  'call-interactively)
                 (lambda (command &rest _)
                   (push command
                         interactive-calls)
                   (setq
                    ispell-local-dictionary
                    "fr")
                   'chosen)))
             (list
              (adict-change-dictionary)
              (nreverse interactive-calls)
              hook-calls
              ispell-local-dictionary
              adict-lighter))))"##,
        expect![[r#"OK (nil (ispell-change-dictionary) 1 "fr" " fr")"#]],
    )
}

fn auto_dictionary_manual_change_can_keep_automatic_timer_when_policy_disabled() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_manual_change_can_keep_automatic_timer_when_policy_disabled",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en")
               (adict-stop-updating-on-dictionary-change
                nil)
               (cancelled 0))
           (setq-local adict-timer
                       'active)
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (setq
                    ispell-local-dictionary
                    lang)))
                ((symbol-function
                  'cancel-timer)
                 (lambda (_)
                   (setq cancelled
                         (1+ cancelled)))))
             (adict-change-dictionary "de")
             (list
              ispell-local-dictionary
              adict-timer
              cancelled
              adict-lighter))))"##,
        expect![[r#"OK ("de" active 0 " de")"#]],
    )
}

fn auto_dictionary_guess_changes_only_different_dictionary_and_updates_last_check()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_guess_changes_only_different_dictionary_and_updates_last_check",
        r##"(with-temp-buffer
         (insert
          "bonjour vous allez revoir cette nouvelle")
         (let ((ispell-local-dictionary "en")
               (ispell-dictionary "en")
               (changes nil)
               (adict-change-dictionary-hook nil))
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (push lang changes)
                   (setq
                    ispell-local-dictionary
                    lang))))
             (let ((before
                    (buffer-modified-tick))
                   (first
                    (adict-guess-dictionary)))
               (let ((after-first
                      adict-last-check)
                     (second
                      (adict-guess-dictionary)))
                 (list
                  first
                  (= after-first before)
                  second
                  (= adict-last-check
                     after-first)
                  (nreverse changes)
                  ispell-local-dictionary
                  adict-lighter))))))"##,
        expect![[r#"OK ("fr" t "fr" t ("fr") "fr" " fr")"#]],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dictionary_mode_enable_disable_manages_lighter_timer_and_local_kill_hook(),
        auto_dictionary_mode_with_nil_idle_time_enables_without_scheduling(),
        auto_dictionary_mode_reuses_existing_buffer_timer_without_duplicate_schedule(),
        auto_dictionary_cancel_timer_is_idempotent_and_kills_local_binding(),
        auto_dictionary_lighter_shortens_long_names_and_preserves_short_codes(),
        auto_dictionary_next_guess_tick_uses_never_sentinel_size_and_fractional_threshold(),
        auto_dictionary_timer_callback_requires_same_buffer_and_sufficient_modification(),
        auto_dictionary_valid_manual_change_calls_ispell_hook_lighter_and_cancels_timer(),
        auto_dictionary_invalid_manual_change_signals_before_side_effects(),
        auto_dictionary_nil_manual_change_delegates_interactively_then_updates_hook(),
        auto_dictionary_manual_change_can_keep_automatic_timer_when_policy_disabled(),
        auto_dictionary_guess_changes_only_different_dictionary_and_updates_last_check(),
    ]
}
