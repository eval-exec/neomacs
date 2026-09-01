use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_sage_cache_clear_commands_mutate_only_the_live_process_buffer() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_sage_cache_clear_commands_mutate_only_the_live_process_buffer",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-cache-process*"))
                               (other-buffer
                                (generate-new-buffer
                                 " *acsage-cache-other*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     process-buffer
                                   (setq
                                    ac-sage--sage-commands-doc-cached
                                    '(("plot" . "command doc"))
                                    ac-sage--repl-methods-cached
                                    '(("matrix.rank" . "method doc"))))
                                 (with-current-buffer
                                     other-buffer
                                   (setq
                                    ac-sage--sage-commands-doc-cached
                                    '(("keep" . "other command"))
                                    ac-sage--repl-methods-cached
                                    '(("keep.method" . "other method")))
                                   (let ((sage-shell:process-buffer
                                          process-buffer))
                                     (ac-sage--sage-commands-doc-clear-cache)
                                     (ac-sage--doc-clear-cache)))
                                 (list
                                  (with-current-buffer
                                      process-buffer
                                    (list
                                     ac-sage--sage-commands-doc-cached
                                     ac-sage--repl-methods-cached))
                                  (with-current-buffer
                                      other-buffer
                                    (list
                                     ac-sage--sage-commands-doc-cached
                                     ac-sage--repl-methods-cached))
                                  (let ((sage-shell:process-buffer nil))
                                    (list
                                     (ac-sage--sage-commands-doc-clear-cache)
                                     (ac-sage--doc-clear-cache)))))
                             (kill-buffer process-buffer)
                             (kill-buffer other-buffer)))"##,
        expect![[
            r#"OK ((nil nil) ((("keep" . "other command")) (("keep.method" . "other method"))) (nil nil))"#
        ]],
    )
}

fn auto_complete_sage_cache_macro_distinguishes_hits_misses_length_and_top_level_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_cache_macro_distinguishes_hits_misses_length_and_top_level_state",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-cache-matrix*"))
                               calls
                               top-level)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (let ((sage-shell:process-buffer
                                        process-buffer)
                                       (ac-sage--repl-methods-cached
                                        '(("cached.name" . "cached doc"))))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell:at-top-level-and-in-sage-p)
                                         (lambda ()
                                           top-level))
                                        ((symbol-function
                                          'acsage-test-doc-provider)
                                         (lambda (name base-name)
                                           (push
                                            (list name base-name)
                                            calls)
                                           (format
                                            "doc:%s:%s"
                                            name
                                            base-name))))
                                     (setq top-level t)
                                     (let ((results
                                            (list
                                             (ac-sage--cache-doc
                                              acsage-test-doc-provider
                                              "cached.name"
                                              "cached"
                                              ac-sage--repl-methods-cached)
                                             (ac-sage--cache-doc
                                              acsage-test-doc-provider
                                              "new.name"
                                              "new"
                                              ac-sage--repl-methods-cached)
                                             (ac-sage--cache-doc
                                              acsage-test-doc-provider
                                              "new.name"
                                              "ignored"
                                              ac-sage--repl-methods-cached)
                                             (ac-sage--cache-doc
                                              acsage-test-doc-provider
                                              "abcd"
                                              nil
                                              ac-sage--repl-methods-cached
                                              4))))
                                       (setq top-level nil)
                                       (list
                                        results
                                        (ac-sage--cache-doc
                                         acsage-test-doc-provider
                                         "blocked"
                                         nil
                                         ac-sage--repl-methods-cached)
                                        (nreverse calls)
                                        ac-sage--repl-methods-cached)))))
                             (kill-buffer process-buffer)))"##,
        expect![[
            r#"OK (("cached doc" "doc:new.name:new" "doc:new.name:new" "doc:abcd:nil") nil (("new.name" "new") ("abcd" nil)) (("abcd" . "doc:abcd:nil") ("new.name" . "doc:new.name:new") ("cached.name" . "cached doc")))"#
        ]],
    )
}

