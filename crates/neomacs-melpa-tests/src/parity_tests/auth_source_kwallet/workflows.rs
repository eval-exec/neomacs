use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_kwallet_real_auth_source_search_returns_secret_and_exact_process_request()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_real_auth_source_search_returns_secret_and_exact_process_request",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (setq auth-source-kwallet-test-output
                                 "mail-password\n")
                           (list
                            (auth-source-search
                             :host
                             "imap.example"
                             :user
                             "alice"
                             :port
                             "imaps"
                             :max
                             1)
                            (nreverse
                             auth-source-kwallet-test-process-calls)))"##,
        expect![[
            r#"OK (((:user "alice" :secret "mail-password")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@imap.example") t)))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_real_auth_source_pick_first_password_supports_mail_client_usage()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_real_auth_source_pick_first_password_supports_mail_client_usage",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (setq auth-source-kwallet-test-output
                                 "smtp-password\n")
                           (list
                            (auth-source-pick-first-password
                             :host
                             "smtp.example"
                             :user
                             "mailer"
                             :port
                             "submission")
                            (nreverse
                             auth-source-kwallet-test-process-calls)))"##,
        expect![[
            r#"OK ("smtp-password" (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "mailer@smtp.example") t)))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_auth_source_type_filter_is_forwarded_but_backend_runs_for_every_value()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_auth_source_type_filter_is_forwarded_but_backend_runs_for_every_value",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (mapcar
                            (lambda (type)
                              (auth-source-kwallet-test-reset-process)
                              (setq
                               auth-source-kwallet-test-output
                               "typed-secret")
                              (list
                               type
                               (auth-source-search
                                :host
                                "typed.example"
                                :user
                                "alice"
                                :type
                                type
                                :max
                                1)
                               (nreverse
                                auth-source-kwallet-test-process-calls)))
                            '(kwallet
                              (kwallet)
                              netrc
                              (netrc secrets)
                              t
                              nil)))"##,
        expect![[
            r#"OK ((kwallet ((:user "alice" :secret "typed-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@typed.example") t))) ((kwallet) ((:user "alice" :secret "typed-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@typed.example") t))) (netrc ((:user "alice" :secret "typed-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@typed.example") t))) ((netrc secrets) ((:user "alice" :secret "typed-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@typed.example") t))) (t ((:user "alice" :secret "typed-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@typed.example") t))) (nil ((:user "alice" :secret "typed-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@typed.example") t))))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_auth_source_max_zero_returns_boolean_for_success_and_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_auth_source_max_zero_returns_boolean_for_success_and_failure",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (let ((success
                                  (auth-source-search
                                   :host
                                   "max-zero.example"
                                   :user
                                   "alice"
                                   :max
                                   0)))
                             (setq
                              auth-source-kwallet-test-status
                              4)
                             (let ((failure
                                    (auth-source-search
                                     :host
                                     "missing.example"
                                     :user
                                     "alice"
                                     :max
                                     0)))
                               (list
                                success
                                failure
                                (nreverse
                                 auth-source-kwallet-test-process-calls)))))"##,
        expect![[
            r#"OK (t nil (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@max-zero.example") t) ("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@missing.example") t)))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_auth_source_cache_reuses_first_secret_without_second_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_auth_source_cache_reuses_first_secret_without_second_process",
        r##"(let ((auth-source-do-cache t)
                               (spec
                                '(:host
                                  "cached.example"
                                  :user
                                  "alice"
                                  :port
                                  "https"
                                  :max
                                  1)))
                           (auth-source-kwallet-test-enable-clean)
                           (setq auth-source-kwallet-test-output
                                 "first-secret")
                           (let ((first
                                  (apply
                                   #'auth-source-search
                                   spec)))
                             (setq
                              auth-source-kwallet-test-output
                              "second-secret")
                             (let ((second
                                    (apply
                                     #'auth-source-search
                                     spec)))
                               (list
                                first
                                second
                                (auth-source-remembered-p
                                 spec)
                                (auth-source-recall spec)
                                (nreverse
                                 auth-source-kwallet-test-process-calls)))))"##,
        expect![[
            r#"OK (#1=((:user "alice" :secret "first-secret")) #1# t #1# (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@cached.example") t)))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_reenable_flushes_cache_and_fetches_rotated_secret() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_reenable_flushes_cache_and_fetches_rotated_secret",
        r##"(let ((auth-source-do-cache t)
                               (spec
                                '(:host
                                  "rotate.example"
                                  :user
                                  "deploy"
                                  :max
                                  1)))
                           (auth-source-kwallet-test-enable-clean)
                           (setq auth-source-kwallet-test-output
                                 "old-token")
                           (let ((old
                                  (apply
                                   #'auth-source-search
                                   spec)))
                             (setq auth-source-kwallet-test-output
                                   "new-token")
                             (auth-source-kwallet-enable)
                             (let ((new
                                    (apply
                                     #'auth-source-search
                                     spec)))
                               (list
                                old
                                new
                                (nreverse
                                 auth-source-kwallet-test-process-calls)))))"##,
        expect![[
            r#"OK (((:user "deploy" :secret "old-token")) ((:user "deploy" :secret "new-token")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "deploy@rotate.example") t) ("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "deploy@rotate.example") t)))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_require_keys_are_forwarded_but_backend_returns_its_minimal_token()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_require_keys_are_forwarded_but_backend_returns_its_minimal_token",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (mapcar
                            (lambda (required)
                              (auth-source-kwallet-test-reset-process)
                              (setq
                               auth-source-kwallet-test-output
                               "required-secret")
                              (list
                               required
                               (auth-source-search
                                :host
                                "required.example"
                                :user
                                "alice"
                                :require
                                required
                                :max
                                1)
                               (nreverse
                                auth-source-kwallet-test-process-calls)))
                            '(nil
                              (:secret)
                              (:user :secret)
                              (:host)
                              (:port)
                              (:missing))))"##,
        expect![[
            r#"OK ((nil ((:user "alice" :secret "required-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@required.example") t))) ((:secret) ((:user "alice" :secret "required-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@required.example") t))) ((:user :secret) ((:user "alice" :secret "required-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@required.example") t))) ((:host) ((:user "alice" :secret "required-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@required.example") t))) ((:port) ((:user "alice" :secret "required-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@required.example") t))) ((:missing) ((:user "alice" :secret "required-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@required.example") t))))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_create_and_delete_requests_remain_read_only_searches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_create_and_delete_requests_remain_read_only_searches",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (mapcar
                            (lambda (operation)
                              (auth-source-kwallet-test-reset-process)
                              (setq
                               auth-source-kwallet-test-output
                               "read-only-secret")
                              (list
                               operation
                               (apply
                                #'auth-source-search
                                :host
                                "readonly.example"
                                :user
                                "alice"
                                :max
                                1
                                operation)
                               (nreverse
                                auth-source-kwallet-test-process-calls)))
                            '(()
                              (:create t)
                              (:create (:secret))
                              (:delete t)
                              (:create t :delete t))))"##,
        expect![[
            r#"OK ((nil ((:user "alice" :secret "read-only-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@readonly.example") t))) ((:create t) ((:user "alice" :secret "read-only-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@readonly.example") t))) ((:create (:secret)) ((:user "alice" :secret "read-only-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@readonly.example") t))) ((:delete t) ((:user "alice" :secret "read-only-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@readonly.example") t))) ((:create t :delete t) ((:user "alice" :secret "read-only-secret")) (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@readonly.example") t))))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_multiple_account_workflow_caches_each_host_user_spec_independently()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_multiple_account_workflow_caches_each_host_user_spec_independently",
        r##"(let ((auth-source-do-cache t)
                               (first-spec
                                '(:host
                                  "git.example"
                                  :user
                                  "alice"
                                  :max
                                  1))
                               (second-spec
                                '(:host
                                  "git.example"
                                  :user
                                  "robot"
                                  :max
                                  1)))
                           (auth-source-kwallet-test-enable-clean)
                           (setq auth-source-kwallet-test-output
                                 "alice-token")
                           (let ((alice
                                  (apply
                                   #'auth-source-search
                                   first-spec)))
                             (setq
                              auth-source-kwallet-test-output
                              "robot-token")
                             (let ((robot
                                    (apply
                                     #'auth-source-search
                                     second-spec)))
                               (setq
                                auth-source-kwallet-test-output
                                "must-not-be-read")
                               (list
                                alice
                                robot
                                (apply
                                 #'auth-source-search
                                 first-spec)
                                (apply
                                 #'auth-source-search
                                 second-spec)
                                (nreverse
                                 auth-source-kwallet-test-process-calls)))))"##,
        expect![[
            r#"OK (#1=((:user "alice" :secret "alice-token")) #2=((:user "robot" :secret "robot-token")) #1# #2# (("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "alice@git.example") t) ("kwallet-query" nil "*kwallet-output*" nil ("Passwords" "-f" "Passwords" "-r" "robot@git.example") t)))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_kwallet_auth_source_list_values_surface_backend_key_concatenation_limit()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_auth_source_list_values_surface_backend_key_concatenation_limit",
        r##"(let ((auth-source-do-cache nil))
                           (auth-source-kwallet-test-enable-clean)
                           (mapcar
                            (lambda (spec)
                              (auth-source-kwallet-test-reset-process)
                              (list
                               spec
                               (auth-source-kwallet-test-error
                                (lambda ()
                                  (apply
                                   #'auth-source-search
                                   spec)))
                               (nreverse
                                auth-source-kwallet-test-process-calls)
                               (get-buffer
                                "*kwallet-output*")))
                            '((:host
                               ("one.example"
                                "two.example")
                               :user
                               "alice"
                               :max
                               1)
                              (:host
                               "one.example"
                               :user
                               ("alice" "robot")
                               :max
                               1))))"##,
        expect![[
            r#"OK (((:host ("one.example" "two.example") :user "alice" :max 1) (:signal wrong-type-argument (characterp "one.example")) nil nil) ((:host "one.example" :user ("alice" "robot") :max 1) (:signal wrong-type-argument (characterp "alice")) nil nil))"#
        ]],
    )
}

