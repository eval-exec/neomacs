use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_html_export_drawer_special_footnote_filter_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (nil nil nil nil t t \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> Kept</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<div class=\\\"callout aside\\\" data-x=\\\"yes\\\" id=\\\"org-id\\\">\\n<p>\\nAside with <sup><a id=\\\"fnr.a\\\" class=\\\"footref\\\" href=\\\"#fn.a\\\" role=\\\"doc-backlink\\\">1</a></sup><sup>, </sup><sup><a id=\\\"fnr.b\\\" class=\\\"footref\\\" href=\\\"#fn.b\\\" role=\\\"doc-backlink\\\">2</a></sup> and <a href=\\\"https://example.org\\\">link</a>.\\n</p>\\n\\n</div>\\n</div>\\n</div>\\n<div id=\\\"footnotes\\\">\\n<h2 class=\\\"footnotes\\\">Footnotes: </h2>\\n<div id=\\\"text-footnotes\\\">\\n\\n<div class=\\\"footdef\\\"><sup><a id=\\\"fn.a\\\" class=\\\"footnum\\\" href=\\\"#fnr.a\\\" role=\\\"doc-backlink\\\">1</a></sup> <div class=\\\"footpara\\\" role=\\\"doc-footnote\\\"><p class=\\\"footpara\\\">\\nAlpha footnote.\\n</p></div></div>\\n\\n<div class=\\\"footdef\\\"><sup><a id=\\\"fn.b\\\" class=\\\"footnum\\\" href=\\\"#fnr.b\\\" role=\\\"doc-backlink\\\">2</a></sup> <div class=\\\"footpara\\\" role=\\\"doc-footnote\\\"><p class=\\\"footpara\\\">\\nBeta footnote.\\n</p></div></div>\\n\\n\\n</div>\\n</div>\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Export Mix\n")
    (insert "* Kept\n")
    (insert ":LOGBOOK:\n")
    (insert "Drawer text with \\alpha.\n")
    (insert ":END:\n")
    (insert "#+ATTR_HTML: :class callout :data-x yes\n")
    (insert "#+begin_aside\n")
    (insert "Aside with [fn:a][fn:b] and [[https://example.org][link]].\n")
    (insert "#+end_aside\n")
    (insert "[fn:a] Alpha footnote.\n")
    (insert "[fn:b] Beta footnote.\n")
    (insert "** Hidden :noexport:\n")
    (insert "Should not export.\n")
    (insert "* COMMENT Commented\n")
    (insert "Should not export either.\n")
    (let* ((org-export-with-toc nil)
           (org-export-exclude-tags '("noexport"))
           (org-html-format-drawer-function
            (lambda (name contents)
              (format "<section class=\"drawer\" data-name=\"%s\">%s</section>"
                      name contents)))
           (html (org-export-as 'html nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "org[[:alnum:]]+"
             "org-id"
             html)))
      (list
       (not (null (string-match-p "data-name=\"LOGBOOK\"" html)))
       (not (null (string-match-p "&alpha;" html)))
       (not (null (string-match-p "<aside" html)))
       (not (null (string-match-p "class=\"callout\"" html)))
       (not (null (string-match-p "footnotes" html)))
       (null (string-match-p "Should not export" html))
       normalized))))"##,
        expect,
    );
}

