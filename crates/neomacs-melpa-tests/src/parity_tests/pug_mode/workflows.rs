use expect_test::expect;

use super::ParityBatchCase;

fn mode_registers_indent_and_auto_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_registers_indent_and_auto_mode",
        r####"
(with-temp-buffer
  (pug-mode)
  (list :mode major-mode
        :indent-line indent-line-function
        :tab-width pug-tab-width
        :comment-start comment-start
        :auto (cdr (cl-find-if
                    (lambda (cell)
                      (and (stringp (car cell))
                           (string-match-p "pug" (car cell))))
                    auto-mode-alist))))
"####,
        expect![[
            r#"OK (:mode pug-mode :indent-line pug-indent-line :tab-width 2 :comment-start "//-" :auto pug-mode)"#
        ]],
    )
}

fn compute_indentation_nests_under_parent_tag() -> ParityBatchCase {
    ParityBatchCase::value(
        "compute_indentation_nests_under_parent_tag",
        r####"
(with-temp-buffer
  (pug-mode)
  (insert "html\n  head\n    title Hi\n  body\n")
  (goto-char (point-min))
  (search-forward "title")
  (forward-line 0)
  (let ((nested (pug-compute-indentation)))
    (goto-char (point-min))
    (search-forward "html")
    (forward-line 0)
    (list :nested nested
          :top (pug-compute-indentation)
          :tab-width pug-tab-width)))
"####,
        expect![[r#"OK (:nested 4 :top 0 :tab-width 2)"#]],
    )
}

fn indent_line_cycles_to_computed_then_backdents() -> ParityBatchCase {
    ParityBatchCase::value(
        "indent_line_cycles_to_computed_then_backdents",
        r####"
(with-temp-buffer
  (pug-mode)
  (insert "div\nchild\n")
  (goto-char (point-min))
  (search-forward "child")
  (forward-line 0)
  (let ((this-command 'pug-indent-line)
        (last-command 'other))
    (pug-indent-line)
    (let ((first (current-indentation)))
      (setq last-command this-command)
      (pug-indent-line)
      (list :first first
            :second (current-indentation)
            :tab-width pug-tab-width))))
"####,
        expect![[r#"OK (:first 2 :second 0 :tab-width 2)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_registers_indent_and_auto_mode(),
        compute_indentation_nests_under_parent_tag(),
        indent_line_cycles_to_computed_then_backdents(),
    ]
}
