//! Editing an Astro component the way a person does, rather than the way a
//! test does.
//!
//! The existing corpus builds every document with `insert` and then calls
//! `indent-region` over the finished text. Both workflows here go through the
//! command loop instead -- `call-interactively` on whatever the key is bound
//! to, with `electric-indent-mode` live -- because two of this mode's most
//! visible behaviours only exist on that path and neither is reachable from a
//! document that was complete before anything was indented.

use expect_test::expect;

use super::ParityBatchCase;

/// Type a component top down, then indent it, and compare the two products.
///
/// The typed result is completely flat: every line lands at column 0 even
/// with `electric-indent-mode` on and every character arriving through
/// `self-insert-command`. That is not a quirk of electric indent's timing.
/// An element's nesting is established by its **closing** tag, so at the
/// moment `RET` reindents a line the enclosing `<main>` and `<section>` are
/// still unterminated and the line genuinely has no parent element to be
/// indented against. The mid-typing `TAB` in the middle of this workflow
/// pins exactly that: pressed while the document is still open it answers
/// column 0, and pressed on the same line once the closing tags exist it
/// answers column 2.
///
/// Column 2 rather than 4 is the second half of it. The rules are
/// `parent-bol` relative, so a single `TAB` on a line whose parent is itself
/// still at column 0 can only reach one level in; the nested layout the rest
/// of the corpus pins is a property of `indent-region` walking the document
/// in order, not something a user gets line by line.
///
/// The indentation figures are the leading-space count per line, which keeps
/// the comparison legible where two whole buffer strings would not be.
fn typing_a_component_top_down_leaves_it_flat_until_the_closing_tags_exist() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_a_component_top_down_leaves_it_flat_until_the_closing_tags_exist",
        r##"(cl-labels
              ((type-text
                (text)
                (dolist (character (append text nil))
                  (let ((last-command-event character))
                    (call-interactively
                     (key-binding
                      (if (eq character ?\n)
                          (kbd "RET")
                        (vector character)))))))
               (indentations
                (text)
                (mapcar
                 (lambda (line)
                   (- (length line)
                      (length (string-trim-left line))))
                 (split-string text "\n"))))
            (with-temp-buffer
              (astro-ts-mode)
              (let (typed mid-typing-tab after-close-tab)
                (type-text "<main>\n<section>\n<h1>{title}</h1>")
                (call-interactively (key-binding (kbd "TAB")))
                (setq mid-typing-tab (current-indentation))
                (type-text "\n</section>\n</main>\n")
                (setq typed
                      (buffer-substring-no-properties
                       (point-min)
                       (point-max)))
                (goto-char (point-min))
                (forward-line 2)
                (call-interactively (key-binding (kbd "TAB")))
                (setq after-close-tab (current-indentation))
                (indent-region (point-min) (point-max))
                (list
                 :electric-indent electric-indent-mode
                 :typed typed
                 :typed-indentations (indentations typed)
                 :tab-while-open mid-typing-tab
                 :tab-once-closed after-close-tab
                 :after-indent-region
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max))
                 :indent-region-indentations
                 (indentations
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max)))
                 :typing-produced-the-indented-layout
                 (equal typed
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max)))))))"##,
        expect![[
            r#"OK (:electric-indent t :typed "<main>\n<section>\n<h1>{title}</h1>\n</section>\n</main>\n" :typed-indentations (0 0 0 0 0 0) :tab-while-open 0 :tab-once-closed 2 :after-indent-region "<main>\n  <section>\n    <h1>{title}</h1>\n  </section>\n</main>\n" :indent-region-indentations (0 2 4 2 0 0) :typing-produced-the-indented-layout nil)"#
        ]],
    )
}

fn commenting_a_line_uses_html_syntax_in_every_embedded_language() -> ParityBatchCase {
    ParityBatchCase::value(
        "commenting_a_line_uses_html_syntax_in_every_embedded_language",
        r##"(with-temp-buffer
            (insert "---\n")
            (insert "const x = 1;\n")
            (insert "---\n")
            (insert "<div>hi</div>\n")
            (insert "<script>\n")
            (insert "let y = 2;\n")
            (insert "</script>\n")
            (insert "<style>\n")
            (insert ".a { color: red; }\n")
            (insert "</style>\n")
            (astro-ts-mode)
            (font-lock-ensure)
            (let (regions)
              (dolist (needle '("const x" "hi" "let y" "color"))
                (goto-char (point-min))
                (search-forward needle)
                (let ((language
                       (astro-ts-mode--treesit-language-at-point
                        (point))))
                  (call-interactively
                   (key-binding (kbd "C-x C-;")))
                  (goto-char (point-min))
                  (search-forward needle)
                  (push
                   (list
                    needle
                    language
                    comment-start
                    comment-end
                    (buffer-substring-no-properties
                     (line-beginning-position)
                     (line-end-position)))
                   regions)))
              (list
               :regions (nreverse regions)
               :host-tree-error-nodes
               (let ((errors 0))
                 (treesit-search-subtree
                  (treesit-buffer-root-node 'astro)
                  (lambda (node)
                    (when (equal
                           (treesit-node-type node)
                           "ERROR")
                      (setq errors (1+ errors)))
                    nil)
                  nil t)
                 errors)
               :buffer (buffer-string))))"##,
        expect![[
            r#"OK (:regions (("const x" tsx "<!-- " " -->" "<!-- const x = 1; -->") ("hi" astro "<!-- " " -->" "<!-- <div>hi</div> -->") ("let y" tsx "<!-- " " -->" "<!-- let y = 2; -->") ("color" css "<!-- " " -->" "<!-- .a { color: red; } -->")) :host-tree-error-nodes 0 :buffer #("---\n<!-- const x = 1; -->\n---\n<!-- <div>hi</div> -->\n<script>\n<!-- let y = 2; -->\n</script>\n<style>\n<!-- .a { color: red; } -->\n</style>\n" 0 3 (face font-lock-comment-face) 4 5 (syntax-table #1=(2097163)) 24 25 (syntax-table #2=(2097164)) 26 29 (face font-lock-comment-face) 30 31 (syntax-table #1# fontified nil) 31 35 (fontified nil) 35 36 (fontified nil) 36 39 (fontified nil face font-lock-function-name-face) 39 44 (fontified nil) 44 47 (fontified nil face font-lock-function-name-face) 47 48 (fontified nil) 48 51 (fontified nil) 51 52 (syntax-table #2# fontified nil) 54 60 (face font-lock-function-name-face) 62 63 (syntax-table #1#) 80 81 (syntax-table #2#) 84 90 (face font-lock-function-name-face) 93 98 (face font-lock-function-name-face) 100 101 (syntax-table #1#) 126 127 (syntax-table #2#) 130 135 (face font-lock-function-name-face)))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_a_component_top_down_leaves_it_flat_until_the_closing_tags_exist(),
        commenting_a_line_uses_html_syntax_in_every_embedded_language(),
    ]
}
