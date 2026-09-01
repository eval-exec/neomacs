use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_sage_setup_enables_mode_and_prepends_repl_sources_in_real_buffer_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_setup_enables_mode_and_prepends_repl_sources_in_real_buffer_state",
        r##"(with-temp-buffer
                           (setq major-mode
                                 'sage-shell-mode
                                 ac-sources
                                 '(ac-source-filename
                                   ac-source-words-in-buffer))
                           (let ((before
                                  (list
                                   auto-complete-mode
                                   ac-sources)))
                             (unwind-protect
                                 (progn
                                   (ac-sage-setup)
                                   (list
                                    before
                                    auto-complete-mode
                                    ac-sources
                                    (local-variable-p
                                     'ac-sources)
                                    (memq
                                     major-mode
                                     ac-modes)))
                               (auto-complete-mode -1))))"##,
        expect![
            "OK ((nil #1=(ac-source-filename ac-source-words-in-buffer)) t (ac-sage-repl-modules ac-source-sage-methods ac-sage-repl-vars-in-module ac-source-sage-other-interfaces as-source-sage-repl-argspec ac-source-sage-repl-python-kwds ac-source-repl-sage-commands ac-source-sage-words-in-buffers . #1#) t (sage-shell-mode emacs-lisp-mode lisp-mode lisp-interaction-mode slime-repl-mode nim-mode c-mode cc-mode c++-mode objc-mode swift-mode go-mode java-mode malabar-mode clojure-mode clojurescript-mode scala-mode scheme-mode ocaml-mode tuareg-mode coq-mode haskell-mode agda-mode agda2-mode perl-mode cperl-mode python-mode ruby-mode lua-mode tcl-mode ecmascript-mode javascript-mode js-mode js-jsx-mode js2-mode js2-jsx-mode coffee-mode php-mode css-mode scss-mode less-css-mode elixir-mode makefile-mode sh-mode fortran-mode f90-mode ada-mode xml-mode sgml-mode web-mode ts-mode sclang-mode verilog-mode qml-mode apples-mode))"
        ],
    )
}

fn auto_complete_sage_setup_enables_mode_and_appends_edit_sources_in_real_buffer_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_setup_enables_mode_and_appends_edit_sources_in_real_buffer_state",
        r##"(with-temp-buffer
                           (setq major-mode
                                 'sage-shell:sage-mode
                                 ac-sources
                                 '(ac-source-filename
                                   ac-source-words-in-buffer))
                           (unwind-protect
                               (progn
                                 (ac-sage-setup)
                                 (list
                                  auto-complete-mode
                                  ac-sources
                                  (local-variable-p
                                   'ac-sources)
                                  (memq
                                   major-mode
                                   ac-modes)))
                             (auto-complete-mode -1)))"##,
        expect![
            "OK (t (ac-source-filename ac-source-words-in-buffer ac-source-sage-modules ac-source-sage-vars-in-modules ac-source-sage-commands ac-source-sage-words-in-buffers) t (sage-shell:sage-mode sage-shell-mode emacs-lisp-mode lisp-mode lisp-interaction-mode slime-repl-mode nim-mode c-mode cc-mode c++-mode objc-mode swift-mode go-mode java-mode malabar-mode clojure-mode clojurescript-mode scala-mode scheme-mode ocaml-mode tuareg-mode coq-mode haskell-mode agda-mode agda2-mode perl-mode cperl-mode python-mode ruby-mode lua-mode tcl-mode ecmascript-mode javascript-mode js-mode js-jsx-mode js2-mode js2-jsx-mode coffee-mode php-mode css-mode scss-mode less-css-mode elixir-mode makefile-mode sh-mode fortran-mode f90-mode ada-mode xml-mode sgml-mode web-mode ts-mode sclang-mode verilog-mode qml-mode apples-mode))"
        ],
    )
}