#[test]
fn org_html_export_visible_subtree_planning_filter_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Visible Subtree\n")
    (insert "#+OPTIONS: toc:nil prop:t\n")
    (insert "#+MACRO: badge Badge-$1\n")
    (insert "* Parent\n")
    (insert "Parent body hidden during visible export.\n")
    (insert "** TODO Export Me :ship:\n")
    (insert "SCHEDULED: <2026-05-27 Wed> DEADLINE: <2026-05-28 Thu>\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: export-me\n:Owner: Ada\n:END:\n")
    (insert "Visible paragraph {{{badge(ok)}}} with [[#export-me][self]] and [fn:a].\n")
    (insert "#+begin_quote\nquoted visible text\n#+end_quote\n")
    (insert "*** Hidden Child\n")
    (insert "Child body should be invisible-only skipped.\n")
    (insert "** TODO Sibling :noexport:\n")
    (insert "Sibling body should never export.\n")
    (insert "* Tail\nTail body.\n")
    (insert "[fn:a] Footnote visible body.\n")
    (goto-char (point-min))
    (search-forward "Hidden Child")
    (beginning-of-line)
    (org-fold-hide-subtree)
    (goto-char (point-min))
    (search-forward "Export Me")
    (beginning-of-line)
    (let ((calls nil))
      (let ((org-export-exclude-tags '("noexport"))
            (org-export-with-toc nil)
            (org-export-filter-headline-functions
             (list (lambda (text backend info)
                     (push (list 'headline backend
                                 (string-match-p "Export Me" text)
                                 (length text))
                           calls)
                     text)))
            (org-export-filter-final-output-functions
             (list (lambda (text backend info)
                     (push (list 'final backend
                                 (plist-get info :with-toc)
                                 (length text))
                           calls)
                     text))))
        (let* ((html (org-export-as 'html t t t
                                    '(:with-toc nil
                                      :html-html5-fancy t)))
               (info (org-export-get-environment
                      'html t '(:with-toc nil)))
               (tree (plist-get info :parse-tree))
               (headlines
                (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :raw-value h)
                          (org-export-get-relative-level h info)
                          (org-export-get-headline-number h info)
                          (org-export-get-tags h info)
                          (org-export-numbered-headline-p h info)))))
               (links
                (org-element-map tree 'link
                  (lambda (link)
                    (let ((resolved
                           (ignore-errors
                             (org-export-resolve-link link info))))
                      (list (org-element-property :raw-link link)
                            (org-element-type resolved)
                            (and resolved
                                 (org-export-get-reference resolved info)))))))
               (footnotes
                (org-export-collect-footnote-definitions info)))
          (list (nreverse calls)
                headlines
                links
                (mapcar (lambda (entry)
                          (list (car entry)
                                (org-element-property :label (cdr entry))))
                        footnotes)
                (mapcar (lambda (needle)
                          (not (null (string-match-p needle html))))
                        '("Export Me" "Badge-ok" "Visible paragraph"
                          "quoted visible text" "Footnote visible body"
                          "Owner"))
                (mapcar (lambda (needle)
                          (null (string-match-p needle html)))
                        '("Parent body" "Child body" "Sibling body"
                          "Tail body"))
                (replace-regexp-in-string
                 "org[[:alnum:]]+"
                 "org-id"
                 html))))))"##,
        expect,
    );
}

