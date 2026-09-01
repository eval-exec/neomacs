use expect_test::expect;

use super::ParityBatchCase;

fn format_int_and_pluralize_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "format_int_and_pluralize_are_stable",
        r####"
(list :fmt-small (elisp-refs--format-int 42)
      :fmt-big (elisp-refs--format-int 1200)
      :one (elisp-refs--pluralize 1 "match")
      :many (elisp-refs--pluralize 3 "match")
      :zero (elisp-refs--pluralize 0 "file"))
"####,
        expect![[
            r#"OK (:fmt-small "42" :fmt-big "1,200" :one "1 match" :many "3 matchs" :zero "0 files")"#
        ]],
    )
}

fn lines_and_replace_tabs_reshape_snippets() -> ParityBatchCase {
    ParityBatchCase::value(
        "lines_and_replace_tabs_reshape_snippets",
        r####"
(let* ((raw "  foo\n\tbar\n")
       (lines (elisp-refs--lines raw))
       (detabbed (elisp-refs--replace-tabs raw)))
  (list :line-count (length lines)
        :first-line (car lines)
        :detabbed detabbed
        :no-tabs (and (not (string-match-p "\t" detabbed)) t)))
"####,
        expect![[
            r#"OK (:line-count 2 :first-line "  foo\n" :detabbed "  foo\n        bar\n" :no-tabs t)"#
        ]],
    )
}

fn read_and_find_locates_function_calls_in_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "read_and_find_locates_function_calls_in_buffer",
        r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun demo ()\n  (message \"hi\")\n  (message \"there\"))\n")
  (goto-char (point-min))
  (let* ((matches (elisp-refs--read-and-find
                   (current-buffer)
                   'message
                   #'elisp-refs--function-p)))
    (list :count (length matches)
          :all-have-start
          (and (cl-every (lambda (m) (nth 1 m)) matches) t)
          :forms (mapcar #'car matches))))
"####,
        expect![[r#"OK (:count 2 :all-have-start t :forms ((message "hi") (message "there")))"#]],
    )
}

fn proper_list_p_rejects_dotted_pairs() -> ParityBatchCase {
    ParityBatchCase::value(
        "proper_list_p_rejects_dotted_pairs",
        r####"
(list :yes (and (elisp-refs--proper-list-p '(a b c)) t)
      :no (elisp-refs--proper-list-p '(a . b))
      :nil-ok (and (elisp-refs--proper-list-p nil) t))
"####,
        expect!["OK (:yes t :no nil :nil-ok t)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        format_int_and_pluralize_are_stable(),
        lines_and_replace_tabs_reshape_snippets(),
        read_and_find_locates_function_calls_in_buffer(),
        proper_list_p_rejects_dotted_pairs(),
    ]
}
