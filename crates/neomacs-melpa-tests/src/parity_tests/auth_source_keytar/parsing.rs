use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_keytar_read_password_extracts_real_keytar_credential_rendering() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_read_password_extracts_real_keytar_credential_rendering",
        r##"(mapcar
          #'auth-source-keytar--read-password
          '("{ account: 'alice', password: 'correct horse battery staple' }"
            "{ account: 'build-bot', password: 'token-123_ABC' }"
            "{ account: 'unicode', password: 'pässwörd-密钥' }"))"##,
        expect![[r#"OK ("correct horse battery staple" "token-123_ABC" "pässwörd-密钥")"#]],
    )
}

fn auth_source_keytar_read_password_trims_outer_whitespace_but_preserves_internal_content()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_read_password_trims_outer_whitespace_but_preserves_internal_content",
        r##"(mapcar
          #'auth-source-keytar--read-password
          '("password: '   surrounded   ' }"
            "prefix password: 'two  internal  spaces' } suffix"
            "\tpassword: '\tTabbed Secret\t' }\t"))"##,
        expect![[r#"OK ("surrounded" "two  internal  spaces suffix" "Tabbed Secret")"#]],
    )
}

fn auth_source_keytar_read_password_uses_first_marker_and_globally_removes_closing_fragment()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_read_password_uses_first_marker_and_globally_removes_closing_fragment",
        r##"(mapcar
          #'auth-source-keytar--read-password
          '("password: 'first' } password: 'second' }"
            "password: 'left' }middle' }right' }"
            "prefix password: 'one' }\npassword: 'two' }"))"##,
        expect![[r#"OK ("first" "leftmiddleright" "one")"#]],
    )
}

fn auth_source_keytar_read_password_reports_exact_malformed_and_missing_marker_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_read_password_reports_exact_malformed_and_missing_marker_errors",
        r##"(mapcar
          (lambda (secret)
            (list
             secret
             (auth-source-keytar-test-error-data
              (lambda ()
                (auth-source-keytar--read-password secret)))))
          '(""
            "password"
            "password:"
            "password: no quote"
            "{ account: 'alice' }"
            "PASSWORD: 'uppercase' }"))"##,
        expect![[
            r#"OK (("" (:error wrong-type-argument (arrayp nil))) ("password" (:error wrong-type-argument (arrayp nil))) ("password:" (:error wrong-type-argument (arrayp nil))) ("password: no quote" (:error wrong-type-argument (arrayp nil))) ("{ account: 'alice' }" (:error wrong-type-argument (arrayp nil))) ("PASSWORD: 'uppercase' }" (:ok "uppercase")))"#
        ]],
    )
}

fn auth_source_keytar_read_password_rejects_non_string_secrets_with_exact_signals()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_read_password_rejects_non_string_secrets_with_exact_signals",
        r##"(mapcar
          (lambda (secret)
            (list
             secret
             (auth-source-keytar-test-error-data
              (lambda ()
                (auth-source-keytar--read-password secret)))))
          '(nil
            password-symbol
            42
            ("password: 'nested' }")
            [112 97 115 115]))"##,
        expect![[
            r#"OK ((nil (:error wrong-type-argument (stringp nil))) (password-symbol (:error wrong-type-argument (sequencep password-symbol))) (42 (:error wrong-type-argument (sequencep 42))) (#1=("password: 'nested' }") (:error wrong-type-argument (stringp #1#))) (#2=[112 97 115 115] (:error wrong-type-argument (stringp #2#))))"#
        ]],
    )
}

fn auth_source_keytar_build_result_parses_multiline_keytar_output_and_reverses_provider_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_build_result_parses_multiline_keytar_output_and_reverses_provider_order",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-find-credentials)
                (lambda (service)
                  (push service calls)
                  "[\n  { account: 'alice', password: 'alpha secret' },\n  { account: 'bob', password: 'beta-secret' },\n  { account: 'ci', password: 'token-三' }\n]")))
            (list
             (auth-source-keytar--build-result
              "production/api")
             (nreverse calls))))"##,
        expect![[
            r#"OK (((:secret "token-三") (:secret "beta-secret") (:secret "alpha secret")) ("production/api"))"#
        ]],
    )
}

