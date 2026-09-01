use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_xoauth2_search_builds_token_request_and_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_search_builds_token_request_and_match",
        r##"(let ((auth-source-xoauth2-creds
                '(:token-url "https://token.example/oauth"
                  :client-id "client id"
                  :client-secret "s&cret"
                  :refresh-token "refresh+token"))
               calls)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (url data)
                 (push (list :post url data) calls)
                 '((token_type . "Bearer")
                   (expires_in . 3600)
                   (access_token . "access-token"))))
              ((symbol-function 'auth-source-do-debug)
               (lambda (format-string &rest arguments)
                 (push (list :debug format-string arguments) calls))))
           (list
            (auth-source-xoauth2--search
             "smtp.example"
             "alice@example"
             587)
            (nreverse calls))))"##,
        expect![[
            r#"OK ((:host "smtp.example" :port 587 :user "alice@example" :secret "access-token") ((:post "https://token.example/oauth" "client_id=client id&client_secret=s&cret&refresh_token=refresh+token&grant_type=refresh_token") (:debug "XOAUTH2 access token (user=%s host=%s): %s" ("alice@example" "smtp.example" "access-token"))))"#
        ]],
    )
}

fn auth_source_xoauth2_search_requires_every_credential_field() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_search_requires_every_credential_field",
        r##"(let (posts)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (&rest arguments)
                 (push arguments posts)
                 '((access_token . "unexpected")))))
           (list
            (mapcar
             (lambda (creds)
               (let ((auth-source-xoauth2-creds creds))
                 (auth-source-xoauth2--search
                  "host" "user" "port")))
             '(nil
               (:client-id "id"
                :client-secret "secret"
                :refresh-token "refresh")
               (:token-url "url"
                :client-secret "secret"
                :refresh-token "refresh")
               (:token-url "url"
                :client-id "id"
                :refresh-token "refresh")
               (:token-url "url"
                :client-id "id"
                :client-secret "secret")))
            posts)))"##,
        expect!["OK ((nil nil nil nil nil) nil)"],
    )
}

fn auth_source_xoauth2_search_requires_access_token_in_reply() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_search_requires_access_token_in_reply",
        r##"(let ((auth-source-xoauth2-creds
                '(:token-url "url"
                  :client-id "id"
                  :client-secret "secret"
                  :refresh-token "refresh")))
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (_url _data)
                 '((token_type . "Bearer")
                   (expires_in . 3600)))))
           (auth-source-xoauth2--search
            "host" "user" 443)))"##,
        expect!["OK nil"],
    )
}

fn auth_source_xoauth2_function_provider_receives_exact_coordinates() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_function_provider_receives_exact_coordinates",
        r##"(let (provider-calls
               posts)
         (let ((auth-source-xoauth2-creds
                (lambda (host user port)
                  (push (list host user port) provider-calls)
                  '(:token-url "url"
                    :client-id "id"
                    :client-secret "secret"
                    :refresh-token "refresh"))))
           (cl-letf
               (((symbol-function 'auth-source-xoauth2--url-post)
                 (lambda (url data)
                   (push (list url data) posts)
                   '((access_token . "token"))))
                ((symbol-function 'auth-source-do-debug)
                 #'ignore))
             (list
              (auth-source-xoauth2--search
               "imap.example" "alice" 993)
              (nreverse provider-calls)
              (nreverse posts)))))"##,
        expect![[
            r#"OK ((:host "imap.example" :port 993 :user "alice" :secret "token") (("imap.example" "alice" 993)) (("url" "client_id=id&client_secret=secret&refresh_token=refresh&grant_type=refresh_token")))"#
        ]],
    )
}

fn auth_source_xoauth2_string_provider_delegates_to_file_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_string_provider_delegates_to_file_backend",
        r##"(let ((auth-source-xoauth2-creds
                "/fixture/credentials.gpg")
               calls)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--file-creds)
               (lambda (&rest arguments)
                 (push arguments calls)
                 '(:token-url "url"
                   :client-id "id"
                   :client-secret "secret"
                   :refresh-token "refresh")))
              ((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (&rest arguments)
                 (push arguments calls)
                 '((access_token . "file-token"))))
              ((symbol-function 'auth-source-do-debug)
               #'ignore))
           (list
            (auth-source-xoauth2--search
             "host" "user" "port")
            (nreverse calls))))"##,
        expect![[
            r#"OK ((:host "host" :port "port" :user "user" :secret "file-token") (("/fixture/credentials.gpg" "host" "user" "port") ("url" "client_id=id&client_secret=secret&refresh_token=refresh&grant_type=refresh_token")))"#
        ]],
    )
}

