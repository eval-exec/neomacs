use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_utils_transform_real_commands_modules_paths_versions_and_extensions() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alchemist_utils_transform_real_commands_modules_paths_versions_and_extensions",
        r##"(list
                      (mapcar
                       #'alchemist-utils-build-command
                       '(("MIX_ENV=test" "mix" ("test" "--stale") nil "")
                         ("elixir" "-e" "IO.puts(1)")
                         "mix help"))
                      (alchemist-utils-prepare-aliases-for-elixir
                       '(("List" "MyList")
                         ("Plug.Conn.AlreadySentError" "AlreadySentError")))
                      (alchemist-utils-prepare-modules-for-elixir
                       '("Mix.Generator" "ExUnit" ""))
                      (mapcar
                       #'alchemist-utils--snakecase-to-camelcase
                       '("my_app/http_client" "alreadyCamel" "_private_name"))
                      (mapcar
                       (lambda (entry)
                         (apply
                          #'alchemist-utils-add-ext-to-path-if-not-present
                          entry))
                       '(("lib/user" ".ex")
                         ("test/user_test.exs" ".exs")
                         ("name.ex" ".exs")))
                      (mapcar
                       #'alchemist-utils-path-to-module-name
                       '("my_app/accounts/user.ex"
                         "/umbrella/apps/web/lib/web/router.ex"
                         "//foo//"))
                      (mapcar
                       (lambda (version)
                         (list
                          version
                          (alchemist-utils-elixir-version-check-p
                           1 3 0 version)
                          (alchemist-utils-elixir-version-check-p
                           1 4 2 version)))
                       '("1.2.9" "1.3.0" "1.4.2" "2.0.0")))"##,
        expect![[
            r#"OK (("MIX_ENV=test mix test --stale" "elixir -e IO.puts(1)" "mix help") "[{MyList, List},{AlreadySentError, Plug.Conn.AlreadySentError}]" "[Mix.Generator,ExUnit,]" ("MyApp/HttpClient" "Alreadycamel" "PrivateName") ("lib/user.ex" "test/user_test.exs" "name.ex.exs") ("MyApp.Accounts.User" "Umbrella.Apps.Web.Lib.Web.Router" "Foo") (("1.2.9" nil nil) ("1.3.0" t nil) ("1.4.2" t t) ("2.0.0" t t)))"#
        ]],
    )
}

fn alchemist_scope_recovers_nested_module_alias_import_and_use_context_from_real_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_scope_recovers_nested_module_alias_import_and_use_context_from_real_code",
        r##"(with-temp-buffer
                      (insert
                       "defmodule Outer do\n"
                       "  alias Phoenix.Router.{Resource, Scope}\n"
                       "  alias Plug.Conn, as: Connection\n"
                       "  import ExUnit.Assertions\n"
                       "  use GenServer\n\n"
                       "  defmodule Inner42 do\n"
                       "    alias String.{Break, Casing}\n"
                       "    import Mix.Generator\n"
                       "    use MyApp.Web\n"
                       "    def build do\n"
                       "      Connection.assign(nil, :x, 1)\n"
                       "    end\n"
                       "  end\n"
                       "end\n")
                      (goto-char (point-min))
                      (search-forward "Connection.assign")
                      (list
                       (alchemist-scope-module)
                       (alchemist-scope-aliases)
                       (alchemist-scope-import-modules)
                       (alchemist-scope-use-modules)
                       (alchemist-scope-all-modules)
                       (alchemist-scope-alias-full-path
                        "Connection.Query")
                       (alchemist-scope-expression)))"##,
        expect![[
            r#"OK ("Inner42" (("String.Break" "Break") ("String.Casing" "Casing")) ("Mix.Generator") ("MyApp.Web") ("Mix.Generator" "MyApp.Web" "Inner42") "Connection.Query" "Connection.assign")"#
        ]],
    )
}

