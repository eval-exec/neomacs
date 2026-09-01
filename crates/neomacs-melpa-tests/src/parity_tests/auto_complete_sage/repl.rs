use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_sage_repl_generated_prefixes_route_each_completion_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_repl_generated_prefixes_route_each_completion_state",
        r##"(with-temp-buffer
                           (insert "matrix.ra")
                           (goto-char (point-max))
                           (mapcar
                            (lambda (fixture)
                              (let ((sage-shell-cpl:current-state
                                     (cadr fixture))
                                    parse-calls)
                                (cl-letf
                                    (((symbol-function
                                       'sage-shell-cpl:parse-and-set-state)
                                      (lambda ()
                                        (setq parse-calls
                                              (1+ (or parse-calls 0))))))
                                  (list
                                   (car fixture)
                                   (funcall (car fixture))
                                   parse-calls))))
                            `((ac-sage-repl--sage-interface-prefix
                               ((interface . "sage")
                                (types . ("interface"))))
                              (ac-sage-repl--sage-interface-prefix
                               ((interface . "gap")
                                (types . ("interface"))))
                              (ac-sage-repl--other-interface-prefix
                               ((interface . "gap")
                                (types . ("interface"))))
                              (ac-sage-repl--attributes-prefix
                               ((interface . "sage")
                                (var-base-name . "matrix")
                                (types . ("attributes"))))
                              (ac-sage-repl--modules-prefix
                               ((interface . "sage")
                                (types . ("modules"))))
                              (ac-sage-repl--vars-in-module-prefix
                               ((interface . "sage")
                                (types . ("vars-in-module"))))
                              (ac-sage-repl--argspec-prefix
                               ((interface . "sage")
                                (types . ("in-function-call")))))))"##,
        expect![
            "OK ((ac-sage-repl--sage-interface-prefix 8 nil) (ac-sage-repl--sage-interface-prefix nil nil) (ac-sage-repl--other-interface-prefix 8 nil) (ac-sage-repl--attributes-prefix 8 nil) (ac-sage-repl--modules-prefix 8 1) (ac-sage-repl--vars-in-module-prefix 8 nil) (ac-sage-repl--argspec-prefix 8 nil))"
        ],
    )
}

fn auto_complete_sage_repl_source_initializers_forward_sync_flag_and_exact_current_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_repl_source_initializers_forward_sync_flag_and_exact_current_state",
        r##"(let ((sage-shell-cpl:current-state
                                '((interface . "sage")
                                  (prefix . 3)
                                  (var-base-name . "matrix")
                                  (types . ("interface"
                                            "attributes"))))
                               calls)
                           (cl-letf
                               (((symbol-function
                                  'sage-shell-cpl:completion-init)
                                 (lambda (sync &rest arguments)
                                   (push
                                    (list
                                     sync
                                     arguments
                                     (eq
                                      (plist-get
                                       arguments
                                       :compl-state)
                                      sage-shell-cpl:current-state))
                                    calls)
                                   :initialized)))
                             (dolist
                                 (command
                                  '(auto-complete
                                    self-insert-command))
                               (let ((this-command command))
                                 (dolist
                                     (source
                                      (list
                                       ac-source-repl-sage-commands
                                       ac-source-sage-methods
                                       ac-source-sage-other-interfaces
                                       ac-sage-repl-modules
                                       ac-sage-repl-vars-in-module
                                       as-source-sage-repl-argspec))
                                   (funcall
                                    (cdr
                                     (assq
                                      'init
                                      source))))))
                             (nreverse calls)))"##,
        expect![[
            r#"OK ((t (:compl-state #1=((interface . "sage") (prefix . 3) (var-base-name . "matrix") (types "interface" "attributes"))) t) (t (:compl-state #1#) t) (t (:compl-state #1#) t) (t (:compl-state #1#) t) (t (:compl-state #1#) t) (t (:compl-state #1#) t) (nil (:compl-state #1#) t) (nil (:compl-state #1#) t) (nil (:compl-state #1#) t) (nil (:compl-state #1#) t) (nil (:compl-state #1#) t) (nil (:compl-state #1#) t))"#
        ]],
    )
}

fn auto_complete_sage_repl_candidate_closures_gate_and_forward_exact_type_keys() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_sage_repl_candidate_closures_gate_and_forward_exact_type_keys",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'ac-sage-repl:candidates)
                                 (lambda (keys)
                                   (push keys calls)
                                   (mapcar
                                    (lambda (key)
                                      (concat key "-candidate"))
                                    keys))))
                             (mapcar
                              (lambda (fixture)
                                (let ((sage-shell-cpl:current-state
                                       (cadr fixture)))
                                  (list
                                   (car fixture)
                                   (funcall
                                    (cdr
                                     (assq
                                      'candidates
                                      (symbol-value
                                       (car fixture))))))))
                              `((ac-source-repl-sage-commands
                                  ((interface . "sage")
                                   (types . ("interface"))))
                                (ac-source-repl-sage-commands
                                  ((interface . "gap")
                                   (types . ("interface"))))
                                (ac-source-sage-methods
                                  ((interface . "sage")
                                   (types . ("attributes"))))
                                (ac-source-sage-other-interfaces
                                  ((interface . "gap")
                                   (types . ("interface"))))
                                (ac-sage-repl-modules
                                  ((interface . "sage")
                                   (types . ("modules"))))
                                (ac-sage-repl-vars-in-module
                                  ((interface . "sage")
                                   (types . ("vars-in-module"))))
                                (as-source-sage-repl-argspec
                                  ((interface . "sage")
                                   (types . ("in-function-call"))))
                                (ac-source-sage-methods
                                  ((interface . "sage")
                                   (types . ("modules"))))))
                             (nreverse calls)))"##,
        expect![[
            r#"OK (("interface") ("attributes") ("interface") ("modules") ("vars-in-module") ("in-function-call"))"#
        ]],
    )
}

