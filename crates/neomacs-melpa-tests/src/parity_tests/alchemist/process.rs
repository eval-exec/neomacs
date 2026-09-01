use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_server_codes_requests_markers_and_chunk_reassembly_match_wire_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_server_codes_requests_markers_and_chunk_reassembly_match_wire_protocol",
        r##"(list
                      alchemist-server-codes
                      (mapcar
                       #'alchemist-server-api-code
                       '(server-eval server-defl server-info
                         server-docl server-comp unknown))
                      (mapcar
                       (lambda (entry)
                         (apply
                          #'alchemist-server-build-request-string
                          entry))
                       '((server-eval "{ :eval, '/x.exs' }")
                         (server-defl
                          "{ \"List,flatten\", [ context: Elixir ] }")
                         (server-info)
                         (server-docl "{ \"Enum.map\", [] }")
                         (server-comp "{ \"Str\", [] }")))
                      (mapcar
                       #'alchemist-server-contains-end-marker-p
                       '("" nil "END-OF-EVAL"
                         "payload\nEND-OF-DEFL"
                         "END-OF-UNKNOWN"
                         "END-OF-COMP\n"))
                      (alchemist-server-prepare-filter-output
                       '("third\nEND-OF-INFO\n"
                         "second\n"
                         "first\n")))"##,
        expect![[
            r#"OK (((server-eval "EVAL") (server-defl "DEFL") (server-info "INFO") (server-docl "DOCL") (server-comp "COMP")) ("EVAL" "DEFL" "INFO" "DOCL" "COMP" nil) ("EVAL { :eval, '/x.exs' }\n" "DEFL { \"List,flatten\", [ context: Elixir ] }\n" "INFO\n" "DOCL { \"Enum.map\", [] }\n" "COMP { \"Str\", [] }\n") (nil nil 0 8 nil 0) "first\nsecond\nthird")"#
        ]],
    )
}