#[test]
fn org_export_multi_backend_resolution_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"Definition not found for footnote a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (require 'ox-ascii)
  (require 'ox-org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Multi Export\n")
    (insert "#+OPTIONS: toc:nil num:2 tags:t prop:t\n")
    (insert "#+MACRO: mark Mark-$1\n")
    (insert "* TODO Alpha :work:\n")
    (insert "SCHEDULED: <2026-05-27 Wed> DEADLINE: <2026-05-28 Thu>\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:CUSTOM_ID: alpha\n:END:\n")
    (insert "Paragraph {{{mark(ok)}}} with [[#alpha][self]] and [fn:a].\n")
    (insert "#+NAME: tbl\n")
    (insert "#+CAPTION: Table cap\n")
    (insert "| A | B |\n|---+---|\n| 1 | 2 |\n")
    (insert "#+begin_src emacs-lisp -n -r\n")
    (insert "(message \"hi\") ;; (ref:call)\n")
    (insert "#+end_src\n")
    (insert "** Hidden :noexport:\n")
    (insert "Hidden body.\n")
    (insert "* COMMENT Commented\n")
    (insert "Comment body.\n")
    (insert "[fn:a] Footnote body with /italic/.\n")
    (let* ((org-export-exclude-tags '("noexport"))
           (org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil))
           (ascii (org-export-as 'ascii nil nil t nil))
           (org-out (org-export-as 'org nil nil t
                                   '(:time-stamp-file nil)))
           (info (org-export-get-environment 'html nil nil))
           (tree (plist-get info :parse-tree))
           (links (org-element-map tree 'link #'identity))
           (table (car (org-export-collect-tables info)))
           (listing (car (org-export-collect-listings info)))
           (footnotes (org-export-collect-footnote-definitions info)))
      (list (mapcar #'substring-no-properties (plist-get info :title))
            (plist-get info :with-toc)
            (mapcar (lambda (h)
                      (list (org-element-property :raw-value h)
                            (org-export-get-relative-level h info)
                            (org-export-get-headline-number h info)
                            (org-export-get-tags h info)))
                    (org-export-collect-headlines info))
            (mapcar (lambda (link)
                      (let ((resolved (org-export-resolve-link link info)))
                        (list (org-element-property :raw-link link)
                              (org-element-type resolved)
                              (org-export-get-reference resolved info))))
                    links)
            (and table
                 (list (org-export-get-caption table)
                       (org-export-get-reference table info)
                       (org-export-get-ordinal table info)))
            (and listing
                 (list (org-export-get-reference listing info)
                       (org-export-resolve-coderef "call" info)))
            (mapcar (lambda (entry)
                      (let ((def (cdr entry)))
                        (list (car entry)
                              (org-element-property :label def))))
                    footnotes)
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle html))))
                    '("Mark-ok" "Footnote" "Table cap" "message"))
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle ascii))))
                    '("Mark-ok" "Footnote" "Table cap" "message"))
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle org-out))))
                    '("Mark-ok" ":Owner:" "SCHEDULED:" "Footnote"))
            (mapcar (lambda (needle)
                      (null (string-match-p needle html)))
                    '("Hidden body" "Comment body"))
            (replace-regexp-in-string
             "org[[:alnum:]]+"
             "org-id"
             html)
            ascii
            org-out))))"##,
        expect,
    );
}

#[test]
fn org_latex_export_entities_footnotes_special_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t t \"\\\\section{H}\\n\\\\label{sec:org-id}\\nText \\\\(\\\\alpha\\\\) and \\\\(\\\\rightarrow\\\\) with \\\\footnote{First footnote.}\\\\textsuperscript{,}\\\\,\\\\footnote{Second footnote with \\\\emph{italic}.}.\\n\\\\begin{tcolorbox}frametitle={Box}\\nInside \\\\textbf{bold} and \\\\(x^2\\\\).\\n\\\\end{tcolorbox}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-latex)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Latex Mix\n")
    (insert "* H\n")
    (insert "Text \\alpha and \\rightarrow with [fn:x][fn:y].\n")
    (insert "#+ATTR_LATEX: :options frametitle={Box}\n")
    (insert "#+begin_tcolorbox\n")
    (insert "Inside *bold* and $x^2$.\n")
    (insert "#+end_tcolorbox\n")
    (insert "[fn:x] First footnote.\n")
    (insert "[fn:y] Second footnote with /italic/.\n")
    (let* ((org-export-with-toc nil)
           (latex (org-export-as 'latex nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "sec:org[[:alnum:]]+"
             "sec:org-id"
             latex)))
      (list
       (not (null (string-match-p "\\\\alpha" latex)))
       (not (null (string-match-p "\\\\rightarrow" latex)))
       (not (null (string-match-p "\\\\footnote" latex)))
       (not (null (string-match-p "\\\\textsuperscript{,}" latex)))
       (not (null (string-match-p "\\\\begin{tcolorbox}" latex)))
       (not (null (string-match-p "frametitle={Box}" latex)))
       (not (null (string-match-p "\\\\textbf{bold}" latex)))
       normalized))))"##,
        expect,
    );
}

