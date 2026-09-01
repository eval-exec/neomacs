use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_eval_line_region_buffer_and_print_workflows_write_local_temp_payloads_and_callbacks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_eval_line_region_buffer_and_print_workflows_write_local_temp_payloads_and_callbacks",
        r##"(with-temp-buffer
                      (insert
                       "first = 20\n"
                       "second = 22\n"
                       "first + second\n")
                      (goto-char (point-min))
                      (let (requests)
                        (cl-letf
                            (((symbol-function 'alchemist-server-eval)
                              (lambda (arguments filter)
                                (string-match
                                 "'\\([^']+\\)'" arguments)
                                (let ((file (match-string 1 arguments)))
                                  (push
                                   (list
                                    (if
                                        (string-prefix-p
                                         "alchemist-eval"
                                         (file-name-nondirectory file))
                                        "alchemist-eval*.exs"
                                      (file-name-nondirectory file))
                                    (file-in-directory-p
                                     file (getenv "TMPDIR"))
                                    (with-temp-buffer
                                      (insert-file-contents file)
                                      (buffer-string))
                                    filter)
                                   requests))
                                'requested)))
                          (let ((line
                                 (alchemist-eval-current-line))
                                (region
                                 (alchemist-eval-region
                                  (line-beginning-position 2)
                                  (line-end-position 3)))
                                (buffer
                                 (alchemist-eval-buffer))
                                (print-line
                                 (alchemist-eval-print-current-line))
                                (print-region
                                 (alchemist-eval-print-region
                                  (line-end-position 2)
                                  (point-min)))
                                (print-buffer
                                 (alchemist-eval-print-buffer)))
                            (list
                             line region buffer
                             print-line print-region print-buffer
                             (nreverse requests)
                             (point))))))"##,
        expect![[
            r#"OK (requested requested requested requested requested requested (("alchemist-eval*.exs" t "first = 20\n" alchemist-eval-filter) ("alchemist-eval*.exs" t "second = 22\nfirst + second" alchemist-eval-filter) ("alchemist-eval*.exs" t "first = 20\nsecond = 22\nfirst + second\n" alchemist-eval-filter) ("alchemist-eval*.exs" t "first = 20\n" alchemist-eval-insert-filter) ("alchemist-eval*.exs" t "first = 20\nsecond = 22" alchemist-eval-insert-filter) ("alchemist-eval*.exs" t "first = 20\nsecond = 22\nfirst + second\n" alchemist-eval-insert-filter)) 39)"#
        ]],
    )
}

fn alchemist_quoted_eval_variants_preserve_payload_region_direction_and_quote_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_quoted_eval_variants_preserve_payload_region_direction_and_quote_protocol",
        r##"(with-temp-buffer
                      (insert
                       "[4, 2, 1, 3]\n"
                       "|> Enum.sort()\n")
                      (goto-char (point-max))
                      (let (requests)
                        (cl-letf
                            (((symbol-function 'alchemist-server-eval)
                              (lambda (arguments filter)
                                (string-match
                                 "'\\([^']+\\)'" arguments)
                                (let ((file (match-string 1 arguments)))
                                  (push
                                   (list
                                    (string-prefix-p
                                     "{ :quote," arguments)
                                    (with-temp-buffer
                                      (insert-file-contents file)
                                      (buffer-string))
                                    filter
                                    (file-in-directory-p
                                     file (getenv "TMPDIR")))
                                   requests))
                                'quoted)))
                          (list
                           (alchemist-eval-quoted-current-line)
                           (alchemist-eval-quoted-region
                            (point-min) (point-max))
                           (alchemist-eval-quoted-buffer)
                           (alchemist-eval-print-quoted-current-line)
                           (alchemist-eval-print-quoted-region
                            (point-max) (point-min))
                           (alchemist-eval-print-quoted-buffer)
                           (nreverse requests)
                           (point)
                           (mark t)))))"##,
        expect![[
            r#"OK (quoted quoted quoted quoted quoted quoted ((t "|> Enum.sort()\n" alchemist-eval-quoted-filter t) (t "[4, 2, 1, 3]\n|> Enum.sort()\n" alchemist-eval-quoted-filter t) (t "[4, 2, 1, 3]\n|> Enum.sort()\n" alchemist-eval-quoted-filter t) (t "|> Enum.sort()\n" alchemist-eval-quoted-insert-filter t) (t "[4, 2, 1, 3]\n|> Enum.sort()\n" alchemist-eval-quoted-insert-filter t) (t "[4, 2, 1, 3]\n|> Enum.sort()\n" alchemist-eval-quoted-insert-filter t)) 29 nil)"#
        ]],
    )
}

fn alchemist_eval_filters_accumulate_chunked_server_output_into_popup_and_inline_results()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_eval_filters_accumulate_chunked_server_output_into_popup_and_inline_results",
        r##"(let ((alchemist-eval-filter-output nil)
                          events)
                      (cl-letf
                          (((symbol-function
                             'alchemist-interact-create-popup)
                            (lambda (name content mode)
                              (push
                               (list
                                'popup name content
                                (if (symbolp mode)
                                    mode
                                  :anonymous-mode))
                               events)
                              'popup))
                           ((symbol-function
                             'alchemist-interact-insert-as-comment)
                            (lambda (content)
                              (push (list 'insert content) events)
                              'insert)))
                        (list
                         (alchemist-eval-filter
                          'process "first\n")
                         alchemist-eval-filter-output
                         (alchemist-eval-filter
                          'process "second\nEND-OF-EVAL\n")
                         alchemist-eval-filter-output
                         (alchemist-eval-insert-filter
                          'process "42\nEND-OF-EVAL\n")
                         (alchemist-eval-quoted-filter
                          'process "{:+, [], [1, 2]}\nEND-OF-EVAL\n")
                         (alchemist-eval-quoted-insert-filter
                          'process "{:ok, 1}\nEND-OF-EVAL\n")
                         alchemist-eval-filter-output
                         (nreverse events))))"##,
        expect![[
            r#"OK (nil ("first\n") nil nil nil nil nil nil ((popup "*alchemist-eval-mode*" "first\nsecond" :anonymous-mode) (insert "42") (popup "*alchemist-eval-mode*" "{:+, [], [1, 2]}" alchemist-eval-mode) (insert "{:ok, 1}")))"#
        ]],
    )
}

fn alchemist_macroexpand_line_and_region_workflows_write_local_payloads_and_select_exact_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_macroexpand_line_and_region_workflows_write_local_payloads_and_select_exact_protocol",
        r##"(with-temp-buffer
                      (insert
                       "unless false do\n"
                       "  IO.puts(\"kept\")\n"
                       "end\n")
                      (goto-char (point-min))
                      (let (requests)
                        (cl-letf
                            (((symbol-function 'alchemist-server-eval)
                              (lambda (arguments filter)
                                (string-match
                                 "'\\([^']+\\)'" arguments)
                                (let ((file (match-string 1 arguments)))
                                  (push
                                   (list
                                    (cond
                                     ((string-prefix-p
                                       "{ :expand_once," arguments)
                                      :expand-once)
                                     ((string-prefix-p
                                       "{ :expand," arguments)
                                      :expand)
                                     (t :unknown))
                                    (with-temp-buffer
                                      (insert-file-contents file)
                                      (buffer-string))
                                    filter
                                    (file-in-directory-p
                                     file (getenv "TMPDIR")))
                                   requests))
                                'expanded)))
                          (list
                           (alchemist-macroexpand-current-line)
                           (alchemist-macroexpand-print-current-line)
                           (alchemist-macroexpand-once-current-line)
                           (alchemist-macroexpand-once-print-current-line)
                           (alchemist-macroexpand-region
                            (point-min) (point-max))
                           (alchemist-macroexpand-print-region
                            (point-max) (point-min))
                           (alchemist-macroexpand-once-region
                            (point-min) (point-max))
                           (alchemist-macroexpand-once-print-region
                            (point-max) (point-min))
                           (nreverse requests)
                           (point)
                           (mark t)))))"##,
        expect![[
            r#"OK (expanded expanded expanded expanded expanded expanded expanded expanded ((:expand "unless false do\n" alchemist-macroexpand-filter t) (:expand "unless false do\n" alchemist-macroexpand-insert-filter t) (:expand-once "unless false do\n" alchemist-macroexpand-filter t) (:expand-once "unless false do\n" alchemist-macroexpand-insert-filter t) (:expand "unless false do\n  IO.puts(\"kept\")\nend\n" alchemist-macroexpand-filter t) (:expand "unless false do\n  IO.puts(\"kept\")\nend\n" alchemist-macroexpand-insert-filter t) (:expand-once "unless false do\n  IO.puts(\"kept\")\nend\n" alchemist-macroexpand-filter t) (:expand-once "unless false do\n  IO.puts(\"kept\")\nend\n" alchemist-macroexpand-insert-filter t)) 1 nil)"#
        ]],
    )
}

fn alchemist_macroexpand_filters_render_chunked_popup_and_inline_expansion_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_macroexpand_filters_render_chunked_popup_and_inline_expansion_exactly",
        r##"(let ((alchemist-macroexpand-filter-output nil)
                          events)
                      (cl-letf
                          (((symbol-function
                             'alchemist-interact-create-popup)
                            (lambda (name content mode)
                              (push
                               (list
                                'popup name content
                                (if (symbolp mode)
                                    mode
                                  :anonymous-mode))
                               events)
                              'popup))
                           ((symbol-function
                             'alchemist-interact-insert-as-comment)
                            (lambda (content)
                              (push (list 'insert content) events)
                              'insert)))
                        (list
                         (alchemist-macroexpand-filter
                          'process "case(false) do\n")
                         alchemist-macroexpand-filter-output
                         (alchemist-macroexpand-filter
                          'process "end\nEND-OF-EVAL\n")
                         alchemist-macroexpand-filter-output
                         (alchemist-macroexpand-insert-filter
                          'process "if(true), do: :ok\nEND-OF-EVAL\n")
                         alchemist-macroexpand-filter-output
                         (nreverse events))))"##,
        expect![[
            r#"OK (nil ("case(false) do\n") nil nil nil nil ((popup "*alchemist macroexpand*" "case(false) do\nend" :anonymous-mode) (insert "if(true), do: :ok")))"#
        ]],
    )
}

pub(super) fn eval_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_eval_line_region_buffer_and_print_workflows_write_local_temp_payloads_and_callbacks(),
        alchemist_quoted_eval_variants_preserve_payload_region_direction_and_quote_protocol(),
        alchemist_eval_filters_accumulate_chunked_server_output_into_popup_and_inline_results(),
        alchemist_macroexpand_line_and_region_workflows_write_local_payloads_and_select_exact_protocol(),
        alchemist_macroexpand_filters_render_chunked_popup_and_inline_expansion_exactly(),
    ]
}