fn auth_source_keytar_build_result_empty_and_whitespace_outputs_have_exact_nil_or_error_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_build_result_empty_and_whitespace_outputs_have_exact_nil_or_error_contracts",
        r##"(mapcar
          (lambda (output)
            (cl-letf
                (((symbol-function 'keytar-find-credentials)
                  (lambda (_)
                    output)))
              (list
               output
               (auth-source-keytar-test-error-data
                (lambda ()
                  (auth-source-keytar--build-result
                   "empty-service"))))))
          '("[]"
            "[\n]"
            ""
            "\n\n"
            "[ \n \t\n ]"))"##,
        expect![[
            r#"OK (("[]" (:ok nil)) ("[\n]" (:ok nil)) ("" (:ok nil)) ("\n\n" (:ok nil)) ("[ \n \11\n ]" (:error wrong-type-argument (arrayp nil))))"#
        ]],
    )
}

fn auth_source_keytar_build_result_preserves_empty_unicode_and_shell_like_passwords_as_data()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_build_result_preserves_empty_unicode_and_shell_like_passwords_as_data",
        r##"(cl-letf
          (((symbol-function 'keytar-find-credentials)
            (lambda (_)
              "[\n{ account: 'empty', password: '' },\n{ account: 'unicode', password: '密钥🔐' },\n{ account: 'shell', password: '$(touch nope); $HOME & spaces' }\n]")))
          (auth-source-keytar--build-result
           "special-secrets"))"##,
        expect![[
            r#"OK ((:secret "$(touch nope); $HOME & spaces") (:secret "密钥🔐") (:secret ""))"#
        ]],
    )
}

fn auth_source_keytar_build_result_single_line_provider_output_yields_only_first_embedded_password()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_build_result_single_line_provider_output_yields_only_first_embedded_password",
        r##"(cl-letf
          (((symbol-function 'keytar-find-credentials)
            (lambda (_)
              "[{ account: 'one', password: 'first' }, { account: 'two', password: 'second' }]")))
          (auth-source-keytar--build-result
           "single-line"))"##,
        expect![[r#"OK ((:secret "first, { account: 'two',"))"#]],
    )
}

fn auth_source_keytar_build_result_blank_lines_surface_failure_before_trailing_comma_cleanup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_build_result_blank_lines_surface_failure_before_trailing_comma_cleanup",
        r##"(cl-letf
          (((symbol-function 'keytar-find-credentials)
            (lambda (_)
              "[\n\n { account: 'one', password: 'first,inside' },\n\t\n { account: 'two', password: 'second' },   \n\n]")))
          (auth-source-keytar-test-error-data
           (lambda ()
             (auth-source-keytar--build-result
              "formatting"))))"##,
        expect!["OK (:error wrong-type-argument (arrayp nil))"],
    )
}

fn auth_source_keytar_build_result_propagates_provider_failures_and_non_string_results()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_build_result_propagates_provider_failures_and_non_string_results",
        r##"(mapcar
          (lambda (case)
            (cl-letf
                (((symbol-function 'keytar-find-credentials)
                  (lambda (_)
                    (pcase case
                      ('provider-error
                       (error "fixture provider failed"))
                      (_ case)))))
              (list
               case
               (auth-source-keytar-test-error-data
                (lambda ()
                  (auth-source-keytar--build-result
                   "service"))))))
          '(provider-error
            nil
            17
            credential-symbol
            ("list-result")))"##,
        expect![[
            r#"OK ((provider-error (:error error ("fixture provider failed"))) (nil (:error wrong-type-argument (arrayp nil))) (17 (:error wrong-type-argument (sequencep 17))) (credential-symbol (:error wrong-type-argument (sequencep credential-symbol))) (#1=("list-result") (:error wrong-type-argument (stringp #1#))))"#
        ]],
    )
}

pub(super) fn parsing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_keytar_read_password_extracts_real_keytar_credential_rendering(),
        auth_source_keytar_read_password_trims_outer_whitespace_but_preserves_internal_content(),
        auth_source_keytar_read_password_uses_first_marker_and_globally_removes_closing_fragment(),
        auth_source_keytar_read_password_reports_exact_malformed_and_missing_marker_errors(),
        auth_source_keytar_read_password_rejects_non_string_secrets_with_exact_signals(),
        auth_source_keytar_build_result_parses_multiline_keytar_output_and_reverses_provider_order(),
        auth_source_keytar_build_result_empty_and_whitespace_outputs_have_exact_nil_or_error_contracts(),
        auth_source_keytar_build_result_preserves_empty_unicode_and_shell_like_passwords_as_data(),
        auth_source_keytar_build_result_single_line_provider_output_yields_only_first_embedded_password(),
        auth_source_keytar_build_result_blank_lines_surface_failure_before_trailing_comma_cleanup(),
        auth_source_keytar_build_result_propagates_provider_failures_and_non_string_results(),
    ]
}
