use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_xoauth2_curl_transport_posts_exact_request_and_parses_json() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_curl_transport_posts_exact_request_and_parses_json",
        r##"(let ((auth-source-xoauth2-use-curl
                t)
               calls)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (program infile destination display
                        &rest arguments)
                 (push
                  (list
                   program infile destination display arguments)
                  calls)
                 (insert
                  "{\"access_token\":\"curl-token\","
                  "\"expires_in\":3600,"
                  "\"token_type\":\"Bearer\"}")
                 0)))
           (list
            (auth-source-xoauth2--url-post
             "https://token.example/oauth"
             "client_id=id&refresh_token=refresh")
            (nreverse calls))))"##,
        expect![[
            r#"OK (((access_token . "curl-token") (expires_in . 3600) (token_type . "Bearer")) (("curl" nil t nil ("--silent" "--request" "POST" "--data" "client_id=id&refresh_token=refresh" "--header" "Content-Type:application/x-www-form-urlencoded" "https://token.example/oauth"))))"#
        ]],
    )
}

fn auth_source_xoauth2_curl_transport_parses_output_even_on_nonzero_status() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_curl_transport_parses_output_even_on_nonzero_status",
        r##"(let ((auth-source-xoauth2-use-curl
                t))
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest _arguments)
                 (insert
                  "{\"error\":\"invalid_grant\","
                  "\"error_description\":\"expired\"}")
                 22)))
           (auth-source-xoauth2--url-post
            "https://token.example"
            "payload")))"##,
        expect![[r#"OK ((error . "invalid_grant") (error_description . "expired"))"#]],
    )
}

fn auth_source_xoauth2_curl_transport_propagates_invalid_json_signal() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_curl_transport_propagates_invalid_json_signal",
        r##"(let ((auth-source-xoauth2-use-curl
                t))
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest _arguments)
                 (insert "not-json")
                 0)))
           (auth-source-xoauth2-test-error-data
            (lambda ()
              (auth-source-xoauth2--url-post
               "https://token.example"
               "payload")))))"##,
        expect![[r#"OK (:error json-unknown-keyword ("not"))"#]],
    )
}

fn auth_source_xoauth2_url_transport_sets_request_bindings_and_kills_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_url_transport_sets_request_bindings_and_kills_buffer",
        r##"(let ((auth-source-xoauth2-use-curl
                nil)
               captured
               response-buffer)
         (cl-letf
             (((symbol-function 'url-retrieve-synchronously)
               (lambda (url)
                 (setq captured
                       (list
                        url
                        url-request-method
                        url-request-data
                        url-request-extra-headers)
                       response-buffer
                       (generate-new-buffer
                        " *xoauth2 response*"))
                 (with-current-buffer response-buffer
                   (insert
                    "HTTP/1.1 200 OK\n"
                    "Content-Type: application/json\n"
                    "\n"
                    "{\"access_token\":\"url-token\","
                    "\"expires_in\":1800}"))
                 response-buffer)))
           (let ((result
                  (auth-source-xoauth2--url-post
                   "https://token.example/oauth"
                   "client_id=id")))
             (list
              result
              captured
              (buffer-live-p
               response-buffer)))))"##,
        expect![[
            r#"OK (((access_token . "url-token") (expires_in . 1800)) ("https://token.example/oauth" "POST" "client_id=id" (("Content-Type" . "application/x-www-form-urlencoded"))) nil)"#
        ]],
    )
}

fn auth_source_xoauth2_url_transport_returns_nil_without_header_separator() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_url_transport_returns_nil_without_header_separator",
        r##"(let ((auth-source-xoauth2-use-curl
                nil)
               response-buffer)
         (cl-letf
             (((symbol-function 'url-retrieve-synchronously)
               (lambda (_url)
                 (setq response-buffer
                       (generate-new-buffer
                        " *xoauth2 malformed response*"))
                 (with-current-buffer response-buffer
                   (insert
                    "HTTP/1.1 200 OK\n"
                    "Content-Type: application/json\n"
                    "{\"access_token\":\"hidden\"}"))
                 response-buffer)))
           (unwind-protect
               (list
                (auth-source-xoauth2--url-post
                 "https://token.example"
                 "payload")
                (buffer-live-p response-buffer)
                (with-current-buffer response-buffer
                  (buffer-string)))
             (when
                 (buffer-live-p response-buffer)
               (kill-buffer response-buffer)))))"##,
        expect![[
            r#"OK (nil t "HTTP/1.1 200 OK\nContent-Type: application/json\n{\"access_token\":\"hidden\"}")"#
        ]],
    )
}

fn auth_source_xoauth2_url_transport_propagates_retrieval_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_url_transport_propagates_retrieval_failure",
        r##"(let ((auth-source-xoauth2-use-curl
                nil))
         (cl-letf
             (((symbol-function 'url-retrieve-synchronously)
               (lambda (_url)
                 (error "network unavailable"))))
           (auth-source-xoauth2-test-error-data
            (lambda ()
              (auth-source-xoauth2--url-post
               "https://token.example"
               "payload")))))"##,
        expect![[r#"OK (:error error ("network unavailable"))"#]],
    )
}

pub(super) fn transport_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_xoauth2_curl_transport_posts_exact_request_and_parses_json(),
        auth_source_xoauth2_curl_transport_parses_output_even_on_nonzero_status(),
        auth_source_xoauth2_curl_transport_propagates_invalid_json_signal(),
        auth_source_xoauth2_url_transport_sets_request_bindings_and_kills_buffer(),
        auth_source_xoauth2_url_transport_returns_nil_without_header_separator(),
        auth_source_xoauth2_url_transport_propagates_retrieval_failure(),
    ]
}