#[test]
fn org_export_data_entities_footnote_numbers_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument hash-table-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Data\n")
    (insert "* H\n")
    (insert "A \\beta ref [fn:1] and inline [fn::Inline note].\n")
    (insert "[fn:1] Defined note with [[https://example.org][url]].\n")
    (let* ((info (org-export-get-environment 'html nil nil))
           (tree (org-element-parse-buffer))
           (refs (org-element-map tree 'footnote-reference #'identity))
           (entities (org-element-map tree 'entity
                       (lambda (entity)
                         (list (org-element-property :name entity)
                               (org-element-property :html entity)
                               (org-element-property :latex entity)))))
           (numbers (mapcar
                     (lambda (ref)
                       (list (org-element-property :label ref)
                             (org-export-get-footnote-number ref info)
                             (org-export-footnote-first-reference-p ref info)))
                     refs))
           (rendered (mapcar
                      (lambda (ref)
                        (org-export-data ref info))
                      refs)))
      (list entities numbers rendered))))"##,
        expect,
    );
}

#[test]
fn org_export_filter_pipeline_order_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Filters\n")
    (insert "* Alpha\n")
    (insert "Plain [[https://example.org][link]] and /italic/ text.\n")
    (insert "#+begin_quote\nquoted text\n#+end_quote\n")
    (let (calls)
      (let ((org-export-filter-plain-text-functions
             (list (lambda (text backend info)
                     (push (list 'plain backend text) calls)
                     (replace-regexp-in-string "Plain" "PLAIN" text))))
            (org-export-filter-link-functions
             (list (lambda (text backend info)
                     (push (list 'link backend text) calls)
                     (concat text "<!--link-filter-->"))))
            (org-export-filter-headline-functions
             (list (lambda (text backend info)
                     (push (list 'headline backend
                                 (plist-get info :title)) calls)
                     text)))
            (org-export-filter-final-output-functions
             (list (lambda (text backend info)
                     (push (list 'final backend (length text)) calls)
                     (concat text "\n<!--final-filter-->")))))
        (let* ((org-export-with-toc nil)
               (html (org-export-as 'html nil nil t nil)))
          (list (mapcar (lambda (call)
                          (pcase call
                            (`(plain ,backend ,text)
                             (list 'plain backend
                                   (not (null (string-match-p "Plain" text)))))
                            (`(link ,backend ,text)
                             (list 'link backend
                                   (not (null (string-match-p "<a href" text)))))
                            (`(headline ,backend ,title)
                             (list 'headline backend title))
                            (`(final ,backend ,len)
                             (list 'final backend (numberp len)))))
                        (nreverse calls))
                (not (null (string-match-p "PLAIN" html)))
                (not (null (string-match-p "link-filter" html)))
                (not (null (string-match-p "final-filter" html)))
                (replace-regexp-in-string
                 "org[[:alnum:]]+"
                 "org-id"
                 html))))))"##,
        expect,
    );
}

#[test]
fn org_export_collect_options_references_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument hash-table-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Collect\n")
    (insert "#+OPTIONS: toc:nil num:2 tags:not-in-toc\n")
    (insert "* One :tag:\n")
    (insert "#+NAME: tbl\n")
    (insert "| A | B |\n|---+---|\n| 1 | 2 |\n")
    (insert "#+CAPTION: Table caption\n")
    (insert "[[tbl][Table link]] and <<target>> target link [[target]].\n")
    (insert "** Two\n")
    (insert "#+begin_src emacs-lisp -n -r\n")
    (insert "(message \"hi\") ;; (ref:msg)\n")
    (insert "#+end_src\n")
    (let* ((info (org-export-get-environment 'html nil '(:with-tags nil)))
           (headlines
            (mapcar (lambda (h)
                      (list (org-element-property :raw-value h)
                            (org-export-get-relative-level h info)
                            (org-export-get-headline-number h info)
                            (org-export-get-tags h info)))
                    (org-export-collect-headlines info)))
           (tables
            (mapcar (lambda (tbl)
                      (list (org-export-get-reference tbl info)
                            (org-export-get-caption tbl)
                            (org-export-get-ordinal tbl info)))
                    (org-export-collect-tables info)))
           (links
            (org-element-map (org-element-parse-buffer) 'link
              (lambda (link)
                (list (org-element-property :raw-link link)
                      (org-export-data link info))))))
      (list (plist-get info :title)
            (plist-get info :with-toc)
            (plist-get info :section-numbers)
            (plist-get info :with-tags)
            headlines
            tables
            links))))"##,
        expect,
    );
}

#[test]
fn org_export_derived_backend_transcoder_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable html)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (org-export-define-derived-backend
      'oracle-html 'html
    :translate-alist
    '((bold . (lambda (bold contents info)
                (format "<strong data-oracle=\"yes\">%s</strong>" contents)))
      (paragraph . (lambda (paragraph contents info)
                     (format "<p class=\"oracle-p\">%s</p>" contents)))))
    :filters-alist
    '((:filter-final-output
       . (lambda (output backend info)
           (concat output "\n<!--oracle-html-->")))))
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Derived\n")
    (insert "* H\n")
    (insert "Text *bold* and [[https://example.org][link]].\n")
    (let* ((org-export-with-toc nil)
           (out (org-export-as 'oracle-html nil nil t nil))
           (backend (org-export-get-backend 'oracle-html)))
      (list (not (null (memq 'oracle-html org-export-registered-backends)))
            (not (null (assq 'bold (org-export-get-all-transcoders backend))))
            (not (null (assq :filter-final-output
                             (org-export-get-all-filters backend))))
            (not (null (string-match-p "data-oracle" out)))
            (not (null (string-match-p "oracle-p" out)))
            (not (null (string-match-p "oracle-html" out)))
                 (replace-regexp-in-string
                  "org[[:alnum:]]+"
                  "org-id"
                  html))))))"##,
        expect,
    );
}

#[test]
fn org_export_headline_number_tags_todo_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 38 64)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Export State\n")
    (insert "#+OPTIONS: tags:t num:t todo:t\n\n")
    (insert "* TODO Alpha :work:\n")
    (insert "Alpha body.\n")
    (insert "** DONE Sub Alpha\n")
    (insert "Sub alpha body.\n")
    (insert "** WAIT Sub Alpha2\n")
    (insert "Sub alpha2 body.\n")
    (insert "* Beta :home:\n")
    (insert "Beta body.\n")
    (insert "** TODO Sub Beta\n")
    (insert "Sub beta body.\n")
    (let* ((org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil)))
      ;; Count specific patterns
      (let ((count-re (lambda (re)
                        (let ((c 0) (s 0))
                          (while (string-match re html s)
                            (setq s (match-end 0) c (1+ c)))
                          c))))
        (list (funcall count-re "TODO")
              (funcall count-re "DONE")
              (funcall count-re "WAIT")
              (funcall count-re "work")
              (funcall count-re "home")
              (funcall count-re "<h[1-3]")
              (funcall count-re "<li>")
              (not (null (string-match-p "Alpha" html)))
              (not (null (string-match-p "Beta" html)))
              (replace-regexp-in-string
               "sec:org[[:alnum:]-]+" "sec:org-id"
               (replace-regexp-in-string
                "org[[:alnum:]-]\\{8,\\}" "orgHASH" html))))))))"##,
        expect,
    );
}

