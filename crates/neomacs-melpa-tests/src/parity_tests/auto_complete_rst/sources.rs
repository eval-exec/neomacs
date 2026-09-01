use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_rst_sources_expose_exact_auto_complete_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_sources_expose_exact_auto_complete_contracts",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (auto-complete-rst-test-source-shape
                               (symbol-value symbol))))
                           '(ac-source-rst-directives
                             ac-source-rst-roles
                             ac-source-rst-options))"##,
        expect![[
            r####"OK ((ac-source-rst-directives ((candidates . auto-complete-rst-directives-candidates) (available fboundp 'auto-complete-rst-directives-candidates) (prefix . "[[:space:]]\\.\\. \\([[:alnum:]-:]*\\)") (symbol . "D") (requires . 0))) (ac-source-rst-roles ((candidates . auto-complete-rst-roles-candidates) (available fboundp 'auto-complete-rst-roles-candidates) (prefix . "[^[:alnum:]:]:\\([[:alnum:]-:]*\\)") (symbol . "R") (requires . 0) (action . :function))) (ac-source-rst-options ((candidates . :function) (prefix . "[[:space:]]\\{4,\\}:\\([^:]*\\)") (symbol . "O") (requires . 0))))"####
        ]],
    )
    .fresh_process()
}

fn auto_complete_rst_generated_candidate_availability_changes_after_source_eval() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_generated_candidate_availability_changes_after_source_eval",
        r##"(let
                             ((auto-complete-rst-directive-options-map
                               (make-hash-table :test 'equal)))
                           (fmakunbound
                            'auto-complete-rst-directives-candidates)
                           (fmakunbound
                            'auto-complete-rst-roles-candidates)
                           (let
                               ((before
                                 (mapcar
                                  (lambda (source)
                                    (eval
                                     (cdr
                                      (assq
                                       'available
                                       (symbol-value source)))))
                                  '(ac-source-rst-directives
                                    ac-source-rst-roles))))
                             (with-temp-buffer
                               (auto-complete-rst-test-insert-generated-source)
                               (eval-buffer))
                             (list
                              before
                              (mapcar
                               (lambda (source)
                                 (eval
                                  (cdr
                                   (assq
                                    'available
                                    (symbol-value source)))))
                               '(ac-source-rst-directives
                                 ac-source-rst-roles))
                              (auto-complete-rst-directives-candidates)
                              (auto-complete-rst-roles-candidates))))"##,
        expect![[
            r####"OK ((nil nil) (t t) ("note::" "code-block::" "image::" "py:function::") ("ref:" "doc:" "py:class:" "emphasis:"))"####
        ]],
    )
}

fn auto_complete_rst_source_prefixes_extract_real_partial_directive_role_and_option()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_source_prefixes_extract_real_partial_directive_role_and_option",
        r##"(mapcar
                           (lambda (case)
                             (let
                                 ((source
                                   (symbol-value (car case)))
                                  (text (cdr case)))
                               (with-temp-buffer
                                 (insert text)
                                 (goto-char (point-max))
                                 (let
                                     ((prefix
                                       (cdr
                                        (assq 'prefix source))))
                                   (list
                                    (car case)
                                    text
                                    (and
                                     (string-match prefix text)
                                     (match-string 1 text)))))))
                           '((ac-source-rst-directives
                              . "Intro\n\n.. code-bl")
                             (ac-source-rst-roles
                              . "See :py:cla")
                             (ac-source-rst-options
                              . ".. image:: x.png\n    :hei")))"##,
        expect![[
            r####"OK ((ac-source-rst-directives "Intro\n\n.. code-bl" "code-bl") (ac-source-rst-roles "See :py:cla" "py:cla") (ac-source-rst-options ".. image:: x.png\n    :hei" "hei"))"####
        ]],
    )
}

fn auto_complete_rst_source_prefixes_reject_non_rst_and_boundary_lookalikes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_source_prefixes_reject_non_rst_and_boundary_lookalikes",
        r##"(mapcar
                           (lambda (case)
                             (let
                                 ((regexp
                                   (cdr
                                    (assq
                                     'prefix
                                     (symbol-value
                                      (car case)))))
                                  (text (cdr case)))
                               (list
                                (car case)
                                text
                                (and
                                 (string-match regexp text)
                                 (match-string 1 text)))))
                           '((ac-source-rst-directives
                              . ".. note")
                             (ac-source-rst-directives
                              . "prefix.. note")
                             (ac-source-rst-roles
                              . "scheme::ref")
                             (ac-source-rst-roles
                              . "word:ref")
                             (ac-source-rst-options
                              . "   :alt")
                             (ac-source-rst-options
                              . "    :alt:value")))"##,
        expect![[
            r####"OK ((ac-source-rst-directives ".. note" nil) (ac-source-rst-directives "prefix.. note" nil) (ac-source-rst-roles "scheme::ref" nil) (ac-source-rst-roles "word:ref" nil) (ac-source-rst-options "   :alt" nil) (ac-source-rst-options "    :alt:value" "alt"))"####
        ]],
    )
}

