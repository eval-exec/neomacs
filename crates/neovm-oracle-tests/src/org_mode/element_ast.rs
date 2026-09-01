use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_element_ast_adopt_extract_set_interpret_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let* ((doc (org-element-create 'org-data nil))
         (h1 (org-element-create
              'headline
              '(:level 1 :raw-value "Alpha" :title ("Alpha") :todo-keyword "TODO")
              (org-element-create
               'section nil
               (org-element-create
                'paragraph nil
                "First paragraph with "
                (org-element-create 'bold nil "bold")
                ".\n"))))
         (h2 (org-element-create
              'headline
              '(:level 1 :raw-value "Beta" :title ("Beta"))
              (org-element-create
               'section nil
               (org-element-create 'paragraph nil "Second paragraph.\n"))))
         (before nil)
         (after-extract nil))
    (org-element-adopt doc h1 h2)
    (setq before (org-element-interpret-data doc))
    (let* ((section (car (org-element-contents h1)))
           (paragraph (car (org-element-contents section)))
           (bold (car (org-element-map paragraph 'bold #'identity))))
      (org-element-extract bold)
      (setq after-extract (org-element-interpret-data doc))
      (org-element-set
       paragraph
       (org-element-create
        'paragraph nil
        "Replacement with "
        (org-element-create 'italic nil "italic")
        " and =literal= text.\n")))
    (list before
          after-extract
          (org-element-property :parent h1)
          (mapcar (lambda (headline)
                    (list (org-element-property :raw-value headline)
                          (org-element-property :level headline)
                          (mapcar #'org-element-type
                                  (org-element-contents headline))))
                  (org-element-map doc 'headline #'identity))
          (org-element-interpret-data doc)))"##,
        expect,
    );
}

#[test]
fn org_element_parse_lineage_skip_affiliated_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((table italic link) ((\"https\" \"//example.org\" (link nil)) (\"custom-id\" \"tbl\" (link paragraph section headline headline org-data))) \"Child\" \"#+caption: A *caption* with [[https://example.org][link]]\\n#+name: tbl\\n| A | B |\\n| 1 | 2 |\\n\\n* Parent\\n** Child\\nParagraph with /italic/ and [[#tbl][table link]].\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "#+CAPTION: A *caption* with [[https://example.org][link]]\n")
    (insert "#+NAME: tbl\n")
    (insert "| A | B |\n| 1 | 2 |\n\n")
    (insert "* Parent\n")
    (insert "** Child\n")
    (insert "Paragraph with /italic/ and [[#tbl][table link]].\n")
    (let* ((tree (org-element-parse-buffer))
           (no-table-objects
            (org-element-map tree t
              (lambda (node)
                (when (memq (org-element-type node) '(table link italic))
                  (org-element-type node)))
              nil nil 'table))
           (with-affiliated
            (org-element-map tree 'link
              (lambda (link)
                (list (org-element-property :type link)
                      (org-element-property :path link)
                      (mapcar #'org-element-type
                              (org-element-lineage link nil t))))
              nil nil nil t))
           (first-child
            (org-element-map tree 'headline
              (lambda (headline)
                (and (= 2 (org-element-property :level headline))
                     (throw :org-element-skip
                            (org-element-property :raw-value headline))))
              nil t)))
      (list no-table-objects
            with-affiliated
            first-child
            (substring-no-properties
             (org-element-interpret-data tree))))))"##,
        expect,
    );
}

#[test]
fn org_element_buffer_context_swap_refresh_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert "First paragraph with *bold*.\n\n")
    (insert "#+BEGIN_QUOTE\nquoted\n#+END_QUOTE\n\n")
    (insert "* Beta\n")
    (insert "Second paragraph with /italic/.\n")
    (goto-char (point-min))
    (search-forward "First")
    (let* ((context-before (org-element-context))
           (lineage-before
            (mapcar #'org-element-type
                    (org-element-lineage context-before nil t)))
           (para-a (org-element-at-point))
           (quote (progn
                    (search-forward "quoted")
                    (org-element-at-point))))
      (org-element-swap-A-B para-a quote)
      (let ((after-swap (buffer-substring-no-properties
                         (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "Second")
        (let ((context-after (org-element-context)))
          (delete-region (line-beginning-position) (line-end-position))
          (insert "Second paragraph now has [[https://gnu.org][GNU]].")
          (org-element-cache-refresh (line-beginning-position))
          (list (org-element-type context-before)
                lineage-before
                after-swap
                (org-element-type context-after)
                (org-element-type (org-element-context))
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_element_visible_only_lineage_inherited_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 \"Visible\" (\"keep\") \"visible\") (2 \"Hidden\" (\"skip\") nil) (2 \"Visible Child\" nil nil) (1 \"Tail\" nil nil)) ((table 32 79) (bold 150 190) (link 156 188) (macro 194 210) (quote-block 212 259) (italic 233 242) (planning 276 308) (drawer 308 364) (link 386 417) (src-block 429 485) (underline 534 541) (footnote-definition 566 624) (link 598 622))) (((1 \"Visible\") (2 \"Hidden\") (2 \"Visible Child\") (1 \"Tail\")) (org-data section keyword table table-row table-cell plain-text table-cell plain-text table-row table-cell plain-text table-cell plain-text headline plain-text section property-drawer node-property paragraph plain-text bold plain-text link plain-text plain-text macro plain-text quote-block paragraph plain-text italic plain-text plain-text headline plain-text headline plain-text section paragraph plain-text footnote-reference plain-text underline plain-text plain-text headline plain-text section paragraph plain-text footnote-definition paragraph plain-text link plain-text plain-text)) ((link bold paragraph section headline org-data) headline ((bold nil) (paragraph nil) (section nil) (headline \"Visible\") (org-data nil)) \"Visible\") ((document bold-object) (bold-object) (document bold-object) (document)) (link \"https\" \"//example.org\" (link bold paragraph section headline org-data)) ((org-data section keyword table headline plain-text section property-drawer paragraph quote-block headline plain-text section planning drawer paragraph src-block headline plain-text section paragraph headline plain-text section paragraph footnote-definition) (org-data section keyword table table-row table-row headline plain-text section property-drawer node-property paragraph quote-block paragraph headline plain-text section planning drawer plain-list item paragraph paragraph src-block headline plain-text section paragraph headline plain-text section paragraph footnote-definition paragraph)) \"#+title: Parser visibility mix\\n#+name: intro-table\\n| Key | Value |\\n| a   |     1 |\\n\\n* Visible                                                              :keep:\\n:PROPERTIES:\\n:CUSTOM_ID: visible\\n:END:\\nParagraph with *bold [[https://example.org][Example]]* and {{{macro(arg)}}}.\\n#+begin_quote\\nquoted /italic/ text\\n#+end_quote\\n** Hidden                                                              :skip:\\nSCHEDULED: <2026-06-01 Mon +1w>\\n:LOGBOOK:\\n- State \\\"TODO\\\" from \\\"\\\" [2026-05-27 Wed]\\n:END:\\nHidden paragraph with [[#intro-table][table target]] and =code=.\\n#+begin_src emacs-lisp :results value\\n  (+ 1 2)\\n#+end_src\\n** Visible Child\\nChild paragraph with [fn:1] and _under_.\\n* Tail\\nTail paragraph.\\n[fn:1] Footnote definition with [[https://gnu.org][GNU]].\\n\" \"#+TITLE: Parser visibility mix\\n#+NAME: intro-table\\n| Key | Value |\\n| a | 1 |\\n\\n* Visible :keep:\\n:PROPERTIES:\\n:CUSTOM_ID: visible\\n:END:\\nParagraph with *bold [[https://example.org][Example]]* and {{{macro(arg)}}}.\\n#+begin_quote\\nquoted /italic/ text\\n#+end_quote\\n** Hidden :skip:\\nSCHEDULED: <2026-06-01 Mon +1w>\\n:LOGBOOK:\\n- State \\\"TODO\\\" from \\\"\\\" [2026-05-27 Wed]\\n:END:\\nHidden paragraph with [[#intro-table][table target]] and =code=.\\n#+begin_src emacs-lisp :results value\\n(+ 1 2)\\n#+end_src\\n** Visible Child\\nChild paragraph with [fn:1] and _under_.\\n* Tail\\nTail paragraph.\\n[fn:1] Footnote definition with [[https://gnu.org][GNU]].\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Parser visibility mix\n")
    (insert "#+NAME: intro-table\n")
    (insert "| Key | Value |\n| a | 1 |\n\n")
    (insert "* Visible :keep:\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: visible\n:END:\n")
    (insert "Paragraph with *bold [[https://example.org][Example]]* and {{{macro(arg)}}}.\n")
    (insert "#+begin_quote\nquoted /italic/ text\n#+end_quote\n")
    (insert "** Hidden :skip:\n")
    (insert "SCHEDULED: <2026-06-01 Mon +1w>\n")
    (insert ":LOGBOOK:\n- State \"TODO\" from \"\" [2026-05-27 Wed]\n:END:\n")
    (insert "Hidden paragraph with [[#intro-table][table target]] and =code=.\n")
    (insert "#+begin_src emacs-lisp :results value\n(+ 1 2)\n#+end_src\n")
    (insert "** Visible Child\nChild paragraph with [fn:1] and _under_.\n")
    (insert "* Tail\nTail paragraph.\n")
    (insert "[fn:1] Footnote definition with [[https://gnu.org][GNU]].\n")
    (let ((full-before nil)
          (visible-after nil)
          (lineage nil)
          (inherited nil)
          (context-summary nil)
          (granularity nil)
          (interpreted nil))
      (setq full-before
            (let ((tree (org-element-parse-buffer)))
              (list
               (mapcar
                (lambda (h)
                  (list (org-element-property :level h)
                        (org-element-property :raw-value h)
                        (org-element-property :tags h)
                        (org-element-property :CUSTOM_ID h)))
                (org-element-map tree 'headline #'identity))
               (mapcar
                (lambda (node)
                  (list (org-element-type node)
                        (org-element-property :begin node)
                        (org-element-property :end node)))
                (org-element-map
                    tree '(table quote-block src-block planning
                           drawer footnote-definition link macro code
                           underline bold italic)
                  #'identity)))))
      (goto-char (point-min))
      (search-forward "Hidden")
      (beginning-of-line)
      (org-fold-hide-subtree)
      (setq visible-after
            (let ((tree (org-element-parse-buffer nil t)))
              (list
               (mapcar (lambda (h)
                         (list (org-element-property :level h)
                               (org-element-property :raw-value h)))
                       (org-element-map tree 'headline #'identity))
               (mapcar #'org-element-type
                       (org-element-map tree t #'identity nil nil
                                        '(src-block drawer))))))
      (org-fold-show-all)
      (let* ((tree (org-element-parse-buffer))
             (visible (car (org-element-map tree 'headline
                             (lambda (h)
                               (and (equal (org-element-property :raw-value h)
                                           "Visible")
                                    h))
                             nil t)))
             (child (car (org-element-map tree 'headline
                           (lambda (h)
                             (and (equal (org-element-property :raw-value h)
                                         "Visible Child")
                                  h))
                           nil t)))
             (bold (car (org-element-map tree 'bold #'identity)))
             (link (car (org-element-map bold 'link #'identity)))
             (footnote (car (org-element-map tree 'footnote-reference
                              #'identity))))
        (org-element-put-property tree :scope '(document))
        (org-element-put-property visible :scope '(visible-root))
        (org-element-put-property child :scope '(child-node))
        (org-element-put-property bold :scope '(bold-object))
        (setq lineage
              (list
               (mapcar #'org-element-type
                       (org-element-lineage link nil t))
               (org-element-type
                (org-element-lineage link 'headline))
               (org-element-lineage-map
                   link
                 (lambda (node)
                   (and (memq (org-element-type node)
                              '(bold paragraph section headline org-data))
                        (list (org-element-type node)
                              (org-element-property :raw-value node))))
                 nil t)
               (org-element-lineage-map
                   link
                 (lambda (node)
                   (and (eq (org-element-type node) 'headline)
                        (org-element-property :raw-value node)))
                 '(headline) nil t)))
        (setq inherited
              (list
               (org-element-property-inherited :scope link t t)
               (org-element-property-inherited :scope link nil nil)
               (org-element-property-inherited :scope link nil t nil t)
               (org-element-property-inherited :scope footnote t t)))
        (goto-char (org-element-property :begin link))
        (setq context-summary
              (list (org-element-type (org-element-context))
                    (org-element-property :type (org-element-context))
                    (org-element-property :path (org-element-context))
                    (mapcar #'org-element-type
                            (org-element-lineage (org-element-context)
                                                 nil t))))
        (setq granularity
              (list
               (mapcar #'org-element-type
                       (org-element-map
                           (org-element-parse-buffer 'greater-element)
                           t #'identity))
               (mapcar #'org-element-type
                       (org-element-map
                           (org-element-parse-buffer 'element)
                           t #'identity))))
        (setq interpreted
              (substring-no-properties
               (org-element-interpret-data tree))))
      (list full-before
            visible-after
            lineage
            inherited
            context-summary
            granularity
            interpreted
             (buffer-substring-no-properties
              (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_full_ast_dump_with_properties_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((1 \"TODO\" \"Alpha\" (\"work\" \"urgent\") nil) (2 \"DONE\" \"Sub A1\" (\"deep\") nil) (3 nil \"WAIT Sub A1a\" nil nil) (2 \"TODO\" \"Sub A2\" nil nil) (1 nil \"Beta\" (\"home\") nil)) ((\"<2026-05-27 Wed 09:00>\" nil nil) (nil nil \"[2026-05-26 Mon 15:00]\") (\"<2026-05-28 Thu>\" nil nil)) nil ((\"1:30\" closed)) ((\"emacs-lisp\" \"(+ 1 2)\\n\")) ((org nil)) ((\"https\" \"//example.org\" \"https://example.org\")) ((\"1\")) (org-data section keyword keyword headline plain-text section planning paragraph plain-text timestamp plain-text drawer paragraph plain-text drawer clock paragraph plain-text bold plain-text plain-text italic plain-text plain-text headline plain-text section planning paragraph plain-text headline plain-text section planning paragraph plain-text src-block table table-row table-cell plain-text table-cell plain-text table-row table-row table-cell plain-text table-cell plain-text table-row table-cell plain-text table-cell plain-text headline plain-text section paragraph link plain-text plain-text headline plain-text section paragraph plain-text footnote-reference plain-text footnote-definition paragraph plain-text) nil nil \"#+TITLE: AST Probe\\n#+AUTHOR: Tester\\n\\n* TODO Alpha :work:urgent:\\nSCHEDULED: <2026-05-27 Wed 09:00>\\nDEADLINE: <2026-05-29 Fri>\\n:PROPERTIES:\\n:Effort: 2:00\\n:Owner: Ada\\n:ID: alpha-id-1\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-26 Mon 10:00]--[2026-05-26 Mon 11:30] =>  1:30\\n:END:\\nAlpha body with *bold* and /italic/.\\n\\n** DONE Sub A1 :deep:\\nCLOSED: [2026-05-26 Mon 15:00]\\nSub A1 body.\\n*** WAIT Sub A1a\\nSCHEDULED: <2026-05-28 Thu>\\nSub A1a body.\\n\\n#+begin_src emacs-lisp\\n(+ 1 2)\\n#+end_src\\n\\n| Name | Val |\\n|------+-----|\\n| foo | 1 |\\n| bar | 2 |\\n\\n** TODO Sub A2\\n[[https://example.org][Example Link]]\\n\\n* Beta :home:\\nBeta body with footnote[fn:1].\\n\\n[fn:1] Footnote definition.\\n\")""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: AST Probe\n")
    (insert "#+AUTHOR: Tester\n\n")
    (insert "* TODO Alpha :work:urgent:\n")
    (insert "SCHEDULED: <2026-05-27 Wed 09:00>\n")
    (insert "DEADLINE: <2026-05-29 Fri>\n")
    (insert ":PROPERTIES:\n:Effort: 2:00\n:Owner: Ada\n:ID: alpha-id-1\n:END:\n")
    (insert ":LOGBOOK:\nCLOCK: [2026-05-26 Mon 10:00]--[2026-05-26 Mon 11:30] =>  1:30\n:END:\n")
    (insert "Alpha body with *bold* and /italic/.\n\n")
    (insert "** DONE Sub A1 :deep:\n")
    (insert "CLOSED: [2026-05-26 Mon 15:00]\n")
    (insert "Sub A1 body.\n")
    (insert "*** WAIT Sub A1a\n")
    (insert "SCHEDULED: <2026-05-28 Thu>\n")
    (insert "Sub A1a body.\n\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\n")
    (insert "| Name | Val |\n|------+-----|\n| foo | 1 |\n| bar | 2 |\n\n")
    (insert "** TODO Sub A2\n")
    (insert "[[https://example.org][Example Link]]\n\n")
    (insert "* Beta :home:\n")
    (insert "Beta body with footnote[fn:1].\n\n")
    (insert "[fn:1] Footnote definition.\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (h)
                (list (org-element-property :level h)
                      (org-element-property :todo-keyword h)
                      (org-element-property :raw-value h)
                      (org-element-property :tags h)
                      (org-element-property :priority h)))))
           (planning
            (org-element-map tree 'planning
              (lambda (p)
                (list (and (org-element-property :scheduled p)
                           (org-element-property :raw-value
                            (org-element-property :scheduled p)))
                      (and (org-element-property :deadline p)
                           (org-element-property :raw-value
                            (org-element-property :deadline p)))
                      (and (org-element-property :closed p)
                           (org-element-property :raw-value
                            (org-element-property :closed p)))))))
           (properties
            (org-element-map tree 'node-property
              (lambda (np)
                (list (org-element-property :key np)
                      (org-element-property :value np)))))
           (clocks
            (org-element-map tree 'clock
              (lambda (c)
                (list (org-element-property :duration c)
                      (org-element-property :status c)))))
           (blocks
            (org-element-map tree 'src-block
              (lambda (sb)
                (list (org-element-property :language sb)
                      (org-element-property :value sb)))))
           (tables
            (org-element-map tree 'table
              (lambda (tb)
                (list (org-element-property :type tb)
                      (org-element-property :tblfm tb)))))
           (links
            (org-element-map tree 'link
              (lambda (lk)
                (list (org-element-property :type lk)
                      (org-element-property :path lk)
                      (org-element-property :raw-link lk)))))
           (footnotes
            (org-element-map tree 'footnote-definition
              (lambda (fn)
                (list (org-element-property :label fn)))))
           (all-types
            (org-element-map tree t
              (lambda (el) (org-element-type el)))))
      (list headlines
            planning
            properties
            clocks
            blocks
            tables
            links
            footnotes
            all-types
            (org-element-property :title tree)
            (org-element-property :author tree)
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_clock_property_planning_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"Alpha\" 1 \"TODO\" (\"work\") nil) (\"WAIT Gamma\" 3 nil (\"urgent\") nil) (\"Beta\" 2 \"DONE\" (\"home\") nil)) (((\"Effort\" \"2h30m\") (\"CUSTOM_ID\" \"alpha\")) ((\"Effort\" \"0h45m\")) ((\"Effort\" \"1h\") (\"CUSTOM_ID\" \"beta\"))) ((\"work\") (\"urgent\") (\"home\")) \"#+TITLE: DeepParseTest\\n#+AUTHOR: Oracle\\n\\n* TODO Alpha :work:\\n:PROPERTIES:\\n:Effort: 2h30m\\n:CUSTOM_ID: alpha\\n:END:\\nBody text under alpha.\\n*** WAIT Gamma :urgent:\\n:PROPERTIES:\\n:Effort: 0h45m\\n:END:\\nBody under gamma.\\n\\n\\n** DONE Beta :home:\\n:PROPERTIES:\\n:Effort: 1h\\n:CUSTOM_ID: beta\\n:END:\\nBody text under beta.\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: DeepParseTest\n#+AUTHOR: Oracle\n\n")
    (insert "* TODO Alpha :work:\n")
    (insert ":PROPERTIES:\n:Effort: 2h30m\n:CUSTOM_ID: alpha\n:END:\n")
    (insert "Body text under alpha.\n\n")
    (insert "** DONE Beta :home:\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:CUSTOM_ID: beta\n:END:\n")
    (insert "Body text under beta.\n\n")
    ;; Edit: insert a new sub-heading under Alpha
    (goto-char (point-min))
    (search-forward "Body text under alpha.")
    (end-of-line)
    (insert "\n*** WAIT Gamma :urgent:\n")
    (insert ":PROPERTIES:\n:Effort: 0h45m\n:END:\n")
    (insert "Body under gamma.\n")
    ;; Parse
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)
                      (org-element-property :tags hl)
                      (org-element-property :priority hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np)))))))
           (tags
            (org-element-map tree 'headline
              (lambda (hl) (org-element-property :tags hl)))))
      (list headlines properties tags
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_scheduled_deadline_clock_divergence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 1 \"TODO\")) (((\"Effort\" \"1h\"))) \"* TODO Alpha\\nSCHEDULED: <2026-05-28 Wed>\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))))
      (list headlines properties
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_clock_line_divergence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 1 \"TODO\")) (((\"Effort\" \"1h\"))) \"* TODO Alpha\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "CLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\n")
    (insert "Body.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))))
      (list headlines properties
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_scheduled_deadline_closed_clock_divergence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 1 \"TODO\")) nil \"* TODO Alpha\\nSCHEDULED: <2026-05-28 Wed>\\nDEADLINE: <2026-06-01 Mon>\\nCLOSED: [2026-05-27 Tue 14:30]\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert "CLOSED: [2026-05-27 Tue 14:30]\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "CLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\n")
    (insert "Body.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))))
      (list headlines properties
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_single_scheduled_single_deadline_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 29 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "Body alpha.\n\n")
    (insert "* DONE Beta\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert "Body beta.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (planning
            (org-element-map tree 'planning
              (lambda (pl)
                (list (org-element-property :type pl)
                      (let ((ts (org-element-property :timestamp pl)))
                        (and ts (list (org-element-property :raw-value ts)
                                      (org-element-property :day-start ts)
                                      (org-element-property :month-start ts)
                                      (org-element-property :year-start ts)))))))))
      (list headlines planning
            (buffer-substring-no-properties
             (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_property_drawer_set_delete_reparse_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 43 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:CUSTOM_ID: alpha\n:Owner: Alice\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Owner: Bob\n:END:\n")
    (insert "Body beta.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-set-property "Status" "active")
        (goto-char (point-min))
        (search-forward "Beta")
        (beginning-of-line)
        (org-set-property "Priority" "high")
        (let ((after-set (funcall snap)))
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-delete-property "Owner")
          (let ((after-delete (funcall snap)))
            (list initial after-set after-delete
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_scheduled_deadline_no_closed_no_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 1 \"TODO\")) nil \"* TODO Alpha\\nSCHEDULED: <2026-05-28 Wed>\\nDEADLINE: <2026-06-01 Mon>\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))))
      (list headlines properties
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_scheduled_deadline_closed_no_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 1 \"TODO\")) nil \"* TODO Alpha\\nSCHEDULED: <2026-05-28 Wed>\\nDEADLINE: <2026-06-01 Mon>\\nCLOSED: [2026-05-27 Tue 14:30]\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert "CLOSED: [2026-05-27 Tue 14:30]\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))))
      (list headlines properties
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_scheduled_deadline_clock_no_closed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 1 \"TODO\")) nil \"* TODO Alpha\\nSCHEDULED: <2026-05-28 Wed>\\nDEADLINE: <2026-06-01 Mon>\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "CLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\n")
    (insert "Body.\n\n")
    (let* ((tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (hl)
                (list (org-element-property :raw-value hl)
                      (org-element-property :level hl)
                      (org-element-property :todo-keyword hl)))))
           (properties
            (org-element-map tree 'property-drawer
              (lambda (pd)
                (org-element-map pd 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))))
      (list headlines properties
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_tag_property_drawer_edit_reparse_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:important:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:CUSTOM_ID: alpha\n:Owner: Alice\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta :home:\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Owner: Bob\n:END:\n")
    (insert "Body beta.\n\n")
    (insert "*** WAIT Gamma :work:urgent:\n")
    (insert ":PROPERTIES:\n:Effort: 30m\n:END:\n")
    (insert "Body gamma.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-toggle-tag "review" 'on)
        (goto-char (point-min))
        (search-forward "Beta")
        (beginning-of-line)
        (org-set-property "Status" "verified")
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_separate_scheduled_deadline_clock_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 53 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    ;; Alpha has only SCHEDULED
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:END:\n")
    (insert "Body alpha.\n\n")
    ;; Beta has only DEADLINE
    (insert "** DONE Beta\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body beta.\n\n")
    ;; Gamma has SCHEDULED only
    (insert "** TODO Gamma\n")
    (insert "SCHEDULED: <2026-05-29 Thu>\n")
    (insert "Body gamma.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl))))
                      (org-element-map tree 'planning
                        (lambda (pl)
                          (list (org-element-property :type pl)
                                (let ((ts (org-element-property :timestamp pl)))
                                  (and ts (org-element-property :raw-value ts))))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Edit: add tag to Alpha
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-toggle-tag "work" 'on)
        (let ((after-tag (funcall snap)))
          ;; Edit: set property on Beta
          (goto-char (point-min))
          (search-forward "Beta")
          (beginning-of-line)
          (org-set-property "Status" "verified")
          (let ((after-prop (funcall snap)))
            (list initial after-tag after-prop
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_clock_logbook_edit_reparse_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 43 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Owner: Bob\n:END:\n")
    (insert "Body beta.\n\n")
    (insert "*** TODO Gamma :work:urgent:\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body gamma.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Edit: toggle tag
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-toggle-tag "review" 'on)
        ;; Edit: set property
        (goto-char (point-min))
        (search-forward "Gamma")
        (beginning-of-line)
        (org-set-property "Status" "in-progress")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_multi_heading_tag_prop_edit_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 49 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO A :work:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Owner: X\n:END:\n")
    (insert "Body A.\n\n")
    (insert "** DONE B :home:\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Owner: Y\n:END:\n")
    (insert "Body B.\n\n")
    (insert "*** TODO C :work:urgent:\n")
    (insert ":PROPERTIES:\n:Effort: 30m\n:END:\n")
    (insert "Body C.\n\n")
    (insert "* NEXT D :errands:\n")
    (insert ":PROPERTIES:\n:Effort: 45m\n:Owner: Z\n:END:\n")
    (insert "Body D.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        (goto-char (point-min))
        (search-forward "Body A.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        (goto-char (point-min))
        (search-forward "NEXT D")
        (beginning-of-line)
        (org-set-property "Status" "pending")
        (goto-char (point-min))
        (search-forward "DONE B")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_four_heading_tag_prop_edit_reparse_v2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 49 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO P :project:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:CATEGORY: dev\n:END:\n")
    (insert "Body P.\n\n")
    (insert "** DONE S1 :backend:\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:Tier: P0\n:END:\n")
    (insert "Body S1.\n\n")
    (insert "** TODO S2 :frontend:\n")
    (insert ":PROPERTIES:\n:Effort: 5h\n:Tier: P1\n:END:\n")
    (insert "Body S2.\n\n")
    (insert "** WAIT S3 :devops:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Tier: P2\n:Assigned: Bob\n:END:\n")
    (insert "Body S3.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        (goto-char (point-min))
        (search-forward "Body P.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        (goto-char (point-min))
        (search-forward "WAIT S3")
        (beginning-of-line)
        (org-set-property "Status" "blocked")
        (goto-char (point-min))
        (search-forward "WAIT S3")
        (beginning-of-line)
        (org-delete-property "Assigned")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_five_heading_tag_prop_edit_reparse_v3() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 55 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Root :root:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:CATEGORY: main\n:END:\n")
    (insert "Body Root.\n\n")
    (insert "** DONE Branch-A :fe:\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:Tier: P0\n:END:\n")
    (insert "Body A.\n\n")
    (insert "** TODO Branch-B :be:\n")
    (insert ":PROPERTIES:\n:Effort: 5h\n:Tier: P1\n:END:\n")
    (insert "Body B.\n\n")
    (insert "** WAIT Branch-C :ops:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Tier: P2\n:Owner: Bob\n:END:\n")
    (insert "Body C.\n\n")
    (insert "** NEXT Branch-D :qa:\n")
    (insert ":PROPERTIES:\n:Effort: 4h\n:Tier: P1\n:Owner: Carol\n:END:\n")
    (insert "Body D.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Toggle tag on Root
        (goto-char (point-min))
        (search-forward "Body Root.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        ;; Set property on Branch-D
        (goto-char (point-min))
        (search-forward "NEXT Branch-D")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner from Branch-C
        (goto-char (point-min))
        (search-forward "WAIT Branch-C")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_six_heading_tag_prop_edit_reparse_v4() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 58 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Root :root:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:CATEGORY: main\n:END:\n")
    (insert "Body Root.\n\n")
    (insert "** DONE S1 :fe:\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:Tier: P0\n:END:\n")
    (insert "Body S1.\n\n")
    (insert "** TODO S2 :be:\n")
    (insert ":PROPERTIES:\n:Effort: 5h\n:Tier: P1\n:END:\n")
    (insert "Body S2.\n\n")
    (insert "** WAIT S3 :ops:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Tier: P2\n:Owner: Bob\n:END:\n")
    (insert "Body S3.\n\n")
    (insert "** NEXT S4 :qa:\n")
    (insert ":PROPERTIES:\n:Effort: 4h\n:Tier: P1\n:Owner: Carol\n:END:\n")
    (insert "Body S4.\n\n")
    (insert "** CANCEL S5 :docs:\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Tier: P3\n:END:\n")
    (insert "Body S5.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Toggle tag on Root
        (goto-char (point-min))
        (search-forward "Body Root.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        ;; Set property on S4
        (goto-char (point-min))
        (search-forward "NEXT S4")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner from S3
        (goto-char (point-min))
        (search-forward "WAIT S3")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_seven_heading_tag_prop_edit_reparse_v5() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 61 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Epic :epic:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:CATEGORY: dev\n:END:\n")
    (insert "Body Epic.\n\n")
    (insert "** DONE S-A :fe:\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:Tier: P0\n:END:\n")
    (insert "Body A.\n\n")
    (insert "** TODO S-B :be:\n")
    (insert ":PROPERTIES:\n:Effort: 5h\n:Tier: P1\n:END:\n")
    (insert "Body B.\n\n")
    (insert "** WAIT S-C :ops:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Tier: P2\n:Owner: Bob\n:END:\n")
    (insert "Body C.\n\n")
    (insert "** NEXT S-D :qa:\n")
    (insert ":PROPERTIES:\n:Effort: 4h\n:Tier: P1\n:Owner: Carol\n:END:\n")
    (insert "Body D.\n\n")
    (insert "** CANCEL S-E :docs:\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Tier: P3\n:END:\n")
    (insert "Body E.\n\n")
    (insert "* TODO Feature :feature:\n")
    (insert ":PROPERTIES:\n:Owner: Dave\n:CATEGORY: product\n:END:\n")
    (insert "Body Feature.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Toggle tag on Epic
        (goto-char (point-min))
        (search-forward "Body Epic.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        ;; Set property on S-D
        (goto-char (point-min))
        (search-forward "NEXT S-D")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner from S-C
        (goto-char (point-min))
        (search-forward "WAIT S-C")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_eight_heading_tag_prop_edit_reparse_v6() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 64 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Company :company:\n")
    (insert ":PROPERTIES:\n:Owner: CEO\n:CATEGORY: org\n:END:\n")
    (insert "Body Company.\n\n")
    (insert "** DONE Eng :eng:\n")
    (insert ":PROPERTIES:\n:Effort: 40h\n:Budget: 100k\n:END:\n")
    (insert "Body Eng.\n\n")
    (insert "** TODO Product :product:\n")
    (insert ":PROPERTIES:\n:Effort: 20h\n:Budget: 50k\n:END:\n")
    (insert "Body Product.\n\n")
    (insert "** WAIT Design :design:\n")
    (insert ":PROPERTIES:\n:Effort: 15h\n:Budget: 30k\n:Owner: ArtDir\n:END:\n")
    (insert "Body Design.\n\n")
    (insert "** NEXT Marketing :marketing:\n")
    (insert ":PROPERTIES:\n:Effort: 25h\n:Budget: 80k\n:Owner: CMO\n:END:\n")
    (insert "Body Marketing.\n\n")
    (insert "** CANCEL Ops :ops:\n")
    (insert ":PROPERTIES:\n:Effort: 10h\n:Budget: 20k\n:END:\n")
    (insert "Body Ops.\n\n")
    (insert "** TODO Sales :sales:\n")
    (insert ":PROPERTIES:\n:Effort: 30h\n:Budget: 60k\n:Owner: VP-Sales\n:END:\n")
    (insert "Body Sales.\n\n")
    (insert "* DONE Q1-Goal :goal:\n")
    (insert ":PROPERTIES:\n:Owner: CEO\n:CATEGORY: planning\n:END:\n")
    (insert "Body Q1.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Toggle tag on Company
        (goto-char (point-min))
        (search-forward "Body Company.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        ;; Set property on Marketing
        (goto-char (point-min))
        (search-forward "NEXT Marketing")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner from Design
        (goto-char (point-min))
        (search-forward "WAIT Design")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_nine_heading_tag_prop_edit_reparse_v7() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 67 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Org :org:\n")
    (insert ":PROPERTIES:\n:CEO: Alice\n:CATEGORY: corp\n:END:\n")
    (insert "Body Org.\n\n")
    (insert "** DONE RnD :rnd:\n")
    (insert ":PROPERTIES:\n:Effort: 100h\n:Budget: 500k\n:END:\n")
    (insert "Body RnD.\n\n")
    (insert "** TODO Sales :sales:\n")
    (insert ":PROPERTIES:\n:Effort: 80h\n:Budget: 300k\n:END:\n")
    (insert "Body Sales.\n\n")
    (insert "** WAIT Support :support:\n")
    (insert ":PROPERTIES:\n:Effort: 40h\n:Budget: 100k\n:Owner: Bob\n:END:\n")
    (insert "Body Support.\n\n")
    (insert "** NEXT Finance :finance:\n")
    (insert ":PROPERTIES:\n:Effort: 60h\n:Budget: 200k\n:Owner: Carol\n:END:\n")
    (insert "Body Finance.\n\n")
    (insert "** CANCEL Legal :legal:\n")
    (insert ":PROPERTIES:\n:Effort: 20h\n:Budget: 50k\n:END:\n")
    (insert "Body Legal.\n\n")
    (insert "** TODO HR :hr:\n")
    (insert ":PROPERTIES:\n:Effort: 30h\n:Budget: 80k\n:Owner: Dave\n:END:\n")
    (insert "Body HR.\n\n")
    (insert "** DONE IT :it:\n")
    (insert ":PROPERTIES:\n:Effort: 50h\n:Budget: 150k\n:Owner: Eve\n:END:\n")
    (insert "Body IT.\n\n")
    (insert "* TODO Vision :vision:\n")
    (insert ":PROPERTIES:\n:CEO: Alice\n:CATEGORY: strategy\n:END:\n")
    (insert "Body Vision.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Toggle tag on Org
        (goto-char (point-min))
        (search-forward "Body Org.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        ;; Set property on Finance
        (goto-char (point-min))
        (search-forward "NEXT Finance")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner from Support
        (goto-char (point-min))
        (search-forward "WAIT Support")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_ten_heading_tag_prop_edit_reparse_v8() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 70 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Enterprise :enterprise:\n")
    (insert ":PROPERTIES:\n:CEO: Alice\n:CATEGORY: biz\n:END:\n")
    (insert "Body Enterprise.\n\n")
    (insert "** DONE Engineering :eng:\n")
    (insert ":PROPERTIES:\n:Effort: 200h\n:Budget: 2M\n:END:\n")
    (insert "Body Eng.\n\n")
    (insert "** TODO Sales :sales:\n")
    (insert ":PROPERTIES:\n:Effort: 150h\n:Budget: 1M\n:END:\n")
    (insert "Body Sales.\n\n")
    (insert "** WAIT Support :support:\n")
    (insert ":PROPERTIES:\n:Effort: 80h\n:Budget: 500k\n:Owner: Bob\n:END:\n")
    (insert "Body Support.\n\n")
    (insert "** NEXT Finance :finance:\n")
    (insert ":PROPERTIES:\n:Effort: 100h\n:Budget: 800k\n:Owner: Carol\n:END:\n")
    (insert "Body Finance.\n\n")
    (insert "** CANCEL Legal :legal:\n")
    (insert ":PROPERTIES:\n:Effort: 40h\n:Budget: 200k\n:END:\n")
    (insert "Body Legal.\n\n")
    (insert "** TODO HR :hr:\n")
    (insert ":PROPERTIES:\n:Effort: 60h\n:Budget: 300k\n:Owner: Dave\n:END:\n")
    (insert "Body HR.\n\n")
    (insert "** DONE IT :it:\n")
    (insert ":PROPERTIES:\n:Effort: 120h\n:Budget: 600k\n:Owner: Eve\n:END:\n")
    (insert "Body IT.\n\n")
    (insert "** TODO Marketing :marketing:\n")
    (insert ":PROPERTIES:\n:Effort: 90h\n:Budget: 400k\n:Owner: Frank\n:END:\n")
    (insert "Body Marketing.\n\n")
    (insert "* DONE Vision :vision:\n")
    (insert ":PROPERTIES:\n:CEO: Alice\n:CATEGORY: strategy\n:END:\n")
    (insert "Body Vision.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)
                                (org-element-property :todo-keyword hl)
                                (org-element-property :tags hl))))
                      (org-element-map tree 'property-drawer
                        (lambda (pd)
                          (org-element-map pd 'node-property
                            (lambda (np)
                              (list (org-element-property :key np)
                                    (org-element-property :value np)))))))))))
      (let ((initial (funcall snap)))
        ;; Toggle tag on Enterprise
        (goto-char (point-min))
        (search-forward "Body Enterprise.")
        (beginning-of-line)
        (org-up-heading-safe)
        (org-toggle-tag "review" 'on)
        ;; Set property on Finance
        (goto-char (point-min))
        (search-forward "NEXT Finance")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner from Support
        (goto-char (point-min))
        (search-forward "WAIT Support")
        (beginning-of-line)
        (org-delete-property "Owner")
        (let ((after-edit (funcall snap)))
          (list initial after-edit
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}
