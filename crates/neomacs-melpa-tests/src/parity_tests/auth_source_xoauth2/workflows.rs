use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_xoauth2_real_auth_source_search_returns_access_token() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_real_auth_source_search_returns_access_token",
        r##"(let ((auth-sources
                '(xoauth2))
               (auth-source-xoauth2-creds
                '(:token-url "https://token.example"
                  :client-id "client"
                  :client-secret "secret"
                  :refresh-token "refresh"))
               calls)
         (auth-source-forget-all-cached)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (url data)
                 (push (list url data) calls)
                 '((access_token . "integration-token"))))
              ((symbol-function 'auth-source-do-debug)
               #'ignore))
           (let ((matches
                  (auth-source-search
                   :host "smtp.example"
                   :user "alice@example"
                   :port 587
                   :require '(:user :secret)
                   :max 1)))
             (list
              matches
              (mapcar
               (lambda (entry)
                 (list
                  (plist-get entry :host)
                  (plist-get entry :port)
                  (plist-get entry :user)
                  (plist-get entry :secret)))
               matches)
              (nreverse calls)))))"##,
        expect![[
            r#"OK (((:host "smtp.example" :port 587 :user "alice@example" :secret "integration-token")) (("smtp.example" 587 "alice@example" "integration-token")) (("https://token.example" "client_id=client&client_secret=secret&refresh_token=refresh&grant_type=refresh_token")))"#
        ]],
    )
}

fn auth_source_xoauth2_real_password_lookup_returns_access_token() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_real_password_lookup_returns_access_token",
        r##"(let ((auth-sources
                '(xoauth2))
               (auth-source-xoauth2-creds
                (lambda (host user port)
                  (list
                   :token-url
                   (format "https://%s/token" host)
                   :client-id user
                   :client-secret "secret"
                   :refresh-token
                   (format "refresh-%s" port))))
               calls)
         (auth-source-forget-all-cached)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (url data)
                 (push (list url data) calls)
                 '((access_token . "password-token"))))
              ((symbol-function 'auth-source-do-debug)
               #'ignore))
           (list
            (auth-source-pick-first-password
             :host "smtp.example"
             :user "alice"
             :port "submission")
            (nreverse calls))))"##,
        expect![[
            r#"OK ("password-token" (("https://smtp.example/token" "client_id=alice&client_secret=secret&refresh_token=refresh-submission&grant_type=refresh_token")))"#
        ]],
    )
}

fn auth_source_xoauth2_enable_then_search_models_application_startup() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_enable_then_search_models_application_startup",
        r##"(let ((auth-sources
                '("~/.authinfo"))
               (auth-source-xoauth2-creds
                '(:token-url "url"
                  :client-id "id"
                  :client-secret "secret"
                  :refresh-token "refresh"))
               calls)
         (cl-letf
             (((symbol-function 'auth-source-xoauth2--url-post)
               (lambda (&rest arguments)
                 (push arguments calls)
                 '((access_token . "startup-token"))))
              ((symbol-function 'auth-source-do-debug)
               #'ignore))
           (auth-source-xoauth2-enable)
           (let ((matches
                  (auth-source-search
                   :host "imap.example"
                   :user "alice"
                   :port 993
                   :max 1)))
             (list
              auth-sources
              (car matches)
              (nreverse calls)))))"##,
        expect![[
            r#"OK ((xoauth2 "~/.authinfo") (:host "imap.example" :port 993 :user "alice" :secret "startup-token") (("url" "client_id=id&client_secret=secret&refresh_token=refresh&grant_type=refresh_token")))"#
        ]],
    )
}

fn auth_source_xoauth2_file_provider_drives_full_token_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_file_provider_drives_full_token_workflow",
        r##"(let ((file-name-handler-alist nil)
               (file
                (auth-source-xoauth2-test-file
                 "workflow.gpg"))
               calls)
         (with-temp-file file
           (prin1
            (let ((table
                   (make-hash-table
                    :test #'equal)))
              (puthash
               '("smtp.example" "alice" 465)
               '(:token-url "https://token.example"
                 :client-id "client"
                 :client-secret "secret"
                 :refresh-token "refresh")
               table)
              table)
            (current-buffer)))
         (let ((auth-sources '(xoauth2))
               (auth-source-xoauth2-creds file))
           (auth-source-forget-all-cached)
           (cl-letf
               (((symbol-function 'auth-source-xoauth2--url-post)
                 (lambda (url data)
                   (push (list url data) calls)
                   '((access_token . "file-workflow-token"))))
                ((symbol-function 'auth-source-do-debug)
                 #'ignore))
             (list
              (auth-source-pick-first-password
               :host "smtp.example"
               :user "alice"
               :port 465)
              (auth-source-pick-first-password
               :host "smtp.example"
               :user "alice"
               :port 587)
              (nreverse calls)))))"##,
        expect![[
            r#"OK ("file-workflow-token" nil (("https://token.example" "client_id=client&client_secret=secret&refresh_token=refresh&grant_type=refresh_token")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_xoauth2_real_auth_source_search_returns_access_token(),
        auth_source_xoauth2_real_password_lookup_returns_access_token(),
        auth_source_xoauth2_enable_then_search_models_application_startup(),
        auth_source_xoauth2_file_provider_drives_full_token_workflow(),
    ]
}
