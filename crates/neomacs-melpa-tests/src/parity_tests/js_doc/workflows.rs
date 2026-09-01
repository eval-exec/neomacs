use expect_test::expect;

use super::ParityBatchCase;

fn format_string_expands_author_and_file_placeholders() -> ParityBatchCase {
    ParityBatchCase::value(
        "format_string_expands_author_and_file_placeholders",
        r####"
(with-temp-buffer
  (setq buffer-file-name "/tmp/demo.js")
  (list :author (js-doc-format-string "by %a")
        :file (js-doc-format-string "file %F")
        :param
        (progn
          (setq js-doc-current-parameter-name "count")
          (js-doc-format-string "param %p"))))
"####,
        expect![[r#"OK (:author "by Test Author" :file "file  *temp*" :param "param count")"#]],
    )
}

fn pick_symbol_name_and_param_parse_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "pick_symbol_name_and_param_parse_are_stable",
        r####"
(list :simple (js-doc-pick-symbol-name "foo")
      :typed (js-doc-pick-symbol-name "/** @type {number} */ bar")
      :params
      (with-temp-buffer
        (insert "function demo(a, b, c) { return a + b; }")
        (js-doc--parse-function-params
         (progn (goto-char (point-min)) (search-forward "(") (point))
         (progn (goto-char (point-min)) (search-forward ")") (1- (point))))))
"####,
        expect![[r#"OK (:simple "foo" :typed "bar" :params ("a" "b" "c"))"#]],
    )
}

fn function_doc_metadata_detects_params_return_and_throw() -> ParityBatchCase {
    ParityBatchCase::value(
        "function_doc_metadata_detects_params_return_and_throw",
        r####"
(with-temp-buffer
  (insert "function demo(x, y) {\n  if (!x) throw new Error('x');\n  return x + y;\n}\n")
  (js-mode)
  (goto-char (point-min))
  (js-doc--beginning-of-defun)
  (let ((meta (js-doc--function-doc-metadata)))
    (list :params (cdr (assoc 'params meta))
          :returns (and (assoc 'returns meta) t)
          :throws (and (assoc 'throws meta) t))))
"####,
        expect![[r#"OK (:params ("x" "y") :returns t :throws t)"#]],
    )
}

fn insert_function_doc_writes_param_and_return_tags() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_function_doc_writes_param_and_return_tags",
        r####"
(with-temp-buffer
  (insert "function add(a, b) {\n  return a + b;\n}\n")
  (js-mode)
  (goto-char (point-min))
  (js-doc-insert-function-doc)
  (let ((text (buffer-substring-no-properties (point-min) (point-max))))
    (list :has-top (and (string-match-p "/\\*\\*" text) t)
          :has-param-a (and (string-match-p "@param.*a" text) t)
          :has-param-b (and (string-match-p "@param.*b" text) t)
          :has-return (and (string-match-p "@return" text) t)
          :function-still-present
          (and (string-match-p "function add" text) t)
          :line-count (length (split-string text "\n" t)))))
"####,
        expect![
            "OK (:has-top t :has-param-a t :has-param-b t :has-return t :function-still-present t :line-count 9)"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        format_string_expands_author_and_file_placeholders(),
        pick_symbol_name_and_param_parse_are_stable(),
        function_doc_metadata_detects_params_return_and_throw(),
        insert_function_doc_writes_param_and_return_tags(),
    ]
}
