use expect_test::expect;

use super::ParityBatchCase;

fn parser_builds_a_complete_realistic_mixed_language_astro_tree() -> ParityBatchCase {
    ParityBatchCase::value(
        "parser_builds_a_complete_realistic_mixed_language_astro_tree",
        r##"(with-temp-buffer
          (insert "---\n")
          (insert "import Card from './Card.astro';\n")
          (insert "const title = 'Parity';\n")
          (insert "---\n")
          (insert "<main class=\"page\">\n")
          (insert "  <Card title={title} />\n")
          (insert "  <script>console.log(title);</script>\n")
          (insert "  <style>.page { display: grid; }</style>\n")
          (insert "</main>\n")
          (astro-ts-mode)
          (font-lock-ensure)
          (mapcar
           (lambda (parser)
             (list
              (treesit-parser-language parser)
              (treesit-parser-tag parser)
              (treesit-parser-embed-level parser)
              (treesit-parser-included-ranges parser)
              (treesit-node-string
               (treesit-parser-root-node parser))))
           (sort
            (copy-sequence (treesit-parser-list nil nil t))
            (lambda (left right)
              (string<
               (format "%S:%S"
                       (treesit-parser-language left)
                       (treesit-parser-included-ranges left))
               (format "%S:%S"
                       (treesit-parser-language right)
                       (treesit-parser-included-ranges
                        right)))))))"##,
        expect![[
            r#"OK ((astro nil nil nil "(document (frontmatter (frontmatter_js_block)) (element (start_tag (tag_name) (attribute (attribute_name) (quoted_attribute_value (attribute_value)))) (element (self_closing_tag (tag_name) (attribute (attribute_name) (attribute_interpolation (attribute_js_expr))))) (script_element (start_tag (tag_name)) (raw_text) (end_tag (tag_name))) (style_element (start_tag (tag_name)) (raw_text) (end_tag (tag_name))) (end_tag (tag_name))))") (css embedded 1 ((159 . 183)) "(stylesheet (rule_set (selectors (class_selector (class_name (identifier)))) (block (declaration (property_name) (plain_value)))))") (tsx embedded 1 ((101 . 106)) "(program (expression_statement (identifier)))") (tsx embedded 1 ((121 . 140)) "(program (expression_statement (call_expression function: (member_expression object: (identifier) property: (property_identifier)) arguments: (arguments (identifier)))))") (tsx embedded 1 ((4 . 61)) "(program (import_statement (import_clause (identifier)) source: (string (string_fragment))) (lexical_declaration (variable_declarator name: (identifier) value: (string (string_fragment)))))"))"#
        ]],
    )
}

