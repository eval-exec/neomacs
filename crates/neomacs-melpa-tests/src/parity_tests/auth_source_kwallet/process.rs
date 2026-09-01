use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_kwallet_default_search_invokes_exact_cli_and_trims_secret() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_default_search_invokes_exact_cli_and_trims_secret",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (setq auth-source-kwallet-test-output
                                "  correct horse battery staple \n")
                          (list
                           (auth-source-kwallet--kwallet-search
                            :host
                            "mail.example"
                            :user
                            "alice"
                            :port
                            "imaps")
                           (nreverse
                            auth-source-kwallet-test-executable-calls)
                           (nreverse
                            auth-source-kwallet-test-process-calls)
                           (get-buffer "*kwallet-output*")))"##,
        expect![[
            r#"OK (((:user "alice" :secret "correct horse battery staple")) ("kwallet-query") (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@mail.example") t)) nil)"#
        ]],
    )
}

fn auth_source_kwallet_custom_wallet_folder_separator_and_executable_reach_cli_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_custom_wallet_folder_separator_and_executable_reach_cli_exactly",
        r##"(let ((auth-source-kwallet-wallet
                                "Engineering Wallet")
                               (auth-source-kwallet-folder
                                "Production Tokens")
                               (auth-source-kwallet-key-separator
                                "::")
                               (auth-source-kwallet-executable
                                "kwallet-query-v2"))
                           (auth-source-kwallet-test-reset-process)
                           (setq auth-source-kwallet-test-output
                                 "deploy-token")
                           (list
                            (auth-source-kwallet--kwallet-search
                             :host
                             "api.internal"
                             :user
                             "deploy")
                            (nreverse
                             auth-source-kwallet-test-executable-calls)
                            (nreverse
                             auth-source-kwallet-test-process-calls)))"##,
        expect![[
            r#"OK (((:user "deploy" :secret "deploy-token")) ("kwallet-query-v2") (("kwallet-query-v2" nil "*kwallet-output*" nil ("Engineering Wallet" "-f" "Production Tokens" "-r" "deploy::api.internal") t)))"#
        ]],
    )
}

fn auth_source_kwallet_secret_trimming_removes_all_edge_whitespace_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_secret_trimming_removes_all_edge_whitespace_only",
        r##"(mapcar
                          (lambda (output)
                            (auth-source-kwallet-test-reset-process)
                            (setq
                             auth-source-kwallet-test-output
                             output)
                            (list
                             output
                             (auth-source-kwallet--kwallet-search
                              :host
                              "trim.example"
                              :user
                              "trim-user")))
                          '("secret"
                            " secret "
                            "\nsecret\n"
                            "\t\r\n secret \f\v"
                            "  two words  "))"##,
        expect![[
            r#"OK (("secret" ((:user "trim-user" :secret "secret"))) (" secret " ((:user "trim-user" :secret "secret"))) ("\nsecret\n" ((:user "trim-user" :secret "secret"))) ("\11\15\n secret \f\13" ((:user "trim-user" :secret "secret \f\13"))) ("  two words  " ((:user "trim-user" :secret "two words"))))"#
        ]],
    )
}

fn auth_source_kwallet_multiline_and_unicode_secret_preserves_interior_content() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_kwallet_multiline_and_unicode_secret_preserves_interior_content",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (setq auth-source-kwallet-test-output
                                "\n  première ligne\n密碼 line\nlast\tfield  \n")
                          (auth-source-kwallet--kwallet-search
                           :host
                           "unicode.example"
                           :user
                           "δοκιμή"))"##,
        expect![[r#"OK ((:user "δοκιμή" :secret "première ligne\n密碼 line\nlast\11field"))"#]],
    )
}

fn auth_source_kwallet_empty_and_whitespace_only_outputs_are_successful_empty_secrets()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_empty_and_whitespace_only_outputs_are_successful_empty_secrets",
        r##"(mapcar
                          (lambda (output)
                            (auth-source-kwallet-test-reset-process)
                            (setq
                             auth-source-kwallet-test-output
                             output)
                            (list
                             output
                             (auth-source-kwallet--kwallet-search
                              :host
                              "empty.example"
                              :user
                              "service")))
                          '("" " " "\n\t\r"))"##,
        expect![[
            r#"OK (("" ((:user "service" :secret ""))) (" " ((:user "service" :secret ""))) ("\n\11\15" ((:user "service" :secret ""))))"#
        ]],
    )
}

fn auth_source_kwallet_every_nonzero_process_status_returns_nil_after_exact_call() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_kwallet_every_nonzero_process_status_returns_nil_after_exact_call",
        r##"(mapcar
                          (lambda (status)
                            (auth-source-kwallet-test-reset-process)
                            (setq
                             auth-source-kwallet-test-status
                             status
                             auth-source-kwallet-test-output
                             "ignored-output\n")
                            (list
                             status
                             (auth-source-kwallet--kwallet-search
                              :host
                              "failure.example"
                              :user
                              "service")
                             (nreverse
                              auth-source-kwallet-test-process-calls)
                             (get-buffer
                              "*kwallet-output*")))
                          '(1 2 7 126 127 255))"##,
        expect![[
            r#"OK ((1 nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "service@failure.example") t)) nil) (2 nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "service@failure.example") t)) nil) (7 nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "service@failure.example") t)) nil) (126 nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "service@failure.example") t)) nil) (127 nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "service@failure.example") t)) nil) (255 nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "service@failure.example") t)) nil))"#
        ]],
    )
}