#[test]
fn org_export_latex_math_table_footnote_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 63)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-latex)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Math Table\n\n")
    (insert "* Section\n")
    (insert "Formula $x^2 + y^2 = z^2$ inline.\n\n")
    (insert "Display equation:\n")
    (insert "\\[E = mc^2\\]\n\n")
    (insert "| A | B | Sum |\n")
    (insert "|---+---+-----|\n")
    (insert "| 1 | 2 | 3 |\n")
    (insert "| 4 | 5 | 9 |\n\n")
    (insert "Footnote[fn:1] reference.\n\n")
    (insert "[fn:1] Definition with $\\alpha$ math.\n")
    (let* ((org-export-with-toc nil)
           (latex (org-export-as 'latex nil nil t nil)))
      ;; Count LaTeX patterns
      (let ((count-re (lambda (re)
                        (let ((c 0) (s 0))
                          (while (string-match re latex s)
                            (setq s (match-end 0) c (1+ c)))
                          c))))
        (list (funcall count-re "\\\\section")
              (funcall count-re "\\\\begin{tabular}")
              (funcall count-re "\\\\footnote")
              (funcall count-re "\\$")
              (funcall count-re "\\\\alpha")
              (not (null (string-match-p "Math Table" latex)))
              (not (null (string-match-p "z\\^2" latex)))
              (not (null (string-match-p "E = mc" latex)))
              (replace-regexp-in-string
               "sec:org[[:alnum:]-]+" "sec:org-id" latex)))))))"##,
        expect,
    );
}

