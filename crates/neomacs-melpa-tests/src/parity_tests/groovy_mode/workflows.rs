use expect_test::expect;

use super::ParityBatchCase;

/// Drive `groovy-indent-line' (the mode's indent entry point) on a mis-indented
/// body line inside a closure and assert it indents to `groovy-indent-offset'.
fn indent_line_indents_body_to_offset() -> ParityBatchCase {
    ParityBatchCase::value(
        "indent_line_indents_body_to_offset",
        r####"
(with-temp-buffer
  (groovy-mode)
  (insert "def f() {\nx\n}\n")
  (forward-line -2)
  (groovy-indent-line)
  (list :indent (current-indentation) :offset groovy-indent-offset))
"####,
        expect![[r#"OK (:indent 4 :offset 4)"#]],
    )
}

/// Drive `font-lock-ensure' on a `def' form and assert the keyword gets
/// font-lock-keyword-face.
fn font_lock_highlights_def_keyword() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_highlights_def_keyword",
        r####"
(with-temp-buffer
  (groovy-mode)
  (insert "def foo() { return 1 }\n")
  (font-lock-ensure)
  (goto-char (point-min))
  (search-forward "def")
  (backward-char 1)
  (list :face (get-text-property (point) 'face)))
"####,
        expect![[r#"OK (:face font-lock-keyword-face)"#]],
    )
}

/// Drive `font-lock-ensure' on a "//" line comment and assert the comment text
/// gets font-lock-comment-face -- the mode's actual comment fontification
/// (the "/" itself is punctuation; comments are detected by font-lock).
fn line_comment_text_is_fontified() -> ParityBatchCase {
    ParityBatchCase::value(
        "line_comment_text_is_fontified",
        r####"
(with-temp-buffer
  (groovy-mode)
  (insert "code // a remark\n")
  (font-lock-ensure)
  (goto-char (point-min))
  (search-forward "remark")
  (backward-char 1)
  (list :face (get-text-property (point) 'face)))
"####,
        expect![[r#"OK (:face font-lock-comment-face)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        indent_line_indents_body_to_offset(),
        font_lock_highlights_def_keyword(),
        line_comment_text_is_fontified(),
    ]
}
