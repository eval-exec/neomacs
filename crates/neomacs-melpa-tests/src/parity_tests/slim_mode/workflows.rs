use expect_test::expect;

use super::ParityBatchCase;

fn mode_registers_indent_and_auto_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_registers_indent_and_auto_mode",
        r####"
(neomacs-slim-mode-test-with-buffer
 (lambda ()
   (list :mode major-mode
         :indent-line indent-line-function
         :indent-region indent-region-function
         :offset slim-indent-offset
         :comment-start comment-start
         :tabs indent-tabs-mode
         :auto (cdr (assoc "\\.slim\\'" auto-mode-alist)))))
"####,
        expect![[
            r#"OK (:mode slim-mode :indent-line slim-indent-line :indent-region slim-indent-region :offset 2 :comment-start "/" :tabs nil :auto slim-mode)"#
        ]],
    )
}

fn compute_indentation_and_indent_line_respect_offset() -> ParityBatchCase {
    ParityBatchCase::value(
        "compute_indentation_and_indent_line_respect_offset",
        r####"
(neomacs-slim-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "title Hello")
   (forward-line 0)
   (let ((nested (slim-compute-indentation)))
     (goto-char (point-min))
     (search-forward "html")
     (forward-line 0)
     (let ((top (slim-compute-indentation)))
       (goto-char (point-min))
       (search-forward "p Welcome")
       (forward-line 0)
       (back-to-indentation)
       (let ((before (current-column)))
         (slim-indent-line)
         (list :nested nested
               :top top
               :before before
               :after (current-column)
               :offset slim-indent-offset))))))
"####,
        expect!["OK (:nested 4 :top 0 :before 6 :after 4 :offset 2)"],
    )
}

fn forward_sexp_skips_nested_blocks() -> ParityBatchCase {
    ParityBatchCase::value(
        "forward_sexp_skips_nested_blocks",
        r####"
(neomacs-slim-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "body")
   (forward-line 0)
   (back-to-indentation)
   (let ((start-line (line-number-at-pos)))
     (slim-forward-sexp 1)
     (list :start-line start-line
           :end-line (line-number-at-pos)
           :end-text
           (string-trim
            (buffer-substring-no-properties
             (line-beginning-position) (line-end-position)))
           :at-indent (and (slim-at-indent-p) t)))))
"####,
        expect![[r#"OK (:start-line 5 :end-line 11 :end-text "" :at-indent t)"#]],
    )
}

fn comment_block_inserts_slash_and_reindents() -> ParityBatchCase {
    ParityBatchCase::value(
        "comment_block_inserts_slash_and_reindents",
        r####"
(neomacs-slim-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "p Welcome")
   (forward-line 0)
   (back-to-indentation)
   (slim-comment-block)
   (goto-char (point-min))
   (search-forward "p Welcome")
   (forward-line -1)
   (list :comment-line
         (string-trim
          (buffer-substring-no-properties
           (line-beginning-position) (line-end-position)))
         :body-has-comment
         (save-excursion
           (goto-char (point-min))
           (and (re-search-forward "^[ \t]*/" nil t) t)))))
"####,
        expect![[r#"OK (:comment-line "/" :body-has-comment t)"#]],
    )
}

fn reindent_region_by_shifts_nested_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "reindent_region_by_shifts_nested_lines",
        r####"
(neomacs-slim-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "li One")
   (forward-line 0)
   (let ((start (point)))
     (search-forward "li Two")
     (end-of-line)
     (let ((end (point)))
       (slim-reindent-region-by slim-indent-offset)
       (goto-char start)
       (list :one (current-indentation)
             :two (progn
                    (search-forward "li Two")
                    (forward-line 0)
                    (current-indentation))
             :offset slim-indent-offset)))))
"####,
        expect!["OK (:one 8 :two 8 :offset 2)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_registers_indent_and_auto_mode(),
        compute_indentation_and_indent_line_respect_offset(),
        forward_sexp_skips_nested_blocks(),
        comment_block_inserts_slash_and_reindents(),
        reindent_region_by_shifts_nested_lines(),
    ]
}
