use expect_test::expect;

use super::ParityBatchCase;

fn transient_parse_suffixes_returns_canonical_layout_specs() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_parse_suffixes_returns_canonical_layout_specs",
        r##"(progn
               (transient-define-suffix neomacs-test-run ()
                 :key "r"
                 :description "Run"
                 (interactive))
               (transient-define-argument neomacs-test-verbose ()
                 :class 'transient-switch
                 :shortarg "-v"
                 :argument "--verbose")
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-run)
                  (neomacs-test-verbose)])
               (let* ((prefix
                       (transient--init-prefix 'neomacs-test-menu))
                      (specs
                       (transient-parse-suffixes
                        prefix
                        '((neomacs-test-run)
                          (neomacs-test-verbose)))))
                 specs))"##,
        expect![[
            r#"OK ((transient-suffix :command neomacs-test-run) (transient-suffix :command neomacs-test-verbose))"#
        ]],
    )
}

fn transient_scope_resolves_active_matching_and_default_prefix_scope() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_scope_resolves_active_matching_and_default_prefix_scope",
        r##"(progn
               (transient-define-prefix neomacs-test-menu ()
                 :scope 'default-scope
                 [])
               (transient-define-prefix neomacs-test-other ()
                 :scope '(other scope)
                 [])
               (let* ((active
                       (transient--init-prefix 'neomacs-test-other))
                      (transient--prefix active)
                      (transient-current-prefix nil))
                 (list
                  (copy-tree (transient-scope))
                  (copy-tree
                   (transient-scope 'neomacs-test-other))
                  (transient-scope 'neomacs-test-menu)
                  (copy-tree
                   (transient-scope
                    nil 'transient-prefix)))))"##,
        expect![[r#"OK ((other scope) (other scope) default-scope (other scope))"#]],
    )
}

fn transient_history_key_initialization_and_push_deduplicate_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_history_key_initialization_and_push_deduplicate_values",
        r##"(progn
               (transient-define-prefix neomacs-test-menu ()
                 :history-key 'neomacs-shared-history
                 [])
               (let* ((transient-history
                       '((neomacs-shared-history
                          ("old") ("duplicate") ("old"))))
                      (object
                       (transient--init-prefix 'neomacs-test-menu)))
                 (oset object value '("current"))
                 (transient--history-init object)
                 (let ((initialized
                        (copy-tree (oref object history))))
                   (transient--history-push object '("old"))
                   (transient--history-push object '("new"))
                   (transient--history-push object '("old"))
                   (list
                    (transient--history-key object)
                    initialized
                    (alist-get
                     'neomacs-shared-history
                     transient-history)))))"##,
        expect![[
            r#"OK (neomacs-shared-history (nil ("old") ("duplicate") ("old")) (("old") ("new") ("duplicate")))"#
        ]],
    )
}

fn transient_suffix_dispatch_metadata_selects_no_export_call_and_exit_behaviors() -> ParityBatchCase
{
    ParityBatchCase::value(
        "transient_suffix_dispatch_metadata_selects_no_export_call_and_exit_behaviors",
        r##"(progn
               (transient-define-suffix neomacs-test-no-export ()
                 :key "s"
                 :transient #'transient--do-stay
                 (interactive))
               (transient-define-suffix neomacs-test-call ()
                 :key "c"
                 :transient #'transient--do-call
                 (interactive))
               (transient-define-suffix neomacs-test-exit ()
                 :key "x"
                 :transient nil
                 (interactive))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-no-export)
                  (neomacs-test-call)
                  (neomacs-test-exit)])
               (transient--init-objects
                'neomacs-test-menu nil nil)
               (let ((transient--predicate-map
                      (transient--make-predicate-map)))
                 (mapcar
                  (lambda (command)
                    (lookup-key
                     transient--predicate-map
                     (vector command)))
                  '(neomacs-test-no-export
                    neomacs-test-call
                    neomacs-test-exit))))"##,
        expect![[r#"OK (transient--do-stay transient--do-call transient--do-exit)"#]],
    )
}

fn transient_dispatch_executes_suffixes_and_exports_call_and_exit_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_dispatch_executes_suffixes_and_exports_call_and_exit_state",
        r##"(progn
               (setq neomacs-test-events nil)
               (transient-define-suffix neomacs-test-no-export ()
                 :key "s"
                 :transient #'transient--do-stay
                 (interactive)
                 (push 'no-export neomacs-test-events))
               (transient-define-suffix neomacs-test-call ()
                 :key "c"
                 :transient #'transient--do-call
                 (interactive)
                 (push 'call neomacs-test-events))
               (transient-define-suffix neomacs-test-exit ()
                 :key "x"
                 :transient nil
                 (interactive)
                 (push 'exit neomacs-test-events))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-no-export)
                  (neomacs-test-call)
                  (neomacs-test-exit)])
               (let (results)
                 (transient--init-objects
                  'neomacs-test-menu nil nil)
                 (setq transient--predicate-map
                       (transient--make-predicate-map))
                 (dolist
                     (command
                      '(neomacs-test-no-export
                        neomacs-test-call
                        neomacs-test-exit))
                   (setq this-command command
                         transient-current-command nil
                         transient--exitp nil)
                   (let ((action
                          (transient--call-pre-command)))
                     (call-interactively command)
                     (push
                      (list
                       command
                       action
                       transient--pre-command
                       transient-current-command
                       transient--exitp)
                      results)))
                 (list
                  (nreverse results)
                  (nreverse neomacs-test-events))))"##,
        expect![[
            r#"OK (((neomacs-test-no-export t transient--do-stay nil nil) (neomacs-test-call t transient--do-call neomacs-test-menu nil) (neomacs-test-exit nil transient--do-exit neomacs-test-menu t)) (no-export call exit))"#
        ]],
    )
}

fn transient_get_suffix_signals_for_missing_binding() -> ParityBatchCase {
    ParityBatchCase::signal(
        "transient_get_suffix_signals_for_missing_binding",
        r##"(progn
               (transient-define-prefix neomacs-test-menu () [])
               (transient-get-suffix
                'neomacs-test-menu "missing"))"##,
        expect![[r#"ERR (error "missing not found in neomacs-test-menu")"#]],
    )
}

pub(super) fn state_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        transient_parse_suffixes_returns_canonical_layout_specs(),
        transient_scope_resolves_active_matching_and_default_prefix_scope(),
        transient_history_key_initialization_and_push_deduplicate_values(),
        transient_suffix_dispatch_metadata_selects_no_export_call_and_exit_behaviors(),
        transient_dispatch_executes_suffixes_and_exports_call_and_exit_state(),
        transient_get_suffix_signals_for_missing_binding(),
    ]
}
