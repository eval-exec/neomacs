use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_completion_builds_real_elixir_erlang_module_and_arity_candidates_with_metadata()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_completion_builds_real_elixir_erlang_module_and_arity_candidates_with_metadata",
        r##"(let (rows)
                      (dolist
                          (case
                           '(("Lis"
                              ("List." "delete/2" "to_string/1"))
                             ("List."
                              ("List.delete" "delete/2" "delete_at/2"))
                             ("List.del"
                              ("List.delete" "delete/2" "delete_at/2"))
                             (":file"
                              (":file" "filename" "file_server"))
                             (":file."
                              (":file." "pid2name/1" "set_cwd/1"))
                             ("En"
                              ("Enum" "Enum" "Enumerable"))))
                        (setq alchemist-company-last-completion
                              (car case))
                        (let ((candidates
                               (alchemist-complete--build-candidates
                                (cadr case))))
                          (push
                           (list
                            (car case)
                            (mapcar
                             (lambda (candidate)
                               (list
                                (substring-no-properties candidate)
                                (get-text-property
                                 0 'meta candidate)))
                             candidates))
                           rows)))
                      (nreverse rows))"##,
        expect![[
            r#"OK (("Lis" (("List" nil) ("List.delete" "/2") ("List.to_string" "/1"))) ("List." (("List.delete" "/2") ("List.delete_at" "/2"))) ("List.del" (("List.delete" "/2") ("List.delete_at" "/2"))) (":file" ((":filename" nil) (":file_server" nil))) (":file." ((":file.pid2name" "/1") (":file.set_cwd" "/1"))) ("En" (("Enum" "") ("Enumerable" ""))))"#
        ]],
    )
}

