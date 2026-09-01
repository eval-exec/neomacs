use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_keytar_search_service_and_account_delegate_to_exact_keytar_lookup() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_keytar_search_service_and_account_delegate_to_exact_keytar_lookup",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list service account)
                   calls)
                  "production-secret")))
            (list
             (auth-source-keytar-search
              :service "api.example"
              :account "deploy")
             (nreverse calls))))"##,
        expect![[r#"OK ("production-secret" (("api.example" "deploy")))"#]],
    )
}

fn auth_source_keytar_search_host_and_user_translate_to_keytar_service_and_account()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_host_and_user_translate_to_keytar_service_and_account",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list service account)
                   calls)
                  (list
                   :password-for
                   service
                   account))))
            (list
             (auth-source-keytar-search
              :host "git.example"
              :user "alice")
             (nreverse calls))))"##,
        expect![[r#"OK ((:password-for "git.example" "alice") (("git.example" "alice")))"#]],
    )
}

fn auth_source_keytar_search_service_account_take_precedence_over_host_user() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_service_account_take_precedence_over_host_user",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list service account)
                   calls)
                  :selected)))
            (list
             (auth-source-keytar-search
              :host "ignored-host"
              :user "ignored-user"
              :service "selected-service"
              :account "selected-account")
             (nreverse calls))))"##,
        expect![[r#"OK (:selected (("selected-service" "selected-account")))"#]],
    )
}

fn auth_source_keytar_search_service_only_builds_multiple_secret_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_service_only_builds_multiple_secret_entries",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'auth-source-keytar--build-result)
                (lambda (service)
                  (push service calls)
                  '((:secret "second")
                    (:secret "first")))))
            (list
             (auth-source-keytar-search
              :service "registry.example")
             (nreverse calls))))"##,
        expect![[r#"OK (((:secret "second") (:secret "first")) ("registry.example"))"#]],
    )
}

fn auth_source_keytar_search_host_only_uses_host_as_build_result_service() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_host_only_uses_host_as_build_result_service",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'auth-source-keytar--build-result)
                (lambda (service)
                  (push service calls)
                  (list
                   (list :secret
                         (concat service "-secret"))))))
            (list
             (auth-source-keytar-search
              :host "database.internal")
             (nreverse calls))))"##,
        expect![[r#"OK (((:secret "database.internal-secret")) ("database.internal"))"#]],
    )
}

fn auth_source_keytar_search_partial_query_matrix_exposes_exact_branch_selection() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_keytar_search_partial_query_matrix_exposes_exact_branch_selection",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list :get service account)
                   calls)
                  :get-result))
               ((symbol-function
                 'auth-source-keytar--build-result)
                (lambda (service)
                  (push
                   (list :build service)
                   calls)
                  :build-result)))
            (list
             (mapcar
              (lambda (arguments)
                (list
                 arguments
                 (auth-source-keytar-test-error-data
                  (lambda ()
                    (apply
                     #'auth-source-keytar-search
                     arguments)))))
              '((:service "service" :account nil
                 :host "host" :user "user")
                (:service nil :account "account"
                 :host "host" :user "user")
                (:service "service" :account nil)
                (:account "account")
                (:user "user")
                (:host nil :user "user")
                nil))
             (nreverse calls))))"##,
        expect![[
            r#"OK ((((:service "service" :account nil :host "host" :user "user") (:ok :get-result)) ((:service nil :account "account" :host "host" :user "user") (:ok :get-result)) ((:service "service" :account nil) (:ok :build-result)) ((:account "account") (:error user-error ("Missing key ‘service‘ in search query"))) ((:user "user") (:error user-error ("Missing key ‘service‘ in search query"))) ((:host nil :user "user") (:error user-error ("Missing key ‘service‘ in search query"))) (nil (:error user-error ("Missing key ‘service‘ in search query")))) ((:get "host" "user") (:get "host" "user") (:build "service")))"#
        ]],
    )
}

fn auth_source_keytar_search_forwards_truthy_non_string_identifiers_without_validation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_forwards_truthy_non_string_identifiers_without_validation",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list service account)
                   calls)
                  :raw-result)))
            (list
             (mapcar
              (lambda (pair)
                (auth-source-keytar-search
                 :service
                 (car pair)
                 :account
                 (cadr pair)))
              '((service-symbol account-symbol)
                (17 0)
                ((nested service) (nested account))
                ("" "")))
             (nreverse calls))))"##,
        expect![[
            r#"OK ((:raw-result :raw-result :raw-result :raw-result) ((service-symbol account-symbol) (17 0) ((nested service) (nested account)) ("" "")))"#
        ]],
    )
}