fn auth_source_kwallet_real_executable_round_trip_returns_wallet_folder_and_key_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_real_executable_round_trip_returns_wallet_folder_and_key_output",
        r##"(let* ((fixture-root
                                 (expand-file-name
                                  "kwallet-real-process"
                                  user-emacs-directory))
                                (bin-directory
                                 (expand-file-name
                                  "bin"
                                  fixture-root))
                                (script
                                 (expand-file-name
                                  "kwallet-fixture-query"
                                  bin-directory))
                                (auth-source-kwallet-wallet
                                 "Real Wallet")
                                (auth-source-kwallet-folder
                                 "Real Folder")
                                (auth-source-kwallet-key-separator
                                 "::")
                                (auth-source-kwallet-executable
                                 "kwallet-fixture-query"))
                           (unwind-protect
                               (progn
                                 (make-directory
                                  bin-directory
                                  t)
                                 (write-region
                                  "#!/bin/sh\nprintf 'wallet=%s\\nfolder=%s\\nkey=%s\\n' \"$1\" \"$3\" \"$5\"\n"
                                  nil
                                  script
                                  nil
                                  'silent)
                                 (set-file-modes
                                  script
                                  #o755)
                                 (cl-letf
                                     (((symbol-function
                                        'executable-find)
                                       #'auth-source-kwallet-test-real-executable-find)
                                      ((symbol-function
                                        'call-process)
                                       #'auth-source-kwallet-test-real-call-process))
                                   (let ((exec-path
                                          (cons
                                           bin-directory
                                           exec-path)))
                                     (auth-source-kwallet--kwallet-search
                                      :host
                                      "real.example"
                                      :user
                                      "deploy"))))
                             (when
                                 (file-directory-p
                                  fixture-root)
                               (delete-directory
                                fixture-root
                                t))))"##,
        expect![[
            r#"OK ((:user "deploy" :secret "wallet=Real Wallet\nfolder=Real Folder\nkey=deploy::real.example"))"#
        ]],
    )
}

fn auth_source_kwallet_real_executable_nonzero_exit_is_a_missing_credential() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_real_executable_nonzero_exit_is_a_missing_credential",
        r##"(let* ((fixture-root
                                 (expand-file-name
                                  "kwallet-real-failure"
                                  user-emacs-directory))
                                (bin-directory
                                 (expand-file-name
                                  "bin"
                                  fixture-root))
                                (script
                                 (expand-file-name
                                  "kwallet-fixture-failure"
                                  bin-directory))
                                (auth-source-kwallet-executable
                                 "kwallet-fixture-failure"))
                           (unwind-protect
                               (progn
                                 (make-directory
                                  bin-directory
                                  t)
                                 (write-region
                                  "#!/bin/sh\nprintf 'ignored-secret\\n'\nexit 23\n"
                                  nil
                                  script
                                  nil
                                  'silent)
                                 (set-file-modes
                                  script
                                  #o755)
                                 (cl-letf
                                     (((symbol-function
                                        'executable-find)
                                       #'auth-source-kwallet-test-real-executable-find)
                                      ((symbol-function
                                        'call-process)
                                       #'auth-source-kwallet-test-real-call-process))
                                   (let ((exec-path
                                          (cons
                                           bin-directory
                                           exec-path)))
                                     (auth-source-kwallet--kwallet-search
                                      :host
                                      "missing.example"
                                      :user
                                      "deploy"))))
                             (when
                                 (file-directory-p
                                  fixture-root)
                               (delete-directory
                                fixture-root
                                t))))"##,
        expect!["OK nil"],
    )
}

fn auth_source_kwallet_real_executable_integrates_with_auth_source_password_lookup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_kwallet_real_executable_integrates_with_auth_source_password_lookup",
        r##"(let* ((fixture-root
                                 (expand-file-name
                                  "kwallet-real-auth-source"
                                  user-emacs-directory))
                                (bin-directory
                                 (expand-file-name
                                  "bin"
                                  fixture-root))
                                (script
                                 (expand-file-name
                                  "kwallet-fixture-auth"
                                  bin-directory))
                                (auth-source-kwallet-executable
                                 "kwallet-fixture-auth")
                                (auth-source-do-cache
                                 nil))
                           (unwind-protect
                               (progn
                                 (make-directory
                                  bin-directory
                                  t)
                                 (write-region
                                  "#!/bin/sh\nprintf 'token-for-%s\\n' \"$5\"\n"
                                  nil
                                  script
                                  nil
                                  'silent)
                                 (set-file-modes
                                  script
                                  #o755)
                                 (cl-letf
                                     (((symbol-function
                                        'executable-find)
                                       #'auth-source-kwallet-test-real-executable-find)
                                      ((symbol-function
                                        'call-process)
                                       #'auth-source-kwallet-test-real-call-process))
                                   (let ((exec-path
                                          (cons
                                           bin-directory
                                           exec-path)))
                                     (auth-source-kwallet-test-enable-clean)
                                     (auth-source-pick-first-password
                                      :host
                                      "git.example"
                                      :user
                                      "ci-bot"))))
                             (when
                                 (file-directory-p
                                  fixture-root)
                               (delete-directory
                                fixture-root
                                t))))"##,
        expect![[r#"OK "token-for-ci-bot@git.example""#]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_kwallet_real_auth_source_search_returns_secret_and_exact_process_request(),
        auth_source_kwallet_real_auth_source_pick_first_password_supports_mail_client_usage(),
        auth_source_kwallet_auth_source_type_filter_is_forwarded_but_backend_runs_for_every_value(),
        auth_source_kwallet_auth_source_max_zero_returns_boolean_for_success_and_failure(),
        auth_source_kwallet_auth_source_cache_reuses_first_secret_without_second_process(),
        auth_source_kwallet_reenable_flushes_cache_and_fetches_rotated_secret(),
        auth_source_kwallet_require_keys_are_forwarded_but_backend_returns_its_minimal_token(),
        auth_source_kwallet_create_and_delete_requests_remain_read_only_searches(),
        auth_source_kwallet_multiple_account_workflow_caches_each_host_user_spec_independently(),
        auth_source_kwallet_auth_source_list_values_surface_backend_key_concatenation_limit(),
        auth_source_kwallet_real_executable_round_trip_returns_wallet_folder_and_key_output(),
        auth_source_kwallet_real_executable_nonzero_exit_is_a_missing_credential(),
        auth_source_kwallet_real_executable_integrates_with_auth_source_password_lookup(),
    ]
}