fn auto_complete_sage_repeated_setup_preserves_packages_duplicate_source_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_repeated_setup_preserves_packages_duplicate_source_semantics",
        r##"(mapcar
                           (lambda (mode)
                             (with-temp-buffer
                               (setq major-mode mode
                                     ac-sources
                                     '(ac-source-filename))
                               (unwind-protect
                                   (progn
                                     (ac-sage-setup)
                                     (let ((first ac-sources))
                                       (ac-sage-setup)
                                       (list
                                        mode
                                        first
                                        ac-sources
                                        (mapcar
                                         (lambda (source)
                                           (list
                                            source
                                            (cl-count
                                             source
                                             ac-sources)))
                                         '(ac-source-filename
                                           ac-source-sage-methods
                                           ac-source-sage-modules)))))
                                 (auto-complete-mode -1))))
                           '(sage-shell-mode
                             sage-shell:sage-mode))"##,
        expect![
            "OK ((sage-shell-mode #1=(ac-sage-repl-modules ac-source-sage-methods ac-sage-repl-vars-in-module ac-source-sage-other-interfaces as-source-sage-repl-argspec ac-source-sage-repl-python-kwds ac-source-repl-sage-commands ac-source-sage-words-in-buffers ac-source-filename) (ac-sage-repl-modules ac-source-sage-methods ac-sage-repl-vars-in-module ac-source-sage-other-interfaces as-source-sage-repl-argspec ac-source-sage-repl-python-kwds ac-source-repl-sage-commands ac-source-sage-words-in-buffers . #1#) ((ac-source-filename 1) (ac-source-sage-methods 2) (ac-source-sage-modules 0))) (sage-shell:sage-mode (ac-source-filename . #2=(ac-source-sage-modules ac-source-sage-vars-in-modules ac-source-sage-commands ac-source-sage-words-in-buffers)) (ac-source-filename ac-source-sage-modules ac-source-sage-vars-in-modules ac-source-sage-commands ac-source-sage-words-in-buffers . #2#) ((ac-source-filename 1) (ac-source-sage-methods 0) (ac-source-sage-modules 2))))"
        ],
    )
}

fn auto_complete_sage_setup_in_unsupported_mode_only_enables_auto_complete() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_setup_in_unsupported_mode_only_enables_auto_complete",
        r##"(with-temp-buffer
                           (text-mode)
                           (setq ac-sources
                                 '(ac-source-abbrev
                                   ac-source-filename))
                           (let ((before ac-sources))
                             (unwind-protect
                                 (progn
                                   (ac-sage-setup)
                                   (list
                                    major-mode
                                    auto-complete-mode
                                    before
                                    ac-sources
                                    (equal
                                     before
                                     ac-sources)))
                               (auto-complete-mode -1))))"##,
        expect!["OK (text-mode t #1=(ac-source-abbrev ac-source-filename) #1# t)"],
    )
}

fn auto_complete_sage_command_cache_hook_clears_only_command_docs_until_method_clear_runs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_command_cache_hook_clears_only_command_docs_until_method_clear_runs",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-hook-process*")))
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (setq
                                  ac-sage--sage-commands-doc-cached
                                  '(("factor" . "factor docs"))
                                  ac-sage--repl-methods-cached
                                  '(("matrix.rank" . "rank docs")))
                                 (let ((sage-shell:process-buffer
                                        process-buffer)
                                       (sage-shell:clear-command-cache-hook
                                        sage-shell:clear-command-cache-hook))
                                   (run-hooks
                                    'sage-shell:clear-command-cache-hook)
                                   (let ((after-hook
                                          (list
                                           ac-sage--sage-commands-doc-cached
                                           ac-sage--repl-methods-cached)))
                                     (ac-sage--doc-clear-cache)
                                     (list
                                      after-hook
                                      ac-sage--sage-commands-doc-cached
                                      ac-sage--repl-methods-cached
                                      (cl-count
                                       'ac-sage--sage-commands-doc-clear-cache
                                       sage-shell:clear-command-cache-hook)))))
                             (kill-buffer process-buffer)))"##,
        expect![[r#"OK ((nil (("matrix.rank" . "rank docs"))) nil nil 1)"#]],
    )
}

