use expect_test::expect;

use super::ParityBatchCase;

fn atl_markup_inside_tag_classifies_realistic_open_close_nested_and_boundary_positions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_inside_tag_classifies_realistic_open_close_nested_and_boundary_positions",
        r##"(mapcar
          (lambda (contents)
            (cons
             contents
             (atl-markup-test-at-marker
              contents
              nil
              #'atl-markup--inside-tag-p)))
          '("|<article class=\"card\">body</article>"
            "<|article class=\"card\">body</article>"
            "<article class=\"card\"|>body</article>"
            "<article class=\"card\">|body</article>"
            "<article class=\"card\">bo|dy</article>"
            "<article class=\"card\">body</|article>"
            "<article class=\"card\">body</article>|"
            "<main><section><|strong>nested</strong></section></main>"
            "<a></a><|button disabled>save</button>"
            "<x>|<y>second</y>"))"##,
        expect![[
            r#"OK (("|<article class=\"card\">body</article>" nil t t nil) ("<|article class=\"card\">body</article>" t t t nil) ("<article class=\"card\"|>body</article>" t t t nil) ("<article class=\"card\">|body</article>" nil t t nil) ("<article class=\"card\">bo|dy</article>" nil t t nil) ("<article class=\"card\">body</|article>" t t t nil) ("<article class=\"card\">body</article>|" nil t t nil) ("<main><section><|strong>nested</strong></section></main>" t t t nil) ("<a></a><|button disabled>save</button>" t t t nil) ("<x>|<y>second</y>" nil t t nil))"#
        ]],
    )
}

fn atl_markup_inside_tag_records_malformed_math_quoted_delimiter_and_multiline_behavior()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_inside_tag_records_malformed_math_quoted_delimiter_and_multiline_behavior",
        r##"(mapcar
          (lambda (contents)
            (cons
             contents
             (atl-markup-test-at-marker
              contents
              nil
              #'atl-markup--inside-tag-p)))
          '("plain |text without markup"
            "<open |attribute without close"
            "orphan > |text"
            "1 < 2 |and 3 > 1"
            "<tag data-x=\"a | > b\">payload</tag>"
            "<tag data-x=\"a > |b\">payload</tag>"
            "<<|inner>>"
            "<outer\n class=\"wide\"\n data-id=\"42\"|>\nbody"
            "before <!-- |comment --> after"
            "<?xml version=\"1.0\"|?><root/>"
            "<!DOCTYPE |html><html></html>"))"##,
        expect![[
            r#"OK (("plain |text without markup" nil t t nil) ("<open |attribute without close" nil t t nil) ("orphan > |text" nil t t nil) ("1 < 2 |and 3 > 1" t t t nil) ("<tag data-x=\"a | > b\">payload</tag>" t t t nil) ("<tag data-x=\"a > |b\">payload</tag>" nil t t nil) ("<<|inner>>" t t t nil) ("<outer\n class=\"wide\"\n data-id=\"42\"|>\nbody" t t t nil) ("before <!-- |comment --> after" t t t nil) ("<?xml version=\"1.0\"|?><root/>" t t t nil) ("<!DOCTYPE |html><html></html>" t t t nil))"#
        ]],
    )
}

fn atl_markup_inside_tag_respects_narrowing_and_preserves_point_and_contents() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_inside_tag_respects_narrowing_and_preserves_point_and_contents",
        r##"(with-temp-buffer
          (insert
           "prefix <article data-id=\"42\">body</article> suffix")
          (goto-char
           (point-min))
          (search-forward
           "data-id")
          (let ((original-point
                 (point))
                (original-text
                 (buffer-string))
                full
                hidden-left
                hidden-right)
            (setq full
                  (atl-markup--inside-tag-p))
            (save-restriction
              (narrow-to-region
               (1+
                (save-excursion
                  (search-backward "<")))
               (point-max))
              (setq hidden-left
                    (atl-markup--inside-tag-p)))
            (save-restriction
              (narrow-to-region
               (point-min)
               (save-excursion
                 (search-forward ">")
                 (1-
                  (point))))
              (setq hidden-right
                    (atl-markup--inside-tag-p)))
            (list
             full
             hidden-left
             hidden-right
             (=
              original-point
              (point))
             (equal
              original-text
              (buffer-string))
             (buffer-narrowed-p))))"##,
        expect!["OK (t nil nil t t nil)"],
    )
}

