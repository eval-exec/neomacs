use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_gopass_search_trims_a_real_single_line_secret() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_trims_a_real_single_line_secret",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (program)
                 (push (list :find program) events)
                 "/fixture/bin/gopass"))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push (list :shell command) events)
                 "  correct horse battery staple \n")))
           (list
            (auth-source-gopass-search
             :host "smtp.example"
             :user "alice@example"
             :port 587)
            (nreverse events))))"##,
        expect![[
            r#"OK (((:user "alice@example" :secret "correct horse battery staple")) ((:find "gopass") (:shell "gopass show -o accounts/smtp.example/alice\\@example")))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_gopass_search_preserves_internal_newlines_and_unicode() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_preserves_internal_newlines_and_unicode",
        r##"(cl-letf
         (((symbol-function 'executable-find)
           (lambda (_program)
             "/fixture/bin/gopass"))
          ((symbol-function 'shell-command-to-string)
           (lambda (_command)
             "\n  first line\n第二行\nthird line  \n\n")))
         (auth-source-gopass-search
          :host "notes.example"
          :user "λ-user"))"##,
        expect![[r#"OK ((:user "λ-user" :secret "first line\n第二行\nthird line"))"#]],
    )
    .fresh_process()
}

fn auth_source_gopass_search_returns_an_empty_secret_for_whitespace_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_returns_an_empty_secret_for_whitespace_output",
        r##"(cl-letf
         (((symbol-function 'executable-find)
           (lambda (_program)
             "/fixture/bin/gopass"))
          ((symbol-function 'shell-command-to-string)
           (lambda (_command)
             " \t\n\r\n ")))
         (auth-source-gopass-search
          :host "empty.example"
          :user "empty-user"))"##,
        expect![[r#"OK ((:user "empty-user" :secret ""))"#]],
    )
    .fresh_process()
}

fn auth_source_gopass_search_result_uses_requested_user_verbatim() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_result_uses_requested_user_verbatim",
        r##"(let ((auth-source-gopass-construct-query-path
                (lambda (&rest _arguments)
                  "fixed/path")))
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (_program)
                 "/fixture/bin/gopass"))
              ((symbol-function 'shell-command-to-string)
               (lambda (_command)
                 "secret")))
           (mapcar
            (lambda (user)
              (auth-source-gopass-search
               :host "host"
               :user user))
            '("alice" nil user-symbol 17))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "secret")) ((:user nil :secret "secret")) ((:user user-symbol :secret "secret")) ((:user 17 :secret "secret")))"#
        ]],
    )
}

fn auth_source_gopass_search_ignores_unconsumed_auth_source_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_ignores_unconsumed_auth_source_keys",
        r##"(let (captured)
         (let ((auth-source-gopass-construct-query-path
                (lambda (&rest arguments)
                  (setq captured arguments)
                  "accounts/host/alice")))
           (cl-letf
               (((symbol-function 'executable-find)
                 (lambda (_program)
                   "/fixture/bin/gopass"))
                ((symbol-function 'shell-command-to-string)
                 (lambda (_command)
                   "secret")))
             (list
              (auth-source-gopass-search
               :backend :backend
               :type :type
               :host "host"
               :user "alice"
               :port "submission"
               :require '(:secret)
               :max 7
               :create t
               :delete :ignored)
              captured))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "secret")) (:backend :type "host" "alice" "submission"))"#
        ]],
    )
}

fn auth_source_gopass_search_checks_the_configured_executable() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_checks_the_configured_executable",
        r##"(let ((auth-source-gopass-executable
                "gopass-company")
               calls)
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (program)
                 (push (list :find program) calls)
                 "/opt/company/gopass-company"))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push (list :shell command) calls)
                 "secret")))
           (list
            (auth-source-gopass-search
             :host "mail"
             :user "alice")
            (nreverse calls))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "secret")) ((:find "gopass-company") (:shell "gopass-company show -o accounts/mail/alice")))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_gopass_missing_executable_warns_and_skips_the_shell() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_missing_executable_warns_and_skips_the_shell",
        r##"(let ((auth-source-gopass-executable
                "missing-gopass")
               events)
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (program)
                 (push (list :find program) events)
                 nil))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push (list :unexpected-shell command) events)
                 "must-not-run"))
              ((symbol-function 'warn)
               (lambda (format-string &rest arguments)
                 (push
                  (list
                   :warn
                   format-string
                   arguments)
                  events)
                 :warned)))
           (list
            (auth-source-gopass-search
             :host "mail"
             :user "alice")
            (nreverse events))))"##,
        expect![[
            r#"OK (:warned ((:find "missing-gopass") (:warn "`auth-source-gopass': Could not find executable '%s' to query gopass" ("missing-gopass"))))"#
        ]],
    )
}

fn auth_source_gopass_search_does_not_cache_repeated_credentials() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_does_not_cache_repeated_credentials",
        r##"(let ((answers '("first\n" "second\n" "third\n"))
               calls)
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (program)
                 (push (list :find program) calls)
                 "/fixture/bin/gopass"))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (let ((answer (pop answers)))
                   (push (list :shell command answer) calls)
                   answer))))
           (list
            (auth-source-gopass-search :host "mail" :user "alice")
            (auth-source-gopass-search :host "mail" :user "alice")
            (auth-source-gopass-search :host "mail" :user "alice")
            (nreverse calls))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "first")) ((:user "alice" :secret "second")) ((:user "alice" :secret "third")) ((:find "gopass") (:shell "gopass show -o accounts/mail/alice" "first\n") (:find "gopass") (:shell "gopass show -o accounts/mail/alice" "second\n") (:find "gopass") (:shell "gopass show -o accounts/mail/alice" "third\n")))"#
        ]],
    )
    .fresh_process()
}

fn auth_source_gopass_search_propagates_constructor_and_shell_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_propagates_constructor_and_shell_signals",
        r##"(list
         (let ((auth-source-gopass-construct-query-path
                (lambda (&rest _arguments)
                  (error "bad path"))))
           (cl-letf
               (((symbol-function 'executable-find)
                 (lambda (_program) t)))
             (auth-source-gopass-test-error-data
              (lambda ()
                (auth-source-gopass-search
                 :host "mail"
                 :user "alice")))))
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (_program) t))
              ((symbol-function 'shell-command-to-string)
               (lambda (_command)
                 (error "process failed"))))
           (auth-source-gopass-test-error-data
            (lambda ()
              (auth-source-gopass-search
               :host "mail"
               :user "alice")))))"##,
        expect![[r#"OK ((:error error ("bad path")) (:error error ("process failed")))"#]],
    )
    .fresh_process()
}

pub(super) fn search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_gopass_search_trims_a_real_single_line_secret(),
        auth_source_gopass_search_preserves_internal_newlines_and_unicode(),
        auth_source_gopass_search_returns_an_empty_secret_for_whitespace_output(),
        auth_source_gopass_search_result_uses_requested_user_verbatim(),
        auth_source_gopass_search_ignores_unconsumed_auth_source_keys(),
        auth_source_gopass_search_checks_the_configured_executable(),
        auth_source_gopass_missing_executable_warns_and_skips_the_shell(),
        auth_source_gopass_search_does_not_cache_repeated_credentials(),
        auth_source_gopass_search_propagates_constructor_and_shell_signals(),
    ]
}
