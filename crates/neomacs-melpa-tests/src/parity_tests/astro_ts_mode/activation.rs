use expect_test::expect;

use super::ParityBatchCase;

fn mode_activation_builds_mixed_language_parsers_and_exact_local_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_activation_builds_mixed_language_parsers_and_exact_local_contract",
        r##"(with-temp-buffer
          (insert "---\nconst title = \"Hello\";\n---\n")
          (insert "<div class={title}>{title}</div>\n")
          (insert "<script>const count = 1;</script>\n")
          (insert "<style>.card { color: red; }</style>\n")
          (astro-ts-mode)
          (font-lock-ensure)
          (list
           major-mode mode-name
           (derived-mode-p 'html-mode)
           (mapcar #'treesit-parser-language
                   (treesit-parser-list))
           (treesit-parser-language treesit-primary-parser)
           treesit-text-type-regexp
           (mapcar #'car treesit-simple-indent-rules)
           css-indent-offset
           treesit-font-lock-feature-list
           (mapcar
            (lambda (setting)
              (list
               (type-of (nth 0 setting))
               (nth 1 setting)
               (nth 2 setting)
               (nth 3 setting)
               (nth 4 setting)))
            treesit-range-settings)
           treesit-language-at-point-function
           (local-variable-p 'treesit-simple-indent-rules)
           (local-variable-p 'treesit-font-lock-settings)
           (local-variable-p 'treesit-range-settings)))"##,
        expect![[
            r#"OK (astro-ts-mode "Astro" html-mode (astro) astro "\\(?:\\(?:commen\\|tex\\)t\\)" (astro css tsx) 2 ((astro-comment astro-keyword astro-definition css-selector css-comment css-query css-keyword tsx-comment tsx-declaration tsx-jsx) (astro-string css-property css-constant css-string tsx-keyword tsx-string tsx-escape-sequence) (css-error css-variable css-function css-operator tsx-constant tsx-expression tsx-identifier tsx-number tsx-pattern tsx-property) (astro-bracket css-bracket tsx-function tsx-bracket tsx-delimiter)) ((treesit-compiled-query tsx t nil nil) (treesit-compiled-query css t nil nil)) astro-ts-mode--treesit-language-at-point t t t)"#
        ]],
    )
}

fn missing_grammar_errors_are_checked_in_astro_css_then_tsx_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_grammar_errors_are_checked_in_astro_css_then_tsx_order",
        r##"(mapcar
          (lambda (unavailable)
            (cl-letf
                (((symbol-function 'treesit-ready-p)
                  (lambda (language &rest _)
                    (not (eq language unavailable)))))
              (with-temp-buffer
                (condition-case error-data
                    (progn (astro-ts-mode) :unexpected-success)
                  (error
                   (list
                    unavailable
                    (car error-data)
                    (cdr error-data)
                    major-mode))))))
          '(astro css tsx))"##,
        expect![[
            r#"OK ((astro error ("Tree-sitter grammar for Astro isn’t available") astro-ts-mode) (css error ("Tree-sitter grammar for CSS isn’t available") astro-ts-mode) (tsx error ("Tree-sitter grammar for Typescript/TSX isn’t available") astro-ts-mode))"#
        ]],
    )
}

fn real_astro_file_name_selects_mode_only_when_grammar_is_available() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_astro_file_name_selects_mode_only_when_grammar_is_available",
        r##"(mapcar
          (lambda (name)
            (with-temp-buffer
              (setq buffer-file-name
                    (expand-file-name name default-directory))
              (insert "<h1>Hello</h1>")
              (normal-mode)
              (list name major-mode mode-name
                    (mapcar #'treesit-parser-language
                            (treesit-parser-list)))))
          '("component.astro" "component.html" "component.txt"))"##,
        expect![[
            r#"OK (("component.astro" astro-ts-mode "Astro" (astro)) ("component.html" mhtml-mode ((sgml-xml-mode "XHTML+" "HTML+") (:eval (mhtml--submode-lighter))) nil) ("component.txt" text-mode "Text" nil))"#
        ]],
    )
}

fn mode_hook_observes_fully_initialized_parser_and_mixed_language_settings() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_hook_observes_fully_initialized_parser_and_mixed_language_settings",
        r##"(let (observations)
          (let ((astro-ts-mode-hook
                 (list
                  (lambda ()
                    (push
                     (list
                      major-mode
                      (mapcar #'treesit-parser-language
                              (treesit-parser-list))
                      treesit-language-at-point-function
                      css-indent-offset
                      (length treesit-font-lock-settings))
                     observations)))))
            (with-temp-buffer
              (insert "<div>Hello</div>")
              (astro-ts-mode)))
          observations)"##,
        expect!["OK ((astro-ts-mode (astro) astro-ts-mode--treesit-language-at-point 2 33))"],
    )
}