fn auto_complete_rst_role_action_inserts_balanced_backquotes_with_point_inside() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_role_action_inserts_balanced_backquotes_with_point_inside",
        r##"(mapcar
                           (lambda (text)
                             (with-temp-buffer
                               (insert text)
                               (funcall
                                (cdr
                                 (assq
                                  'action
                                  ac-source-rst-roles)))
                               (list
                                (buffer-string)
                                (point)
                                (char-before)
                                (char-after))))
                           '(":ref:"
                             "See :doc:"
                             "Use :py:class:"))"##,
        expect![[
            r####"OK ((":ref:``" 7 96 96) ("See :doc:``" 11 96 96) ("Use :py:class:``" 16 96 96))"####
        ]],
    )
}

fn auto_complete_rst_add_sources_preserves_order_customization_keys_and_idempotence()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_add_sources_preserves_order_customization_keys_and_idempotence",
        r##"(let
                             ((auto-complete-rst-other-sources
                               '(ac-source-filename
                                 ac-source-words-in-buffer)))
                           (with-temp-buffer
                             (use-local-map
                              (make-sparse-keymap))
                             (setq
                              ac-sources
                              '(ac-source-abbrev))
                             (auto-complete-rst-add-sources)
                             (let
                                 ((first ac-sources))
                               (auto-complete-rst-add-sources)
                               (list
                                first
                                ac-sources
                                (key-binding (kbd ":"))
                                (key-binding (kbd "SPC"))
                                (local-variable-p
                                 'ac-sources)
                                (keymap-parent
                                 (current-local-map))))))"##,
        expect![[
            r####"OK ((ac-source-rst-options ac-source-rst-roles ac-source-rst-directives . #1=(ac-source-filename ac-source-words-in-buffer)) (ac-source-rst-options ac-source-rst-roles ac-source-rst-directives . #1#) auto-complete-rst-complete-colon auto-complete-rst-complete-space t nil)"####
        ]],
    )
}

fn auto_complete_rst_add_sources_uses_current_sources_when_override_is_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_add_sources_uses_current_sources_when_override_is_nil",
        r##"(let
                             ((auto-complete-rst-other-sources nil))
                           (with-temp-buffer
                             (use-local-map
                              (make-sparse-keymap))
                             (setq
                              ac-sources
                              '(ac-source-filename
                                ac-source-abbrev))
                             (auto-complete-rst-add-sources)
                             ac-sources))"##,
        expect![[
            r####"OK (ac-source-rst-options ac-source-rst-roles ac-source-rst-directives ac-source-filename ac-source-abbrev)"####
        ]],
    )
}

fn auto_complete_rst_completion_keys_insert_text_and_route_exact_source_sets() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_completion_keys_insert_text_and_route_exact_source_sets",
        r##"(mapcar
                           (lambda (enabled)
                             (let
                                 ((auto-complete-mode enabled)
                                  calls)
                               (cl-letf
                                   (((symbol-function 'auto-complete)
                                     (lambda (sources)
                                       (push sources calls)
                                       :started)))
                                 (with-temp-buffer
                                   (insert ".. note")
                                   (auto-complete-rst-complete-space)
                                   (insert ":ref")
                                   (auto-complete-rst-complete-colon)
                                   (list
                                    enabled
                                    (buffer-string)
                                    (nreverse calls)
                                    (point))))))
                           '(nil t))"##,
        expect![[
            r####"OK ((nil ".. note :ref:" nil 14) (t ".. note :ref:" ((ac-source-rst-directives) (ac-source-rst-directives ac-source-rst-options ac-source-rst-roles)) 14))"####
        ]],
    )
}

pub(super) fn sources_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_rst_sources_expose_exact_auto_complete_contracts(),
        auto_complete_rst_generated_candidate_availability_changes_after_source_eval(),
        auto_complete_rst_source_prefixes_extract_real_partial_directive_role_and_option(),
        auto_complete_rst_source_prefixes_reject_non_rst_and_boundary_lookalikes(),
        auto_complete_rst_role_action_inserts_balanced_backquotes_with_point_inside(),
        auto_complete_rst_add_sources_preserves_order_customization_keys_and_idempotence(),
        auto_complete_rst_add_sources_uses_current_sources_when_override_is_nil(),
        auto_complete_rst_completion_keys_insert_text_and_route_exact_source_sets(),
    ]
}