fn auto_complete_sage_repl_candidate_wrapper_preserves_keywords_results_and_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_repl_candidate_wrapper_preserves_keywords_results_and_errors",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'sage-shell-cpl:candidates)
                                 (lambda (&rest arguments)
                                   (push arguments calls)
                                   (if
                                       (equal
                                        arguments
                                        '(:keys ("attributes")))
                                       '("rank"
                                         "trace"
                                         "transpose")
                                     (signal
                                      'wrong-type-argument
                                      arguments)))))
                             (list
                              (ac-sage-repl:candidates
                               '("attributes"))
                              (acsage-test-error
                               (lambda ()
                                 (ac-sage-repl:candidates
                                  '("unknown"))))
                              (nreverse calls))))"##,
        expect![[
            r#"OK (("rank" "trace" "transpose") (:signal wrong-type-argument #1=(:keys ("unknown"))) ((:keys ("attributes")) #1#))"#
        ]],
    )
}

fn auto_complete_sage_python_keyword_candidates_require_sage_interface_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_python_keyword_candidates_require_sage_interface_state",
        r##"(mapcar
                           (lambda (state)
                             (let ((sage-shell-cpl:current-state
                                    state))
                               (let ((candidates
                                      (ac-sage-repl-python-kwds-candidates)))
                                 (list
                                  state
                                  (and candidates
                                       (length candidates))
                                  (and candidates
                                       (seq-filter
                                        (lambda (candidate)
                                          (string-prefix-p
                                           "is"
                                           candidate))
                                        candidates))))))
                           '(((interface . "sage")
                              (types . ("interface")))
                             ((interface . "sage")
                              (types . ("attributes")))
                             ((interface . "gap")
                              (types . ("interface")))
                             ((interface . "sage")
                              (types))
                             nil))"##,
        expect![[
            r#"OK ((((interface . "sage") (types "interface")) 110 ("is" "isinstance" "issubclass")) (((interface . "sage") (types "attributes")) nil nil) (((interface . "gap") (types "interface")) nil nil) (((interface . "sage") (types)) nil nil) (nil nil nil))"#
        ]],
    )
}

