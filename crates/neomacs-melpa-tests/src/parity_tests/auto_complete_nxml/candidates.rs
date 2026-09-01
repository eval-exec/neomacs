use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_nxml_project_ident_prefers_explicit_then_project_then_default() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_nxml_project_ident_prefers_explicit_then_project_then_default",
        r##"(let ((baseline
                                (auto-complete-nxml-get-project-ident)))
         (provide 'anything-project)
         (cl-letf (((symbol-function 'ap:get-root-directory)
                    (lambda () "/project/root/")))
           (list
            baseline
            (auto-complete-nxml-get-project-ident "explicit")
            (auto-complete-nxml-get-project-ident)
            (cl-letf (((symbol-function 'ap:get-root-directory)
                       (lambda () nil)))
              (auto-complete-nxml-get-project-ident)))))"##,
        expect![[r#"OK ("default" "explicit" "/project/root/" "default")"#]],
    )
}

fn auto_complete_nxml_project_word_stores_are_isolated_by_project_and_kind() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_project_word_stores_are_isolated_by_project_and_kind",
        r##"(let ((auto-complete-nxml-tag-value-words-hash
                                (make-hash-table :test 'equal))
             (auto-complete-nxml-attr-words-hash-hash
              (make-hash-table :test 'equal))
             (alpha-attrs (make-hash-table :test 'equal))
             (beta-attrs (make-hash-table :test 'equal)))
         (puthash "class" '("primary" "wide") alpha-attrs)
         (puthash "lang" '("en" "fr") beta-attrs)
         (auto-complete-nxml-put-project-tag-value-words
          '("alpha" "shared") "project-a")
         (auto-complete-nxml-put-project-tag-value-words
          '("beta" "shared") "project-b")
         (auto-complete-nxml-put-project-attr-words-hash
          alpha-attrs "project-a")
         (auto-complete-nxml-put-project-attr-words-hash
          beta-attrs "project-b")
         (list
          (auto-complete-nxml-get-project-tag-value-words "project-a")
          (auto-complete-nxml-get-project-tag-value-words "project-b")
          (acnxml-test-hash-alist
           (auto-complete-nxml-get-project-attr-words-hash "project-a"))
          (acnxml-test-hash-alist
           (auto-complete-nxml-get-project-attr-words-hash "project-b"))
          (auto-complete-nxml-get-project-tag-value-words "missing")))"##,
        expect![[
            r#"OK (("alpha" "shared") ("beta" "shared") (("class" "primary" "wide")) (("lang" "en" "fr")) nil)"#
        ]],
    )
}

fn auto_complete_nxml_tag_value_scan_collects_words_across_real_xml_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_tag_value_scan_collects_words_across_real_xml_content",
        r##"(let ((auto-complete-nxml-tag-value-words-hash
                                (make-hash-table :test 'equal))
             (ac-prefix "skip"))
         (with-temp-buffer
           (insert
            "<root>"
            "<title>alpha beta</title>"
            "<p>beta, gamma!</p>"
            "<code>skip delta-2</code>"
            "<empty></empty>"
            "</root>")
           (auto-complete-nxml-update-tag-value-words "fixture")
           (list
            (auto-complete-nxml-get-project-tag-value-words "fixture")
            (buffer-string)
            (point))))"##,
        expect![[
            r#"OK (("delta-2" "gamma" "beta" "alpha" "") "<root><title>alpha beta</title><p>beta, gamma!</p><code>skip delta-2</code><empty></empty></root>" 98)"#
        ]],
    )
}

fn auto_complete_nxml_tag_value_scan_merges_with_existing_project_words() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_tag_value_scan_merges_with_existing_project_words",
        r##"(let ((auto-complete-nxml-tag-value-words-hash
                                (make-hash-table :test 'equal))
             (ac-prefix "none"))
         (auto-complete-nxml-put-project-tag-value-words
          '("existing" "alpha") "fixture")
         (with-temp-buffer
           (insert "<a>alpha beta</a><b>beta gamma</b>")
           (auto-complete-nxml-update-tag-value-words "fixture")
           (let ((once
                  (copy-sequence
                   (auto-complete-nxml-get-project-tag-value-words "fixture"))))
             (auto-complete-nxml-update-tag-value-words "fixture")
             (list
              once
              (auto-complete-nxml-get-project-tag-value-words "fixture")))))"##,
        expect![[
            r#"OK (("gamma" "beta" "" "existing" "alpha") ("gamma" "beta" "" "existing" "alpha"))"#
        ]],
    )
}

fn auto_complete_nxml_attribute_scan_groups_values_and_excludes_style_id_and_prefix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_attribute_scan_groups_values_and_excludes_style_id_and_prefix",
        r##"(let ((auto-complete-nxml-attr-words-hash-hash
                                (make-hash-table :test 'equal))
             (ac-prefix "skip"))
         (with-temp-buffer
           (insert
            "<a class=\"primary wide\" role=\"button skip\" "
            "style=\"color: red\" id=\"main\"/>"
            "<b class='wide compact' role='link'/>")
           (goto-char (point-max))
           (auto-complete-nxml-update-attr-words "fixture")
           (acnxml-test-hash-alist
            (auto-complete-nxml-get-project-attr-words-hash "fixture"))))"##,
        expect![[r#"OK (("class" "compact" "wide" "primary") ("role" "link" "button"))"#]],
    )
}

fn auto_complete_nxml_myself_candidates_require_content_context_and_automatic_start()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_myself_candidates_require_content_context_and_automatic_start",
        r##"(let ((auto-complete-nxml-tag-value-words-hash
                                (make-hash-table :test 'equal))
             (ac-prefix "")
             (this-command 'self-insert-command))
         (mapcar
          (lambda (case)
            (with-temp-buffer
              (insert (car case))
              (goto-char (point-max))
              (let ((auto-complete-nxml-automatic-p (cdr case)))
                (list case
                      (auto-complete-nxml-get-tag-value-candidates-by-myself)))))
          '(("<root><a>alpha beta</a><b>ga" . t)
            ("<root attr=\"alpha" . t)
            ("<root><a>alpha</a><b>ga" . nil))))"##,
        expect![[
            r#"OK ((("<root><a>alpha beta</a><b>ga" . t) ("beta" "alpha")) (("<root attr=\"alpha" . t) nil) (("<root><a>alpha</a><b>ga") nil))"#
        ]],
    )
}

fn auto_complete_nxml_css_candidates_switch_between_properties_and_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_css_candidates_switch_between_properties_and_values",
        r##"(let ((auto-complete-nxml-automatic-p t)
             (this-command 'self-insert-command)
             (ac-css-property-alist
              '(("color" . colors)
                ("font-size" . sizes)
                ("display" . displays))))
         (cl-letf (((symbol-function 'ac-css-property-candidates)
                    (lambda () '("red" "green" "blue"))))
           (mapcar
            (lambda (text)
              (with-temp-buffer
                (insert text)
                (goto-char (point-max))
                (list text
                      (auto-complete-nxml-get-css-candidates))))
            '("<p style=\"fo"
              "<p style=\"color: re"
              "<p class=\"fo"
              "outside"))))"##,
        expect![[
            r#"OK (("<p style=\"fo" ("color" "font-size" "display")) ("<p style=\"color: re" ("red" "green" "blue")) ("<p class=\"fo" nil) ("outside" nil))"#
        ]],
    )
}

fn auto_complete_nxml_rng_candidates_support_function_and_alist_tables_and_dedupe()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_rng_candidates_support_function_and_alist_tables_and_dedupe",
        r##"(let ((auto-complete-nxml-automatic-p t)
             (this-command 'self-insert-command))
         (cl-letf (((symbol-function 'rng-complete)
                    (lambda ()
                      (rng-complete-before-point
                       (point-min)
                       (lambda (_input _predicate _all)
                         '("alpha" "beta" "alpha"))
                       "function")
                      (rng-complete-before-point
                       (point-min)
                       '(("beta" . 2) ("gamma" . 3) ("beta" . 4))
                       "alist"))))
           (with-temp-buffer
             (insert "a")
             (auto-complete-nxml-get-candidates))))"##,
        expect![[r#"OK ("beta" "gamma")"#]],
    )
}

fn auto_complete_nxml_nxml_value_candidates_forward_schema_match_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_nxml_value_candidates_forward_schema_match_values",
        r##"(let ((auto-complete-nxml-automatic-p t)
             (this-command 'self-insert-command)
             calls)
         (cl-letf (((symbol-function 'rng-set-state-after)
                    (lambda () (push 'set-state calls)))
                   ((symbol-function 'rng-match-possible-value-strings)
                    (lambda ()
                      (push 'possible-values calls)
                      '("draft" "final" "archived"))))
           (with-temp-buffer
             (insert "<status>dra")
             (goto-char (point-max))
             (list
              (auto-complete-nxml-get-tag-value-candidates-by-nxml)
              (nreverse calls)))))"##,
        expect![[r#"OK (("draft" "final" "archived") (set-state possible-values))"#]],
    )
}

fn auto_complete_nxml_attribute_value_candidates_fall_back_to_buffer_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_attribute_value_candidates_fall_back_to_buffer_history",
        r##"(let ((auto-complete-nxml-attr-words-hash-hash
                                (make-hash-table :test 'equal))
             (auto-complete-nxml-automatic-p t)
             (this-command 'self-insert-command)
             (ac-prefix "pri"))
         (cl-letf (((symbol-function 'auto-complete-nxml-get-candidates)
                    (lambda () nil)))
           (with-temp-buffer
             (insert
              "<a class=\"primary wide\"/>"
              "<b class=\"compact primary\"/>"
              "<c class=\"pri")
             (goto-char (point-max))
             (list
              (auto-complete-nxml-get-attr-value-candidates)
              auto-complete-nxml-buffer-current-attr
              (acnxml-test-hash-alist
               (auto-complete-nxml-get-project-attr-words-hash))))))"##,
        expect![[r#"OK (#1=("compact" "wide" "primary") "class" (("class" . #1#)))"#]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_nxml_project_ident_prefers_explicit_then_project_then_default(),
        auto_complete_nxml_project_word_stores_are_isolated_by_project_and_kind(),
        auto_complete_nxml_tag_value_scan_collects_words_across_real_xml_content(),
        auto_complete_nxml_tag_value_scan_merges_with_existing_project_words(),
        auto_complete_nxml_attribute_scan_groups_values_and_excludes_style_id_and_prefix(),
        auto_complete_nxml_myself_candidates_require_content_context_and_automatic_start(),
        auto_complete_nxml_css_candidates_switch_between_properties_and_values(),
        auto_complete_nxml_rng_candidates_support_function_and_alist_tables_and_dedupe(),
        auto_complete_nxml_nxml_value_candidates_forward_schema_match_values(),
        auto_complete_nxml_attribute_value_candidates_fall_back_to_buffer_history(),
    ]
}