fn auth_source_kwallet_missing_executable_surfaces_upstream_comma_form_failure_without_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_missing_executable_surfaces_upstream_comma_form_failure_without_process",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (setq
                           auth-source-kwallet-test-executable-found
                           nil)
                          (list
                           (auth-source-kwallet-test-error
                            (lambda ()
                              (auth-source-kwallet--kwallet-search
                               :host
                               "missing.example"
                               :user
                               "alice")))
                           (nreverse
                            auth-source-kwallet-test-executable-calls)
                           auth-source-kwallet-test-process-calls
                           (get-buffer
                            "*kwallet-output*")))"##,
        expect![[r#"OK ((:signal void-function (\,)) ("kwallet-query") nil nil)"#]],
    )
}

fn auth_source_kwallet_success_always_kills_generated_output_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_success_always_kills_generated_output_buffer",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (let ((before
                                 (buffer-list))
                                (result
                                 (auth-source-kwallet--kwallet-search
                                  :host
                                  "cleanup.example"
                                  :user
                                  "alice")))
                            (list
                             result
                             (get-buffer
                              "*kwallet-output*")
                             (seq-filter
                              (lambda (buffer)
                                (string-prefix-p
                                 "*kwallet-output*"
                                 (buffer-name buffer)))
                              (seq-difference
                               (buffer-list)
                               before)))))"##,
        expect![[r#"OK (((:user "alice" :secret "fixture-secret")) nil nil)"#]],
    )
}

fn auth_source_kwallet_failure_always_kills_generated_output_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_failure_always_kills_generated_output_buffer",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (setq
                           auth-source-kwallet-test-status
                           9)
                          (let ((before
                                 (buffer-list))
                                (result
                                 (auth-source-kwallet--kwallet-search
                                  :host
                                  "cleanup.example"
                                  :user
                                  "alice")))
                            (list
                             result
                             (get-buffer
                              "*kwallet-output*")
                             (seq-filter
                              (lambda (buffer)
                                (string-prefix-p
                                 "*kwallet-output*"
                                 (buffer-name buffer)))
                              (seq-difference
                               (buffer-list)
                               before)))))"##,
        expect!["OK (nil nil nil)"],
    )
}

fn auth_source_kwallet_process_signal_propagates_and_still_kills_generated_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_process_signal_propagates_and_still_kills_generated_buffer",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (setq
                           auth-source-kwallet-test-signal
                           '(file-error
                             "fixture process failed"
                             "/fixture/bin/kwallet-query"))
                          (list
                           (auth-source-kwallet-test-error
                            (lambda ()
                              (auth-source-kwallet--kwallet-search
                               :host
                               "signal.example"
                               :user
                               "alice")))
                           (nreverse
                            auth-source-kwallet-test-process-calls)
                           (get-buffer
                            "*kwallet-output*")))"##,
        expect![[
            r#"OK ((:signal file-error ("fixture process failed" "/fixture/bin/kwallet-query")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@signal.example") t)) nil)"#
        ]],
    )
}

fn auth_source_kwallet_preexisting_output_buffer_is_preserved_and_collision_buffer_is_cleaned()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_preexisting_output_buffer_is_preserved_and_collision_buffer_is_cleaned",
        r##"(let ((existing
                                (get-buffer-create
                                 "*kwallet-output*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer existing
                                   (insert "keep-me"))
                                 (auth-source-kwallet-test-reset-process)
                                 (list
                                  (auth-source-kwallet--kwallet-search
                                   :host
                                   "collision.example"
                                   :user
                                   "alice")
                                  (buffer-live-p existing)
                                  (with-current-buffer existing
                                    (buffer-string))
                                  (nreverse
                                   auth-source-kwallet-test-process-calls)
                                  (get-buffer
                                   "*kwallet-output*<2>")))
                             (when
                                 (buffer-live-p existing)
                               (kill-buffer existing))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "fixture-secret")) t "keep-me" (("kwallet-query" nil "*kwallet-output*<2>" nil ("Passwords" "-f" "Passwords" "-r" "alice@collision.example") t)) nil)"#
        ]],
    )
}

fn auth_source_kwallet_nil_user_and_host_form_separator_only_key_without_signaling()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_nil_user_and_host_form_separator_only_key_without_signaling",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (list
                           (auth-source-kwallet--kwallet-search)
                           (nreverse
                            auth-source-kwallet-test-process-calls)))"##,
        expect![[
            r#"OK (((:user nil :secret "fixture-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "@") t)))"#
        ]],
    )
}