fn auto_complete_sage_repl_add_sources_prepends_exact_order_and_preserves_duplicates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_repl_add_sources_prepends_exact_order_and_preserves_duplicates",
        r##"(let ((ac-sources
                                '(ac-source-filename
                                  ac-source-sage-methods)))
                           (ac-sage-repl:add-sources)
                           (let ((first ac-sources))
                             (ac-sage-repl:add-sources)
                             (list
                              first
                              ac-sources
                              (mapcar
                               (lambda (source)
                                 (list
                                  source
                                  (cl-count
                                   source
                                   ac-sources)))
                               '(ac-sage-repl-modules
                                 ac-source-sage-methods
                                 ac-source-filename)))))"##,
        expect![
            "OK (#1=(ac-sage-repl-modules ac-source-sage-methods ac-sage-repl-vars-in-module ac-source-sage-other-interfaces as-source-sage-repl-argspec ac-source-sage-repl-python-kwds ac-source-repl-sage-commands ac-source-sage-words-in-buffers ac-source-filename ac-source-sage-methods) (ac-sage-repl-modules ac-source-sage-methods ac-sage-repl-vars-in-module ac-source-sage-other-interfaces as-source-sage-repl-argspec ac-source-sage-repl-python-kwds ac-source-repl-sage-commands ac-source-sage-words-in-buffers . #1#) ((ac-sage-repl-modules 2) (ac-source-sage-methods 3) (ac-source-filename 1)))"
        ],
    )
}

fn auto_complete_sage_repl_source_contracts_expose_callable_runtime_components() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_sage_repl_source_contracts_expose_callable_runtime_components",
        r##"(mapcar
                           (lambda (symbol)
                             (let ((source
                                    (symbol-value symbol)))
                               (list
                                symbol
                                (mapcar
                                 (lambda (property)
                                   (let ((value
                                          (cdr
                                           (assq
                                            property
                                            source))))
                                     (list
                                      property
                                      value
                                      (and
                                       (functionp value)
                                       t))))
                                 '(init
                                   candidates
                                   prefix
                                   document))
                                (cdr
                                 (assq 'cache source))
                                (cdr
                                 (assq 'requires source))
                                (cdr
                                 (assq 'symbol source)))))
                           '(ac-source-repl-sage-commands
                             ac-source-sage-methods
                             ac-source-sage-other-interfaces
                             ac-sage-repl-modules
                             ac-sage-repl-vars-in-module
                             ac-source-sage-repl-python-kwds
                             as-source-sage-repl-argspec))"##,
        expect![[
            r#"OK ((ac-source-repl-sage-commands ((init #[nil (#2=(sage-shell-cpl:completion-init (equal this-command 'auto-complete) :compl-state sage-shell-cpl:current-state)) nil] t) (candidates #[nil ((if (and (sage-shell:in #1="interface" . #4=((sage-shell-cpl:get-current 'types))) (string= (sage-shell-cpl:get-current 'interface) "sage")) (progn (ac-sage-repl:candidates (list #1#))))) nil] t) (prefix ac-sage-repl--sage-interface-prefix t) (document ac-sage-doc t)) nil nil "s") (ac-source-sage-methods ((init #[nil (#2#) nil] t) (candidates #[nil ((if (sage-shell:in #3="attributes" . #6=((sage-shell-cpl:get-current 'types))) (progn (ac-sage-repl:candidates (list #3#))))) nil] t) (prefix ac-sage-repl--attributes-prefix t) (document ac-sage-repl-methods-doc t)) nil 0 "s") (ac-source-sage-other-interfaces ((init #[nil (#2#) nil] t) (candidates #[nil ((if (and (sage-shell:in #5="interface" . #4#) (not (string= (sage-shell-cpl:get-current 'interface) "sage"))) (progn (ac-sage-repl:candidates (list #5#))))) nil] t) (prefix ac-sage-repl--other-interface-prefix t) (document nil nil)) nil nil "s") (ac-sage-repl-modules ((init #[nil (#2#) nil] t) (candidates #[nil ((if (sage-shell:in #7="modules" . #6#) (progn (ac-sage-repl:candidates (list #7#))))) nil] t) (prefix ac-sage-repl--modules-prefix t) (document nil nil)) nil 0 "m") (ac-sage-repl-vars-in-module ((init #[nil (#2#) nil] t) (candidates #[nil ((if (sage-shell:in #8="vars-in-module" . #6#) (progn (ac-sage-repl:candidates (list #8#))))) nil] t) (prefix ac-sage-repl--vars-in-module-prefix t) (document nil nil)) nil nil "s") (ac-source-sage-repl-python-kwds ((init nil nil) (candidates ac-sage-repl-python-kwds-candidates t) (prefix nil nil) (document nil nil)) nil nil nil) (as-source-sage-repl-argspec ((init #[nil (#2#) nil] t) (candidates #[nil ((if (sage-shell:in #9="in-function-call" . #6#) (progn (ac-sage-repl:candidates (list #9#))))) nil] t) (prefix ac-sage-repl--argspec-prefix t) (document nil nil)) nil nil nil))"#
        ]],
    )
}

fn auto_complete_sage_real_repl_menu_completes_a_matrix_method_through_target_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_real_repl_menu_completes_a_matrix_method_through_target_source",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (let ((ac-use-comphist nil)
                                   (ac-use-quick-help nil)
                                   (ac-auto-show-menu t)
                                   (ac-expand-on-auto-complete nil)
                                   (ac-ignore-case nil)
                                   (ac-sources
                                    '(ac-source-sage-methods))
                                   (sage-shell-cpl:current-state
                                    '((interface . "sage")
                                      (prefix . 8)
                                      (var-base-name . "matrix")
                                      (types . ("attributes"))))
                                   events)
                               (cl-letf
                                   (((symbol-function
                                      'sage-shell-cpl:completion-init)
                                     (lambda (sync &rest arguments)
                                       (push
                                        (list
                                         :init
                                         sync
                                         (plist-get
                                          arguments
                                          :compl-state))
                                        events)))
                                    ((symbol-function
                                      'sage-shell-cpl:candidates)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons
                                         :candidates
                                         arguments)
                                        events)
                                       '("rank"
                                         "rank_deficiency"
                                         "randomize"))))
                                 (unwind-protect
                                     (progn
                                       (auto-complete-mode 1)
                                       (insert
                                        "answer = matrix.ra")
                                       (auto-complete)
                                       (let ((initial
                                              (list
                                               ac-prefix
                                               (mapcar
                                                (lambda (candidate)
                                                  (list
                                                   (substring-no-properties
                                                    candidate)
                                                   (popup-item-symbol
                                                    candidate)))
                                                ac-candidates)
                                               (popup-live-p
                                                ac-menu)
                                               (substring-no-properties
                                                (ac-selected-candidate)))))
                                         (ac-next)
                                         (let ((selected
                                                (substring-no-properties
                                                 (ac-selected-candidate))))
                                           (ac-complete)
                                           (list
                                            initial
                                            selected
                                            (buffer-string)
                                            (nreverse events)
                                            ac-menu
                                            ac-completing
                                            ac-prefix))))
                                   (auto-complete-mode -1))))))"##,
        expect![[
            r#"OK (("ra" (("rank" "s") ("rank_deficiency" "s") ("randomize" "s")) t "rank") "rank_deficiency" "answer = matrix.rank_deficiency" ((:init nil ((interface . "sage") (prefix . 8) (var-base-name . "matrix") (types "attributes"))) (:candidates :keys ("attributes"))) nil nil nil)"#
        ]],
    )
}

