use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_gopass_default_path_maps_real_account_coordinates() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_default_path_maps_real_account_coordinates",
        r##"(mapcar
         (lambda (coordinates)
           (apply
            #'auth-source-gopass--gopass-construct-query-path
            coordinates))
         '((backend login "smtp.example.test" "alice@example.test" 587)
           (nil nil "space host" "Ada Lovelace" nil)
           (ignored ignored "δοκιμή.example" "λ-user" "443")))"##,
        expect![[
            r#"OK ("accounts/smtp.example.test/alice@example.test" "accounts/space host/Ada Lovelace" "accounts/δοκιμή.example/λ-user")"#
        ]],
    )
}

fn auth_source_gopass_path_respects_custom_prefix_and_separator() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_path_respects_custom_prefix_and_separator",
        r##"(mapcar
         (lambda (configuration)
           (let ((auth-source-gopass-path-prefix
                  (nth 0 configuration))
                 (auth-source-gopass-path-separator
                  (nth 1 configuration)))
             (auth-source-gopass--gopass-construct-query-path
              :backend
              :type
              (nth 2 configuration)
              (nth 3 configuration)
              993)))
         '(("team/vault" "::" "mail.example" "alice")
           ("" "/" "host" "user")
           ("root" "" "host" "user")
           ("accounts" " → " "主机" "用户")))"##,
        expect![[
            r#"OK ("team/vault::mail.example::alice" "/host/user" "roothostuser" "accounts → 主机 → 用户")"#
        ]],
    )
}

fn auth_source_gopass_path_exposes_nil_and_non_string_component_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_path_exposes_nil_and_non_string_component_contracts",
        r##"(mapcar
         (lambda (arguments)
           (auth-source-gopass-test-error-data
            (lambda ()
              (apply
               #'auth-source-gopass--gopass-construct-query-path
               arguments))))
         '((nil nil nil "alice" nil)
           (nil nil "host" nil nil)
           (nil nil host "alice" nil)
           (nil nil "host" 42 nil)))"##,
        expect![[
            r#"OK ((:ok "accounts//alice") (:ok "accounts/host/") (:error wrong-type-argument (sequencep host)) (:error wrong-type-argument (sequencep 42)))"#
        ]],
    )
}

fn auth_source_gopass_path_dynamic_configuration_does_not_leak() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_path_dynamic_configuration_does_not_leak",
        r##"(list
         (auth-source-gopass--gopass-construct-query-path
          nil nil "host" "alice" nil)
         (let ((auth-source-gopass-path-prefix "work")
               (auth-source-gopass-path-separator "."))
           (auth-source-gopass--gopass-construct-query-path
            nil nil "host" "alice" nil))
         (auth-source-gopass--gopass-construct-query-path
          nil nil "host" "alice" nil)
         auth-source-gopass-path-prefix
         auth-source-gopass-path-separator)"##,
        expect![[
            r#"OK ("accounts/host/alice" "work.host.alice" "accounts/host/alice" "accounts" "/")"#
        ]],
    )
}

fn auth_source_gopass_search_passes_every_coordinate_to_custom_constructor() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_search_passes_every_coordinate_to_custom_constructor",
        r##"(let (constructor-arguments
               events)
         (let ((auth-source-gopass-construct-query-path
                (lambda (&rest arguments)
                  (setq constructor-arguments
                        arguments)
                  "vault/item")))
           (cl-letf
               (((symbol-function 'executable-find)
                 (lambda (program)
                   (push (list :find program) events)
                   "/fixture/bin/gopass"))
                ((symbol-function 'shell-command-to-string)
                 (lambda (command)
                   (push (list :shell command) events)
                   "secret\n")))
             (list
              (auth-source-gopass-search
               :backend 'fixture-backend
               :type 'gopass
               :host "smtp.example"
               :user "alice"
               :port 587
               :require '(:user :secret)
               :max 1)
              constructor-arguments
              (nreverse events)))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "secret")) (fixture-backend gopass "smtp.example" "alice" 587) ((:find "gopass") (:shell "gopass show -o vault/item")))"#
        ]],
    )
}

fn auth_source_gopass_custom_constructor_output_is_shell_quoted_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_gopass_custom_constructor_output_is_shell_quoted_once",
        r##"(let ((auth-source-gopass-executable
                "/fixture/bin/go pass")
               (auth-source-gopass-construct-query-path
                (lambda (&rest _arguments)
                  "team vault/alice's smtp; echo unsafe"))
               commands)
         (cl-letf
             (((symbol-function 'executable-find)
               (lambda (_program)
                 "/fixture/bin/go pass"))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push command commands)
                 "p@ss word\n")))
           (list
            (auth-source-gopass-search
             :host "smtp.example"
             :user "alice")
            (nreverse commands))))"##,
        expect![[
            r#"OK (((:user "alice" :secret "p@ss word")) ("/fixture/bin/go pass show -o team\\ vault/alice\\'s\\ smtp\\;\\ echo\\ unsafe"))"#
        ]],
    )
}

pub(super) fn paths_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_gopass_default_path_maps_real_account_coordinates(),
        auth_source_gopass_path_respects_custom_prefix_and_separator(),
        auth_source_gopass_path_exposes_nil_and_non_string_component_contracts(),
        auth_source_gopass_path_dynamic_configuration_does_not_leak(),
        auth_source_gopass_search_passes_every_coordinate_to_custom_constructor(),
        auth_source_gopass_custom_constructor_output_is_shell_quoted_once(),
    ]
}