fn language_at_point_routes_real_frontmatter_attributes_html_script_and_style() -> ParityBatchCase {
    ParityBatchCase::value(
        "language_at_point_routes_real_frontmatter_attributes_html_script_and_style",
        r##"(with-temp-buffer
          (insert "---\nconst title = \"Hello\";\n---\n")
          (insert "<article class={title}>{title}</article>\n")
          (insert "<script>const count = title.length;</script>\n")
          (insert "<style>.card { color: red; }</style>\n")
          (astro-ts-mode)
          (font-lock-ensure)
          (mapcar
           (lambda (needle)
             (goto-char (point-min))
             (search-forward needle)
             (let ((point (- (point) (/ (length needle) 2))))
               (list needle
                     (treesit-node-type
                      (treesit-node-at point 'astro))
                     (astro-ts-mode--treesit-language-at-point
                      point)
                     (funcall treesit-language-at-point-function
                              point))))
           '("const title" "class" "{title}" "article"
             "const count" ".card" "color")))"##,
        expect![[
            r#"OK (("const title" "frontmatter_js_block" tsx tsx) ("class" "attribute_name" astro astro) ("{title}" "attribute_js_expr" tsx tsx) ("article" "tag_name" astro astro) ("const count" "raw_text" tsx tsx) (".card" "raw_text" css css) ("color" "raw_text" css css))"#
        ]],
    )
}