fn auto_complete_sage_document_caches_and_completion_states_remain_buffer_isolated()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_document_caches_and_completion_states_remain_buffer_isolated",
        r##"(let ((first
                                (generate-new-buffer
                                 " *acsage-isolation-first*"))
                               (second
                                (generate-new-buffer
                                 " *acsage-isolation-second*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     first
                                   (setq
                                    ac-sage--sage-commands-doc-cached
                                    '(("factor" . "first factor"))
                                    ac-sage--repl-methods-cached
                                    '(("matrix.rank" . "first rank"))
                                    sage-shell-cpl:current-state
                                    '((interface . "sage")
                                      (var-base-name . "matrix")
                                      (types . ("attributes")))))
                                 (with-current-buffer
                                     second
                                   (setq
                                    ac-sage--sage-commands-doc-cached
                                    '(("plot" . "second plot"))
                                    ac-sage--repl-methods-cached
                                    '(("graph.order" . "second order"))
                                    sage-shell-cpl:current-state
                                    '((interface . "gap")
                                      (var-base-name . "graph")
                                      (types . ("attributes")))))
                                 (with-current-buffer
                                     first
                                   (let ((sage-shell:process-buffer
                                          first))
                                     (ac-sage--sage-commands-doc-clear-cache)))
                                 (mapcar
                                  (lambda (buffer)
                                    (with-current-buffer
                                        buffer
                                      (list
                                       (buffer-name)
                                       ac-sage--sage-commands-doc-cached
                                       ac-sage--repl-methods-cached
                                       sage-shell-cpl:current-state
                                       (ac-sage-repl--base-name-and-name
                                        "rank"))))
                                  (list first second)))
                             (kill-buffer first)
                             (kill-buffer second)))"##,
        expect![[
            r#"OK ((" *acsage-isolation-first*" nil (("matrix.rank" . "first rank")) ((interface . "sage") (var-base-name . "matrix") (types "attributes")) ("matrix" . "matrix.rank")) (" *acsage-isolation-second*" (("plot" . "second plot")) (("graph.order" . "second order")) ((interface . "gap") (var-base-name . "graph") (types "attributes")) ("graph" . "graph.rank")))"#
        ]],
    )
}

fn auto_complete_sage_setup_routes_independent_repl_and_edit_buffers_without_cross_leakage()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_setup_routes_independent_repl_and_edit_buffers_without_cross_leakage",
        r##"(let ((repl-buffer
                                (generate-new-buffer
                                 " *acsage-routing-repl*"))
                               (edit-buffer
                                (generate-new-buffer
                                 " *acsage-routing-edit*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     repl-buffer
                                   (setq major-mode
                                         'sage-shell-mode
                                         ac-sources
                                         '(ac-source-filename))
                                   (ac-sage-setup))
                                 (with-current-buffer
                                     edit-buffer
                                   (setq major-mode
                                         'sage-shell:sage-mode
                                         ac-sources
                                         '(ac-source-filename))
                                   (ac-sage-setup))
                                 (let ((result
                                        (mapcar
                                         (lambda (buffer)
                                           (with-current-buffer
                                               buffer
                                             (list
                                              major-mode
                                              auto-complete-mode
                                              ac-sources
                                              (local-variable-p
                                               'ac-sources))))
                                         (list
                                          repl-buffer
                                          edit-buffer))))
                                   (with-current-buffer
                                       repl-buffer
                                     (auto-complete-mode -1))
                                   (with-current-buffer
                                       edit-buffer
                                     (auto-complete-mode -1))
                                   result))
                             (kill-buffer repl-buffer)
                             (kill-buffer edit-buffer)))"##,
        expect![
            "OK ((sage-shell-mode t (ac-sage-repl-modules ac-source-sage-methods ac-sage-repl-vars-in-module ac-source-sage-other-interfaces as-source-sage-repl-argspec ac-source-sage-repl-python-kwds ac-source-repl-sage-commands ac-source-sage-words-in-buffers ac-source-filename) t) (sage-shell:sage-mode t (ac-source-filename ac-source-sage-modules ac-source-sage-vars-in-modules ac-source-sage-commands ac-source-sage-words-in-buffers) t))"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_sage_setup_enables_mode_and_prepends_repl_sources_in_real_buffer_state(),
        auto_complete_sage_setup_enables_mode_and_appends_edit_sources_in_real_buffer_state(),
        auto_complete_sage_repeated_setup_preserves_packages_duplicate_source_semantics(),
        auto_complete_sage_setup_in_unsupported_mode_only_enables_auto_complete(),
        auto_complete_sage_command_cache_hook_clears_only_command_docs_until_method_clear_runs(),
        auto_complete_sage_document_caches_and_completion_states_remain_buffer_isolated(),
        auto_complete_sage_setup_routes_independent_repl_and_edit_buffers_without_cross_leakage(),
    ]
}