#[test]
fn org_export_options_tags_toc_num_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 50)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Options Test\n")
    (insert "#+OPTIONS: tags:nil toc:2 num:t\n")
    (insert "#+FILETAGS: :test:demo:\n\n")
    (insert "* Section One :alpha:\n")
    (insert "First section.\n\n")
    (insert "** Sub 1.1\n")
    (insert "Sub content.\n\n")
    (insert "*** Sub 1.1.1\n")
    (insert "Deep content.\n\n")
    (insert "* Section Two :beta:\n")
    (insert "Second section.\n\n")
    (let* ((html (org-export-as 'html nil nil t nil))
           ;; Check TOC depth
           (toc-h2-count
            (let ((c 0) (s 0))
              (while (string-match "<li><a href" html s)
                (setq s (match-end 0) c (1+ c)))
              c))
           ;; Check tags removed
           (has-tag (string-match-p "alpha" html))
           ;; Check numbering
           (has-num (string-match-p "1\\." html))
           ;; Check section content
           (has-section-one (string-match-p "Section One" html))
           (has-section-two (string-match-p "Section Two" html))
           ;; Check toc depth limit
           (toc-has-1-1 (string-match-p "Sub 1\\.1" html))
           (toc-has-1-1-1 (string-match-p "Sub 1\\.1\\.1" html)))
      (list toc-h2-count
            has-tag
            has-num
            has-section-one
            has-section-two
            toc-has-1-1
            toc-has-1-1-1
            (replace-regexp-in-string
             "sec:org[[:alnum:]-]+" "sec:org-id"
             (replace-regexp-in-string "org[[:alnum:]-]\\{8,\\}" "orgHASH"
                                       html)))))))"##,
        expect,
    );
}

#[test]
fn org_export_filter_babel_call_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 64)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (require 'ob-emacs-lisp)
  (let ((calls nil)
        (org-confirm-babel-evaluate nil))
    (org-export-define-derived-backend 'test-html 'html
      :translate-alist
      '((template . (lambda (contents info)
                      (push 'template calls)
                      (concat "<test>" contents "</test>")))))
    (with-temp-buffer
      (org-mode)
      (insert "#+TITLE: Filter Test\n\n")
      (insert "* Section\n")
      (insert "#+begin_src emacs-lisp :results value replace\n(+ 1 2)\n#+end_src\n\n")
      (insert "Paragraph.\n\n")
      (insert "#+begin_quote\nQuoted.\n#+end_quote\n")
      (let* ((html (org-export-as 'html nil nil t nil))
             (tree (org-element-parse-buffer))
             (src-blocks
              (org-element-map tree 'src-block
                (lambda (sb)
                  (list (org-element-property :language sb)
                        (org-element-property :value sb)))))
             (quotes
              (org-element-map tree 'quote-block
                (lambda (q)
                  (buffer-substring-no-properties
                   (org-element-property :contents-begin q)
                   (org-element-property :contents-end q)))))
             (html-has-section (string-match-p "Section" html))
             (html-has-quoted (string-match-p "Quoted" html))
             (html-has-src (string-match-p "src" html)))
        (list calls
              src-blocks
              quotes
              html-has-section
              html-has-quoted
              html-has-src
              (replace-regexp-in-string
               "org[[:alnum:]]+" "org-id"
               (replace-regexp-in-string
                "sec:org[[:alnum:]-]+" "sec:org-id" html))))))))"##,
        expect,
    );
}