fn font_lock_marks_astro_tags_attributes_brackets_tsx_and_css_practically() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_marks_astro_tags_attributes_brackets_tsx_and_css_practically",
        r##"(with-temp-buffer
          (insert "---\nconst count = 3;\n---\n")
          (insert "<button class=\"primary\" disabled={count > 0}>")
          (insert "{count}</button>\n")
          (insert "<style>.primary { color: red; }</style>\n")
          (astro-ts-mode)
          (font-lock-ensure)
          (let ((position (point-min))
                runs)
            (while (< position (point-max))
              (let* ((face (get-text-property position 'face))
                     (next (next-single-property-change
                            position 'face nil (point-max))))
                (when face
                  (push
                   (list
                    (buffer-substring-no-properties position next)
                    face)
                   runs))
                (setq position next)))
            (nreverse runs)))"##,
        expect![[
            r#"OK (("---" font-lock-comment-face) ("---" font-lock-comment-face) ("button" font-lock-function-name-face) ("class" font-lock-constant-face) ("\"primary\"" font-lock-string-face) ("disabled" font-lock-constant-face) ("button" font-lock-function-name-face) ("style" font-lock-function-name-face) ("style" font-lock-function-name-face))"#
        ]],
    )
}

fn indentation_formats_nested_elements_attributes_and_interpolations() -> ParityBatchCase {
    ParityBatchCase::value(
        "indentation_formats_nested_elements_attributes_and_interpolations",
        r##"(with-temp-buffer
          (insert "<main>\n")
          (insert "<section class=\"hero\">\n")
          (insert "<h1>{title}</h1>\n")
          (insert "<Card\n")
          (insert "title={title}\n")
          (insert "featured={true}\n")
          (insert "/>\n")
          (insert "</section>\n")
          (insert "</main>\n")
          (astro-ts-mode)
          (indent-region (point-min) (point-max))
          (buffer-string))"##,
        expect![[
            r#"OK "<main>\n  <section class=\"hero\">\n    <h1>{title}</h1>\n    <Card\n      title={title}\n      featured={true}\n    />\n  </section>\n</main>\n""#
        ]],
    )
}

fn indentation_routes_frontmatter_script_and_style_blocks_to_embedded_languages() -> ParityBatchCase
{
    ParityBatchCase::value(
        "indentation_routes_frontmatter_script_and_style_blocks_to_embedded_languages",
        r##"(with-temp-buffer
          (insert "---\n")
          (insert "const items = [1, 2, 3].map((value) => ({\n")
          (insert "value,\n")
          (insert "label: `Item ${value}`,\n")
          (insert "}));\n")
          (insert "---\n")
          (insert "<script>\n")
          (insert "function announce(message) {\n")
          (insert "console.log(message);\n")
          (insert "}\n")
          (insert "</script>\n")
          (insert "<style>\n")
          (insert ".card {\n")
          (insert "display: grid;\n")
          (insert "color: red;\n")
          (insert "}\n")
          (insert "</style>\n")
          (astro-ts-mode)
          (indent-region (point-min) (point-max))
          (buffer-string))"##,
        expect![[
            r#"OK #("---\nconst items = [1, 2, 3].map((value) => ({\n  value,\n  label: `Item ${value}`,\n}));\n---\n<script>\nfunction announce(message) {\n  console.log(message);\n}\n</script>\n<style>\n.card {\n  display: grid;\n  color: red;\n}\n</style>\n" 41 42 (syntax-table (1)))"#
        ]],
    )
}

fn custom_indent_offset_changes_real_nested_markup_and_css_indentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_indent_offset_changes_real_nested_markup_and_css_indentation",
        r##"(let ((astro-ts-mode-indent-offset 4))
          (with-temp-buffer
            (insert "<article>\n")
            (insert "<div>\n")
            (insert "<span>{label}</span>\n")
            (insert "</div>\n")
            (insert "<style>\n")
            (insert ".item {\n")
            (insert "color: blue;\n")
            (insert "}\n")
            (insert "</style>\n")
            (insert "</article>\n")
            (astro-ts-mode)
            (indent-region (point-min) (point-max))
            (list css-indent-offset (buffer-string))))"##,
        expect![[
            r#"OK (4 "<article>\n    <div>\n\11<span>{label}</span>\n    </div>\n    <style>\n.item {\n    color: blue;\n}\n    </style>\n</article>\n")"#
        ]],
    )
}