fn alchemist_server_high_level_api_starts_filters_and_sends_every_exact_request() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alchemist_server_high_level_api_starts_filters_and_sends_every_exact_request",
        r##"(let (events)
                      (cl-letf
                          (((symbol-function
                             'alchemist-server-start-if-not-running)
                            (lambda ()
                              (push '(start) events)
                              'started))
                           ((symbol-function 'alchemist-server-process)
                            (lambda () 'server-process))
                           ((symbol-function 'set-process-filter)
                            (lambda (process filter)
                              (push
                               (list 'filter process filter)
                               events)
                              'filtered))
                           ((symbol-function 'process-send-string)
                            (lambda (process string)
                              (push
                               (list 'send process string)
                               events)
                              'sent)))
                        (list
                         (alchemist-server-goto
                          "{ \"List,flatten\", [] }"
                          #'alchemist-goto-filter)
                         (alchemist-server-info
                          "{ :type, :modules }"
                          #'alchemist-help-modules-filter)
                         (alchemist-server-help-with-modules
                          #'alchemist-help-modules-filter)
                         (alchemist-server-help
                          "{ \"Enum.map\", [] }"
                          #'alchemist-help-filter-output)
                         (alchemist-server-eval
                          "{ :eval, 'code.exs' }"
                          #'alchemist-eval-filter)
                         (alchemist-server-complete-candidates
                          "{ \"Str\", [] }"
                          #'alchemist-company-filter)
                         (nreverse events))))"##,
        expect![[
            r#"OK (sent sent sent sent sent sent (#1=(start) #1# (filter server-process alchemist-goto-filter) (send server-process "DEFL { \"List,flatten\", [] }\n") #1# #1# (filter server-process alchemist-help-modules-filter) (send server-process "INFO { :type, :modules }\n") #1# #1# (filter server-process alchemist-help-modules-filter) (send server-process "INFO\n") #1# #1# (filter server-process alchemist-help-filter-output) (send server-process "DOCL { \"Enum.map\", [] }\n") #1# #1# (filter server-process alchemist-eval-filter) (send server-process "EVAL { :eval, 'code.exs' }\n") #1# #1# (filter server-process alchemist-company-filter) (send server-process "COMP { \"Str\", [] }\n")))"#
        ]],
    )
}

fn alchemist_server_process_names_follow_project_elixir_and_global_contexts_and_replace_cache_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_server_process_names_follow_project_elixir_and_global_contexts_and_replace_cache_entries",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project
                           (file-name-as-directory
                            (expand-file-name "project" sandbox)))
                          (nested
                           (file-name-as-directory
                            (expand-file-name "lib/deep" project)))
                          (default-directory nested)
                          (alchemist-project-root-path-cache nil)
                          (alchemist-server-processes nil)
                          (alchemist-goto-elixir-source-dir ""))
                      (make-directory nested t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "mix"))
                      (let ((project-name
                             (alchemist-server-process-name)))
                        (setq alchemist-server-processes
                              (list
                               (cons project-name 'old-process)))
                        (alchemist-server--store-process 'new-process)
                        (list
                         (file-relative-name project-name sandbox)
                         (alchemist-server-process)
                         alchemist-server-processes
                         (let ((alchemist-goto-elixir-source-dir
                                sandbox))
                           (alchemist-server-process-name))
                         (let ((default-directory sandbox)
                               (alchemist-project-root-path-cache nil))
                           (alchemist-server-process-name)))))"##,
        expect![[
            r#"OK ("project/" new-process (("[ORACLE-SANDBOX]/project/" . new-process)) "alchemist-server" "alchemist-server")"#
        ]],
    )
}

fn alchemist_server_start_in_environment_uses_project_cwd_quoted_command_and_nonquerying_cache()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_server_start_in_environment_uses_project_cwd_quoted_command_and_nonquerying_cache",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project
                           (file-name-as-directory
                            (expand-file-name "server app" sandbox)))
                          (nested
                           (file-name-as-directory
                            (expand-file-name "lib/deep" project)))
                          (default-directory nested)
                          (alchemist-project-root-path-cache nil)
                          (alchemist-execute-command "/tools/elixir runtime")
                          events)
                      (make-directory nested t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "mix"))
                      (cl-letf
                          (((symbol-function
                             'start-process-shell-command)
                            (lambda (name buffer command)
                              (push
                               (list
                                'start
                                (file-relative-name
                                 default-directory sandbox)
                                name buffer
                                ;; Mask the installed package's own
                                ;; directory.  Pinning it spelled out the
                                ;; harness's acquisition layout, so this
                                ;; expectation broke when the cache moved
                                ;; from package-cache/ to the
                                ;; revision-pinned source-install-cache/ --
                                ;; a failure about the harness wearing the
                                ;; shape of a package regression.
                                (let ((package-directory
                                       (file-name-directory
                                        (getenv "NEOMACS_PACKAGE_SOURCE"))))
                                  (replace-regexp-in-string
                                   (regexp-quote package-directory)
                                   "[PACKAGE]/" command t t)))
                               events)
                              'server-process))
                           ((symbol-function
                             'set-process-query-on-exit-flag)
                            (lambda (process flag)
                              (push
                               (list 'query process flag)
                               events)
                              flag))
                           ((symbol-function
                             'alchemist-server--store-process)
                            (lambda (process)
                              (push
                               (list 'store process)
                               events)
                              'stored)))
                        (list
                         (alchemist-server-start-in-env "shared env")
                         (nreverse events))))"##,
        expect![[
            r#"OK (stored ((start "server app/" "[ORACLE-SANDBOX]/server app/" "*alchemist-server*" "/tools/elixir runtime [PACKAGE]/alchemist-server/run.exs shared\\ env") (query server-process nil) (store server-process)))"#
        ]],
    )
}

fn alchemist_iex_command_line_region_compile_reload_and_project_workflows_send_real_user_intent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_iex_command_line_region_compile_reload_and_project_workflows_send_real_user_intent",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project
                           (file-name-as-directory
                            (expand-file-name "shop" sandbox)))
                          (source
                           (expand-file-name "lib/shop/cart.ex" project))
                          (default-directory
                           (file-name-directory source))
                          (alchemist-project-root-path-cache nil)
                          (alchemist-iex-program-name
                           "iex --erl \"+S 2\"")
                          events)
                      (make-directory (file-name-directory source) t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "mix"))
                      (with-temp-buffer
                        (setq buffer-file-name source)
                        (insert
                         "defmodule Shop.Cart do\n"
                         "  def total, do: 42\n"
                         "end\n")
                        (goto-char (point-min))
                        (forward-line 1)
                        (cl-letf
                            (((symbol-function 'alchemist-iex-process)
                              (lambda (&optional argument)
                                (push (list 'process argument) events)
                                'iex-process))
                             ((symbol-function
                               'alchemist-iex--send-command)
                              (lambda (process command)
                                (push
                                 (list 'send process command)
                                 events)
                                'sent))
                             ((symbol-function 'process-buffer)
                              (lambda (_) 'iex-buffer))
                             ((symbol-function 'pop-to-buffer)
                              (lambda (buffer)
                                (push (list 'pop buffer) events)
                                'popped)))
                          (list
                           (alchemist-iex-command nil)
                           (cl-letf
                               (((symbol-function 'read-string)
                                 (lambda (&rest _)
                                   "iex --sname parity")))
                             (alchemist-iex-command " --sname parity"))
                           (alchemist-iex-send-current-line)
                           (alchemist-iex-send-region
                            (point-min) (point-max))
                           (alchemist-iex-compile-this-buffer)
                           (alchemist-iex-reload-module)
                           (alchemist-iex-project-run)
                           (nreverse events)))))"##,
        expect![[
            r#"OK (("iex" "--erl" "+S 2") ("iex" "--sname" "parity") sent sent sent sent popped ((process nil) (send iex-process "  def total, do: 42\n") (process nil) (send iex-process "defmodule Shop.Cart do\n  def total, do: 42\nend\n") (process nil) (send iex-process "c(\"[ORACLE-SANDBOX]/shop/lib/shop/cart.ex\", \"[ORACLE-SANDBOX]/shop//_build/dev/\")") (process nil) (send iex-process "r(Shop.Cart)") (process " -S mix") (pop iex-buffer)))"#
        ]],
    )
}

fn alchemist_iex_send_command_preserves_multiline_input_process_mark_and_comint_history()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_iex_send_command_preserves_multiline_input_process_mark_and_comint_history",
        r##"(let* ((buffer
                           (generate-new-buffer
                            " *alchemist-iex-parity*"))
                          (process
                           (start-process
                            "alchemist-iex-parity"
                            buffer "sh" "-c" "cat")))
                      (unwind-protect
                          (with-current-buffer buffer
                            (setq-local comint-last-input-end
                                        (copy-marker (point-min)))
                            (set-marker
                             (process-mark process) (point-min))
                            (alchemist-iex--send-command
                             process
                             "first = 20\nsecond = 22\nfirst + second")
                            (accept-process-output process 0.1)
                            (list
                             (buffer-string)
                             (marker-position
                              (process-mark process))
                             (marker-position
                              comint-last-input-end)
                             (process-live-p process)))
                        (when (process-live-p process)
                          (set-process-query-on-exit-flag process nil)
                          (delete-process process))
                        (when (buffer-live-p buffer)
                          (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("first = 20\nsecond = 22\nfirst + second\nfirst = 20\nsecond = 22\nfirst + second\n" 77 77 (run open listen connect stop))"#
        ]],
    )
}

fn alchemist_hex_dependency_parser_reads_real_mix_file_ignores_comments_and_renders_popup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_hex_dependency_parser_reads_real_mix_file_ignores_comments_and_renders_popup",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project
                           (file-name-as-directory
                            (expand-file-name "hex_app" sandbox)))
                          (default-directory project)
                          (alchemist-project-root-path-cache nil)
                          popup)
                      (make-directory project t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert
                         "defmodule HexApp.Mixfile do\n"
                         "  defp deps do\n"
                         "    [\n"
                         "      {:phoenix, \"~> 1.2\"},\n"
                         "      {:ecto, github: \"elixir-ecto/ecto\"},\n"
                         "      # {:ignored, \"~> 0.1\"},\n"
                         "      {:plug, \">= 0.0.0\"}\n"
                         "    ]\n"
                         "  end\n"
                         "end\n"))
                      (cl-letf
                          (((symbol-function
                             'alchemist-interact-create-popup)
                            (lambda (name content mode)
                              (setq popup
                                    (list
                                     name content
                                     (if (symbolp mode)
                                         mode
                                       :anonymous-mode)))
                              'popup)))
                        (list
                         (alchemist-hex-all-dependencies)
                         popup
                         (with-temp-buffer
                           (insert
                            "{:phoenix_html, \"~> 2.6\"}, {:ecto_sql, \"~> 3.0\"}")
                           (search-backward "ecto_sql")
                           (forward-char 4)
                           (alchemist-hex--deps-name-at-point)))))"##,
        expect![[
            r#"OK (popup ("*alchemist-hex*" ":ecto\11\11 github: \"elixir-ecto/ecto\"\n:phoenix\11 \"~> 1.2\"\n:plug\11\11 \">= 0.0.0\"\n" :anonymous-mode) "ecto_sql")"#
        ]],
    )
}

fn alchemist_hex_offline_package_info_renders_description_config_links_releases_and_button_urls()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_hex_offline_package_info_renders_description_config_links_releases_and_button_urls",
        r##"(let ((information
                           '((meta
                              (description . "Composable web middleware")
                              (maintainers . ["José Valim" "Community"])
                              (licenses . ["Apache-2.0"])
                              (links
                               (GitHub . "https://github.com/elixir-plug/plug")
                               (Docs . "https://hexdocs.pm/plug")))
                             (releases .
                              [((version . "1.12.1")
                                (url . "https://hex.pm/api/packages/plug/releases/1.12.1"))
                               ((version . "1.11.0")
                                (url . "https://hex.pm/api/packages/plug/releases/1.11.0"))])))
                          popped)
                      (cl-letf
                          (((symbol-function
                             'alchemist-hex--fetch-package-info)
                            (lambda (_) information))
                           ((symbol-function 'pop-to-buffer)
                            (lambda (buffer)
                              (setq popped (buffer-name buffer))
                              buffer)))
                        (unwind-protect
                            (progn
                              (alchemist-hex--display-info-for "plug")
                              (with-current-buffer
                                  alchemist-hex-buffer-name
                                (let (buttons)
                                  (goto-char (point-min))
                                  (while
                                      (next-button (point))
                                    (goto-char
                                     (button-start
                                      (next-button (point))))
                                    (let ((button
                                           (button-at (point))))
                                      (push
                                       (list
                                        (button-label button)
                                        (button-get button 'url))
                                       buttons))
                                    (goto-char
                                     (button-end
                                      (button-at (point)))))
                                  (list
                                   popped
                                   (buffer-string)
                                   buffer-read-only
                                   alchemist-hex-mode
                                   (nreverse buttons)))))
                          (when (get-buffer alchemist-hex-buffer-name)
                            (kill-buffer
                             alchemist-hex-buffer-name)))))"##,
        expect![[
            r#"OK ("*alchemist-hex*" #("Composable web middleware\n\nConfig: {:plug, => \"~> 1.12.1\"}\nLatest release: 1.12.1\n\nMaintainers: \n  - José Valim\n  - Community\nLicenses: Apache-2.0\nLinks: \n  GitHub: https://github.com/elixir-plug/plug\n  Docs: https://hexdocs.pm/plug\nReleases: \n  - 1.12.1\11     (docs)\n  - 1.11.0\11     (docs)\n" 27 35 (face font-lock-string-face) 59 75 (face font-lock-string-face) 83 96 (face font-lock-string-face) 126 136 (face font-lock-string-face) 147 154 (face font-lock-string-face) 233 244 (face font-lock-string-face)) t t (("1.12.1" "https://hex.pm/packages/plug/1.12.1") ("https://github.com/elixir-plug/plug" "https://github.com/elixir-plug/plug") ("https://hexdocs.pm/plug" "https://hexdocs.pm/plug") ("1.12.1" "https://hex.pm/packages/plug/1.12.1") ("docs" "https://hexdocs.pm/plug/1.12.1") ("1.11.0" "https://hex.pm/packages/plug/1.11.0") ("docs" "https://hexdocs.pm/plug/1.11.0")))"#
        ]],
    )
}

fn alchemist_hex_offline_release_and_search_views_render_dates_docs_and_filtered_results()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_hex_offline_release_and_search_views_render_dates_docs_and_filtered_results",
        r##"(let* ((plug
                           '((name . "plug")
                             (inserted_at . "2017-03-01T12:00:00Z")
                             (url . "https://hex.pm/packages/plug")
                             (releases .
                              [((version . "1.4.0")
                                (inserted_at . "2017-03-01T12:00:00Z")
                                (url . "https://hex.pm/api/packages/plug/releases/1.4.0"))
                               ((version . "1.3.0")
                                (inserted_at . "2016-10-01T12:00:00Z")
                                (url . "https://hex.pm/api/packages/plug/releases/1.3.0"))])))
                          (unrelated
                           '((name . "ecto")
                             (inserted_at . "2017-01-01T12:00:00Z")
                             (url . "https://hex.pm/packages/ecto")
                             (releases .
                              [((version . "2.1.0")
                                (url . "https://hex.pm/api/packages/ecto/releases/2.1.0"))])))
                          popped
                          release-view
                          search-view)
                      (cl-letf
                          (((symbol-function
                             'alchemist-hex--fetch-package-info)
                            (lambda (_) plug))
                           ((symbol-function
                             'alchemist-hex--fetch-search-packages)
                            (lambda (_) (vector plug unrelated)))
                           ((symbol-function 'pop-to-buffer)
                            (lambda (buffer)
                              (setq popped (buffer-name buffer))
                              buffer)))
                        (unwind-protect
                            (progn
                              (alchemist-hex--display-releases-for "plug")
                              (setq release-view
                                    (with-current-buffer
                                        alchemist-hex-buffer-name
                                      (let (buttons button)
                                        (goto-char (point-min))
                                        (while
                                            (setq button
                                                  (next-button (point)))
                                          (goto-char
                                           (button-start button))
                                          (push
                                           (list
                                            (button-label button)
                                            (button-get button 'url))
                                           buttons)
                                          (goto-char
                                           (button-end button)))
                                        (list
                                         (buffer-string)
                                         (nreverse buttons)))))
                              (alchemist-hex-search "plug")
                              (setq search-view
                                    (with-current-buffer
                                        alchemist-hex-buffer-name
                                      (let (buttons button)
                                        (goto-char (point-min))
                                        (while
                                            (setq button
                                                  (next-button (point)))
                                          (goto-char
                                           (button-start button))
                                          (push
                                           (list
                                            (button-label button)
                                            (button-get button 'url))
                                           buttons)
                                          (goto-char
                                           (button-end button)))
                                        (list
                                         (buffer-string)
                                         (nreverse buttons)))))
                              (list popped release-view search-view))
                          (when (get-buffer alchemist-hex-buffer-name)
                            (kill-buffer
                             alchemist-hex-buffer-name)))))"##,
        expect![[
            r#"OK ("*alchemist-hex*" (#("plug versions  (latest version 1.4.0)\n\n1.4.0\11    (released on 2017-03-01)   (docs)\n1.3.0\11    (released on 2016-10-01)   (docs)\n" 0 31 (face font-lock-variable-name-face) 50 61 (face font-lock-string-face) 94 105 (face font-lock-string-face)) (("1.4.0" "https://hex.pm/packages/plug/1.4.0") ("1.4.0" "https://hex.pm/packages/plug/1.4.0") ("docs" "https://hexdocs.pm/plug/1.4.0") ("1.3.0" "https://hex.pm/packages/plug/1.3.0") ("docs" "https://hexdocs.pm/plug/1.3.0"))) (#("search results for: plug \n\nplug\11  1.4.0   (released on 2017-03-01)\11  (docs)\n" 0 20 (face font-lock-variable-name-face) 20 27 (face font-lock-builtin-face) 43 54 (face font-lock-string-face)) (("plug" "https://hex.pm/packages/plug") ("1.4.0" "https://hex.pm/packages/plug/1.4.0") ("docs" "https://hexdocs.pm/plug/1.4.0"))))"#
        ]],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_server_codes_requests_markers_and_chunk_reassembly_match_wire_protocol(),
        alchemist_server_high_level_api_starts_filters_and_sends_every_exact_request(),
        alchemist_server_process_names_follow_project_elixir_and_global_contexts_and_replace_cache_entries(),
        alchemist_server_start_in_environment_uses_project_cwd_quoted_command_and_nonquerying_cache(),
        alchemist_iex_command_line_region_compile_reload_and_project_workflows_send_real_user_intent(),
        alchemist_iex_send_command_preserves_multiline_input_process_mark_and_comint_history(),
        alchemist_hex_dependency_parser_reads_real_mix_file_ignores_comments_and_renders_popup(),
        alchemist_hex_offline_package_info_renders_description_config_links_releases_and_button_urls(),
        alchemist_hex_offline_release_and_search_views_render_dates_docs_and_filtered_results(),
    ]
}