fn auto_complete_sage_repl_interface_sources_partition_sage_and_foreign_commands() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_sage_repl_interface_sources_partition_sage_and_foreign_commands",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'ac-sage-repl:candidates)
                                 (lambda (keys)
                                   (push keys calls)
                                   '("help"
                                     "quit"
                                     "reset"))))
                             (mapcar
                              (lambda (interface)
                                (let ((sage-shell-cpl:current-state
                                       `((interface . ,interface)
                                         (types . ("interface")))))
                                  (list
                                   interface
                                   (funcall
                                    (cdr
                                     (assq
                                      'candidates
                                      ac-source-repl-sage-commands)))
                                   (funcall
                                    (cdr
                                     (assq
                                      'candidates
                                      ac-source-sage-other-interfaces))))))
                              '("sage"
                                "gap"
                                "magma"))
                             (nreverse calls)))"##,
        expect![[r#"OK (("interface") ("interface") ("interface"))"#]],
    )
}

pub(super) fn repl_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_sage_repl_generated_prefixes_route_each_completion_state(),
        auto_complete_sage_repl_source_initializers_forward_sync_flag_and_exact_current_state(),
        auto_complete_sage_repl_candidate_closures_gate_and_forward_exact_type_keys(),
        auto_complete_sage_repl_candidate_wrapper_preserves_keywords_results_and_errors(),
        auto_complete_sage_python_keyword_candidates_require_sage_interface_state(),
        auto_complete_sage_repl_add_sources_prepends_exact_order_and_preserves_duplicates(),
        auto_complete_sage_repl_source_contracts_expose_callable_runtime_components(),
        auto_complete_sage_real_repl_menu_completes_a_matrix_method_through_target_source(),
        auto_complete_sage_repl_interface_sources_partition_sage_and_foreign_commands(),
    ]
}
