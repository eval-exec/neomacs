use expect_test::expect;

use super::ParityBatchCase;

fn seven_fifty_words_public_command_default_and_interactive_specs_match_the_pin() -> ParityBatchCase
{
    ParityBatchCase::value(
        "seven_fifty_words_public_command_default_and_interactive_specs_match_the_pin",
        r##"(list
              750words-client-command
              (interactive-form
               '750words-credentials-setenv)
              (interactive-form
               '750words-region)
              (interactive-form
               '750words-buffer)
              (interactive-form
               '750words-region-or-buffer)
              (commandp
               '750words-credentials-setenv)
              (commandp '750words-file)
              (commandp '750words-region)
              (commandp '750words-buffer)
              (commandp
               '750words-region-or-buffer))"##,
        expect![[
            r#"OK ("750words-client.py %s" (interactive "P") (interactive "r") (interactive nil) (interactive nil) t nil t t t)"#
        ]],
    )
}

fn seven_fifty_words_credentials_searches_exact_host_fields_and_creation_prompts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "seven_fifty_words_credentials_searches_exact_host_fields_and_creation_prompts",
        r##"(let (observed)
               (cl-letf
                   (((symbol-function
                      'auth-source-search)
                     (lambda (&rest args)
                       (setq observed
                             (list
                              args
                              auth-source-creation-prompts))
                       (list
                        (list
                         :user "writer@example.test"
                         :secret "plain-secret"
                         :save-function
                         'save-entry)))))
                 (list
                  (750words-credentials t)
                  observed)))"##,
        expect![[
            r#"OK (("writer@example.test" "plain-secret" save-entry) ((:max 1 :host "750words.com" :require (:user :secret) :create t) ((user . "750words.com username: ") (secret . "750words.com password for %u: "))))"#
        ]],
    )
}

fn seven_fifty_words_credentials_calls_secret_thunk_once_and_preserves_save_function()
-> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_credentials_calls_secret_thunk_once_and_preserves_save_function",
        r##"(let (events)
               (cl-letf
                   (((symbol-function
                      'auth-source-search)
                     (lambda (&rest _)
                       (list
                        (list
                         :user "writer"
                         :secret
                         (lambda ()
                           (push 'secret events)
                           "resolved")
                         :save-function
                         (lambda ()
                           (push 'save events)))))))
                 (let ((credentials
                        (750words-credentials)))
                   (funcall
                    (nth 2 credentials))
                   (list
                    (nth 0 credentials)
                    (nth 1 credentials)
                    (functionp
                     (nth 2 credentials))
                    (nreverse events)))))"##,
        expect![[r#"OK ("writer" "resolved" t (secret save))"#]],
    )
}

fn seven_fifty_words_credentials_returns_nil_when_auth_source_finds_nothing() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_credentials_returns_nil_when_auth_source_finds_nothing",
        r##"(cl-letf
              (((symbol-function
                 'auth-source-search)
                (lambda (&rest _) nil)))
              (750words-credentials))"##,
        expect!["OK nil"],
    )
}

fn seven_fifty_words_credentials_setenv_forwards_save_sets_both_values_and_saves_last()
-> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_credentials_setenv_forwards_save_sets_both_values_and_saves_last",
        r##"(let ((process-environment
                    (copy-sequence
                     process-environment))
                   events)
               (setenv "USER_750WORDS" "old-user")
               (setenv "PASS_750WORDS" "old-pass")
               (cl-letf
                   (((symbol-function
                      '750words-credentials)
                     (lambda (save)
                       (push
                        (list 'credentials save)
                        events)
                       (list
                        "new-user"
                        "new-pass"
                        (lambda ()
                          (push
                           (list
                            'save
                            (getenv
                             "USER_750WORDS")
                            (getenv
                             "PASS_750WORDS"))
                           events)
                          'saved)))))
                 (list
                  (750words-credentials-setenv
                   '(4))
                  (getenv "USER_750WORDS")
                  (getenv "PASS_750WORDS")
                  (nreverse events))))"##,
        expect![[
            r#"OK (saved "new-user" "new-pass" ((credentials (4)) (save "new-user" "new-pass")))"#
        ]],
    )
}

fn seven_fifty_words_credentials_setenv_leaves_environment_unchanged_when_missing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_credentials_setenv_leaves_environment_unchanged_when_missing",
        r##"(let ((process-environment
                    (copy-sequence
                     process-environment))
                   observed)
               (setenv "USER_750WORDS" "kept-user")
               (setenv "PASS_750WORDS" "kept-pass")
               (cl-letf
                   (((symbol-function
                      '750words-credentials)
                     (lambda (save)
                       (setq observed save)
                       nil)))
                 (list
                  (750words-credentials-setenv)
                  observed
                  (getenv "USER_750WORDS")
                  (getenv
                   "PASS_750WORDS"))))"##,
        expect![[r#"OK (nil nil "kept-user" "kept-pass")"#]],
    )
}

fn seven_fifty_words_credentials_setenv_ignores_a_non_function_save_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "seven_fifty_words_credentials_setenv_ignores_a_non_function_save_value",
        r##"(let ((process-environment
                    (copy-sequence
                     process-environment)))
               (cl-letf
                   (((symbol-function
                      '750words-credentials)
                     (lambda (_)
                       '("user" "pass"
                         not-a-function))))
                 (list
                  (750words-credentials-setenv
                   t)
                  (getenv "USER_750WORDS")
                  (getenv
                   "PASS_750WORDS"))))"##,
        expect![[r#"OK (nil "user" "pass")"#]],
    )
}

pub(super) fn authentication_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        seven_fifty_words_public_command_default_and_interactive_specs_match_the_pin(),
        seven_fifty_words_credentials_searches_exact_host_fields_and_creation_prompts(),
        seven_fifty_words_credentials_calls_secret_thunk_once_and_preserves_save_function(),
        seven_fifty_words_credentials_returns_nil_when_auth_source_finds_nothing(),
        seven_fifty_words_credentials_setenv_forwards_save_sets_both_values_and_saves_last(),
        seven_fifty_words_credentials_setenv_leaves_environment_unchanged_when_missing(),
        seven_fifty_words_credentials_setenv_ignores_a_non_function_save_value(),
    ]
}
