use expect_test::expect;

use super::ParityBatchCase;

fn alan_mode_activation_installs_real_syntax_comments_indentation_and_xref_locally()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_mode_activation_installs_real_syntax_comments_indentation_and_xref_locally",
        r##"(with-temp-buffer
                      (alan-mode)
                      (list
                       major-mode
                       mode-name
                       (mapcar
                        (lambda (char)
                          (list char
                                (char-syntax char)
                                (syntax-after (string-match
                                               (regexp-quote (char-to-string char))
                                               "'x' /* y */ [] {}"))))
                        '(?' ?/ ?* ?[ ?] ?{ ?}))
                       comment-start comment-end
                       block-comment-start block-comment-end
                       indent-line-function
                       (memq #'alan--xref-backend xref-backend-functions)
                       (local-variable-p 'syntax-propertize-function)
                       (local-variable-p 'font-lock-defaults)))"##,
        expect![[
            r#"OK (alan-mode "Alan" ((39 34 nil) (47 46 nil) (42 46 nil) (91 95 nil) (93 95 nil) (123 95 nil) (125 95 nil)) "//" "" "/*" "*/" alan-mode-indent-line (alan--xref-backend t) t t)"#
        ]],
    )
}

fn alan_mode_font_lock_distinguishes_identifiers_docs_comments_strings_and_types() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alan_mode_font_lock_distinguishes_identifiers_docs_comments_strings_and_types",
        r##"(with-temp-buffer
                      (cl-letf (((symbol-function
                                 'alan-setup-build-system)
                                (lambda () nil)))
                        (alan-schema-mode))
                      (insert
                       "/// Account documentation\n"
                       "'Account' -> component {\n"
                       "  'status': stategroup\n"
                       "  \"ordinary string\" // trailing comment\n"
                       "  deprecated\n"
                       "}\n")
                      (font-lock-ensure)
                      (mapcar
                       (lambda (needle)
                         (goto-char (point-min))
                         (search-forward needle)
                         (list needle
                               (get-text-property
                                (1- (point)) 'face)
                               (nth 3 (syntax-ppss (1- (point))))
                               (nth 4 (syntax-ppss (1- (point))))))
                       '("documentation" "Account" "component" "status"
                         "stategroup" "ordinary" "comment" "deprecated")))"##,
        expect![[
            r#"OK (("documentation" font-lock-doc-face nil t) ("Account" font-lock-doc-face nil t) ("component" font-lock-builtin-face nil nil) ("status" font-lock-variable-name-face 39 nil) ("stategroup" font-lock-type-face nil nil) ("ordinary" font-lock-string-face 34 nil) ("comment" font-lock-comment-face nil t) ("deprecated" font-lock-warning-face nil nil))"#
        ]],
    )
}

fn alan_mode_indents_nested_and_single_line_blocks_as_a_user_edits_them() -> ParityBatchCase {
    ParityBatchCase::value(
        "alan_mode_indents_nested_and_single_line_blocks_as_a_user_edits_them",
        r##"(with-temp-buffer
                      (setq tab-width 2)
                      (alan-mode)
                      (insert
                       "'root' {\n"
                       "'child' {\n"
                       "'leaf': text\n"
                       "}\n"
                       "('first')\n"
                       "('second')\n"
                       "}\n")
                      (indent-region (point-min) (point-max))
                      (list
                       (buffer-string)
                       (save-excursion
                         (goto-char (point-min))
                         (forward-line 2)
                         (alan--single-block 2))
                       (save-excursion
                         (goto-char (point-min))
                         (forward-line 4)
                         (alan--single-block 0))))"##,
        expect![[
            r#"OK ("'root' {\n'child' {\n'leaf': text\n}\n\11('first')\n\11('second')\n\11}\n" t t)"#
        ]],
    )
}