#[test]
fn org_org_export_native_planning_macro_footnote_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((#(\"Native\" 0 6 (:parent (#(\"Native\" 0 6 (:parent #4)))))) t t t nil ((planning \"<2026-05-27 Wed 09:00>\" \"<2026-05-28 Thu>\") (drawer \"LOGBOOK\") (macro \"badge\" (\"ok\")) (link \"https://example.org\" \"https\" \"//example.org\") (footnote \"n\")) t t t t t \"#+macro: badge Badge-$1\\n* TODO Keep                                                            :work:\\nDEADLINE: <2026-05-28 Thu> SCHEDULED: <2026-05-27 Wed 09:00>\\n:PROPERTIES:\\n:Owner:    Ada\\n:Score:    7\\n:END:\\n:LOGBOOK:\\n- State \\\"TODO\\\" from \\\"\\\" [2026-05-26 Tue]\\n:END:\\nParagraph Badge-ok with [[https://example.org][link]] and footnote[fn:n].\\n#+begin_quote\\nQuoted *bold* text.\\n#+end_quote\\n\\n[fn:n] Note body with /italic/.\\n\")""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'ox-org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Native\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "#+OPTIONS: toc:nil num:nil tags:t prop:t\n")
    (insert "#+MACRO: badge Badge-$1\n")
    (insert "* TODO Keep :work:\n")
    (insert "SCHEDULED: <2026-05-27 Wed 09:00> DEADLINE: <2026-05-28 Thu>\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:Score: 7\n:END:\n")
    (insert ":LOGBOOK:\n")
    (insert "- State \"TODO\" from \"\" [2026-05-26 Tue]\n")
    (insert ":END:\n")
    (insert "Paragraph {{{badge(ok)}}} with [[https://example.org][link]] ")
    (insert "and footnote[fn:n].\n")
    (insert "#+begin_quote\nQuoted *bold* text.\n#+end_quote\n")
    (insert "[fn:n] Note body with /italic/.\n")
    (insert "** Hidden :noexport:\n")
    (insert "Should not appear.\n")
    (insert "* COMMENT Commented\n")
    (insert "Should not appear either.\n")
    (let* ((org-export-exclude-tags '("noexport"))
           (org-export-with-toc nil)
           (org-export-with-properties t)
           (org-export-with-drawers t)
           (org-export-with-planning t)
           (out (org-export-as 'org nil nil t nil))
           (env (org-export-get-environment 'org nil nil))
           (tree (org-element-parse-buffer)))
      (list
       (plist-get env :title)
       (plist-get env :with-properties)
       (plist-get env :with-drawers)
       (plist-get env :with-planning)
       (mapcar (lambda (h)
                 (list (org-element-property :raw-value h)
                       (org-export-get-relative-level h env)
                       (org-export-get-tags h env)))
               (org-export-collect-headlines env))
       (org-element-map tree '(macro footnote-reference link planning drawer)
         (lambda (el)
           (pcase (org-element-type el)
             ('macro (list 'macro
                           (org-element-property :key el)
                           (org-element-property :args el)))
             ('footnote-reference
              (list 'footnote
                    (org-element-property :label el)))
             ('link (list 'link
                          (org-element-property :raw-link el)
                          (org-element-property :type el)
                          (org-element-property :path el)))
             ('planning
              (list 'planning
                    (and (org-element-property :scheduled el)
                         (org-element-property
                          :raw-value
                          (org-element-property :scheduled el)))
                    (and (org-element-property :deadline el)
                         (org-element-property
                          :raw-value
                          (org-element-property :deadline el)))))
             ('drawer (list 'drawer
                            (org-element-property :drawer-name el))))))
       (not (null (string-match-p "Badge-ok" out)))
       (not (null (string-match-p ":Owner:" out)))
       (not (null (string-match-p "SCHEDULED:" out)))
       (not (null (string-match-p "LOGBOOK" out)))
       (null (string-match-p "Should not appear" out))
       out))))"##,
        expect,
    );
}