fn alchemist_help_completion_preserves_qualified_modules_overloads_and_prompt_decisions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_help_completion_preserves_qualified_modules_overloads_and_prompt_decisions",
        r##"(let (prompts)
                      (cl-letf
                          (((symbol-function 'completing-read)
                            (lambda (prompt collection &rest arguments)
                              (push
                               (list prompt collection arguments)
                               prompts)
                              (car (last collection)))))
                        (list
                         (alchemist-complete--build-help-candidates
                          '("List." "delete/2" "delete/3" "to_string/1"))
                         (alchemist-complete--build-help-candidates
                          '("String.Chars.Atom.to_string" "to_string/1"))
                         (alchemist-complete--completing-prompt
                          "List.del"
                          '("List.delete" "delete/2" "delete_at/2"))
                         (alchemist-complete--completing-prompt
                          "Only" '("Only"))
                         (alchemist-complete--completing-prompt
                          "Missing"
                          '("Missing" "Missing.other/1"))
                         (nreverse prompts))))"##,
        expect![[
            r#"OK (("List" "List.delete/2" "List.delete/3" "List.to_string/1") ("String.Chars.Atom" "String.Chars.Atom.to_string/1") "List.delete_at/2" "Only" "Missing.other/1" (("Elixir help: " ("List.delete/2" "List.delete_at/2") (nil nil "List.del")) ("Elixir help: " ("Missing" "Missing.other/1") (nil nil "Missing"))))"#
        ]],
    )
}

fn alchemist_completion_process_output_removes_markers_ansi_duplicates_and_falls_back_to_dabbrev()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_completion_process_output_removes_markers_ansi_duplicates_and_falls_back_to_dabbrev",
        r##"(let ((alchemist-company-last-completion "List.del")
                          (alchemist-company-filter-output nil)
                          delivered)
                      (cl-letf
                          (((symbol-function
                             'alchemist-complete--dabbrev-code-candidates)
                            (lambda ()
                              '("local_helper" "local_helper" "local_other")))
                           ((symbol-function
                             'alchemist-server-contains-end-marker-p)
                            (lambda (output)
                              (string-match-p "END-OF" output))))
                        (setq alchemist-company-callback
                              (lambda (candidates)
                                (setq delivered candidates)))
                        (list
                         (alchemist-complete--build-candidates-from-process-output
                          '("\e[31mList.delete\e[0m\ndelete/2\n"
                            "delete_at/2\nEND-OF-COMP\n"))
                         (alchemist-company-filter
                          'process "List.delete\ndelete/2\n")
                         alchemist-company-filter-output
                         (alchemist-company-filter
                          'process "delete_at/2\nEND-OF-COMP\n")
                         delivered
                         alchemist-company-filter-output
                         (progn
                           (setq alchemist-company-callback
                                 (lambda (candidates)
                                   (setq delivered candidates)))
                           (alchemist-company-serve-candidates-to-callback nil))
                         delivered)))"##,
        expect![[
            r#"OK ((#("delete_at" 0 9 (meta "/2")) #("List.delete" 0 11 (meta "")) #("delete" 0 6 (meta "/2"))) nil ("List.delete\ndelete/2\n") #1=(#("List.delete" 0 11 (meta "/2")) #("List.delete_at" 0 14 (meta "/2"))) #1# nil #2=("local_helper" "local_other") #2#)"#
        ]],
    )
}

fn alchemist_company_builds_context_from_real_nested_source_and_switches_iex_protocol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_company_builds_context_from_real_nested_source_and_switches_iex_protocol",
        r##"(with-temp-buffer
                      (insert
                       "defmodule Shop.Checkout do\n"
                       "  alias Money, as: Cash\n"
                       "  import Enum\n"
                       "  use GenServer\n"
                       "  def total, do: Cash.new(1)\n"
                       "end\n")
                      (goto-char (point-min))
                      (search-forward "Cash.new")
                      (let ((major-mode 'elixir-mode))
                        (list
                         (alchemist-company-build-scope-arg
                          "Cash.ne")
                         (alchemist-company-build-server-arg
                          "Cash.ne")
                         (let ((major-mode 'alchemist-iex-mode))
                           (alchemist-company-build-server-arg
                            "Cash.ne"))
                         (alchemist-company--annotation
                          (propertize
                           "flatten" 'meta "/1")))))"##,
        expect![[
            r#"OK ("{ \"Cash.ne\", [ context: Elixir, imports: [Enum,GenServer,Shop.Checkout], aliases: [{Cash, Money}] ] }" "{ \"Cash.ne\", [ context: Elixir, imports: [Enum,GenServer,Shop.Checkout], aliases: [{Cash, Money}] ] }" "{ \"Cash.ne\", [ context: [], imports: [], aliases: [] ] }" "/1")"#
        ]],
    )
}

fn alchemist_company_backend_runs_real_prefix_candidate_doc_and_location_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_company_backend_runs_real_prefix_candidate_doc_and_location_boundaries",
        r##"(with-temp-buffer
                      (insert
                       "defmodule Shop do\n"
                       "  alias List, as: Items\n"
                       "  def run, do: Items.del\n"
                       "end\n")
                      (goto-char (point-min))
                      (search-forward "Items.del")
                      (let ((major-mode 'elixir-mode)
                            events
                            async-callback)
                        (cl-letf
                            (((symbol-function
                               'alchemist-server-complete-candidates)
                              (lambda (arguments filter)
                                (push
                                 (list 'complete arguments filter)
                                 events)
                                'requested))
                             ((symbol-function
                               'alchemist-company-show-documentation)
                              (lambda (candidate)
                                (push (list 'doc candidate) events)
                                'documented))
                             ((symbol-function
                               'alchemist-goto--open-definition)
                              (lambda (candidate)
                                (push (list 'location candidate) events)
                                'opened)))
                          (let* ((prefix
                                  (alchemist-company 'prefix))
                                 (candidate-request
                                  (alchemist-company
                                   'candidates prefix))
                                 (candidate-callback
                                  (cdr candidate-request))
                                 (doc-request
                                  (alchemist-company
                                   'doc-buffer "Items.delete"))
                                 (doc-callback
                                  (cdr doc-request)))
                            (setq async-callback
                                  (list
                                   (funcall candidate-callback
                                            (lambda (_) 'company-cb))
                                   (funcall doc-callback
                                            (lambda (_) 'doc-cb))))
                            (list
                             (alchemist-company 'init)
                             prefix
                             (car candidate-request)
                             (car doc-request)
                             (alchemist-company
                              'location "Items.delete")
                             (alchemist-company
                              'annotation
                              (propertize
                               "delete" 'meta "/2"))
                             async-callback
                             alchemist-company-last-completion
                             (nreverse events))))))"##,
        expect![[
            r#"OK (t "Items.del" :async :async opened "/2" (requested documented) "Items.del" ((complete "{ \"Items.del\", [ context: Elixir, imports: [Shop], aliases: [{Items, List}] ] }" alchemist-company-filter) (doc "Items.delete") (location "Items.delete")))"#
        ]],
    )
}

pub(super) fn completion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_completion_builds_real_elixir_erlang_module_and_arity_candidates_with_metadata(),
        alchemist_help_completion_preserves_qualified_modules_overloads_and_prompt_decisions(),
        alchemist_completion_process_output_removes_markers_ansi_duplicates_and_falls_back_to_dabbrev(),
        alchemist_company_builds_context_from_real_nested_source_and_switches_iex_protocol(),
        alchemist_company_backend_runs_real_prefix_candidate_doc_and_location_boundaries(),
    ]
}