fn alan_grammar_update_rebuilds_sorted_unique_keywords_and_preserves_annotations() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alan_grammar_update_rebuilds_sorted_unique_keywords_and_preserves_annotations",
        r##"(with-temp-buffer
                      (cl-letf (((symbol-function
                                 'alan-setup-build-system)
                                (lambda () nil)))
                        (alan-grammar-mode))
                      (insert
                       "keywords\n"
                       "\t'old'\n"
                       "\t'keep' @raw\n\n"
                       "root {\n"
                       "\t['zeta', 'alpha', 'keep']\n"
                       "\t['alpha']\n"
                       "\t// ['ignored']\n"
                       "}\n")
                      (alan-grammar-update-keyword)
                      (buffer-string))"##,
        expect![[
            r#"OK "keywords\n\11'alpha'\n\11'keep' @raw\n\11'zeta'\n\n\nroot {\n\11['zeta', 'alpha', 'keep']\n\11['alpha']\n\11// ['ignored']\n}\n""#
        ]],
    )
}

fn alan_template_yank_quotes_multiline_real_text_and_escapes_embedded_quotes() -> ParityBatchCase {
    ParityBatchCase::value(
        "alan_template_yank_quotes_multiline_real_text_and_escapes_embedded_quotes",
        r##"(with-temp-buffer
                      (kill-new "Hello \"Alan\"\nsecond line\n")
                      (alan-template-yank)
                      (list (buffer-string)
                            (point)
                            (current-kill 0 t)))"##,
        expect![[
            r#"OK ("\"Hello \\\"Alan\\\"\" ;\n\"second line\" ;\n\"\" ;" 40 "Hello \"Alan\"\nsecond line\n")"#
        ]],
    )
}

fn alan_application_numerical_types_are_collected_from_the_real_section_backwards()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_application_numerical_types_are_collected_from_the_real_section_backwards",
        r##"(with-temp-buffer
                      (insert
                       "'outside'\n"
                       "numerical-types\n"
                       "\t'integer'\n"
                       "\t'decimal'\n"
                       "\t'currency'\n")
                      (goto-char (point-min))
                      (list
                       (alan-list-nummerical-types)
                       (point)
                       (buffer-string)))"##,
        expect![[
            r#"OK (("currency" "decimal" "integer") 1 "'outside'\nnumerical-types\n\11'integer'\n\11'decimal'\n\11'currency'\n")"#
        ]],
    )
}

fn alan_documentation_marks_and_synchronizes_a_real_multiline_block() -> ParityBatchCase {
    ParityBatchCase::value(
        "alan_documentation_marks_and_synchronizes_a_real_multiline_block",
        r##"(progn
                      (defvar alan-parity-synced)
                      (let (alan-parity-synced)
                        (with-temp-buffer
                          (insert
                           "'before': text\n"
                           "  /// First line\n"
                           "  /// Second line\n"
                           "  /// Third line\n"
                           "'after': text\n")
                          (goto-char (point-min))
                          (search-forward "Second")
                          (let ((on-doc (alan--documentation-p)))
                            (alan-mark-documentation)
                            (let ((marked
                                   (buffer-substring-no-properties
                                    (region-beginning) (region-end))))
                              (deactivate-mark)
                              (setq-local
                               alan-documentation-update
                               (lambda (text)
                                 (setq alan-parity-synced text)))
                              (alan-documentation-sync-buffer)
                              (list
                               on-doc
                               marked
                               (point)
                               (line-number-at-pos)
                               alan-parity-synced))))))"##,
        expect![[
            r#"OK (t "  /// First line\n  /// Second line\n  /// Third line" 16 2 "'before': text\n  /// First line\n  /// Second line\n  /// Third line\n'after': text\n")"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alan_mode_activation_installs_real_syntax_comments_indentation_and_xref_locally(),
        alan_mode_font_lock_distinguishes_identifiers_docs_comments_strings_and_types(),
        alan_mode_indents_nested_and_single_line_blocks_as_a_user_edits_them(),
        alan_grammar_update_rebuilds_sorted_unique_keywords_and_preserves_annotations(),
        alan_template_yank_quotes_multiline_real_text_and_escapes_embedded_quotes(),
        alan_application_numerical_types_are_collected_from_the_real_section_backwards(),
        alan_documentation_marks_and_synchronizes_a_real_multiline_block(),
    ]
}
