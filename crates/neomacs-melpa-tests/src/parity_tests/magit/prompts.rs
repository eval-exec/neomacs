use expect_test::expect;

use super::ParityBatchCase;

fn magit_prompt_matching_selects_patterns_suffixes_and_match_groups() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_prompt_matching_selects_patterns_suffixes_and_match_groups",
        r##"(let* ((prompts
                     '("^bar: ?$"
                       "^foo '\\(?99:.*\\)': ?$"
                       "^foo: ?$"))
                    (matched
                     (magit-process-match-prompt
                      prompts "foo 'payload':"))
                    (payload
                     (match-string-no-properties
                      99 "foo 'payload':")))
               (list
                (magit-process-match-prompt '("^foo: ?$") "bar: ")
                (magit-process-match-prompt '("^foo: ?$") "foo:")
                (magit-process-match-prompt '("^foo: ?$") "foo: ")
                matched
                payload))"##,
        expect![[r#"OK (nil "foo: " "foo: " "foo 'payload': " "payload")"#]],
    )
}

fn magit_password_prompt_patterns_extract_hosts_without_protocol_noise() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_password_prompt_patterns_extract_hosts_without_protocol_noise",
        r##"(mapcar
              (lambda (prompt)
                (and
                 (magit-process-match-prompt
                  magit-process-password-prompt-regexps prompt)
                 (or
                  (match-string-no-properties 99 prompt)
                  t)))
              '("Passphrase: "
                "Enter passphrase for key '/home/me/.ssh/id_rsa': "
                "Password for 'https://example.com': "
                "Password for 'https://me@magit.vc':"
                "Password for ahihi@foo:"
                "(user@host) Password for user@host: "
                "volumio@192.168.0.211's password: "
                "Token: "
                "not a credential prompt"))"##,
        expect![[
            r#"OK (t t "example.com" "me@magit.vc" "ahihi@foo" "user@host" "volumio@192.168.0.211" t nil)"#
        ]],
    )
}

pub(super) fn prompts_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        magit_prompt_matching_selects_patterns_suffixes_and_match_groups(),
        magit_password_prompt_patterns_extract_hosts_without_protocol_noise(),
    ]
}
