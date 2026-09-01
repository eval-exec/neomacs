use expect_test::expect;

use super::ParityBatchCase;

fn aggressive_fill_paragraph_preserves_structured_prefixes_but_fills_ordinary_comments_and_prose()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aggressive_fill_paragraph_preserves_structured_prefixes_but_fills_ordinary_comments_and_prose",
        r####"(list
                            (with-temp-buffer
                              (c++-mode)
                              (setq
                               fill-column
                               42)
                              (aggressive-fill-paragraph-mode
                               1)
                              (insert
                               "// * Deploy the parser canary to one region before promoting it to every production region")
                              (let ((last-command-event
                                     ?\s)
                                    (this-command
                                     'self-insert-command)
                                    (real-this-command
                                     'self-insert-command))
                                (self-insert-command
                                 1))
                              (insert
                               "\n\n// This ordinary explanation is deliberately long enough to require comment-aware wrapping")
                              (let ((last-command-event
                                     ?\s)
                                    (this-command
                                     'self-insert-command)
                                    (real-this-command
                                     'self-insert-command))
                                (self-insert-command
                                 1))
                              (list
                               (buffer-substring-no-properties
                                (point-min)
                                (point-max))
                               (point)
                               (line-number-at-pos)
                               (current-column)))
                            (with-temp-buffer
                              (org-mode)
                              (setq
                               fill-column
                               38)
                              (aggressive-fill-paragraph-mode
                               1)
                              (insert
                               "| Owner | Status |\n"
                               "| Parser | Canary |\n\n"
                               "#+BEGIN_SRC emacs-lisp\n"
                               "(message \"deploy\")\n"
                               "#+END_SRC\n\n"
                               "This operational note explains why the parser canary remains isolated until every recovery check succeeds.")
                              (goto-char
                               (point-min))
                              (search-forward
                               "Owner")
                              (let ((last-command-event
                                     ?\s)
                                    (this-command
                                     'self-insert-command)
                                    (real-this-command
                                     'self-insert-command))
                                (self-insert-command
                                 1))
                              (insert
                               "Name")
                              (search-forward
                               "emacs-lisp")
                              (let ((last-command-event
                                     ?\s)
                                    (this-command
                                     'self-insert-command)
                                    (real-this-command
                                     'self-insert-command))
                                (self-insert-command
                                 1))
                              (insert
                               ":results output")
                              (goto-char
                               (point-max))
                              (let ((last-command-event
                                     ?\s)
                                    (this-command
                                     'self-insert-command)
                                    (real-this-command
                                     'self-insert-command))
                                (self-insert-command
                                 1))
                              (list
                               (buffer-string)
                               (point)
                               (line-number-at-pos)
                               (current-column))))"####,
        expect![[
            r#"OK (("// * Deploy the parser canary to one region before promoting it to every production region \n\n// This ordinary explanation is\n// deliberately long enough to require\n// comment-aware wrapping " 191 5 26) ("| Owner Name | Status |\n| Parser | Canary |\n\n#+BEGIN_SRC emacs-lisp :results output\n(message \"deploy\")\n#+END_SRC\n\nThis operational note explains why the\nparser canary remains isolated until\nevery recovery check succeeds. " 222 10 31))"#
        ]],
    )
}

pub(super) fn suppression_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aggressive_fill_paragraph_preserves_structured_prefixes_but_fills_ordinary_comments_and_prose(),
    ]
}