fn auto_complete_sage_command_documentation_obeys_quick_help_and_short_name_policy()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_command_documentation_obeys_quick_help_and_short_name_policy",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-command-doc*"))
                               calls)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (let ((sage-shell:process-buffer
                                        process-buffer)
                                       (ac-sage--sage-commands-doc-cached
                                        nil))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell:at-top-level-and-in-sage-p)
                                         (lambda () t))
                                        ((symbol-function
                                          'ac-sage--doc)
                                         (lambda (name base-name)
                                           (push
                                            (list name base-name)
                                            calls)
                                           (concat "DOC:" name))))
                                     (let ((ac-sage-show-quick-help nil))
                                       (setq calls nil)
                                       (list
                                        (ac-sage-doc "factor")
                                        calls))
                                     (let ((ac-sage-show-quick-help t))
                                       (list
                                        (ac-sage-doc "plot")
                                        (ac-sage-doc "factor")
                                        (ac-sage-doc "factor")
                                        (nreverse calls)
                                        ac-sage--sage-commands-doc-cached)))))
                             (kill-buffer process-buffer)))"##,
        expect![[
            r#"OK ("DOC:plot" "DOC:factor" "DOC:factor" (("plot" nil) ("factor" nil)) (("factor" . "DOC:factor") ("plot" . "DOC:plot")))"#
        ]],
    )
}

fn auto_complete_sage_repl_base_name_and_name_cover_variables_other_interfaces_and_sage_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_repl_base_name_and_name_cover_variables_other_interfaces_and_sage_commands",
        r##"(mapcar
                           (lambda (fixture)
                             (let ((sage-shell-cpl:current-state
                                    fixture))
                               (list
                                fixture
                                (ac-sage-repl--base-name-and-name
                                 "rank"))))
                           '(((interface . "sage")
                              (var-base-name . "matrix")
                              (types . ("attributes")))
                             ((interface . "gap")
                              (var-base-name)
                              (types . ("interface")))
                             ((interface . "sage")
                              (var-base-name)
                              (types . ("interface")))
                             ((interface . "magma")
                              (var-base-name . "group")
                              (types . ("attributes")))))"##,
        expect![[
            r#"OK ((((interface . "sage") (var-base-name . "matrix") (types "attributes")) ("matrix" . "matrix.rank")) (((interface . "gap") (var-base-name) (types "interface")) ("gap" . "gap.rank")) (((interface . "sage") (var-base-name) (types "interface")) (nil . "rank")) (((interface . "magma") (var-base-name . "group") (types "attributes")) ("group" . "group.rank")))"#
        ]],
    )
}

fn auto_complete_sage_method_documentation_qualifies_names_and_uses_process_buffer_cache()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_method_documentation_qualifies_names_and_uses_process_buffer_cache",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-method-doc*"))
                               calls)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (let ((sage-shell:process-buffer
                                        process-buffer)
                                       (sage-shell-cpl:current-state
                                        '((interface . "sage")
                                          (var-base-name . "matrix")
                                          (types . ("attributes"))))
                                       (ac-sage-show-quick-help t)
                                       (ac-sage--repl-methods-cached
                                        nil))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell:at-top-level-and-in-sage-p)
                                         (lambda () t))
                                        ((symbol-function
                                          'ac-sage--doc)
                                         (lambda (name base-name)
                                           (push
                                            (list name base-name)
                                            calls)
                                           (format
                                            "%s=>%s"
                                            base-name
                                            name))))
                                     (list
                                      (ac-sage-repl-methods-doc
                                       "rank")
                                      (ac-sage-repl-methods-doc
                                       "rank")
                                      (nreverse calls)
                                      ac-sage--repl-methods-cached))))
                             (kill-buffer process-buffer)))"##,
        expect![[
            r#"OK ("matrix=>matrix.rank" "matrix=>matrix.rank" (("matrix.rank" "matrix")) (("matrix.rank" . "matrix=>matrix.rank")))"#
        ]],
    )
}