fn auth_source_kwallet_nonstring_user_and_host_inputs_surface_concat_contract_and_cleanup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_nonstring_user_and_host_inputs_surface_concat_contract_and_cleanup",
        r##"(mapcar
                          (lambda (pair)
                            (auth-source-kwallet-test-reset-process)
                            (list
                             pair
                             (auth-source-kwallet-test-error
                              (lambda ()
                                (auth-source-kwallet--kwallet-search
                                 :user
                                 (car pair)
                                 :host
                                 (cadr pair))))
                             (nreverse
                              auth-source-kwallet-test-process-calls)
                             (get-buffer
                              "*kwallet-output*")))
                          '((alice "host.example")
                            ("alice" host.example)
                            (17 "host.example")
                            ("alice" 443)
                            (("alice") "host.example")
                            ("alice" ("host.example"))))"##,
        expect![[
            r#"OK (((alice "host.example") (:signal wrong-type-argument (sequencep alice)) nil nil) (("alice" host.example) (:signal wrong-type-argument (sequencep host.example)) nil nil) ((17 "host.example") (:signal wrong-type-argument (sequencep 17)) nil nil) (("alice" 443) (:signal wrong-type-argument (sequencep 443)) nil nil) ((("alice") "host.example") (:signal wrong-type-argument (characterp "alice")) nil nil) (("alice" ("host.example")) (:signal wrong-type-argument (characterp "host.example")) nil nil))"#
        ]],
    )
}

fn auth_source_kwallet_meta_and_unknown_search_keys_are_ignored_but_forwarded_key_is_stable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_meta_and_unknown_search_keys_are_ignored_but_forwarded_key_is_stable",
        r##"(progn
                          (auth-source-kwallet-test-reset-process)
                          (setq auth-source-kwallet-test-output
                                "meta-secret")
                          (list
                           (auth-source-kwallet--kwallet-search
                            :backend
                            :fixture-backend
                            :type
                            'kwallet
                            :host
                            "meta.example"
                            :user
                            "alice"
                            :port
                            443
                            :max
                            0
                            :require
                            '(:host :secret)
                            :create
                            t
                            :delete
                            t
                            :application
                            "deploy")
                           (nreverse
                            auth-source-kwallet-test-process-calls)))"##,
        expect![[
            r#"OK (((:user "alice" :secret "meta-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@meta.example") t)))"#
        ]],
    )
}

fn auth_source_kwallet_invalid_process_status_surfaces_zerop_type_error_after_cleanup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_invalid_process_status_surfaces_zerop_type_error_after_cleanup",
        r##"(mapcar
                          (lambda (status)
                            (auth-source-kwallet-test-reset-process)
                            (setq
                             auth-source-kwallet-test-status
                             status)
                            (list
                             status
                             (auth-source-kwallet-test-error
                              (lambda ()
                                (auth-source-kwallet--kwallet-search
                                 :host
                                 "status.example"
                                 :user
                                 "alice")))
                             (get-buffer
                              "*kwallet-output*")))
                          '(nil "finished" ok (0)))"##,
        expect![[
            r#"OK ((nil (:signal wrong-type-argument (number-or-marker-p nil)) nil) ("finished" (:signal wrong-type-argument (number-or-marker-p "finished")) nil) (ok (:signal wrong-type-argument (number-or-marker-p ok)) nil) (#1=(0) (:signal wrong-type-argument (number-or-marker-p #1#)) nil))"#
        ]],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_kwallet_default_search_invokes_exact_cli_and_trims_secret(),
        auth_source_kwallet_custom_wallet_folder_separator_and_executable_reach_cli_exactly(),
        auth_source_kwallet_secret_trimming_removes_all_edge_whitespace_only(),
        auth_source_kwallet_multiline_and_unicode_secret_preserves_interior_content(),
        auth_source_kwallet_empty_and_whitespace_only_outputs_are_successful_empty_secrets(),
        auth_source_kwallet_every_nonzero_process_status_returns_nil_after_exact_call(),
        auth_source_kwallet_missing_executable_surfaces_upstream_comma_form_failure_without_process(
        ),
        auth_source_kwallet_success_always_kills_generated_output_buffer(),
        auth_source_kwallet_failure_always_kills_generated_output_buffer(),
        auth_source_kwallet_process_signal_propagates_and_still_kills_generated_buffer(),
        auth_source_kwallet_preexisting_output_buffer_is_preserved_and_collision_buffer_is_cleaned(
        ),
        auth_source_kwallet_nil_user_and_host_form_separator_only_key_without_signaling(),
        auth_source_kwallet_nonstring_user_and_host_inputs_surface_concat_contract_and_cleanup(),
        auth_source_kwallet_meta_and_unknown_search_keys_are_ignored_but_forwarded_key_is_stable(),
        auth_source_kwallet_invalid_process_status_surfaces_zerop_type_error_after_cleanup(),
    ]
}
