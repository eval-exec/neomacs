use expect_test::expect;

use super::ParityBatchCase;

fn auto_dictionary_idle_timer_drives_real_detection_and_dictionary_switch() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_idle_timer_drives_real_detection_and_dictionary_switch",
        r##"(with-temp-buffer
         (let ((scheduled nil)
               (ispell-local-dictionary "de")
               (ispell-dictionary "de")
               (changes nil)
               (adict-change-dictionary-hook
                nil)
               (adict-idle-time 0.25))
           (cl-letf
               (((symbol-function
                  'run-with-idle-timer)
                 (lambda (seconds repeat
                                  function buffer)
                   (setq scheduled
                         (list
                          seconds repeat function
                          buffer))
                   'idle-timer))
                ((symbol-function
                  'cancel-timer)
                 (lambda (_timer) nil))
                ((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (push lang changes)
                   (setq
                    ispell-local-dictionary
                    lang))))
             (auto-dictionary-mode 1)
             (insert
              "hello dear friend you are welcome and we have news")
             (funcall
              (nth 2 scheduled)
              (nth 3 scheduled))
             (list
              auto-dictionary-mode
              (butlast scheduled)
              (nreverse changes)
              ispell-local-dictionary
              adict-lighter
              (numberp adict-last-check)
              adict-timer))))"##,
        expect![[r#"OK (t (0.25 t adict-guess-dictionary-maybe) ("en") "en" " en" t idle-timer)"#]],
    )
}

fn auto_dictionary_automatic_switch_updates_localized_text_without_cancelling_timer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_automatic_switch_updates_localized_text_without_cancelling_timer",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en")
               (changes nil)
               (cancelled nil)
               (adict-change-dictionary-hook
                nil))
           (setq-local adict-timer 'periodic)
           (insert "Alice ")
           (adict-conditional-insert
            "en" "writes"
            "de" "schreibt")
           (insert
            "\n\nzunächst zwei deutsche Wörter sind dafür und nicht englisch")
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
             (let ((detected
                    (adict-guess-dictionary)))
               (list
                detected
                ispell-local-dictionary
                (buffer-string)
                (nreverse changes)
                cancelled
                adict-timer
                adict-lighter
                (length
                 adict-conditional-overlay-list))))))"##,
        expect![[
            r#"OK ("de" "de" "Alice schreibt\n\nzunächst zwei deutsche Wörter sind dafür und nicht englisch" ("de") nil periodic " de" 1)"#
        ]],
    )
}

fn auto_dictionary_guess_same_effective_dictionary_updates_tick_without_hook_or_ispell_call()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_guess_same_effective_dictionary_updates_tick_without_hook_or_ispell_call",
        r##"(with-temp-buffer
         (insert
          "hello dear friend you are welcome and we have news")
         (let ((ispell-local-dictionary "en")
               (ispell-dictionary "de")
               (changes 0)
               (hooks 0)
               (adict-change-dictionary-hook
                (list
                 (lambda ()
                   (setq hooks
                         (1+ hooks))))))
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (_lang)
                   (setq changes
                         (1+ changes)))))
             (let ((before
                    (buffer-modified-tick))
                   (detected
                    (adict-guess-dictionary)))
               (list
                detected
                (= adict-last-check before)
                changes
                hooks
                ispell-local-dictionary
                ispell-dictionary)))))"##,
        expect![[r#"OK ("en" t 0 0 "en" "de")"#]],
    )
}

fn auto_dictionary_idle_abort_preserves_dictionary_last_check_and_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_idle_abort_preserves_dictionary_last_check_and_hooks",
        r##"(with-temp-buffer
         (insert
          "bonjour vous allez revoir cette nouvelle")
         (let ((ispell-local-dictionary "en")
               (adict-last-check :never)
               (changes 0)
               (hooks 0)
               (adict-change-dictionary-hook
                (list
                 (lambda ()
                   (setq hooks
                         (1+ hooks))))))
           (cl-letf
               (((symbol-function
                  'input-pending-p)
                 (lambda () t))
                ((symbol-function
                  'ispell-change-dictionary)
                 (lambda (_lang)
                   (setq changes
                         (1+ changes)))))
             (list
              (adict-guess-dictionary t)
              ispell-local-dictionary
              adict-last-check
              changes
              hooks))))"##,
        expect![[r#"OK (nil "en" :never 0 0)"#]],
    )
}

fn auto_dictionary_threshold_callback_waits_then_switches_after_practical_edit() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_threshold_callback_waits_then_switches_after_practical_edit",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en")
               (changes nil)
               (adict-change-threshold 0.05)
               (adict-change-dictionary-hook
                nil))
           (insert
            "hello dear friend you are welcome")
           (setq adict-last-check
                 (buffer-modified-tick))
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (push lang changes)
                   (setq
                    ispell-local-dictionary
                    lang))))
             (adict-guess-dictionary-maybe
              (current-buffer))
             (let ((before-edit
                    (copy-sequence changes)))
               (insert
                "\nbonjour vous allez revoir cette nouvelle française")
               (adict-guess-dictionary-maybe
                (current-buffer))
               (list
                before-edit
                (nreverse changes)
                ispell-local-dictionary
                (numberp adict-last-check))))))"##,
        expect![[r#"OK (nil ("fr") "fr" t)"#]],
    )
}

