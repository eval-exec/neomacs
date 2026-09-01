use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_entities_user_table_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (require 'ox-html)
  (require 'ox-ascii)
  (require 'ox-latex)
  (let ((org-entities-user
         '(("myarrow" "\\Rightarrow" t "&rArr;" "=>" "=>" "⇒")
           ("brand" "\\textsc{Neo}" nil "<span>Neo</span>" "Neo" "Neo" "Neo"))))
    (with-temp-buffer
      (org-mode)
      (insert "#+TITLE: Entities\n")
      (insert "* H\n")
      (insert "A \\myarrow B and \\brand{} plus builtins \\alpha \\ndash.\n")
      (let* ((table (org-entities-create-table))
             (picked (mapcar (lambda (name)
                               (cdr (assoc name table)))
                             '("myarrow" "brand" "alpha" "ndash")))
             (tree (org-element-parse-buffer))
             (entities (org-element-map tree 'entity
                         (lambda (e)
                           (list (org-element-property :name e)
                                 (org-element-property :latex e)
                                 (org-element-property :latex-math-p e)
                                 (org-element-property :html e)
                                 (org-element-property :ascii e)
                                 (org-element-property :utf-8 e)))))
             (html (org-export-as 'html nil nil t '(:with-toc nil)))
             (ascii (let ((org-ascii-charset 'ascii))
                      (org-export-as 'ascii nil nil t '(:with-toc nil))))
             (utf8 (let ((org-ascii-charset 'utf-8))
                     (org-export-as 'ascii nil nil t '(:with-toc nil))))
             (latex (org-export-as 'latex nil nil t '(:with-toc nil))))
        (list picked
              entities
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle html))))
                      '("&rArr;" "<span>Neo</span>" "&alpha;" "&ndash;"))
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle ascii))))
                      '("=>" "Neo" "alpha" "-"))
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle utf8))))
                      '("⇒" "Neo" "α" "–"))
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle latex))))
                      '("\\\\Rightarrow" "\\\\textsc{Neo}" "\\\\alpha" "--"))))))"##,
        expect,
    );
}