fn auth_source_xoauth2_public_search_scans_host_port_product_until_first_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_xoauth2_public_search_scans_host_port_product_until_first_match",
        r##"(let ((backend
                auth-source-xoauth2-backend)
               calls)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--search)
               (lambda (host user port)
                 (push (list host user port) calls)
                 (and
                  (equal
                   (list host port)
                   '("second.example" 993))
                  (list
                   :host host
                   :user user
                   :port port
                   :secret "token")))))
           (list
            (auth-source-xoauth2-search
             :backend backend
             :type 'xoauth2
             :host '("first.example" "second.example" "third.example")
             :user "alice"
             :port '(143 993 995)
             :max 1)
            (nreverse calls))))"##,
        expect![[
            r#"OK (((:host "second.example" :user "alice" :port 993 :secret "token")) (("first.example" "alice" 143) ("first.example" "alice" 993) ("first.example" "alice" 995) ("second.example" "alice" 143) ("second.example" "alice" 993)))"#
        ]],
    )
}

fn auth_source_xoauth2_public_search_exhausts_product_and_normalizes_scalars() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_public_search_exhausts_product_and_normalizes_scalars",
        r##"(let ((backend
                auth-source-xoauth2-backend)
               calls)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--search)
               (lambda (&rest arguments)
                 (push arguments calls)
                 nil)))
           (list
            (auth-source-xoauth2-search
             :backend backend
             :host "one.example"
             :user "alice"
             :port 443)
            (auth-source-xoauth2-search
             :backend backend
             :host nil
             :user nil
             :port nil)
            (nreverse calls))))"##,
        expect![[r#"OK (nil nil (("one.example" "alice" 443) (nil nil nil)))"#]],
    )
}

fn auth_source_xoauth2_public_search_validates_requested_type() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_public_search_validates_requested_type",
        r##"(let ((backend
                auth-source-xoauth2-backend))
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--search)
               (lambda (&rest _arguments)
                 nil)))
           (mapcar
            (lambda (type)
              (auth-source-xoauth2-test-error-data
               (lambda ()
                 (auth-source-xoauth2-search
                  :backend backend
                  :type type
                  :host "host"
                  :user "user"
                  :port 443))))
            '(nil xoauth2 password-store pass))))"##,
        expect![[
            r#"OK ((:ok nil) (:ok nil) (:error error ("Invalid XOAuth2 search: nil nil")) (:error error ("Invalid XOAuth2 search: nil nil")))"#
        ]],
    )
}

fn auth_source_xoauth2_backend_parser_and_slots_match_registration() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_backend_parser_and_slots_match_registration",
        r##"(let ((parsed
                (auth-source-backend-parse
                 'xoauth2)))
         (list
          (object-of-class-p
           auth-source-xoauth2-backend
           'auth-source-backend)
          (mapcar
           (lambda (slot)
             (slot-value
              auth-source-xoauth2-backend
              slot))
           '(type source host user port search-function))
          (eq parsed
              auth-source-xoauth2-backend)
          (mapcar
           #'auth-source-xoauth2-backend-parse
           '(nil "xoauth2" pass default xoauth2-other))))"##,
        expect![[
            r#"OK (t (xoauth2 "." t t t auth-source-xoauth2-search) t (nil nil nil nil nil))"#
        ]],
    )
}

pub(super) fn backend_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_xoauth2_search_builds_token_request_and_match(),
        auth_source_xoauth2_search_requires_every_credential_field(),
        auth_source_xoauth2_search_requires_access_token_in_reply(),
        auth_source_xoauth2_function_provider_receives_exact_coordinates(),
        auth_source_xoauth2_string_provider_delegates_to_file_backend(),
        auth_source_xoauth2_public_search_scans_host_port_product_until_first_match(),
        auth_source_xoauth2_public_search_exhausts_product_and_normalizes_scalars(),
        auth_source_xoauth2_public_search_validates_requested_type(),
        auth_source_xoauth2_backend_parser_and_slots_match_registration(),
    ]
}