fn incremental_edit_reparses_tag_name_attributes_and_interpolation_expression() -> ParityBatchCase {
    ParityBatchCase::value(
        "incremental_edit_reparses_tag_name_attributes_and_interpolation_expression",
        r##"(with-temp-buffer
          (insert "<Card title={oldTitle}>{oldTitle}</Card>")
          (astro-ts-mode)
          (font-lock-ensure)
          (let ((before
                 (treesit-node-string
                  (treesit-buffer-root-node 'astro))))
            (goto-char (point-min))
            (while (search-forward "oldTitle" nil t)
              (replace-match "newTitle" t t))
            (goto-char (point-min))
            (search-forward "Card")
            (replace-match "Panel" t t)
            (goto-char (point-max))
            (search-backward "Card")
            (replace-match "Panel" t t)
            (font-lock-flush)
            (font-lock-ensure)
            (list
             before
             (buffer-string)
             (treesit-node-string
              (treesit-buffer-root-node 'astro))
             (mapcar
              (lambda (needle)
                (goto-char (point-min))
                (search-forward needle)
                (get-text-property
                 (- (point) (length needle)) 'face))
              '("Panel" "title" "newTitle")))))"##,
        expect![[
            r#"OK ("(document (element (start_tag (tag_name) (attribute (attribute_name) (attribute_interpolation (attribute_js_expr)))) (html_interpolation (permissible_text)) (end_tag (tag_name))))" #("<Panel title={newTitle}>{newTitle}</Panel>" 1 6 (face font-lock-function-name-face) 7 12 (face font-lock-constant-face) 36 41 (face font-lock-function-name-face)) "(document (element (start_tag (tag_name) (attribute (attribute_name) (attribute_interpolation (attribute_js_expr)))) (html_interpolation (permissible_text)) (end_tag (tag_name))))" (font-lock-function-name-face font-lock-constant-face nil))"#
        ]],
    )
}

fn adding_and_removing_embedded_blocks_updates_parser_ranges_without_stale_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "adding_and_removing_embedded_blocks_updates_parser_ranges_without_stale_state",
        r##"(cl-labels
          ((snapshot
            ()
            (mapcar
             (lambda (parser)
               (list
                (treesit-parser-language parser)
                (treesit-parser-tag parser)
                (treesit-parser-embed-level parser)
                (treesit-parser-included-ranges parser)
                (treesit-node-string
                 (treesit-parser-root-node parser))))
             (sort
              (copy-sequence
               (treesit-parser-list nil nil t))
              (lambda (left right)
                (string<
                 (format "%S:%S"
                         (treesit-parser-language left)
                         (treesit-parser-included-ranges left))
                 (format "%S:%S"
                         (treesit-parser-language right)
                         (treesit-parser-included-ranges
                          right))))))))
          (with-temp-buffer
            (insert "<div>{value}</div>\n")
            (astro-ts-mode)
            (font-lock-ensure)
            (let ((initial (snapshot)))
              (goto-char (point-max))
              (insert "<script>const value = 1;</script>\n")
              (insert "<style>div { color: red; }</style>\n")
              (font-lock-flush)
              (font-lock-ensure)
              (let ((added (snapshot)))
                (goto-char (point-min))
                (forward-line 1)
                (delete-region (point) (point-max))
                (font-lock-flush)
                (font-lock-ensure)
                (list initial added (snapshot))))))"##,
        expect![[
            r#"OK (((astro nil nil nil "(document (element (start_tag (tag_name)) (html_interpolation (permissible_text)) (end_tag (tag_name))))") (tsx embedded 1 #1=((7 . 12)) "(program (expression_statement (identifier)))")) ((astro nil nil nil "(document (element (start_tag (tag_name)) (html_interpolation (permissible_text)) (end_tag (tag_name))) (script_element (start_tag (tag_name)) (raw_text) (end_tag (tag_name))) (style_element (start_tag (tag_name)) (raw_text) (end_tag (tag_name))))") (css embedded 1 ((61 . 80)) "(stylesheet (rule_set (selectors (tag_name)) (block (declaration (property_name) (plain_value)))))") (tsx embedded 1 ((28 . 44)) "(program (lexical_declaration (variable_declarator name: (identifier) value: (number))))") (tsx embedded 1 #1# "(program (expression_statement (identifier)))")) ((astro nil nil nil "(document (element (start_tag (tag_name)) (html_interpolation (permissible_text)) (end_tag (tag_name))))") (tsx embedded 1 #1# "(program (expression_statement (identifier)))")))"#
        ]],
    )
}

fn malformed_template_keeps_error_nodes_and_recovers_after_closing_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "malformed_template_keeps_error_nodes_and_recovers_after_closing_edit",
        r##"(with-temp-buffer
          (insert "<main><section>{value</main>")
          (astro-ts-mode)
          (font-lock-ensure)
          (let* ((root (treesit-buffer-root-node 'astro))
                 (before
                  (list
                   (treesit-node-check root 'has-error)
                   (treesit-node-string root))))
            (goto-char (point-min))
            (search-forward "{value")
            (insert "}")
            (search-forward "</main>")
            (goto-char (match-beginning 0))
            (insert "</section>")
            (font-lock-flush)
            (font-lock-ensure)
            (setq root (treesit-buffer-root-node 'astro))
            (list
             before
             (buffer-string)
             (treesit-node-check root 'has-error)
             (treesit-node-string root))))"##,
        expect![[
            r#"OK ((t "(document (ERROR (start_tag (tag_name)) (start_tag (tag_name)) (permissible_text) (attribute_js_expr)))") #("<main><section>{value}</section></main>" 1 5 (face font-lock-function-name-face) 7 14 (face font-lock-function-name-face) 24 31 (face font-lock-function-name-face) 34 38 (face font-lock-function-name-face)) nil "(document (element (start_tag (tag_name)) (element (start_tag (tag_name)) (html_interpolation (permissible_text)) (end_tag (tag_name))) (end_tag (tag_name))))")"#
        ]],
    )
}

fn unicode_text_attributes_and_expressions_keep_character_positions_and_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "unicode_text_attributes_and_expressions_keep_character_positions_and_faces",
        r##"(with-temp-buffer
          (insert "---\nconst greeting = \"λ café 東京\";\n---\n")
          (insert "<p title=\"naïve\">{greeting} — 😀</p>\n")
          (astro-ts-mode)
          (font-lock-ensure)
          (mapcar
           (lambda (needle)
             (goto-char (point-min))
             (search-forward needle)
             (let ((start (- (point) (length needle))))
               (list needle start (point)
                     (treesit-node-type
                      (treesit-node-at start 'astro))
                     (get-text-property start 'face))))
           '("λ" "café" "東京" "title" "naïve"
             "greeting" "😀")))"##,
        expect![[
            r#"OK (("λ" 23 24 "frontmatter_js_block" nil) ("café" 25 29 "frontmatter_js_block" nil) ("東京" 30 32 "frontmatter_js_block" nil) ("title" 42 47 "attribute_name" font-lock-constant-face) ("naïve" 49 54 "attribute_value" font-lock-string-face) ("greeting" 11 19 "frontmatter_js_block" nil) ("😀" 69 70 "text" nil))"#
        ]],
    )
}

fn empty_and_comment_only_buffers_have_stable_roots_and_fontification() -> ParityBatchCase {
    ParityBatchCase::value(
        "empty_and_comment_only_buffers_have_stable_roots_and_fontification",
        r##"(mapcar
          (lambda (contents)
            (with-temp-buffer
              (insert contents)
              (astro-ts-mode)
              (font-lock-ensure)
              (let ((root
                     (treesit-buffer-root-node 'astro)))
                (list
                 contents
                 (treesit-node-type root)
                 (treesit-node-check root 'has-error)
                 (treesit-node-string root)
                 (buffer-string)
                 (get-text-property (point-min) 'face)))))
          '("" "<!-- package parity -->" "---\n// note\n---\n"))"##,
        expect![[
            r#"OK (("" "document" nil "(document)" "" nil) ("<!-- package parity -->" "document" nil "(document (comment))" #("<!-- package parity -->" 0 23 (face font-lock-comment-face)) font-lock-comment-face) ("---\n// note\n---\n" "document" nil "(document (frontmatter (frontmatter_js_block)))" #("---\n// note\n---\n" 0 3 (face font-lock-comment-face) 12 15 (face font-lock-comment-face)) font-lock-comment-face))"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        parser_builds_a_complete_realistic_mixed_language_astro_tree(),
        font_lock_marks_astro_tags_attributes_brackets_tsx_and_css_practically(),
        indentation_formats_nested_elements_attributes_and_interpolations(),
        indentation_routes_frontmatter_script_and_style_blocks_to_embedded_languages(),
        custom_indent_offset_changes_real_nested_markup_and_css_indentation(),
        incremental_edit_reparses_tag_name_attributes_and_interpolation_expression(),
        adding_and_removing_embedded_blocks_updates_parser_ranges_without_stale_state(),
        malformed_template_keeps_error_nodes_and_recovers_after_closing_edit(),
        unicode_text_attributes_and_expressions_keep_character_positions_and_faces(),
        empty_and_comment_only_buffers_have_stable_roots_and_fontification(),
    ]
}
