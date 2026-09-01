use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_1password_default_reference_builds_real_vault_host_user_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_default_reference_builds_real_vault_host_user_paths",
        r##"(mapcar
          (lambda (case)
            (let ((auth-source-1password-vault
                   (nth 0 case)))
              (list
               case
               (auth-source-1password--1password-construct-entry-path
                'fixture-backend
                'password-store
                (nth 1 case)
                (nth 2 case)
                (nth 3 case)))))
          '(("Personal" "api.example.com" "deploy" 443)
            ("Engineering" "git.example.net" "alice@example.net" "ssh")
            ("共有" "例え.テスト" "利用者" nil)
            ("vault with spaces" "service host" "user name" 8443)))"##,
        expect![[
            r#"OK ((("Personal" "api.example.com" "deploy" 443) "Personal/api.example.com/deploy") (("Engineering" "git.example.net" "alice@example.net" "ssh") "Engineering/git.example.net/alice@example.net") (("共有" "例え.テスト" "利用者" nil) "共有/例え.テスト/利用者") (("vault with spaces" "service host" "user name" 8443) "vault with spaces/service host/user name"))"#
        ]],
    )
}

fn auth_source_1password_default_reference_preserves_empty_and_embedded_slashes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_default_reference_preserves_empty_and_embedded_slashes",
        r##"(mapcar
          (lambda (case)
            (let ((auth-source-1password-vault
                   (nth 0 case)))
              (auth-source-1password--1password-construct-entry-path
               nil
               nil
               (nth 1 case)
               (nth 2 case)
               nil)))
          '(("" "" "")
            ("Personal/" "/api/v2" "team/alice")
            ("A//B" "host/" "/user")
            (" vault " " host " " user ")))"##,
        expect![[
            r#"OK ("//" "Personal///api/v2/team/alice" "A//B/host///user" " vault / host / user ")"#
        ]],
    )
}

fn auth_source_1password_reference_ignores_backend_type_port_but_reads_dynamic_vault()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_reference_ignores_backend_type_port_but_reads_dynamic_vault",
        r##"(let ((auth-source-1password-vault
                "Operations"))
          (list
           (auth-source-1password--1password-construct-entry-path
            'backend-a
            'type-a
            "db.internal"
            "reader"
            5432)
           (auth-source-1password--1password-construct-entry-path
            'backend-b
            '(custom type)
            "db.internal"
            "reader"
            "postgres")
           (let ((auth-source-1password-vault
                  "Temporary"))
             (auth-source-1password--1password-construct-entry-path
              nil nil
              "db.internal"
              "reader"
              nil))
           auth-source-1password-vault))"##,
        expect![[
            r#"OK ("Operations/db.internal/reader" "Operations/db.internal/reader" "Temporary/db.internal/reader" "Operations")"#
        ]],
    )
}

fn auth_source_1password_custom_reference_receives_full_auth_source_context_once() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_custom_reference_receives_full_auth_source_context_once",
        r##"(let (calls commands)
          (let ((auth-source-1password-executable
                 "fixture-op")
                (auth-source-1password-construct-secret-reference
                 (lambda (backend type host user port)
                   (push
                    (list
                     (eq
                      backend
                      auth-source-1password-backend)
                     type host user port)
                    calls)
                   (format
                    "Custom/%s/%s/%s"
                    host
                    port
                    user))))
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (list :found program)))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push command commands)
                    " generated-secret\n")))
              (list
               (auth-source-1password-search
                :backend
                auth-source-1password-backend
                :type 'password-store
                :host "db.prod"
                :user "service-account"
                :port 5432
                :max 9
                :require '(:secret))
               (nreverse calls)
               (nreverse commands)))))"##,
        expect![[
            r#"OK (((:user "service-account" :secret "generated-secret")) ((t password-store "db.prod" "service-account" 5432)) ("fixture-op read op://Custom/db.prod/5432/service-account"))"#
        ]],
    )
}

fn auth_source_1password_reference_supports_symbol_and_lambda_customizers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_reference_supports_symbol_and_lambda_customizers",
        r##"(let (calls)
          (cl-labels
              ((fixture-reference
                (backend type host user port)
                (push
                 (list backend type host user port)
                 calls)
                (format
                 "%s:%s@%s#%s"
                 type user host port)))
            (list
             (let ((auth-source-1password-construct-secret-reference
                    #'fixture-reference))
               (funcall
                auth-source-1password-construct-secret-reference
                'backend
                'password-store
                "host"
                "user"
                443))
             (let ((auth-source-1password-construct-secret-reference
                    (lambda (_backend _type host user _port)
                      (concat
                       "lambda/"
                       host
                       "/"
                       user))))
               (funcall
                auth-source-1password-construct-secret-reference
                nil nil
                "example.org"
                "ci"
                nil))
             (nreverse calls))))"##,
        expect![[
            r#"OK ("password-store:user@host#443" "lambda/example.org/ci" ((backend password-store "host" "user" 443)))"#
        ]],
    )
}

fn auth_source_1password_default_reference_treats_nil_components_as_empty_segments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_default_reference_treats_nil_components_as_empty_segments",
        r##"(list
          (let ((auth-source-1password-vault
                 "Personal"))
            (auth-source-1password--1password-construct-entry-path
             nil nil nil "user" nil))
          (let ((auth-source-1password-vault
                 "Personal"))
            (auth-source-1password--1password-construct-entry-path
             nil nil "host" nil nil))
          (let ((auth-source-1password-vault
                 nil))
            (auth-source-1password--1password-construct-entry-path
             nil nil "host" "user" nil))
          (let ((auth-source-1password-vault
                 nil))
            (auth-source-1password--1password-construct-entry-path
             nil nil nil nil nil)))"##,
        expect![[r#"OK ("Personal//user" "Personal/host/" "/host/user" "//")"#]],
    )
}

fn auth_source_1password_default_reference_non_string_components_signal_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_default_reference_non_string_components_signal_exactly",
        r##"(list
          (let ((auth-source-1password-vault
                 "Personal"))
            (auth-source-1password-test-error-data
             (lambda ()
               (auth-source-1password--1password-construct-entry-path
                nil nil 42 "user" nil))))
          (let ((auth-source-1password-vault
                 "Personal"))
            (auth-source-1password-test-error-data
             (lambda ()
               (auth-source-1password--1password-construct-entry-path
                nil nil "host" 'user-symbol nil))))
          (let ((auth-source-1password-vault
                 '("Personal")))
            (auth-source-1password-test-error-data
             (lambda ()
               (auth-source-1password--1password-construct-entry-path
                nil nil "host" "user" nil))))
          (let ((auth-source-1password-vault
                 "Personal"))
            (auth-source-1password-test-error-data
             (lambda ()
               (auth-source-1password--1password-construct-entry-path
                nil nil "host" '(user) nil)))))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (sequencep 42)) (:error wrong-type-argument (sequencep user-symbol)) (:error wrong-type-argument (characterp "Personal")) (:error wrong-type-argument (characterp user)))"#
        ]],
    )
}

pub(super) fn reference_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_1password_default_reference_builds_real_vault_host_user_paths(),
        auth_source_1password_default_reference_preserves_empty_and_embedded_slashes(),
        auth_source_1password_reference_ignores_backend_type_port_but_reads_dynamic_vault(),
        auth_source_1password_custom_reference_receives_full_auth_source_context_once(),
        auth_source_1password_reference_supports_symbol_and_lambda_customizers(),
        auth_source_1password_default_reference_treats_nil_components_as_empty_segments(),
        auth_source_1password_default_reference_non_string_components_signal_exactly(),
    ]
}