fn auto_complete_sage_document_transport_builds_exact_python_command_and_trims_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_document_transport_builds_exact_python_command_and_trims_output",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-doc-transport*"))
                               responses
                               commands)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (let ((sage-shell:process-buffer
                                        process-buffer))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell:output-finished-p)
                                         (lambda (&optional _buffer)
                                           t))
                                        ((symbol-function
                                          'sage-shell:redirect-finished-p)
                                         (lambda () t))
                                        ((symbol-function
                                          'sage-shell:py-mod-func)
                                         (lambda (name)
                                           (concat "sage_mod." name)))
                                        ((symbol-function
                                          'sage-shell:send-command-to-string)
                                         (lambda (command)
                                           (push command commands)
                                           (prog1
                                               (pop responses)))))
                                     (setq responses
                                           '("Rank documentation  \n"
                                             "   \n"))
                                     (list
                                      (ac-sage--doc
                                       "matrix.rank"
                                       "matrix")
                                      (ac-sage--doc
                                       "factor"
                                       nil)
                                      (nreverse commands)))))
                             (kill-buffer process-buffer)))"##,
        expect![[
            r#"OK ("Rank documentation" nil ("sage_mod.print_short_doc_and_def('matrix.rank', base_name='matrix')" "sage_mod.print_short_doc_and_def('factor')"))"#
        ]],
    )
}

fn auto_complete_sage_document_transport_short_circuits_each_unfinished_protocol_phase()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_document_transport_short_circuits_each_unfinished_protocol_phase",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-doc-gates*"))
                               output-finished
                               redirect-finished
                               calls)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (let ((sage-shell:process-buffer
                                        process-buffer))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell:output-finished-p)
                                         (lambda (&optional _buffer)
                                           (push :output calls)
                                           output-finished))
                                        ((symbol-function
                                          'sage-shell:redirect-finished-p)
                                         (lambda ()
                                           (push :redirect calls)
                                           redirect-finished))
                                        ((symbol-function
                                          'sage-shell:send-command-to-string)
                                         (lambda (command)
                                           (push
                                            (list :send command)
                                            calls)
                                           "unexpected")))
                                     (let (results)
                                       (setq output-finished nil
                                             redirect-finished t
                                             calls nil)
                                       (push
                                        (list
                                         (ac-sage--doc "first" nil)
                                         (nreverse calls))
                                        results)
                                       (setq output-finished t
                                             redirect-finished nil
                                             calls nil)
                                       (push
                                        (list
                                         (ac-sage--doc "second" nil)
                                         (nreverse calls))
                                        results)
                                       (nreverse results)))))
                             (kill-buffer process-buffer)))"##,
        expect!["OK ((nil (:output)) (nil (:output :redirect)))"],
    )
}

fn auto_complete_sage_nil_document_results_are_recorded_but_retried_on_the_next_lookup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_nil_document_results_are_recorded_but_retried_on_the_next_lookup",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-nil-doc-cache*"))
                               calls)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (let ((sage-shell:process-buffer
                                        process-buffer)
                                       (ac-sage-show-quick-help t)
                                       (ac-sage--sage-commands-doc-cached
                                        nil))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell:at-top-level-and-in-sage-p)
                                         (lambda () t))
                                        ((symbol-function
                                          'ac-sage--doc)
                                         (lambda (name base-name)
                                           (push
                                            (list name base-name)
                                            calls)
                                           nil)))
                                     (list
                                      (ac-sage-doc "factor")
                                      (ac-sage-doc "factor")
                                      (nreverse calls)
                                      ac-sage--sage-commands-doc-cached))))
                             (kill-buffer process-buffer)))"##,
        expect![[r#"OK (nil nil (("factor" nil) ("factor" nil)) (("factor") ("factor")))"#]],
    )
}

pub(super) fn cache_docs_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_sage_cache_clear_commands_mutate_only_the_live_process_buffer(),
        auto_complete_sage_cache_macro_distinguishes_hits_misses_length_and_top_level_state(),
        auto_complete_sage_command_documentation_obeys_quick_help_and_short_name_policy(),
        auto_complete_sage_repl_base_name_and_name_cover_variables_other_interfaces_and_sage_commands(),
        auto_complete_sage_method_documentation_qualifies_names_and_uses_process_buffer_cache(),
        auto_complete_sage_document_transport_builds_exact_python_command_and_trims_output(),
        auto_complete_sage_document_transport_short_circuits_each_unfinished_protocol_phase(),
        auto_complete_sage_nil_document_results_are_recorded_but_retried_on_the_next_lookup(),
    ]
}
