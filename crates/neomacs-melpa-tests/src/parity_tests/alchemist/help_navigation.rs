use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_help_prepares_alias_qualified_searches_context_arguments_and_module_candidates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_help_prepares_alias_qualified_searches_context_arguments_and_module_candidates",
        r##"(with-temp-buffer
                      (insert
                       "defmodule App do\n"
                       "  alias Phoenix.Controller, as: Controller\n"
                       "  import ExUnit.Assertions\n"
                       "  use GenServer\n"
                       "  def show, do: Controller.redirect(nil, to: \"/\")\n"
                       "end\n")
                      (goto-char (point-min))
                      (search-forward "Controller.redirect")
                      (let ((major-mode 'elixir-mode))
                        (list
                         (mapcar
                          #'alchemist-help--prepare-search-expr
                          '("Controller.redirect"
                            "Controller"
                            ":gen_tcp.accept"
                            "assert"))
                         (alchemist-help--server-arguments
                          "Controller.redirect")
                         (alchemist-help--completion-server-arguments
                          "Controller.redirect")
                         (let ((major-mode 'alchemist-iex-mode))
                           (list
                            (alchemist-help--server-arguments "Enum.map")
                            (alchemist-help--completion-server-arguments
                             "Enum.map")))
                         (alchemist-help--elixir-modules-to-list
                          "Elixir.String\nEnum\nElixir.String\n:lists\nMap\n"))))"##,
        expect![[
            r#"OK (("Phoenix.Controller.redirect" "Phoenix.Controller" ":gen_tcp.accept" "assert") "{ \"Controller.redirect\", [ context: Elixir, imports: [ExUnit.Assertions,GenServer,App], aliases: [] ] }" "{ \"Controller.redirect\", [ context: Elixir, imports: [ExUnit.Assertions,GenServer,App], aliases: [{Controller, Phoenix.Controller}] ] }" ("{ \"Enum.map\", [ context: Elixir, imports: [], aliases: [] ] }" "{ \"Enum.map\", [ context: Elixir, imports: [], aliases: [] ] }") (":lists" "Enum" "Map" "String"))"#
        ]],
    )
}