fn atl_markup_comment_predicate_follows_emacs_lisp_syntax_not_comment_like_text() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_comment_predicate_follows_emacs_lisp_syntax_not_comment_like_text",
        r##"(mapcar
          (lambda (contents)
            (cons
             contents
             (atl-markup-test-at-marker
              contents
              #'emacs-lisp-mode
              #'atl-markup--comment-block-p)))
          '(";; ordinary com|ment"
            "(message \"semicolon ; in str|ing\")"
            "(setq value |42) ;; trailing comment"
            "(setq value 42) ;; trailing com|ment"
            ";; first comment\n(setq |value 42)"
            ";;; <tag attr=\"x|\"> documentation"
            "(list '<tag |attribute>)"
            "(message \"<!-- not a com|ment -->\")"))"##,
        expect![[
            r#"OK ((";; ordinary com|ment" t t t nil) ("(message \"semicolon ; in str|ing\")" nil t t nil) ("(setq value |42) ;; trailing comment" nil t t nil) ("(setq value 42) ;; trailing com|ment" t t t nil) (";; first comment\n(setq |value 42)" nil t t nil) (";;; <tag attr=\"x|\"> documentation" t t t nil) ("(list '<tag |attribute>)" nil t t nil) ("(message \"<!-- not a com|ment -->\")" nil t t nil))"#
        ]],
    )
}

fn atl_markup_comment_predicate_handles_markup_comments_and_real_tag_attributes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_comment_predicate_handles_markup_comments_and_real_tag_attributes",
        r##"(mapcar
          (lambda (contents)
            (cons
             contents
             (atl-markup-test-at-marker
              contents
              #'html-mode
              #'atl-markup--comment-block-p)))
          '("<div><!-- ordinary com|ment --><span>x</span></div>"
            "<div><!-- <tag attr=\"x|\"> --></div>"
            "<div class=\"not-a-comment|\">body</div>"
            "<script>/* javascript-like com|ment */</script>"
            "<style>/* css-like com|ment */</style>"
            "<div>text |outside comment</div>"
            "<!-- multiline\n comment |body\n --><p>x</p>"))"##,
        expect![[
            r#"OK (("<div><!-- ordinary com|ment --><span>x</span></div>" t t t nil) ("<div><!-- <tag attr=\"x|\"> --></div>" t t t nil) ("<div class=\"not-a-comment|\">body</div>" nil t t nil) ("<script>/* javascript-like com|ment */</script>" nil t t nil) ("<style>/* css-like com|ment */</style>" nil t t nil) ("<div>text |outside comment</div>" nil t t nil) ("<!-- multiline\n comment |body\n --><p>x</p>" t t t nil))"#
        ]],
    )
}

fn atl_markup_comment_predicate_obeys_buffer_local_custom_comment_syntax() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_comment_predicate_obeys_buffer_local_custom_comment_syntax",
        r##"(mapcar
          (lambda (contents)
            (with-temp-buffer
              (set-syntax-table
               (copy-syntax-table
                (standard-syntax-table)))
              (modify-syntax-entry
               ?#
               "<"
               (syntax-table))
              (modify-syntax-entry
               ?\n
               ">"
               (syntax-table))
              (atl-markup-test-place-marker
               contents)
              (set-buffer-modified-p nil)
              (let ((point-before
                     (point))
                    (text-before
                     (buffer-string)))
                (list
                 contents
                 (atl-markup--comment-block-p)
                 (=
                  point-before
                  (point))
                 (equal
                  text-before
                  (buffer-string))
                 (buffer-modified-p)))))
          '("# custom com|ment\nnext"
            "value # trailing com|ment\nnext"
            "\"# string |text\"\nnext"
            "plain |text\nnext"))"##,
        expect![[
            r##"OK (("# custom com|ment\nnext" t t t nil) ("value # trailing com|ment\nnext" t t t nil) ("\"# string |text\"\nnext" nil t t nil) ("plain |text\nnext" nil t t nil))"##
        ]],
    )
}

pub(super) fn predicates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_markup_inside_tag_classifies_realistic_open_close_nested_and_boundary_positions(),
        atl_markup_inside_tag_records_malformed_math_quoted_delimiter_and_multiline_behavior(),
        atl_markup_inside_tag_respects_narrowing_and_preserves_point_and_contents(),
        atl_markup_comment_predicate_follows_emacs_lisp_syntax_not_comment_like_text(),
        atl_markup_comment_predicate_handles_markup_comments_and_real_tag_attributes(),
        atl_markup_comment_predicate_obeys_buffer_local_custom_comment_syntax(),
    ]
}