fn auth_source_keytar_search_allows_unrelated_auth_source_keys_without_changing_lookup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_allows_unrelated_auth_source_keys_without_changing_lookup",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list service account)
                   calls)
                  "secret")))
            (list
             (auth-source-keytar-search
              :service "service"
              :account "account"
              :port 443
              :max 7
              :require '(:secret)
              :create t
              :delete nil
              :custom-key "ignored")
             (nreverse calls))))"##,
        expect![[r#"OK ("secret" (("service" "account")))"#]],
    )
}

fn auth_source_keytar_search_duplicate_keywords_follow_cl_keyword_binding_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_duplicate_keywords_follow_cl_keyword_binding_semantics",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (service account)
                  (push
                   (list service account)
                   calls)
                  :found)))
            (list
             (auth-source-keytar-search
              :service "first-service"
              :service "second-service"
              :account "first-account"
              :account "second-account")
             (nreverse calls))))"##,
        expect![[r#"OK (:found (("first-service" "first-account")))"#]],
    )
}

fn auth_source_keytar_search_preserves_nil_empty_and_structured_provider_return_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_preserves_nil_empty_and_structured_provider_return_values",
        r##"(mapcar
          (lambda (result)
            (cl-letf
                (((symbol-function 'keytar-get-password)
                  (lambda (&rest _)
                    result)))
              (list
               result
               (auth-source-keytar-search
                :service "service"
                :account "account"))))
          '(nil
            ""
            "null"
            0
            provider-symbol
            (:secret "nested")))"##,
        expect![[
            r#"OK ((nil nil) ("" "") ("null" "null") (0 0) (provider-symbol provider-symbol) (#1=(:secret "nested") #1#))"#
        ]],
    )
}

fn auth_source_keytar_search_propagates_password_and_credential_provider_failures()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_propagates_password_and_credential_provider_failures",
        r##"(list
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (&rest _)
                  (user-error
                   "keychain unavailable"))))
            (auth-source-keytar-test-error-data
             (lambda ()
               (auth-source-keytar-search
                :service "service"
                :account "account"))))
          (cl-letf
              (((symbol-function
                 'auth-source-keytar--build-result)
                (lambda (_)
                  (error
                   "credential listing failed"))))
            (auth-source-keytar-test-error-data
             (lambda ()
               (auth-source-keytar-search
                :service "service")))))"##,
        expect![[
            r#"OK ((:error user-error ("keychain unavailable")) (:error error ("credential listing failed")))"#
        ]],
    )
}

fn auth_source_keytar_search_arity_and_non_keyword_calls_signal_without_provider_side_effects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_search_arity_and_non_keyword_calls_signal_without_provider_side_effects",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'keytar-get-password)
                (lambda (&rest arguments)
                  (push arguments calls)
                  :unexpected))
               ((symbol-function
                 'auth-source-keytar--build-result)
                (lambda (&rest arguments)
                  (push arguments calls)
                  :unexpected)))
            (list
             (auth-source-keytar-test-error-data
              (lambda ()
                (auth-source-keytar-search
                 "not-a-keyword")))
             (auth-source-keytar-test-error-data
              (lambda ()
                (auth-source-keytar-search
                 :service)))
             (auth-source-keytar-test-error-data
              (lambda ()
                (auth-source-keytar-search
                 :unknown-only
                 "value")))
             (nreverse calls))))"##,
        expect![[
            r#"OK ((:error user-error ("Missing key ‘service‘ in search query")) (:error user-error ("Missing key ‘service‘ in search query")) (:error user-error ("Missing key ‘service‘ in search query")) nil)"#
        ]],
    )
}

pub(super) fn search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_keytar_search_service_and_account_delegate_to_exact_keytar_lookup(),
        auth_source_keytar_search_host_and_user_translate_to_keytar_service_and_account(),
        auth_source_keytar_search_service_account_take_precedence_over_host_user(),
        auth_source_keytar_search_service_only_builds_multiple_secret_entries(),
        auth_source_keytar_search_host_only_uses_host_as_build_result_service(),
        auth_source_keytar_search_partial_query_matrix_exposes_exact_branch_selection(),
        auth_source_keytar_search_forwards_truthy_non_string_identifiers_without_validation(),
        auth_source_keytar_search_allows_unrelated_auth_source_keys_without_changing_lookup(),
        auth_source_keytar_search_duplicate_keywords_follow_cl_keyword_binding_semantics(),
        auth_source_keytar_search_preserves_nil_empty_and_structured_provider_return_values(),
        auth_source_keytar_search_propagates_password_and_credential_provider_failures(),
        auth_source_keytar_search_arity_and_non_keyword_calls_signal_without_provider_side_effects(
        ),
    ]
}