fn alchemist_scope_expression_and_extractors_cover_elixir_erlang_locals_and_punctuation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_scope_expression_and_extractors_cover_elixir_erlang_locals_and_punctuation",
        r##"(let ((expressions
                           '(":gen_tcp.accept"
                             ":erlang"
                             "List.duplicate"
                             "String.Chars.impl_for"
                             "Whatever._duplicate"
                             "Enum.take!"
                             "_local?"
                             "Module."
                             "to_string")))
                      (list
                       (mapcar
                        (lambda (expression)
                          (list
                           expression
                           (alchemist-scope-extract-module expression)
                           (alchemist-scope-extract-function expression)))
                        expressions)
                       (with-temp-buffer
                         (insert
                          "Map.get(config, :key)\n"
                          "foo[:bar]\n"
                          "\"Enum.map ignored\"\n"
                          "My.App.call!(value)\n")
                         (mapcar
                          (lambda (needle)
                            (goto-char (point-min))
                            (search-forward needle)
                            (alchemist-scope-expression))
                          '("Map.get" ":bar" "Enum.map" "call!")))))"##,
        expect![[
            r#"OK (((":gen_tcp.accept" ":gen_tcp" "accept") (":erlang" ":erlang" nil) ("List.duplicate" "List" "duplicate") ("String.Chars.impl_for" "String.Chars" "impl_for") ("Whatever._duplicate" "Whatever" "_duplicate") ("Enum.take!" "Enum" "take!") ("_local?" nil "_local?") ("Module." "Module" nil) ("to_string" nil "to_string")) ("Map.get" ":bar" "Enum.map" "My.App.call!"))"#
        ]],
    )
}

fn alchemist_scope_syntax_distinguishes_real_module_body_heredoc_and_nested_locations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_scope_syntax_distinguishes_real_module_body_heredoc_and_nested_locations",
        r##"(with-temp-buffer
                      (insert
                       "defmodule Outer do\n"
                       "  @moduledoc \"\"\"\n"
                       "  defmodule Fake do\n"
                       "  end\n"
                       "  \"\"\"\n"
                       "  defmodule Inner do\n"
                       "    def run, do: :ok\n"
                       "  end\n"
                       "end\n")
                      (mapcar
                       (lambda (needle)
                         (goto-char (point-min))
                         (search-forward needle)
                         (list
                          needle
                          (alchemist-scope-inside-string-p)
                          (alchemist-scope-inside-module-p)
                          (alchemist-scope-module)))
                       '("Fake" "Inner" "run" "defmodule Outer")))"##,
        expect![[
            r#"OK (("Fake" 35 t "Outer") ("Inner" nil t "Outer") ("run" nil t "Inner") ("defmodule Outer" nil t ""))"#
        ]],
    )
}

fn alchemist_interact_formats_single_and_multiline_results_and_builds_a_real_popup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_interact_formats_single_and_multiline_results_and_builds_a_real_popup",
        r##"(list
                      (with-temp-buffer
                        (insert "value = ")
                        (alchemist-interact-insert-as-comment
                         "sum = fn (a, b) ->\n  a + b\nend\nsum.(21, 33)")
                        (buffer-string))
                      (with-temp-buffer
                        (alchemist-interact-insert-as-comment
                         "IO.puts 1 + 1")
                        (buffer-string))
                      (let ((result
                             (alchemist-interact-create-popup
                              "*alchemist-parity-popup*"
                              "line one\nline two"
                              #'fundamental-mode))
                            (buffer
                             (get-buffer
                              "*alchemist-parity-popup*")))
                        (prog1
                            (with-current-buffer buffer
                              (list
                               result
                               (buffer-name)
                               major-mode
                               buffer-read-only
                               (buffer-string)))
                          (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("value = \n# => sum = fn (a, b) ->\n# =>   a + b\n# => end\n# => sum.(21, 33)" "  # => IO.puts 1 + 1" (nil "*alchemist-parity-popup*" fundamental-mode t "line one\nline two"))"#
        ]],
    )
}

pub(super) fn utils_scope_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_utils_transform_real_commands_modules_paths_versions_and_extensions(),
        alchemist_scope_recovers_nested_module_alias_import_and_use_context_from_real_code(),
        alchemist_scope_expression_and_extractors_cover_elixir_erlang_locals_and_punctuation(),
        alchemist_scope_syntax_distinguishes_real_module_body_heredoc_and_nested_locations(),
        alchemist_interact_formats_single_and_multiline_results_and_builds_a_real_popup(),
    ]
}