fn auto_dictionary_program_mode_predicate_scores_only_comment_words() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_program_mode_predicate_scores_only_comment_words",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert
          "(defun bonjour () zunächst)\n"
          ";; hello dear friend you are welcome and we have news\n")
         (font-lock-ensure)
         (let ((flyspell-generic-check-word-p
                (lambda ()
                  (nth 4
                       (syntax-ppss)))))
           (list
            (append
             (adict-evaluate-buffer)
             nil)
            (adict-guess-buffer-language))))"##,
        expect![[r#"OK ((3 7 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0) "en")"#]],
    )
}

fn auto_dictionary_custom_region_mapping_returns_and_activates_configured_dictionary_name()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_custom_region_mapping_returns_and_activates_configured_dictionary_name",
        r##"(with-temp-buffer
         (insert
          "hello dear friend you are welcome and we have news")
         (let ((adict-dictionary-list
                '(("en" . "en_GB")
                  ("de" . "de_CH")))
               (adict-test-valid-dictionaries
                '("en_GB" "de_CH"))
               (ispell-local-dictionary nil)
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
             (list
              (adict--evaluate-buffer-find-lang
               nil)
              (adict--evaluate-buffer-find-dictionary
               nil)
              (adict-guess-dictionary)
              (nreverse changes)
              ispell-local-dictionary
              adict-lighter))))"##,
        expect![[r#"OK ("en" "en_GB" "en_GB" ("en_GB") "en_GB" " en")"#]],
    )
}

fn auto_dictionary_repeated_real_document_rewrites_switch_english_french_esperanto()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_repeated_real_document_rewrites_switch_english_french_esperanto",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "de")
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
             (insert
              "hello dear friend you are welcome and we have news")
             (let ((english
                    (adict-guess-dictionary)))
               (erase-buffer)
               (insert
                "bonjour vous allez revoir cette nouvelle française")
               (let ((french
                      (adict-guess-dictionary)))
                 (erase-buffer)
                 (insert
                  "morgaŭ kaj ĉiam ŝi estas ĉi tie ĉar ankaŭ eble")
                 (let ((esperanto
                        (adict-guess-dictionary)))
                   (list
                    english french esperanto
                    (nreverse changes)
                    ispell-local-dictionary
                    adict-lighter)))))))"##,
        expect![[r#"OK ("en" "fr" "eo" ("en" "fr" "eo") "eo" " eo")"#]],
    )
}

fn auto_dictionary_killing_enabled_buffer_cancels_exact_scheduled_timer_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_killing_enabled_buffer_cancels_exact_scheduled_timer_once",
        r##"(let ((buffer
                (generate-new-buffer
                 " *adict-kill-workflow*"))
               (cancelled nil))
         (cl-letf
             (((symbol-function
                'run-with-idle-timer)
               (lambda (&rest _)
                 'scheduled-timer))
              ((symbol-function
                'cancel-timer)
               (lambda (timer)
                 (push timer cancelled))))
           (with-current-buffer buffer
             (auto-dictionary-mode 1))
           (kill-buffer buffer)
           (list
            (buffer-live-p buffer)
            (nreverse cancelled))))"##,
        expect!["OK (nil (scheduled-timer))"],
    )
}

fn auto_dictionary_conditional_foreign_signature_does_not_overpower_authored_language()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_conditional_foreign_signature_does_not_overpower_authored_language",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "de"))
           (adict-conditional-insert
            "de"
            "bonjour vous allez revoir cette nouvelle française"
            "en"
            "hello dear friend")
           (insert
            "\n\nhello dear friend you are welcome and we have news")
           (list
            (buffer-string)
            (append
             (adict-evaluate-buffer)
             nil)
            (adict-guess-buffer-language)
            (length
             adict-conditional-overlay-list))))"##,
        expect![[
            r#"OK ("bonjour vous allez revoir cette nouvelle française\n\nhello dear friend you are welcome and we have news" (3 7 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0) "en" 1)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dictionary_idle_timer_drives_real_detection_and_dictionary_switch(),
        auto_dictionary_automatic_switch_updates_localized_text_without_cancelling_timer(),
        auto_dictionary_guess_same_effective_dictionary_updates_tick_without_hook_or_ispell_call(),
        auto_dictionary_idle_abort_preserves_dictionary_last_check_and_hooks(),
        auto_dictionary_threshold_callback_waits_then_switches_after_practical_edit(),
        auto_dictionary_program_mode_predicate_scores_only_comment_words(),
        auto_dictionary_custom_region_mapping_returns_and_activates_configured_dictionary_name(),
        auto_dictionary_repeated_real_document_rewrites_switch_english_french_esperanto(),
        auto_dictionary_killing_enabled_buffer_cancels_exact_scheduled_timer_once(),
        auto_dictionary_conditional_foreign_signature_does_not_overpower_authored_language(),
    ]
}