fn alchemist_help_lookup_completion_and_document_filters_execute_the_full_async_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_help_lookup_completion_and_document_filters_execute_the_full_async_protocol",
        r##"(let ((alchemist-help-search-history nil)
                          (alchemist-help-current-search-text nil)
                          (alchemist-help-filter-output nil)
                          events)
                      (cl-letf
                          (((symbol-function
                             'alchemist-server-complete-candidates)
                            (lambda (arguments filter)
                              (push
                               (list 'complete arguments filter)
                               events)
                              'completion))
                           ((symbol-function 'alchemist-server-help)
                            (lambda (arguments filter)
                              (push (list 'help arguments filter) events)
                              'help))
                           ((symbol-function
                             'alchemist-complete--completing-prompt)
                            (lambda (initial candidates)
                              (push
                               (list 'prompt initial candidates)
                               events)
                              "List.delete/2"))
                           ((symbol-function
                             'alchemist-help-display-doc)
                            (lambda (content)
                              (push (list 'display content) events)
                              'display)))
                        (list
                         (alchemist-help-lookup-doc "List.del")
                         alchemist-help-current-search-text
                         (alchemist-help-complete-filter-output
                          'process
                          "List.delete\ndelete/2\ndelete_at/2\nEND-OF-COMP\n")
                         alchemist-help-current-search-text
                         alchemist-help-filter-output
                         (alchemist-help-filter-output
                          'process
                          "Deletes an element.\nEND-OF-DOCL\n")
                         alchemist-help-current-search-text
                         alchemist-help-filter-output
                         (nreverse events))))"##,
        expect![[
            r#"OK (completion "List.del" help "List.delete/2" nil nil nil nil ((complete "{ \"List.del\", [ context: Elixir, imports: [], aliases: [] ] }" alchemist-help-complete-filter-output) (prompt "List.del" ("List.delete" "delete/2" "delete_at/2")) (help "{ \"List.delete/2\", [ context: Elixir, imports: [], aliases: [] ] }" alchemist-help-filter-output) (display "Deletes an element.")))"#
        ]],
    )
}

fn alchemist_help_modules_filter_sorts_deduplicates_prompts_and_normalizes_module_selection()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_help_modules_filter_sorts_deduplicates_prompts_and_normalizes_module_selection",
        r##"(let ((alchemist-help-filter-output nil)
                          events)
                      (cl-letf
                          (((symbol-function 'completing-read)
                            (lambda (prompt modules &rest _)
                              (push
                               (list 'prompt prompt modules)
                               events)
                              "String.Chars."))
                           ((symbol-function
                             'alchemist-help-lookup-doc)
                            (lambda (search)
                              (push (list 'lookup search) events)
                              'looked-up)))
                        (list
                         (alchemist-help-modules-filter
                          'process
                          "Elixir.String\nEnum\nElixir.String.Chars\n")
                         alchemist-help-filter-output
                         (alchemist-help-modules-filter
                          'process
                          "Elixir.String\nEND-OF-INFO\n")
                         alchemist-help-filter-output
                         (nreverse events))))"##,
        expect![[
            r#"OK (nil #1=("Elixir.String\nEnum\nElixir.String.Chars\n") looked-up ("Elixir.String\nEND-OF-INFO\n" . #1#) ((prompt "Elixir help: " ("Enum" "String" "String.Chars")) (lookup "String.Chars")))"#
        ]],
    )
}

fn alchemist_help_display_replaces_real_buffer_content_enables_mode_and_tracks_history()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_help_display_replaces_real_buffer_content_enables_mode_and_tracks_history",
        r##"(let ((alchemist-help-search-history
                           '("Enum.map/2"))
                          (alchemist-help-current-search-text
                           "List.flatten/1")
                          popped messages)
                      (cl-letf
                          (((symbol-function 'pop-to-buffer)
                            (lambda (buffer)
                              (setq popped (buffer-name buffer))
                              buffer))
                           ((symbol-function 'message)
                            (lambda (format-string &rest arguments)
                              (setq messages
                                    (apply #'format
                                           format-string arguments)))))
                        (unwind-protect
                            (progn
                              (with-current-buffer
                                  (get-buffer-create
                                   alchemist-help-buffer-name)
                                (insert "stale"))
                              (alchemist-help-display-doc
                               "\e[31mList.flatten/1\e[0m\nFlattens nested lists.")
                              (let ((first
                                     (with-current-buffer
                                         alchemist-help-buffer-name
                                       (list
                                        (buffer-string)
                                        buffer-read-only
                                        alchemist-help-minor-mode))))
                                (setq
                                 alchemist-help-current-search-text
                                 "Unknown.Module")
                                (alchemist-help-display-doc
                                 "Could not load module Unknown.Module")
                                (list
                                 first
                                 popped
                                 alchemist-help-search-history
                                 messages
                                 (with-current-buffer
                                     alchemist-help-buffer-name
                                   (buffer-string)))))
                          (when (get-buffer alchemist-help-buffer-name)
                            (kill-buffer
                             alchemist-help-buffer-name)))))"##,
        expect![[
            r#"OK (("List.flatten/1\nFlattens nested lists." t t) "*alchemist help*" ("List.flatten/1" "Enum.map/2") "No documentation for [Unknown.Module] found." "List.flatten/1\nFlattens nested lists.")"#
        ]],
    )
}

fn alchemist_goto_maps_core_source_paths_classifies_files_and_extracts_real_definitions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_goto_maps_core_source_paths_classifies_files_and_extracts_real_definitions",
        r##"(let ((alchemist-goto-elixir-source-dir
                           "/sources/elixir")
                          (alchemist-goto-erlang-source-dir
                           "/sources/otp"))
                      (list
                       (mapcar
                        (lambda (file)
                          (list
                           file
                           (alchemist-goto-elixir-file-p file)
                           (alchemist-goto-erlang-file-p file)))
                        '("lib/elixir/lib/list.ex"
                          "test/example.exs"
                          "lib/stdlib/src/lists.erl"
                          "README.md"))
                       (alchemist-goto--build-elixir-ex-core-file
                        "/build/elixir/lib/elixir/lib/string.ex")
                       (alchemist-goto--build-elixir-erl-core-file
                        "/build/elixir/lib/elixir/src/elixir.erl")
                       (alchemist-goto--build-erlang-core-file
                        "/build/otp-20/lib/stdlib/src/lists.erl")
                       (mapcar
                        #'alchemist-goto--extract-symbol
                        '("def dgettext(backend, domain, msgid, bindings) when is_list(bindings) do"
                          "defmodule Test.Module do"
                          "defp render! do"
                          "defmacro __using__(_) do"
                          "def one_line, do: :ok"))))"##,
        expect![[
            r#"OK ((("lib/elixir/lib/list.ex" 19 nil) ("test/example.exs" 12 nil) ("lib/stdlib/src/lists.erl" nil 20) ("README.md" nil nil)) "/sources/elixir/lib/elixir/lib/string.ex" "/sources/elixir/lib/elixir/src/elixir.erl" "/sources/otp/lib/stdlib/src/lists.erl" (#("def dgettext(backend, domain, msgid, bindings) when is_list(bindings)" 0 3 (face alchemist-goto--def-face) 4 12 (face alchemist-goto--name-face)) #("defmodule Test.Module" 0 9 (face alchemist-goto--def-face) 10 14 (face alchemist-goto--name-face)) #("defp render!" 0 4 (face alchemist-goto--def-face) 5 12 (face alchemist-goto--name-face)) #("defmacro __using__(_)" 0 8 (face alchemist-goto--def-face) 9 18 (face alchemist-goto--name-face)) #("def one_line" 0 3 (face alchemist-goto--def-face) 4 12 (face alchemist-goto--name-face))))"#
        ]],
    )
}

fn alchemist_goto_scans_real_buffer_skips_heredoc_definitions_and_navigates_duplicates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_goto_scans_real_buffer_skips_heredoc_definitions_and_navigates_duplicates",
        r##"(with-temp-buffer
                      (insert
                       "defmodule Demo do\n"
                       "  @doc \"\"\"\n"
                       "  def fake(value) do\n"
                       "  end\n"
                       "  \"\"\"\n"
                       "  def run(value) do\n"
                       "    value\n"
                       "  end\n"
                       "  def run(value, opts) do\n"
                       "    {value, opts}\n"
                       "  end\n"
                       "  defp helper!, do: :ok\n"
                       "end\n")
                      (goto-char (point-min))
                      (let (prompt candidates)
                        (cl-letf
                            (((symbol-function 'completing-read)
                              (lambda (actual-prompt actual-candidates
                                       &rest _)
                                (setq prompt actual-prompt
                                      candidates actual-candidates)
                                (cadr actual-candidates))))
                          (alchemist-goto--fetch-symbol-definitions)
                          (let ((before (point)))
                            (alchemist-goto--goto-symbol "run")
                            (list
                             (mapcar
                              #'substring-no-properties
                              alchemist-goto--symbol-list)
                             alchemist-goto--symbol-list-bare
                             (mapcar
                              (lambda (entry)
                                (cons
                                 (substring-no-properties (car entry))
                                 (cdr entry)))
                              alchemist-goto--symbol-name-and-pos)
                             before
                             (point)
                             (line-number-at-pos)
                             prompt
                             (mapcar
                              #'substring-no-properties
                              candidates))))))"##,
        expect![[
            r#"OK (("defmodule Demo" "def run(value)" "def run(value, opts)" "defp helper!") ("Demo" "run" "run" "helper!") (("defmodule Demo" :marker nil nil) ("def run(value)" :marker nil nil) ("def run(value, opts)" :marker nil nil) ("defp helper!" :marker nil nil)) 1 99 9 "Symbol definitions:" ("def run(value)" "def run(value, opts)"))"#
        ]],
    )
}

fn alchemist_goto_remote_request_filter_and_real_file_open_land_on_requested_function()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_goto_remote_request_filter_and_real_file_open_land_on_requested_function",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (source
                           (expand-file-name "lib/shop/cart.ex" sandbox))
                          request
                          opened)
                      (make-directory (file-name-directory source) t)
                      (with-temp-file source
                        (insert
                         "defmodule Shop.Cart do\n"
                         "  def total(items), do: length(items)\n"
                         "  def total(items, tax), do: length(items) + tax\n"
                         "end\n"))
                      (with-temp-buffer
                        (insert
                         "defmodule Caller do\n"
                         "  alias Shop.Cart, as: Cart\n"
                         "  import Enum\n"
                         "  def run, do: Cart.total([])\n"
                         "end\n")
                        (goto-char (point-min))
                        (search-forward "Cart.total")
                        (cl-letf
                            (((symbol-function 'alchemist-server-goto)
                              (lambda (arguments filter)
                                (setq request
                                      (list arguments filter))
                                'requested))
                             ((symbol-function 'switch-to-buffer)
                              (lambda (buffer)
                                (setq opened (buffer-name buffer))
                                (set-buffer buffer)
                                buffer))
                             ((symbol-function 'completing-read)
                              (lambda (_prompt candidates &rest _)
                                (car candidates))))
                          (alchemist-goto--open-definition "Cart.total")
                          (funcall alchemist-goto-callback source)
                          (prog1
                              (list
                               request
                               (file-relative-name
                                buffer-file-name sandbox)
                               opened
                               (line-number-at-pos)
                               (thing-at-point 'line t))
                            (set-buffer-modified-p nil)
                            (kill-buffer (current-buffer))))))"##,
        expect![[
            r#"OK (("{ \"Cart,total\", [ context: Elixir, imports: [Enum,Caller], aliases: [{Cart, Shop.Cart}] ] }" alchemist-goto-filter) "lib/shop/cart.ex" "cart.ex" 2 "  def total(items), do: length(items)\n")"#
        ]],
    )
}

fn alchemist_info_extracts_real_terms_builds_requests_and_renders_chunked_datatype_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_info_extracts_real_terms_builds_requests_and_renders_chunked_datatype_output",
        r##"(with-temp-buffer
                      (insert
                       "Enum.any?(items)\n"
                       ":ok\n"
                       "\"a string\"\n")
                      (let (requests events)
                        (cl-letf
                            (((symbol-function 'alchemist-server-info)
                              (lambda (arguments filter)
                                (push
                                 (list arguments filter)
                                 requests)
                                'requested))
                             ((symbol-function
                               'alchemist-interact-create-popup)
                              (lambda (name content mode)
                                (push
                                 (list
                                  name content
                                  (if (symbolp mode)
                                      mode
                                    :anonymous-mode))
                                 events)
                                'popup)))
                          (goto-char (point-min))
                          (search-forward "any?")
                          (let ((first
                                 (alchemist-info-expression-at-point)))
                            (alchemist-info-datatype-at-point)
                            (alchemist-info-types-at-point)
                            (goto-char (point-min))
                            (forward-line 1)
                            (search-forward ":ok")
                            (let ((second
                                   (alchemist-info-expression-at-point)))
                              (alchemist-info-datatype-filter
                               'process "Term\n  :ok\n")
                              (alchemist-info-datatype-filter
                               'process
                               "Data type\n  Atom\nEND-OF-INFO\n")
                              (list
                               first second
                               (nreverse requests)
                               alchemist-info-filter-output
                               (nreverse events)))))))"##,
        expect![[
            r#"OK ("Enum.any?" ":ok" (("{ :type, :info, 'Enum.any?' }" alchemist-info-datatype-filter) ("{ :type, :types, 'Enum.any?' }" alchemist-info-datatype-filter)) nil (("*alchemist-info-mode*" "Term\n  :ok\nData type\n  Atom" :anonymous-mode)))"#
        ]],
    )
}

pub(super) fn help_navigation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_help_prepares_alias_qualified_searches_context_arguments_and_module_candidates(),
        alchemist_help_lookup_completion_and_document_filters_execute_the_full_async_protocol(),
        alchemist_help_modules_filter_sorts_deduplicates_prompts_and_normalizes_module_selection(),
        alchemist_help_display_replaces_real_buffer_content_enables_mode_and_tracks_history(),
        alchemist_goto_maps_core_source_paths_classifies_files_and_extracts_real_definitions(),
        alchemist_goto_scans_real_buffer_skips_heredoc_definitions_and_navigates_duplicates(),
        alchemist_goto_remote_request_filter_and_real_file_open_land_on_requested_function(),
        alchemist_info_extracts_real_terms_builds_requests_and_renders_chunked_datatype_output(),
    ]
}
