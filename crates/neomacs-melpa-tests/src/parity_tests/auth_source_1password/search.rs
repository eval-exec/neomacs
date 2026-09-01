use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_1password_search_builds_exact_default_cli_command_and_trims_secret()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_builds_exact_default_cli_command_and_trims_secret",
        r##"(let (find-calls commands)
          (cl-letf
              (((symbol-function 'executable-find)
                (lambda (program)
                  (push program find-calls)
                  "/fixture/bin/op"))
               ((symbol-function 'shell-command-to-string)
                (lambda (command)
                  (push command commands)
                  " \tproduction-secret\n\n")))
            (list
             (auth-source-1password-search
              :backend
              auth-source-1password-backend
              :type 'password-store
              :host "api.example.com"
              :user "deploy"
              :port 443)
             (nreverse find-calls)
             (nreverse commands))))"##,
        expect![[
            r#"OK (((:user "deploy" :secret "production-secret")) ("op") ("op read op://Personal/api.example.com/deploy"))"#
        ]],
    )
}

fn auth_source_1password_search_shell_quotes_complex_secret_reference_as_one_argument()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_shell_quotes_complex_secret_reference_as_one_argument",
        r##"(let (commands)
          (let ((auth-source-1password-vault
                 "Team's Shared Vault")
                (auth-source-1password-executable
                 "/opt/one password/bin/op"))
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (_program)
                    t))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push command commands)
                    "secret")))
              (list
               (auth-source-1password-search
                :host "host name;printf injected"
                :user "o'hara/$USER/$(id)"
                :port "https")
               (nreverse commands)))))"##,
        expect![[
            r#"OK (((:user "o'hara/$USER/$(id)" :secret "secret")) ("/opt/one password/bin/op read op://Team\\'s\\ Shared\\ Vault/host\\ name\\;printf\\ injected/o\\'hara/\\$USER/\\$\\(id\\)"))"#
        ]],
    )
}

fn auth_source_1password_search_trims_only_outer_whitespace_across_real_outputs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_search_trims_only_outer_whitespace_across_real_outputs",
        r##"(let ((outputs
                '("secret"
                  "  secret  "
                  "\nsecret\n"
                  "\t secret with internal  spaces \r\n"
                  "line one\nline two\n"
                  " \n\t\r")))
          (mapcar
           (lambda (output)
             (cl-letf
                 (((symbol-function 'executable-find)
                   (lambda (_program)
                     t))
                  ((symbol-function 'shell-command-to-string)
                   (lambda (_command)
                     output)))
               (list
                output
                (auth-source-1password-search
                 :host "host"
                 :user "user"))))
           outputs))"##,
        expect![[
            r#"OK (("secret" ((:user "user" :secret "secret"))) ("  secret  " ((:user "user" :secret "secret"))) ("\nsecret\n" ((:user "user" :secret "secret"))) ("\11 secret with internal  spaces \15\n" ((:user "user" :secret "secret with internal  spaces"))) ("line one\nline two\n" ((:user "user" :secret "line one\nline two"))) (" \n\11\15" ((:user "user" :secret ""))))"#
        ]],
    )
}

fn auth_source_1password_search_empty_cli_output_is_a_present_empty_secret() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_empty_cli_output_is_a_present_empty_secret",
        r##"(cl-letf
          (((symbol-function 'executable-find)
            (lambda (_program)
              t))
           ((symbol-function 'shell-command-to-string)
            (lambda (_command)
              "")))
          (let ((result
                 (auth-source-1password-search
                  :host "empty.example"
                  :user nil
                  :port nil)))
            (list
             result
             (length result)
             (plist-member
              (car result)
              :user)
             (plist-member
              (car result)
              :secret)
             (equal
              (plist-get
               (car result)
               :secret)
              ""))))"##,
        expect![[r#"OK ((#1=(:user nil . #2=(:secret ""))) 1 #1# #2# t)"#]],
    )
}

fn auth_source_1password_search_calls_each_seam_once_in_strict_order_and_ignores_extra_keys()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_calls_each_seam_once_in_strict_order_and_ignores_extra_keys",
        r##"(let (events)
          (let ((auth-source-1password-executable
                 "fixture-op")
                (auth-source-1password-construct-secret-reference
                 (lambda (backend type host user port)
                   (push
                    (list
                     :construct
                     (eq
                      backend
                      auth-source-1password-backend)
                     type host user port)
                    events)
                   "reference")))
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (push
                     (list :find program)
                     events)
                    "/found/op"))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push
                     (list :shell command)
                     events)
                    "value")))
              (list
               (auth-source-1password-search
                :backend
                auth-source-1password-backend
                :type 'password-store
                :host "host"
                :user "user"
                :port 8443
                :max 17
                :require '(:secret)
                :create :fixture
                :delete t
                :unknown 'preserved-by-caller)
               (nreverse events)))))"##,
        expect![[
            r#"OK (((:user "user" :secret "value")) ((:find "fixture-op") (:construct t password-store "host" "user" 8443) (:shell "fixture-op read op://reference")))"#
        ]],
    )
}

