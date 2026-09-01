use expect_test::expect;

use super::ParityBatchCase;

/// Drive `julia-indent-line' (the mode's indent entry point) on a mis-indented
/// body line inside a function and assert it indents to `julia-indent-offset'.
fn indent_line_indents_body_to_offset() -> ParityBatchCase {
    ParityBatchCase::value(
        "indent_line_indents_body_to_offset",
        r####"
(with-temp-buffer
  (julia-mode)
  (insert "function f()\nx\nend\n")
  (forward-line -2)
  (julia-indent-line)
  (list :indent (current-indentation) :offset julia-indent-offset))
"####,
        expect![[r#"OK (:indent 4 :offset 4)"#]],
    )
}

/// Drive `font-lock-ensure' on a function definition and assert the keyword
/// gets font-lock-keyword-face -- the mode's actual fontification behavior.
fn font_lock_highlights_function_keyword() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_highlights_function_keyword",
        r####"
(with-temp-buffer
  (julia-mode)
  (insert "function foo() return 1 end\n")
  (font-lock-ensure)
  (goto-char (point-min))
  (search-forward "function")
  (backward-char 1)
  (list :face (get-text-property (point) 'face)))
"####,
        expect![[r#"OK (:face font-lock-keyword-face)"#]],
    )
}

/// Assert "#" carries comment syntax under the mode's syntax table.
fn hash_is_comment_syntax() -> ParityBatchCase {
    ParityBatchCase::value(
        "hash_is_comment_syntax",
        r####"
(with-temp-buffer
  (julia-mode)
  (insert "x # note\n")
  (goto-char (point-min))
  (search-forward "#")
  (list :syntax (string (char-syntax (char-before)))))
"####,
        expect![[r#"OK (:syntax "<")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        indent_line_indents_body_to_offset(),
        font_lock_highlights_function_keyword(),
        hash_is_comment_syntax(),
    ]
}