#[test]
fn org_export_hooks_parse_tree_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Hooked\n")
    (insert "#+MACRO: wrap Before-$1\n")
    (insert "* Keep\n")
    (insert "First paragraph with {{{wrap(macro)}}}.\n")
    (insert "#+begin_comment\n")
    (insert "comment should vanish\n")
    (insert "#+end_comment\n")
    (insert "#+begin_export html\n")
    (insert "<strong>raw html</strong>\n")
    (insert "#+end_export\n")
    (insert "* Drop :noexport:\n")
    (insert "Dropped paragraph.\n")
    (let ((calls nil))
      (let ((org-export-before-processing-functions
             (list (lambda (backend)
                     (push (list 'processing backend
                                 (buffer-substring-no-properties
                                  (point-min) (line-end-position)))
                           calls)
                     (goto-char (point-max))
                     (insert "* Added\n")
                     (insert "Added paragraph with [[https://example.org][link]].\n"))))
            (org-export-before-parsing-functions
             (list (lambda (backend)
                     (push (list 'parsing backend
                                 (buffer-substring-no-properties
                                  (point-min) (line-end-position)))
                           calls)
                     (goto-char (point-min))
                     (search-forward "First paragraph")
                     (end-of-line)
                     (insert "\nSecond paragraph inserted before parsing.\n"))))
            (org-export-filter-options-functions
             (list (lambda (info backend)
                     (push (list 'options backend
                                 (plist-get info :title)
                                 (plist-get info :with-toc))
                           calls)
                     (plist-put info :with-toc nil))))
            (org-export-filter-parse-tree-functions
             (list (lambda (tree backend info)
                     (push
                      (list 'tree backend
                            (mapcar
                             (lambda (h)
                               (org-element-property :raw-value h))
                             (org-element-map tree 'headline #'identity))
                            (length (plist-get info :ignore-list)))
                      calls)
                     tree))))
        (let* ((org-export-exclude-tags '("noexport"))
               (html (org-export-as 'html nil nil t nil))
               (info (org-export-get-environment 'html nil
                                                 '(:with-toc nil)))
               (tree (plist-get info :parse-tree))
               (paragraphs
                (org-element-map tree 'paragraph #'identity))
               (first-p (car paragraphs))
               (second-p (cadr paragraphs))
               (link (car (org-element-map tree 'link #'identity))))
          (list (nreverse calls)
                (mapcar
                 (lambda (p)
                   (list (org-element-type
                          (org-export-get-previous-element p info))
                         (org-element-type
                          (org-export-get-next-element p info))
                         (org-element-type
                          (org-export-get-parent-headline p))))
                 paragraphs)
                (and link
                     (list (org-element-property :raw-link link)
                           (org-element-property
                            :raw-value
                            (org-export-get-parent-headline link))
                           (org-element-type
                            (org-export-get-previous-element link info t))))
                (org-export-get-category first-p info)
                (org-export-get-category second-p info)
                (not (null (string-match-p "Before-macro" html)))
                (not (null (string-match-p "Second paragraph" html)))
                (not (null (string-match-p "Added paragraph" html)))
                (not (null (string-match-p "raw html" html)))
                (null (string-match-p "Dropped paragraph" html))
                (null (string-match-p "comment should vanish" html))
                (replace-regexp-in-string
                 "org[[:alnum:]]+"
                 "org-id"
                 html))))))"##,
        expect,
    );
}
