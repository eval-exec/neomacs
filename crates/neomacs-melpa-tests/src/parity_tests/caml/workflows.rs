use expect_test::expect;

use super::ParityBatchCase;

/// The core value: `caml-indent-command' (bound to TAB) indents each
/// line of a realistic match expression to its exact column.
fn the_indent_command_aligns_a_match_expression() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_indent_command_aligns_a_match_expression",
        r####"(let ((buf (caml--test-ml-buffer
                       (concat "let f x =\n"
                               "match x with\n"
                               "| Some y ->\n"
                               "y + 1\n"
                               "| None ->\n"
                               "0\n"))))
  (unwind-protect
      (with-current-buffer buf
        (goto-char (point-min))
        (while (not (eobp))
          (caml-indent-command)
          (forward-line 1))
        (list :source (caml--test-source-state)
              :mode major-mode
              :indent-line-function indent-line-function
              :comment-start comment-start
              :comment-end comment-end
              :text (buffer-substring-no-properties (point-min) (point-max))
              :columns
              (save-excursion
                (goto-char (point-min))
                (let (cols)
                  (while (not (eobp))
                    (push (current-column) cols)
                    (forward-line 1))
                  (nreverse cols)))))
    (kill-buffer buf)))"####,
        expect![[
            r#"OK (:source (:upstream-tree "e635b82cce1666662555900bbf12d084989d73ed" :feature t :version "20250227.1734") :mode caml-mode :indent-line-function caml-indent-command :comment-start "(*" :comment-end "*)" :text "let f x =\n  match x with\n  | Some y ->\n      y + 1\n  | None ->\n      0\n" :columns (0 0 0 0 0 0))"#
        ]],
    )
}

/// Phrase movement: `caml-find-phrase' and `caml-mark-phrase' split the
/// buffer on the `;;' phrase separator.
fn the_phrase_movement_splits_on_the_phrase_separator() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_phrase_movement_splits_on_the_phrase_separator",
        r####"(let ((buf (caml--test-ml-buffer
                       "let a = 1;;\nlet b = 2;;\nlet c = 3;;\n")))
  (unwind-protect
      (with-current-buffer buf
        (goto-char (point-min))
        (let* ((first-end (save-excursion (caml-find-phrase) (point)))
               (mark-1 (progn (caml-mark-phrase)
                              (buffer-substring-no-properties
                               (region-beginning) (region-end))))
               (second-end
                (save-excursion
                  (goto-char first-end)
                  (caml-find-phrase)
                  (point))))
          (goto-char (point-min))
          (list :first-end first-end
                :second-end second-end
                :first-mark mark-1)))
    (kill-buffer buf)))"####,
        expect![[r#"OK (:first-end 13 :second-end 25 :first-mark "let a = 1;;\n")"#]],
    )
}

/// The current-defun function and the previous-index-position scan
/// classify the OCaml top-level forms.  (The package's
/// `caml-create-index-function' calls `imenu--sort-by-name', an
/// imenu internal removed in Emacs 31, so the full index builder is
/// left unexercised.)
fn the_current_defun_and_prev_index_position_classify_the_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_current_defun_and_prev_index_position_classify_the_forms",
        r####"(let ((buf (caml--test-ml-buffer
                       "type t = int\n\nlet helper x = x + 1\n\nlet main () =\n  helper 1\n")))
  (unwind-protect
      (with-current-buffer buf
        (goto-char (point-min))
        (search-forward "main")
        (let ((defun-main (caml-current-defun)))
          (goto-char (point-min))
          (search-forward "helper")
          (let ((defun-helper (caml-current-defun))
                (prev-pos (save-excursion
                            (goto-char (point-max))
                            (caml-prev-index-position-function)))
                (prev-data (progn (goto-char (point-max))
                                  (caml-prev-index-position-function)
                                  (caml-match-string 5))))
            (list :defun-main defun-main
                  :defun-helper defun-helper
                  :prev-pos prev-pos
                  :prev-data prev-data))))
    (kill-buffer buf)))"####,
        expect![[
            r#"OK (:defun-main "main" :defun-helper "helper" :prev-pos 37 :prev-data "main")"#
        ]],
    )
}

/// Comment movement skips (* ... *) comments including nested ones and
/// `caml-in-comment-p' detects the context.
fn the_comment_movement_skips_nested_comments() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_comment_movement_skips_nested_comments",
        r####"(let ((buf (caml--test-ml-buffer
                       "let x = 1 (* outer (* inner *) still *)\nlet y = 2\n")))
  (unwind-protect
      (with-current-buffer buf
        (goto-char (point-min))
        (search-forward "inner")
        (let ((inside (caml-in-comment-p)))
          (goto-char (point-min))
          (search-forward "outer")
          (let ((skip-end (save-excursion
                            (goto-char (match-beginning 0))
                            (caml-skip-comments-forward)
                            (point))))
            (list :inside-nested inside :skip-end skip-end
                  :after (buffer-substring-no-properties skip-end (point-max))))))
    (kill-buffer buf)))"####,
        expect![[r#"OK (:inside-nested 2 :skip-end 41 :after "let y = 2\n")"#]],
    )
}

/// `caml-insert-match-form' inserts the match skeleton at point.
fn the_match_form_skeleton_is_inserted() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_match_form_skeleton_is_inserted",
        r####"(let ((buf (caml--test-ml-buffer "let f x =\n")))
  (unwind-protect
      (with-current-buffer buf
        (goto-char (point-max))
        (caml-insert-match-form)
        (list :text (buffer-substring-no-properties (point-min) (point-max))
              :point (point)))
    (kill-buffer buf)))"####,
        expect![[r#"OK (:text "let f x =\nmatch\n  \nwith\n  " :point 19)"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_indent_command_aligns_a_match_expression(),
        the_phrase_movement_splits_on_the_phrase_separator(),
        the_current_defun_and_prev_index_position_classify_the_forms(),
        the_comment_movement_skips_nested_comments(),
        the_match_form_skeleton_is_inserted(),
    ]
}
