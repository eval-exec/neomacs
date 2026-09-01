use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_builtin_link_export_backends_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"<a href=\\\"https://doi.org/10.1000/xyz123\\\">Paper</a>\" \"\\\\href{https://doi.org/10.1000/xyz123}{Paper}\" \"<https://doi.org/10.1000/xyz123>\" \"<a target=\\\"_blank\\\" href=\\\"http://man.he.net/?topic=printf(3)&section=all\\\">Printf</a>\" \"\\\\href{http://man.he.net/?topic=printf(3)&section=all}{printf(3)}\" \"[Printf] (<http://man.he.net/?topic=printf(3)&section=all>)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'ol-doi)
  (require 'ol-man)
  (list
   (org-link-doi-export "10.1000/xyz123" "Paper" 'html nil)
   (org-link-doi-export "10.1000/xyz123" "Paper" 'latex nil)
   (org-link-doi-export "10.1000/xyz123" nil 'ascii nil)
   (org-man-export "printf(3)" "Printf" 'html)
   (org-man-export "printf(3)" nil 'latex)
   (org-man-export "printf(3)" "Printf" 'ascii)))"#,
        expect,
    );
}

#[test]
fn org_man_export_sections_markup_links_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t \".SH \\\"NAME\\\"\\n.PP\\ndemo - short description\\n.SH \\\"SYNOPSIS\\\"\\n.PP\\n\\\\fIdemo \\\\-\\\\-help\\\\fP\\n.SH \\\"DESCRIPTION\\\"\\n.PP\\nText with \\\\fBbold\\\\fP, \\\\fIitalic\\\\fP, and https://example.org \\\\fBat\\\\fP \\\\fIlink\\\\fP.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-man)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Demo Manual\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "* NAME\n")
    (insert "demo - short description\n")
    (insert "* SYNOPSIS\n")
    (insert "=demo --help=\n")
    (insert "* DESCRIPTION\n")
    (insert "Text with *bold*, /italic/, and [[https://example.org][link]].\n")
    (let* ((org-export-with-toc nil)
           (man (org-export-as 'man nil nil t nil)))
      (list (not (null (string-match-p "\\.SH \"NAME\"" man)))
            (not (null (string-match-p "\\.SH \"SYNOPSIS\"" man)))
            (not (null (string-match-p "\\\\fBbold\\\\fP" man)))
            (not (null (string-match-p "\\\\fIitalic\\\\fP" man)))
            (not (null (string-match-p "\\\\fIdemo \\\\-\\\\-help\\\\fP" man)))
            (not (null (string-match-p "https://example.org" man)))
            man))))"##,
        expect,
    );
}

#[test]
fn org_link_abbrev_radio_custom_reveal_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (require 'ox-html)
  (require 'ox-ascii)
  (require 'org-fold)
  (let ((followed nil))
    (org-link-set-parameters
     "neomacs-ticket"
     :follow (lambda (path arg) (push (list path arg) followed))
     :export (lambda (path desc backend _info)
               (pcase backend
                 ('html (format "<span class=\"ticket\">%s:%s</span>"
                                (or desc "ticket") path))
                 ('ascii (format "%s<%s>" (or desc "ticket") path))
                 (_ (format "%s:%s" (or desc "ticket") path)))))
    (with-temp-buffer
      (let ((org-link-abbrev-alist
             '(("gh" . "https://github.com/%s")
               ("rfc" . "https://www.rfc-editor.org/rfc/rfc%s.txt")))
            (org-link-descriptive t)
            (org-export-with-broken-links t)
            (org-hide-emphasis-markers t))
        (org-mode)
        (insert "#+TITLE: Link Matrix\n")
        (insert "* Intro\n")
        (insert "Radio target <<<Release Plan>>> appears here.\n")
        (insert "Links: [[neomacs-ticket:ABC-42][Ticket *ABC*]] ")
        (insert "[[gh:eval-exec/neomacs][repo]] [[rfc:9110][RFC 9110]] ")
        (insert "[[*Target Heading][jump target]] and Release Plan reference.\n")
        (insert "* Target Heading\n")
        (insert ":PROPERTIES:\n:CUSTOM_ID: target-heading\n:END:\n")
        (insert "Hidden body with [[neomacs-ticket:XYZ-7]] and /markup/.\n")
        (font-lock-ensure (point-min) (point-max))
        (let ((snapshot
               (lambda (label)
                 (list label
                       (point)
                       (org-element-map (org-element-parse-buffer) 'link
                         (lambda (link)
                           (list (org-element-property :type link)
                                 (org-element-property :path link)
                                 (and (org-element-property :contents-begin link)
                                      (buffer-substring-no-properties
                                       (org-element-property :contents-begin link)
                                       (org-element-property :contents-end link)))
                                 (org-element-property :begin link)
                                 (org-element-property :end link))))
                       (mapcar
                        (lambda (needle)
                          (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (list needle
                                  (line-number-at-pos)
                                  (invisible-p (point))
                                  (get-text-property
                                   (match-beginning 0) 'face)
                                  (get-text-property
                                   (match-beginning 0) 'mouse-face)
                                  (get-text-property
                                   (match-beginning 0) 'help-echo)
                                  (keymapp
                                   (get-text-property
                                    (match-beginning 0) 'keymap)))))
                        '("Ticket" "repo" "RFC 9110" "jump target"
                          "Release Plan" "Hidden body" "XYZ-7"))
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))
          (let (states)
            (push (funcall snapshot 'initial) states)
            (goto-char (point-min))
            (search-forward "Target Heading")
            (beginning-of-line)
            (org-fold-hide-subtree)
            (push (funcall snapshot 'hidden-target) states)
            (goto-char (point-min))
            (search-forward "Ticket")
            (org-open-at-point)
            (push (funcall snapshot 'after-custom-open) states)
            (goto-char (point-min))
            (search-forward "jump target")
            (org-open-at-point)
            (push (funcall snapshot 'after-heading-open) states)
            (let* ((html (org-export-as 'html nil nil t '(:with-toc nil)))
                   (ascii (let ((org-ascii-charset 'utf-8))
                            (org-export-as 'ascii nil nil t
                                           '(:with-toc nil)))))
              (list (nreverse states)
                    (nreverse followed)
                    (org-link-expand-abbrev "gh:eval-exec/neomacs")
                    (org-link-expand-abbrev "rfc:9110")
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle html))))
                            '("class=\"ticket\"" "ABC-42"
                              "https://github.com/eval-exec/neomacs"
                              "rfc9110.txt" "id=\"target-heading\""
                              "Release Plan"))
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle ascii))))
                            '("Ticket \\*ABC\\*<ABC-42>"
                              "https://github.com/eval-exec/neomacs"
                              "RFC 9110" "Release Plan"))
                    (org-get-heading t t t t)
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_link_export_html_latex_structure_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Org export aborted.  Unable to resolve link: \\\"No match for fuzzy expression: *Heading\\\"\\nSee ‘org-export-with-broken-links’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (require 'ox-latex)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Link Export\n\n")
    (insert "* Section\n")
    (insert "Plain link: https://example.org/path?q=1\n\n")
    (insert "Named link: [[https://example.org][Example Site]]\n\n")
    (insert "File link: [[file:/tmp/test.org::*Heading][File Ref]]\n\n")
    (insert "Radio: <<<radio-target>>> elsewhere\n\n")
    (insert "Angle link: <https://angle.example.org>\n\n")
    (let* ((org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil))
           (latex (org-export-as 'latex nil nil t nil))
           ;; Extract all links from HTML
           (html-links nil)
           (s 0)
           (_ (while (string-match "href=\"\\([^\"]+\\)\"" html s)
                (push (match-string 1 html) html-links)
                (setq s (match-end 0))))
           ;; Extract all links from LaTeX
           (latex-links nil)
           (s2 0)
           (_ (while (string-match "\\\\\\(?:href\\|url\\){\\([^}]+\\)}" latex s2)
                (push (match-string 1 latex) latex-links)
                (setq s2 (match-end 0))))
           ;; Check specific patterns
           (html-has-plain (string-match-p "https://example.org/path" html))
           (html-has-named (string-match-p "Example Site" html))
           (html-has-angle (string-match-p "https://angle.example.org" html))
           (latex-has-plain (string-match-p "https://example.org/path" latex))
           (latex-has-named (string-match-p "Example Site" latex)))
      (list (nreverse html-links)
            (nreverse latex-links)
            html-has-plain
            html-has-named
            html-has-angle
            latex-has-plain
            latex-has-named
            (replace-regexp-in-string
             "sec:org[[:alnum:]-]+" "sec:org-id"
             (replace-regexp-in-string
              "org[[:alnum:]-]\\{8,\\}" "orgHASH"
              html))
              (replace-regexp-in-string
              "sec:org[[:alnum:]-]+" "sec:org-id"
              latex))))))"##,
        expect,
    );
}

#[test]
fn org_link_export_footnote_anchor_custom_id_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Org export aborted.  Unable to resolve link: \\\"some-id\\\"\\nSee ‘org-export-with-broken-links’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Link Footnote\n\n")
    (insert "* Section :tag:\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: sec-one\n:END:\n")
    (insert "See [[#sec-one][Section]] and footnote[fn:1].\n\n")
    (insert "Another[fn:2:inline note].\n\n")
    (insert "[fn:1] Definition with *bold*.\n\n")
    (insert "* Other\n")
    (insert "Link to [[id:some-id][target]].\n")
    (let* ((org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil)))
      ;; Count anchors
      (let ((anchor-count
             (let ((c 0) (s 0))
               (while (string-match "<a " html s)
                 (setq s (match-end 0) c (1+ c)))
               c))
            ;; Count href patterns
            (href-count
             (let ((c 0) (s 0))
               (while (string-match "href=" html s)
                 (setq s (match-end 0) c (1+ c)))
               c))
            ;; Check specific patterns
            (has-custom-id (string-match-p "sec-one" html))
            (has-fn-1 (string-match-p "fn\\.1" html))
            (has-fn-2 (string-match-p "fn\\.2" html))
            (has-bold (string-match-p "<b>bold</b>" html))
            (has-tag (string-match-p "tag" html)))
        (list anchor-count
              href-count
              has-custom-id
              has-fn-1
              has-fn-2
              has-bold
              has-tag
              (replace-regexp-in-string
               "sec:org[[:alnum:]-]+" "sec:org-id"
               (replace-regexp-in-string "org[[:alnum:]-]\\{8,\\}" "orgHASH"
                                         html))))))))"##,
        expect,
    );
}
