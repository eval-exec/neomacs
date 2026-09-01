use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_keytar_backend_parse_builds_exact_keytar_backend_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_backend_parse_builds_exact_keytar_backend_contract",
        r##"(let ((backend
                                (auth-source-keytar-backend-parse
                                 'keytar)))
          (list
           (auth-source-keytar-test-backend-data
            backend)
           (auth-source-backend-p backend)
           (eq
            (slot-value backend 'search-function)
            #'auth-source-keytar-search)))"##,
        expect![[r#"OK (("Keytar" keytar auth-source-keytar-search) t t)"#]],
    )
}

fn auth_source_keytar_backend_parse_rejects_every_non_keytar_entry_without_side_effects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_backend_parse_rejects_every_non_keytar_entry_without_side_effects",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'auth-source-backend-parse-parameters)
                (lambda (&rest arguments)
                  (push arguments calls)
                  :unexpected)))
            (list
             (mapcar
              (lambda (entry)
                (list
                 entry
                 (auth-source-keytar-backend-parse
                  entry)))
              '(nil
                "keytar"
                KEYTAR
                (keytar)
                (:source keytar)
                keytar-config
                0))
             (nreverse calls))))"##,
        expect![[
            r#"OK (((nil nil) ("keytar" nil) (KEYTAR nil) ((keytar) nil) ((:source keytar) nil) (keytar-config nil) (0 nil)) nil)"#
        ]],
    )
}

fn auth_source_keytar_backend_parse_forwards_entry_and_unmodified_backend_to_parameter_parser()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_backend_parse_forwards_entry_and_unmodified_backend_to_parameter_parser",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'auth-source-backend-parse-parameters)
                (lambda (entry backend)
                  (push
                   (list
                    entry
                    (auth-source-keytar-test-backend-data
                     backend))
                   calls)
                   (list
                    :parsed
                    entry
                    (slot-value backend 'source)))))
            (list
             (auth-source-keytar-backend-parse
              'keytar)
             (nreverse calls))))"##,
        expect![[
            r#"OK ((:parsed keytar "Keytar") ((keytar ("Keytar" keytar auth-source-keytar-search))))"#
        ]],
    )
}

fn auth_source_keytar_backend_parse_propagates_parameter_parser_failure_after_backend_construction()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_backend_parse_propagates_parameter_parser_failure_after_backend_construction",
        r##"(let (observed)
          (cl-letf
              (((symbol-function
                 'auth-source-backend-parse-parameters)
                (lambda (entry backend)
                  (setq observed
                        (list
                         entry
                         (auth-source-keytar-test-backend-data
                          backend)))
                  (error
                   "fixture backend parser failed"))))
            (list
             (auth-source-keytar-test-error-data
              (lambda ()
                (auth-source-keytar-backend-parse
                 'keytar)))
             observed)))"##,
        expect![[
            r#"OK ((:error error ("fixture backend parser failed")) (keytar ("Keytar" keytar auth-source-keytar-search)))"#
        ]],
    )
}

fn auth_source_keytar_load_registers_parser_once_in_modern_auth_source_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_load_registers_parser_once_in_modern_auth_source_hook",
        r##"(list
          (boundp
           'auth-source-backend-parser-functions)
          (memq
           #'auth-source-keytar-backend-parse
           auth-source-backend-parser-functions)
          (length
           (seq-filter
            (lambda (function)
              (eq
               function
               #'auth-source-keytar-backend-parse))
            auth-source-backend-parser-functions))
          (advice-member-p
           #'auth-source-keytar-backend-parse
           'auth-source-backend-parse))"##,
        expect![
            "OK (t (auth-source-keytar-backend-parse auth-source-backends-parser-secrets auth-source-backends-parser-macos-keychain auth-source-backends-parser-file) 1 nil)"
        ],
    )
}

fn auth_source_keytar_registered_hook_parses_keytar_and_declines_other_sources() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_keytar_registered_hook_parses_keytar_and_declines_other_sources",
        r##"(list
          (auth-source-keytar-test-backend-data
           (run-hook-with-args-until-success
            'auth-source-backend-parser-functions
            'keytar))
          (run-hook-with-args-until-success
           'auth-source-backend-parser-functions
           "fixture.authinfo")
          (run-hook-with-args-until-success
           'auth-source-backend-parser-functions
           'unknown-backend))"##,
        expect![[
            r#"OK (("Keytar" keytar auth-source-keytar-search) #s(auth-source-backend ignore "" t t t nil ignore ignore) #s(auth-source-backend ignore "" t t t nil ignore ignore))"#
        ]],
    )
}

fn auth_source_keytar_real_auth_source_backend_parser_accepts_keytar_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_real_auth_source_backend_parser_accepts_keytar_source",
        r##"(let ((backend
                                (auth-source-backend-parse
                                 'keytar)))
          (list
           (auth-source-keytar-test-backend-data
            backend)
           (auth-source-backend-p backend)))"##,
        expect![[r#"OK (("Keytar" keytar auth-source-keytar-search) t)"#]],
    )
}

fn auth_source_keytar_source_reloads_do_not_duplicate_modern_parser_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_source_reloads_do_not_duplicate_modern_parser_hook",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE")))
          (load source nil t t)
          (load source nil t t)
          (load source nil t t)
          (list
           (length
            (seq-filter
             (lambda (function)
               (eq
                function
                #'auth-source-keytar-backend-parse))
             auth-source-backend-parser-functions))
           (auth-source-keytar-test-backend-data
            (run-hook-with-args-until-success
             'auth-source-backend-parser-functions
             'keytar))))"##,
        expect![[r#"OK (1 ("Keytar" keytar auth-source-keytar-search))"#]],
    )
}

fn auth_source_keytar_legacy_reload_uses_before_until_advice_when_parser_hook_is_unbound()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_legacy_reload_uses_before_until_advice_when_parser_hook_is_unbound",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE")))
          (remove-hook
           'auth-source-backend-parser-functions
           #'auth-source-keytar-backend-parse)
          (makunbound
           'auth-source-backend-parser-functions)
          (load source nil t t)
          (let ((backend
                 (auth-source-backend-parse
                  'keytar)))
            (list
             (boundp
              'auth-source-backend-parser-functions)
             (and
              (advice-member-p
               #'auth-source-keytar-backend-parse
               'auth-source-backend-parse)
              t)
             (auth-source-keytar-test-backend-data
              backend))))"##,
        expect![[r#"OK (nil t ("Keytar" keytar auth-source-keytar-search))"#]],
    )
}

pub(super) fn backend_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_keytar_backend_parse_builds_exact_keytar_backend_contract(),
        auth_source_keytar_backend_parse_rejects_every_non_keytar_entry_without_side_effects(),
        auth_source_keytar_backend_parse_forwards_entry_and_unmodified_backend_to_parameter_parser(),
        auth_source_keytar_backend_parse_propagates_parameter_parser_failure_after_backend_construction(),
        auth_source_keytar_load_registers_parser_once_in_modern_auth_source_hook(),
        auth_source_keytar_registered_hook_parses_keytar_and_declines_other_sources(),
        auth_source_keytar_real_auth_source_backend_parser_accepts_keytar_source(),
        auth_source_keytar_source_reloads_do_not_duplicate_modern_parser_hook(),
        auth_source_keytar_legacy_reload_uses_before_until_advice_when_parser_hook_is_unbound(),
    ]
}