fn parser_ranges_materialize_tsx_and_css_regions_with_exact_source_slices() -> ParityBatchCase {
    ParityBatchCase::value(
        "parser_ranges_materialize_tsx_and_css_regions_with_exact_source_slices",
        r##"(with-temp-buffer
          (insert "---\nconst front = 1;\n---\n")
          (insert "<div data-value={front}>{front + 1}</div>\n")
          (insert "<script>const embedded = front;</script>\n")
          (insert "<style>.box { display: grid; }</style>\n")
          (astro-ts-mode)
          (font-lock-ensure)
          (mapcar
           (lambda (parser)
             (list
              (treesit-parser-language parser)
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
            r#"OK ((astro nil "(document (frontmatter (frontmatter_js_block)) (element (start_tag (tag_name) (attribute (attribute_name) (attribute_interpolation (attribute_js_expr)))) (html_interpolation (permissible_text)) (end_tag (tag_name))) (script_element (start_tag (tag_name)) (raw_text) (end_tag (tag_name))) (style_element (start_tag (tag_name)) (raw_text) (end_tag (tag_name))))") (css ((116 . 139)) "(stylesheet (rule_set (selectors (class_selector (class_name (identifier)))) (block (declaration (property_name) (plain_value)))))") (tsx ((4 . 21)) "(program (lexical_declaration (variable_declarator name: (identifier) value: (number))))") (tsx ((43 . 48)) "(program (expression_statement (identifier)))") (tsx ((51 . 60)) "(program (expression_statement (binary_expression left: (identifier) right: (number))))") (tsx ((76 . 99)) "(program (lexical_declaration (variable_declarator name: (identifier) value: (identifier))))"))"#
        ]],
    )
}

fn custom_indent_offset_is_copied_into_both_astro_and_css_local_settings() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_indent_offset_is_copied_into_both_astro_and_css_local_settings",
        r##"(let ((astro-ts-mode-indent-offset 5))
          (with-temp-buffer
            (insert "<div><span>Hello</span></div>")
            (astro-ts-mode)
            (list
             astro-ts-mode-indent-offset
             css-indent-offset
             (seq-filter
              (lambda (rule)
                (equal (car-safe (car-safe rule))
                       'parent-is))
              (alist-get
               'astro treesit-simple-indent-rules)))))"##,
        expect![[
            r#"OK (5 5 (((parent-is "document") column-0 0) ((parent-is "comment") prev-adaptive-prefix 0) ((parent-is "element") parent-bol astro-ts-mode-indent-offset) ((parent-is "script_element") parent-bol astro-ts-mode-indent-offset) ((parent-is "style_element") parent-bol astro-ts-mode-indent-offset) ((parent-is "start_tag") parent-bol astro-ts-mode-indent-offset) ((parent-is "self_closing_tag") parent-bol astro-ts-mode-indent-offset)))"#
        ]],
    )
}

fn repeated_mode_activation_replaces_parsers_without_accumulating_duplicates() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeated_mode_activation_replaces_parsers_without_accumulating_duplicates",
        r##"(with-temp-buffer
          (insert "<div>{value}</div>")
          (let (snapshots)
            (dotimes (_ 3)
              (astro-ts-mode)
              (font-lock-ensure)
              (push
               (list
                major-mode
                (sort
                 (mapcar #'treesit-parser-language
                         (treesit-parser-list))
                 (lambda (left right)
                   (string< (symbol-name left)
                            (symbol-name right))))
                (length (treesit-parser-list)))
               snapshots))
            (list
             snapshots
             (and (equal (nth 0 snapshots) (nth 1 snapshots))
                  (equal (nth 1 snapshots)
                         (nth 2 snapshots))))))"##,
        expect![
            "OK (((astro-ts-mode (astro) 1) (astro-ts-mode (astro) 1) (astro-ts-mode (astro) 1)) t)"
        ],
    )
}

fn inherited_html_comment_and_syntax_behavior_remains_practical() -> ParityBatchCase {
    ParityBatchCase::value(
        "inherited_html_comment_and_syntax_behavior_remains_practical",
        r##"(with-temp-buffer
          (insert "<section>\nHello\n</section>")
          (astro-ts-mode)
          (goto-char (point-min))
          (search-forward "Hello")
          (beginning-of-line)
          (comment-line 1)
          (list
           comment-start comment-end
           (buffer-string)
           (nth 4 (syntax-ppss
                   (+ (line-beginning-position) 5)))
           (char-syntax ?<)
           (char-syntax ?>)))"##,
        expect![[
            r#"OK ("<!-- " " -->" #("<section>\n<!-- Hello -->\n</section>" 0 10 (fontified nil) 10 11 (syntax-table (2097163) fontified nil) 11 15 (fontified nil) 15 20 (fontified nil) 20 23 (fontified nil) 23 24 (syntax-table (2097164) fontified nil) 24 35 (fontified nil)) nil 40 41)"#
        ]],
    )
}

pub(super) fn activation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_activation_builds_mixed_language_parsers_and_exact_local_contract(),
        missing_grammar_errors_are_checked_in_astro_css_then_tsx_order(),
        real_astro_file_name_selects_mode_only_when_grammar_is_available(),
        mode_hook_observes_fully_initialized_parser_and_mixed_language_settings(),
        language_at_point_routes_real_frontmatter_attributes_html_script_and_style(),
        parser_ranges_materialize_tsx_and_css_regions_with_exact_source_slices(),
        custom_indent_offset_is_copied_into_both_astro_and_css_local_settings(),
        repeated_mode_activation_replaces_parsers_without_accumulating_duplicates(),
        inherited_html_comment_and_syntax_behavior_remains_practical(),
    ]
}