#[test]
fn org_subsuperscript_parse_export_modes_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((t ((subscript nil \"_2O \") (superscript t \"^{2+y} \") (subscript nil \"_b \") (subscript t \"_{d e} \") (subscript nil \"_underscore \") (subscript nil \"_a\")) \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> H</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\nH<sub>2O</sub> x<sup>2+y</sup> a<sub>b</sub> c<sub>d e</sub> raw<sub>underscore</sub> email<sub>a</sub>@b.\\n</p>\\n</div>\\n</div>\\n\" \"1 H\\n═══\\n\\n  H_2O x^{2+y} a_b c_{d e} raw_underscore email_a@b.\\n\" \"\\\\section{H}\\n\\\\label{sec:org-id}\\nH\\\\textsubscript{2O} x\\\\textsuperscript{2+y} a\\\\textsubscript{b} c\\\\textsubscript{d e} raw\\\\textsubscript{underscore} email\\\\textsubscript{a}@b.\\n\") ({} ((subscript nil \"_2O \") (superscript t \"^{2+y} \") (subscript nil \"_b \") (subscript t \"_{d e} \") (subscript nil \"_underscore \") (subscript nil \"_a\")) \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> H</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\nH<sub>2O</sub> x<sup>2+y</sup> a<sub>b</sub> c<sub>d e</sub> raw<sub>underscore</sub> email<sub>a</sub>@b.\\n</p>\\n</div>\\n</div>\\n\" \"1 H\\n═══\\n\\n  H_2O x^{2+y} a_b c_{d e} raw_underscore email_a@b.\\n\" \"\\\\section{H}\\n\\\\label{sec:org-id}\\nH\\\\textsubscript{2O} x\\\\textsuperscript{2+y} a\\\\textsubscript{b} c\\\\textsubscript{d e} raw\\\\textsubscript{underscore} email\\\\textsubscript{a}@b.\\n\") (nil ((subscript nil \"_2O \") (superscript t \"^{2+y} \") (subscript nil \"_b \") (subscript t \"_{d e} \") (subscript nil \"_underscore \") (subscript nil \"_a\")) \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> H</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\nH<sub>2O</sub> x<sup>2+y</sup> a<sub>b</sub> c<sub>d e</sub> raw<sub>underscore</sub> email<sub>a</sub>@b.\\n</p>\\n</div>\\n</div>\\n\" \"1 H\\n═══\\n\\n  H_2O x^{2+y} a_b c_{d e} raw_underscore email_a@b.\\n\" \"\\\\section{H}\\n\\\\label{sec:org-id}\\nH\\\\textsubscript{2O} x\\\\textsuperscript{2+y} a\\\\textsubscript{b} c\\\\textsubscript{d e} raw\\\\textsubscript{underscore} email\\\\textsubscript{a}@b.\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (require 'ox-ascii)
  (require 'ox-latex)
  (let ((sample "* H\nH_2O x^{2+y} a_b c_{d e} raw_underscore email_a@b.\n"))
    (mapcar
     (lambda (mode)
       (with-temp-buffer
         (let ((org-use-sub-superscripts mode))
           (org-mode)
           (insert sample)
           (let* ((tree (org-element-parse-buffer))
                  (objects (org-element-map tree '(subscript superscript)
                             (lambda (o)
                               (list (org-element-type o)
                                     (org-element-property :use-brackets-p o)
                                     (buffer-substring-no-properties
                                      (org-element-property :begin o)
                                      (org-element-property :end o)))))))
             (list mode
                   objects
                   (replace-regexp-in-string
                    "org[[:alnum:]]+"
                    "org-id"
                    (org-export-as 'html nil nil t '(:with-toc nil)))
                   (let ((org-ascii-charset 'utf-8))
                     (org-export-as 'ascii nil nil t '(:with-toc nil)))
                   (replace-regexp-in-string
                    "sec:org[[:alnum:]]+"
                    "sec:org-id"
                    (org-export-as 'latex nil nil t '(:with-toc nil))))))))
     '(t {} nil))))"##,
        expect,
    );
}

#[test]
fn org_pretty_entities_fontify_display_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (with-temp-buffer
    (let ((org-pretty-entities t)
          (org-entities-user
           '(("myheart" "\\heartsuit" t "&hearts;" "<3" "<3" "♥"))))
      (org-mode)
      (insert "Alpha \\alpha, arrow \\rightarrow, custom \\myheart{}, escaped \\\\alpha.\n")
      (font-lock-ensure (point-min) (point-max))
      (let (before after)
        (goto-char (point-min))
        (while (re-search-forward "\\\\\\([A-Za-z]+\\)\\({}\\)?" nil t)
          (push (list (match-string 0)
                      (get-text-property (match-beginning 0) 'display)
                      (get-text-property (match-beginning 0) 'composition)
                      (get-text-property (match-beginning 0) 'face))
                before))
        (org-toggle-pretty-entities)
        (font-lock-ensure (point-min) (point-max))
        (goto-char (point-min))
        (while (re-search-forward "\\\\\\([A-Za-z]+\\)\\({}\\)?" nil t)
          (push (list (match-string 0)
                      (get-text-property (match-beginning 0) 'display)
                      (get-text-property (match-beginning 0) 'composition)
                      (get-text-property (match-beginning 0) 'face))
                after))
        (list (nreverse before)
              (nreverse after)
              org-pretty-entities
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_markup_entities_fontlock_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (require 'ox-html)
  (require 'ox-ascii)
  (require 'ox-latex)
  (with-temp-buffer
    (let ((org-hide-emphasis-markers t)
          (org-pretty-entities t)
          (org-export-with-broken-links t)
          (org-entities-user
           '(("brandx" "\\mathsf{X}" t "&Xopf;" "X" "X" "𝕏"))))
      (org-mode)
      (insert "#+TITLE: Mixed Markup\n")
      (insert "* H\n")
      (insert "/italic \\alpha/ and *bold \\beta* and _under \\gamma_.\n")
      (insert "=code \\delta= plus ~verb \\epsilon~ and +strike \\ndash+.\n")
      (insert "Nested link [[https://example.org/a_b?x=1][site \\rightarrow *not-bold*]] and custom \\brandx{}.\n")
      (font-lock-ensure (point-min) (point-max))
      (let* ((tree (org-element-parse-buffer))
             (objects
              (org-element-map tree
                  '(italic bold underline code verbatim strike-through entity link)
                (lambda (object)
                  (let ((type (org-element-type object)))
                    (list type
                          (cond
                           ((eq type 'entity)
                            (org-element-property :name object))
                           ((eq type 'link)
                            (list (org-element-property :type object)
                                  (org-element-property :path object)
                                  (buffer-substring-no-properties
                                   (org-element-property :contents-begin object)
                                   (org-element-property :contents-end object))))
                           (t
                            (buffer-substring-no-properties
                             (org-element-property :begin object)
                             (org-element-property :end object)))))))))
             (props
              (mapcar
               (lambda (needle)
                 (save-excursion
                   (goto-char (point-min))
                   (search-forward needle)
                   (list needle
                         (get-text-property (match-beginning 0) 'invisible)
                         (get-text-property (match-beginning 0) 'face)
                         (get-text-property (match-beginning 0) 'display)
                         (get-text-property (match-beginning 0) 'composition)
                         (get-text-property (match-beginning 0) 'org-emphasis))))
               '("/" "italic" "\\alpha" "*" "bold" "\\beta" "_"
                 "under" "=" "code" "~" "verb" "+" "strike"
                 "\\ndash" "\\brandx")))
             (html (org-export-as 'html nil nil t '(:with-toc nil)))
             (ascii (let ((org-ascii-charset 'ascii))
                      (org-export-as 'ascii nil nil t '(:with-toc nil))))
             (utf8 (let ((org-ascii-charset 'utf-8))
                     (org-export-as 'ascii nil nil t '(:with-toc nil))))
             (latex (replace-regexp-in-string
                     "sec:org[[:alnum:]]+"
                     "sec:org-id"
                     (org-export-as 'latex nil nil t '(:with-toc nil)))))
        (list objects
              props
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle html))))
                      '("<i>italic" "<b>bold" "<span class=\"underline\">under"
                        "<code>code" "<code>verb" "<del>strike"
                        "&alpha;" "&beta;" "&gamma;" "&ndash;" "&Xopf;"))
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle ascii))))
                      '("italic alpha" "bold beta" "_under gamma_"
                        "=code delta=" "~verb epsilon~" "+strike -+"
                        "site ->" "X"))
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle utf8))))
                      '("italic α" "bold β" "_under γ_"
                        "=code δ=" "~verb ε~" "+strike –+"
                        "site →" "𝕏"))
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle latex))))
                      '("\\\\emph{italic" "\\\\textbf{bold"
                        "\\\\uline{under" "\\\\texttt{code"
                        "\\\\texttt{verb" "\\\\sout{strike"
                        "\\\\alpha" "\\\\beta" "\\\\gamma"
                        "--" "\\\\mathsf{X}"))))))"##,
        expect,
    );
}

#[test]
fn org_emphasize_region_replace_remove_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function prop-at)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (require 'ox-ascii)
  (with-temp-buffer
    (let ((org-hide-emphasis-markers t)
          (org-pretty-entities t))
      (org-mode)
      (insert "#+TITLE: Emphasize\n")
      (insert "* H\n")
      (insert "Alpha beta gamma \\alpha.\n")
      (insert "Second line with words.\n")
      (cl-labels
          ((mark-word
            (word)
            (goto-char (point-min))
            (search-forward word)
            (let ((beg (match-beginning 0))
                  (end (match-end 0)))
              (goto-char beg)
              (push-mark end t t)
              (setq mark-active t
                    transient-mark-mode t))))
           (prop-at
            (needle)
            (save-excursion
              (goto-char (point-min))
              (search-forward needle)
              (list needle
                    (get-text-property (match-beginning 0) 'face)
                    (get-text-property (match-beginning 0) 'invisible)
                    (get-text-property (match-beginning 0) 'display)
                    (get-text-property (match-beginning 0) 'org-emphasis)))))
        (mark-word "beta")
        (org-emphasize ?*)
        (let ((after-bold
               (buffer-substring-no-properties (point-min) (point-max))))
          (mark-word "beta")
          (org-emphasize ?/)
          (let ((after-replace
                 (buffer-substring-no-properties (point-min) (point-max))))
            (mark-word "beta")
            (org-emphasize ?\s)
            (let ((after-remove
                   (buffer-substring-no-properties (point-min) (point-max))))
              (goto-char (point-min))
              (search-forward "gamma")
              (end-of-line)
              (org-emphasize ?=)
              (insert "code")
              (forward-char)
              (font-lock-ensure (point-min) (point-max))
              (let* ((tree (org-element-parse-buffer))
                     (objects
                      (org-element-map tree '(bold italic code entity)
                        (lambda (object)
                          (list (org-element-type object)
                                (buffer-substring-no-properties
                                 (org-element-property :begin object)
                                 (org-element-property :end object))))))
                     (props (mapcar #'prop-at
                                    '("Alpha" "beta" "\\alpha" "=code=")))
                     (html (org-export-as 'html nil nil t '(:with-toc nil)))
                     (ascii (let ((org-ascii-charset 'utf-8))
                              (org-export-as 'ascii nil nil t
                                             '(:with-toc nil)))))
                (list after-bold
                      after-replace
                      after-remove
                      objects
                      props
                      (mapcar (lambda (needle)
                                (not (null (string-match-p needle html))))
                              '("Alpha beta gamma" "<code>code</code>"
                                "&alpha;"))
                      (mapcar (lambda (needle)
                                (not (null (string-match-p needle ascii))))
                              '("Alpha beta gamma" "=code=" "α"))
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_entity_latex_utf8_html_parse_element_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((bold \"*bold*\" (bold)) (italic \"/italic/\" (italic)) (underline \"_underline_\" (underline)) (verbatim \"=code=\" (org-verbatim)) (code \"~verbatim~\" (org-code)) (strike-through \"+strike+\" ((:strike-through t))) (entity \"\\\\alpha \" nil) (entity \"\\\\beta \" nil) (entity \"\\\\gamma \" nil) (entity \"\\\\rightarrow \" nil) (entity \"\\\\nbsp \" nil) (entity \"\\\\copy \" nil) (entity \"\\\\infty\" nil) (subscript \"_2O \" nil) (superscript \"^2 \" nil) (subscript \"_{n+1} \" nil) (superscript \"^{b+c}\" nil) (bold \"*/bold italic/* \" (bold)) (italic \"/bold italic/\" (bold)) (bold \"*_bold underline_* \" (bold)) (underline \"_bold underline_\" (bold)) (code \"~=code verbatim=~\" (org-code))) ((\"alpha\" \"\\\\alpha\" \"&alpha;\" \"α\") (\"beta\" \"\\\\beta\" \"&beta;\" \"β\") (\"gamma\" \"\\\\gamma\" \"&gamma;\" \"γ\") (\"rightarrow\" \"\\\\rightarrow\" \"&rarr;\" \"→\") (\"nbsp\" \"~\" \"&nbsp;\" \"\u{a0}\") (\"copy\" \"\\\\textcopyright{}\" \"&copy;\" \"©\") (\"infty\" \"\\\\infty\" \"&infin;\" \"∞\")) (t t t t t nil t t) (t nil t t t t t t) \"* Markup Section\\nText with *bold*, /italic/, _underline_, =code=, ~verbatim~, and +strike+.\\n\\nEntities: \\\\alpha \\\\beta \\\\gamma \\\\rightarrow \\\\nbsp \\\\copy \\\\infty\\n\\nSub/super: H_2O and E=mc^2 and x_{n+1} and a^{b+c}\\n\\nNested: */bold italic/* and *_bold underline_* and ~=code verbatim=~\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (with-temp-buffer
    (org-mode)
    (insert "* Markup Section\n")
    (insert "Text with *bold*, /italic/, _underline_, =code=, ~verbatim~, and +strike+.\n\n")
    (insert "Entities: \\alpha \\beta \\gamma \\rightarrow \\nbsp \\copy \\infty\n\n")
    (insert "Sub/super: H_2O and E=mc^2 and x_{n+1} and a^{b+c}\n\n")
    (insert "Nested: */bold italic/* and *_bold underline_* and ~=code verbatim=~\n\n")
    (font-lock-ensure (point-min) (point-max))
    ;; Parse element types and content
    (let* ((tree (org-element-parse-buffer))
           (objects
            (org-element-map tree '(bold italic underline code verbatim strike-through entity subscript superscript)
              (lambda (obj)
                (list (org-element-type obj)
                      (buffer-substring-no-properties
                       (org-element-property :begin obj)
                       (org-element-property :end obj))
                      (get-text-property (org-element-property :begin obj) 'face)))))
           ;; Entity lookup
           (entity-vals
            (mapcar (lambda (name)
                      (let ((entry (assoc name org-entities)))
                        (list name
                              (nth 1 entry)  ; latex
                              (nth 3 entry)  ; html
                              (nth 6 entry)))) ; utf-8
                    '("alpha" "beta" "gamma" "rightarrow" "nbsp" "copy" "infty")))
           ;; Export
           (html (org-export-as 'html nil nil t '(:with-toc nil)))
           (latex (org-export-as 'latex nil nil t '(:with-toc nil)))
           ;; Check HTML patterns
           (html-patterns
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle html))))
                    '("<b>bold</b>" "<i>italic</i>" "<code>code</code>"
                      "&alpha;" "&beta;" "&infty;" "<sub>" "<sup>")))
           ;; Check LaTeX patterns
           (latex-patterns
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle latex))))
                    '("\\\\textbf" "\\\\textit" "\\\\texttt"
                      "\\\\alpha" "\\\\beta" "\\\\infty"
                      "\\\\textsubscript" "\\\\textsuperscript"))))
      (list objects
            entity-vals
            html-patterns
            latex-patterns
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_entity_markup_edit_reexport_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((entity nil nil) (entity nil nil) (entity nil nil) (entity nil nil) (entity nil nil) (entity nil nil) (bold nil \"bold\") (italic nil \"italic\") (underline nil \"underline\") (strike-through nil \"strike\") (subscript nil \"2\") (superscript nil \"2\")) ((\"alpha\" \"\\\\alpha\" \"&alpha;\" \"α\") (\"beta\" \"\\\\beta\" \"&beta;\" \"β\") (\"gamma\" \"\\\\gamma\" \"&gamma;\" \"γ\") (\"infty\" \"\\\\infty\" \"&infin;\" \"∞\") (\"pm\" \"\\\\textpm{}\" \"&plusmn;\" \"±\") (\"times\" \"\\\\texttimes{}\" \"&times;\" \"×\")) (t t t nil nil t t t nil nil t t) (t nil t t t t t nil t nil t t) \"* Test Entities\\n\\nGreek: \\\\alpha \\\\beta \\\\gamma\\n\\nMath: \\\\infty \\\\pm \\\\times\\n\\nMarkup: *bold* /italic/ _underline_ +strike+\\n\\nSub/super: H_{2}O E=mc^{2}\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (require 'ox-html)
  (require 'ox-latex)
  (with-temp-buffer
    (org-mode)
    (insert "* Test Entities\n\n")
    (insert "Greek: \\alpha \\beta \\gamma\n\n")
    (insert "Math: \\infty \\pm \\times\n\n")
    (insert "Markup: *bold* /italic/ _underline_ +strike+\n\n")
    (insert "Sub/super: H_{2}O E=mc^{2}\n\n")
    ;; Parse objects
    (let* ((tree (org-element-parse-buffer))
           (objects
            (org-element-map tree '(bold italic underline strike-through
                                         superscript subscript entity)
              (lambda (obj)
                (list (org-element-type obj)
                      (org-element-property :value obj)
                      (and (org-element-contents-begin obj)
                           (buffer-substring-no-properties
                            (org-element-contents-begin obj)
                            (org-element-contents-end obj)))))))
           ;; Entity values
           (entity-vals
            (mapcar (lambda (name)
                      (let ((entry (assoc name org-entities)))
                        (list name
                              (nth 1 entry)  ; latex
                              (nth 3 entry)  ; html
                              (nth 6 entry)))) ; utf-8
                    '("alpha" "beta" "gamma" "infty" "pm" "times")))
           ;; Export HTML
           (html (org-export-as 'html nil nil t '(:with-toc nil)))
           ;; Export LaTeX
           (latex (org-export-as 'latex nil nil t '(:with-toc nil)))
           ;; Check patterns
           (html-checks
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle html))))
                    '("&alpha;" "&beta;" "&gamma;" "&infty;" "&pm;" "&times;"
                      "<b>" "<i>" "<u>" "<s>" "<sub>" "<sup>")))
           (latex-checks
            (mapcar (lambda (needle)
                      (not (null (string-match-p needle latex))))
                    '("\\alpha" "\\beta" "\\gamma" "\\infty" "\\pm" "\\times"
                      "\\textbf" "\\textit" "\\underline" "\\sout"
                      "\\textsubscript" "\\textsuperscript"))))
      (list objects entity-vals html-checks latex-checks
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}