fn auth_source_1password_search_missing_executable_warns_once_and_skips_all_secret_work()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_missing_executable_warns_once_and_skips_all_secret_work",
        r##"(let (events warnings)
          (let ((auth-source-1password-executable
                 "missing-op")
                (auth-source-1password-construct-secret-reference
                 (lambda (&rest arguments)
                   (push
                    (cons :construct arguments)
                    events)
                   "unexpected")))
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (push
                     (list :find program)
                     events)
                    nil))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push
                     (list :shell command)
                     events)
                    "unexpected"))
                 ((symbol-function 'warn)
                  (lambda (format-string &rest arguments)
                    (push
                     (list
                      format-string
                      arguments
                      (apply
                       #'format-message
                       format-string
                       arguments))
                     warnings)
                    :warning-recorded)))
              (list
               (auth-source-1password-search
                :host "host"
                :user "user")
               (nreverse events)
               (nreverse warnings)))))"##,
        expect![[
            r#"OK (:warning-recorded ((:find "missing-op")) (("`auth-source-1password': Could not find executable '%s' to query 1password" ("missing-op") "‘auth-source-1password’: Could not find executable ’missing-op’ to query 1password")))"#
        ]],
    )
}

fn auth_source_1password_search_propagates_reference_constructor_failure_before_shell()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_propagates_reference_constructor_failure_before_shell",
        r##"(let (events)
          (let ((auth-source-1password-construct-secret-reference
                 (lambda (&rest arguments)
                   (push
                    (cons :construct arguments)
                    events)
                   (error
                    "cannot construct fixture reference"))))
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (push
                     (list :find program)
                     events)
                    t))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push
                     (list :shell command)
                     events)
                    "unexpected")))
              (list
               (auth-source-1password-test-error-data
                (lambda ()
                  (auth-source-1password-search
                   :host "host"
                   :user "user")))
               (nreverse events)))))"##,
        expect![[
            r#"OK ((:error error ("cannot construct fixture reference")) ((:find "op") (:construct nil nil "host" "user" nil)))"#
        ]],
    )
}

fn auth_source_1password_search_propagates_executable_lookup_and_shell_failures() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_search_propagates_executable_lookup_and_shell_failures",
        r##"(list
          (let (events)
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (push
                     (list :find program)
                     events)
                    (error
                     "lookup failed")))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push
                     (list :shell command)
                     events)
                    "unexpected")))
              (list
               (auth-source-1password-test-error-data
                (lambda ()
                  (auth-source-1password-search
                   :host "host"
                   :user "user")))
               (nreverse events))))
          (let (events)
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (push
                     (list :find program)
                     events)
                    t))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push
                     (list :shell command)
                     events)
                    (error
                     "CLI failed"))))
              (list
               (auth-source-1password-test-error-data
                (lambda ()
                  (auth-source-1password-search
                   :host "host"
                   :user "user")))
               (nreverse events)))))"##,
        expect![[
            r#"OK (((:error error ("lookup failed")) ((:find "op"))) ((:error error ("CLI failed")) ((:find "op") (:shell "op read op://Personal/host/user"))))"#
        ]],
    )
}

fn auth_source_1password_search_uses_configured_executable_text_but_not_find_result_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_search_uses_configured_executable_text_but_not_find_result_path",
        r##"(let (find-calls commands)
          (let ((auth-source-1password-executable
                 "custom-op --account work"))
            (cl-letf
                (((symbol-function 'executable-find)
                  (lambda (program)
                    (push program find-calls)
                    "/completely/different/resolved-op"))
                 ((symbol-function 'shell-command-to-string)
                  (lambda (command)
                    (push command commands)
                    "secret")))
              (list
               (auth-source-1password-search
                :host "service"
                :user "robot")
               (nreverse find-calls)
               (nreverse commands)))))"##,
        expect![[
            r#"OK (((:user "robot" :secret "secret")) ("custom-op --account work") ("custom-op --account work read op://Personal/service/robot"))"#
        ]],
    )
}

fn auth_source_1password_real_fake_cli_receives_read_and_one_reference_argument() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_real_fake_cli_receives_read_and_one_reference_argument",
        r##"(let* ((root
                (getenv
                 "NEOMACS_TEST_SANDBOX_ROOT"))
               (bin
                (expand-file-name
                 "bin"
                 root))
               (script
                (expand-file-name
                 "op-fixture"
                 bin))
               (arguments
                (expand-file-name
                 "op-arguments.txt"
                 root)))
          (make-directory bin t)
          (with-temp-file script
            (insert
             "#!/bin/sh\n"
             "set -eu\n"
             "printf '%s\\n' \"$@\" > \"$NEOMACS_TEST_SANDBOX_ROOT/op-arguments.txt\"\n"
             "printf '  fixture-secret:line-one\\nline-two  \\n'\n"))
          (set-file-modes script #o755)
          (let ((auth-source-1password-executable
                 script)
                (auth-source-1password-vault
                 "Automation"))
            (list
             (auth-source-1password-search
              :host "ci.example"
              :user "release-bot"
              :port 443)
             (auth-source-1password-test-read-file
              arguments)
             (file-modes script))))"##,
        expect![[
            r#"OK (((:user "release-bot" :secret "fixture-secret:line-one\nline-two")) "read\nop://Automation/ci.example/release-bot\n" 493)"#
        ]],
    )
}

pub(super) fn search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_1password_search_builds_exact_default_cli_command_and_trims_secret(),
        auth_source_1password_search_shell_quotes_complex_secret_reference_as_one_argument(),
        auth_source_1password_search_trims_only_outer_whitespace_across_real_outputs(),
        auth_source_1password_search_empty_cli_output_is_a_present_empty_secret(),
        auth_source_1password_search_calls_each_seam_once_in_strict_order_and_ignores_extra_keys(),
        auth_source_1password_search_missing_executable_warns_once_and_skips_all_secret_work(),
        auth_source_1password_search_propagates_reference_constructor_failure_before_shell(),
        auth_source_1password_search_propagates_executable_lookup_and_shell_failures(),
        auth_source_1password_search_uses_configured_executable_text_but_not_find_result_path(),
        auth_source_1password_real_fake_cli_receives_read_and_one_reference_argument(),
    ]
}
