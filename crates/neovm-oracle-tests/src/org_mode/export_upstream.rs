//! Ported upstream ERT tests from org-mode's test-ox.el (9.7.11).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Export: parse-option-keyword ─────────────────────────────────────

#[test]
fn upstream_ox_parse_option_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (1 t t t t t t t t t t t t t t t t t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((options (org-export--parse-option-keyword
                  "H:1 num:t \\n:t timestamp:t arch:t author:t creator:t d:t email:t \
*:t e:t ::t f:t pri:t -:t ^:t toc:t |:t tags:t tasks:t <:t todo:t inline:nil \
stat:t title:t")))
    (list (plist-get options :headline-levels)
          (plist-get options :section-numbers)
          (plist-get options :preserve-breaks)
          (plist-get options :time-stamp-file)
          (plist-get options :with-archived-trees)
          (plist-get options :with-author)
          (plist-get options :with-drawers)
          (plist-get options :with-email)
          (plist-get options :with-emphasize)
          (plist-get options :with-entities)
          (plist-get options :with-fixed-width)
          (plist-get options :with-footnotes)
          (plist-get options :with-priority)
          (plist-get options :with-special-strings)
          (plist-get options :with-sub-superscript)
          (plist-get options :with-toc)
          (plist-get options :with-tables)
          (plist-get options :with-tags)
          (plist-get options :with-tasks)
          (plist-get options :with-timestamps)
          (plist-get options :with-todo-keywords)
          (plist-get options :with-inlinetasks)
          (plist-get options :with-statistics-cookies)
          (plist-get options :with-title))))"##,
        expect,
    );
}

// ── Export: get-inbuffer-options ──────────────────────────────────────

#[test]
fn upstream_ox_get_inbuffer_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test Title\" 0 10 (:parent (#(\"Test Title\" 0 10 (:parent #4)))))) (#(\"Test Author\" 0 11 (:parent (#(\"Test Author\" 0 11 (:parent #4)))))) \"test@example.org\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "#+TITLE: Test Title\n#+AUTHOR: Test Author\n#+EMAIL: test@example.org\n#+DESCRIPTION: Test description\n#+KEYWORDS: test org\n#+LANGUAGE: en\n")
      (goto-char (point-min))
      (let ((info (org-export-get-environment)))
        (list (plist-get info :title)
              (plist-get info :author)
              (plist-get info :email))))))"##,
        expect,
    );
}

// ── Export: get-subtree-options ───────────────────────────────────────

#[test]
fn upstream_ox_get_subtree_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-subtree-options)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* Heading\n:PROPERTIES:\n:EXPORT_TITLE: Sub Title\n:EXPORT_AUTHOR: Sub Author\n:END:")
      (goto-char (point-min))
      (org-export-get-subtree-options))))"##,
        expect,
    );
}

// ── Export: get-relative-level ───────────────────────────────────────

#[test]
fn upstream_ox_get_relative_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n*** H3\n* H1b")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-get-relative-level h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: number-to-roman ──────────────────────────────────────────

#[test]
fn upstream_ox_number_to_roman() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"I\" \"IV\" \"IX\" \"XIV\" \"XXXIX\" \"XL\" \"XLIX\" \"L\" \"MCMLXXXIV\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   (org-export-number-to-roman 1)
   (org-export-number-to-roman 4)
   (org-export-number-to-roman 9)
   (org-export-number-to-roman 14)
   (org-export-number-to-roman 39)
   (org-export-number-to-roman 40)
   (org-export-number-to-roman 49)
   (org-export-number-to-roman 50)
   (org-export-number-to-roman 1984)))"##,
        expect,
    );
}

// ── Export: low-level-p ──────────────────────────────────────────────

#[test]
fn upstream_ox_low_level_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n*** H3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-low-level-p h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: first-sibling-p / last-sibling-p ─────────────────────────

#[test]
fn upstream_ox_first_last_sibling_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n* H2\n* H3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         (mapcar #'org-export-first-sibling-p headlines)
         (mapcar #'org-export-last-sibling-p headlines))))))"##,
        expect,
    );
}

// ── Export: get-node-property ────────────────────────────────────────

#[test]
fn upstream_ox_get_node_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"myid\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H\n:PROPERTIES:\n:CUSTOM_ID: myid\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity))))
        (org-export-get-node-property :CUSTOM_ID headline)))))"##,
        expect,
    );
}

// ── Export: get-category ─────────────────────────────────────────────

#[test]
fn upstream_ox_get_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Work\" \"???\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; From keyword.
     (with-temp-buffer
       (org-mode)
       (insert "#+CATEGORY: Work\n* Heading")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment))))
              (headline (car (org-element-map tree 'headline #'identity))))
         (org-export-get-category headline info)))
     ;; Default.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment))))
              (headline (car (org-element-map tree 'headline #'identity))))
         (org-export-get-category headline info))))))"##,
        expect,
    );
}

// ── Export: get-tags ─────────────────────────────────────────────────

#[test]
fn upstream_ox_get_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 4) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* Heading :tag1:tag2:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity))))
        (org-export-get-tags headline)))))"##,
        expect,
    );
}

// ── Export: get-date ─────────────────────────────────────────────────

#[test]
fn upstream_ox_get_date() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"2023-10-13\" 0 10 (:parent (#(\"2023-10-13\" 0 10 (:parent #4)))))) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; From keyword.
     (with-temp-buffer
       (org-mode)
       (insert "#+DATE: 2023-10-13\n* Heading")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-export-get-date info)))
     ;; Default.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-export-get-date info))))))"##,
        expect,
    );
}

// ── Export: get-footnote-number ──────────────────────────────────────

#[test]
fn upstream_ox_get_footnote_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def 1\n\n[fn:2] Def 2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (fn)
                  (org-export-get-footnote-number fn info))
                (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ── Export: export-block filter ──────────────────────────────────────

#[test]
fn upstream_ox_export_block_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "#+BEGIN_EXPORT html\n<p>Keep</p>\n#+END_EXPORT\n#+BEGIN_EXPORT latex\nRemove\n#+END_EXPORT")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (length (org-element-map tree 'export-block #'identity))))))"##,
        expect,
    );
}

// ── Export: export-snippet filter ─────────────────────────────────────

#[test]
fn upstream_ox_export_snippet_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"html\" \"latex\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "Text @@html:<b>bold</b>@@ more @@latex:\\textbf{bold}@@")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (snippets (org-element-map tree 'export-snippet #'identity)))
        (mapcar (lambda (s) (org-element-property :back-end s)) snippets)))))"##,
        expect,
    );
}

// ── Export: comments handling ────────────────────────────────────────

#[test]
fn upstream_ox_comments_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "# This is a comment\n* Heading\n# Another comment\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (comments (org-element-map tree 'comment #'identity)))
        (length comments)))))"##,
        expect,
    );
}

// ── Export: comment-tree handling ─────────────────────────────────────

#[test]
fn upstream_ox_comment_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* COMMENT Hidden heading\nBody\n* Visible heading\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (org-export--delete-comment-trees)
        (length (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: handle-options (title/author/email) ──────────────────────

#[test]
fn upstream_ox_handle_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments #<subr identity> 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Title.
     (with-temp-buffer
       (org-mode)
       (insert "#+TITLE: My Title\nBody")
       (goto-char (point-min))
       (let ((info (org-export-get-environment)))
         (org-export-data-with-backend
          (plist-get info :title)
          (org-export-create-backend :transcoders '((plain-text . identity)))
          nil)))
     ;; Author.
     (with-temp-buffer
       (org-mode)
       (insert "#+AUTHOR: Me\nBody")
       (goto-char (point-min))
       (let ((info (org-export-get-environment)))
         (plist-get info :author)))
     ;; Email.
     (with-temp-buffer
       (org-mode)
       (insert "#+EMAIL: me@example.org\nBody")
       (goto-char (point-min))
       (let ((info (org-export-get-environment)))
         (plist-get info :email))))))"##,
        expect,
    );
}

// ── Export: get-optional-title ────────────────────────────────────────

#[test]
fn upstream_ox_get_optional_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-optional-title)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "#+TITLE: My Title\n* Heading\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (org-export-get-optional-title (car (org-element-map tree 'headline #'identity)) info)))))"##,
        expect,
    );
}

// ── Export: numbered-headline-p ──────────────────────────────────────

#[test]
fn upstream_ox_numbered_headline_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n*** H3\n* H4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-numbered-headline-p h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: get-headline-number ──────────────────────────────────────

#[test]
fn upstream_ox_get_headline_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1) (1 1) (1 2) (2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n** H3\n* H4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-get-headline-number h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: get-caption ──────────────────────────────────────────────

#[test]
fn upstream_ox_get_caption() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"long caption\" 0 12 (:parent (#(\"long caption\" 0 12 (:parent #4)))))) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Short and long caption.
     (with-temp-buffer
       (org-mode)
       (insert "#+CAPTION[short]: long caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (org-export-get-caption table)))
     ;; Short only.
     (with-temp-buffer
       (org-mode)
       (insert "#+CAPTION: only long\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
          (org-export-get-caption table t))))))"##,
        expect,
    );
}

#[test]
fn org_export_headline_number_category_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Numbering\n")
    (insert "#+CATEGORY: test-cat\n\n")
    (insert "* Alpha\n")
    (insert "** Beta\n")
    (insert "*** Gamma\n")
    (insert "** Delta\n")
    (insert "* Epsilon\n")
    (let* ((tree (org-element-parse-buffer))
           (info (org-combine-plists
                  (org-export--get-buffer-attributes)
                  (org-export-get-environment)))
           (headlines (org-element-map tree 'headline #'identity))
           (numbers (mapcar (lambda (h)
                              (org-export-get-headline-number h info))
                            headlines))
           (categories (mapcar (lambda (h)
                                 (org-export-get-category h info))
                               headlines))
           (tags (mapcar (lambda (h)
                           (org-export-get-tags h info))
                         headlines))
           (todo (mapcar (lambda (h)
                           (org-export-get-todo-keyword h info))
                         headlines)))
      (list numbers categories tags todo
            (mapcar (lambda (h)
                      (list (org-element-property :level h)
                            (org-element-property :raw-value h)))
                     headlines)))))"##,
        expect,
    );
}

#[test]
fn org_export_headline_number_category_tags_todo_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Export Deep\n")
    (insert "#+CATEGORY: test\n\n")
    (insert "* TODO Alpha :work:\n")
    (insert "** DONE Beta\n")
    (insert "*** TODO Gamma\n")
    (insert "** WAIT Delta\n")
    (insert "* NEXT Epsilon\n")
    (let* ((tree (org-element-parse-buffer))
           (info (org-combine-plists
                  (org-export--get-buffer-attributes)
                  (org-export-get-environment)))
           (headlines (org-element-map tree 'headline #'identity))
           (numbers (mapcar (lambda (h)
                              (org-export-get-headline-number h info))
                            headlines))
           (categories (mapcar (lambda (h)
                                 (org-export-get-category h info))
                               headlines))
           (tags (mapcar (lambda (h)
                           (org-export-get-tags h info))
                         headlines))
           (todos (mapcar (lambda (h)
                            (org-export-get-todo-keyword h info))
                          headlines)))
      (list numbers categories tags todos
            (mapcar (lambda (h)
                      (list (org-element-property :level h)
                            (org-element-property :raw-value h)))
                    headlines)))))"##,
        expect,
    );
}

#[test]
fn org_export_headline_numbering_edit_reexport_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: ExportEditTest\n\n")
    (insert "* Alpha\nBody alpha.\n\n")
    (insert "** Beta\nBody beta.\n\n")
    (insert "*** Gamma\nBody gamma.\n\n")
    (insert "** Delta\nBody delta.\n\n")
    (insert "* Epsilon\nBody epsilon.\n\n")
    (let* ((snap (lambda ()
                   (let* ((tree (org-element-parse-buffer))
                          (info (org-combine-plists
                                 (org-export--get-buffer-attributes)
                                 (org-export-get-environment)))
                          (headlines (org-element-map tree 'headline #'identity)))
                     (mapcar (lambda (h)
                               (list (org-element-property :raw-value h)
                                     (org-element-property :level h)
                                     (org-export-get-headline-number h info)
                                     (org-export-get-tags h info)
                                     (org-export-get-todo-keyword h info)))
                             headlines)))))
      (let ((before (funcall snap)))
        ;; Edit: insert new heading under Alpha
        (goto-char (point-min))
        (search-forward "Body alpha.")
        (end-of-line)
        (insert "\n** Zeta\nBody zeta.\n")
        (let ((after (funcall snap)))
          (list before after
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_export_todo_tags_planning_edit_reexport_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: PlanningExport\n")
    (insert "#+TODO: TODO DONE WAIT\n\n")
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert "CLOSED: [2026-05-27 Tue 10:00]\n")
    (insert "Body beta.\n\n")
    (insert "** WAIT Gamma\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert "Body gamma.\n\n")
    (let* ((snap (lambda ()
                   (let* ((tree (org-element-parse-buffer))
                          (info (org-combine-plists
                                 (org-export--get-buffer-attributes)
                                 (org-export-get-environment)))
                          (headlines (org-element-map tree 'headline #'identity)))
                     (mapcar (lambda (h)
                               (list (org-element-property :raw-value h)
                                     (org-element-property :level h)
                                     (org-export-get-headline-number h info)
                                     (org-export-get-tags h info)
                                     (org-export-get-todo-keyword h info)))
                             headlines)))))
      (let ((before (funcall snap)))
        ;; Edit: change WAIT to TODO
        (goto-char (point-min))
        (search-forward "WAIT Gamma")
        (replace-match "TODO Gamma")
        ;; Change Alpha to DONE
        (goto-char (point-min))
        (search-forward "TODO Alpha")
        (replace-match "DONE Alpha")
        (let ((after (funcall snap)))
          ;; Export
          (let ((html (org-export-as 'html nil nil t '(:with-toc nil))))
            (list before after html
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_export_tags_property_planning_edit_reexport_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: DeepExport\n")
    (insert "#+TODO: TODO DONE WAIT\n")
    (insert "#+FILETAGS: :project:\n\n")
    (insert "* TODO Alpha :work:\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:CUSTOM_ID: alpha\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta :home:\n")
    (insert "CLOSED: [2026-05-27 Tue 10:00]\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body beta.\n\n")
    (insert "** WAIT Gamma :work:urgent:\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert "Body gamma.\n\n")
    (let* ((snap (lambda ()
                   (let* ((tree (org-element-parse-buffer))
                          (info (org-combine-plists
                                 (org-export--get-buffer-attributes)
                                 (org-export-get-environment)))
                          (headlines (org-element-map tree 'headline #'identity)))
                     (mapcar (lambda (h)
                               (list (org-element-property :raw-value h)
                                     (org-element-property :level h)
                                     (org-export-get-headline-number h info)
                                     (org-export-get-tags h info)
                                     (org-export-get-todo-keyword h info)))
                             headlines)))))
      (let ((before (funcall snap)))
        ;; Edit: change WAIT to TODO
        (goto-char (point-min))
        (search-forward "WAIT Gamma")
        (replace-match "TODO Gamma")
        ;; Change Alpha to DONE
        (goto-char (point-min))
        (search-forward "TODO Alpha")
        (replace-match "DONE Alpha")
        (let ((after (funcall snap)))
          ;; Export HTML
          (let ((html (org-export-as 'html nil nil t '(:with-toc nil))))
            ;; Export LaTeX
            (let ((latex (org-export-as 'latex nil nil t '(:with-toc nil))))
              (list before after html latex
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_export_footnote_citation_edit_reexport_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 35 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: FootnoteExport\n\n")
    (insert "* Introduction\n")
    (insert "This is a claim[fn:1] with evidence[fn:2].\n\n")
    (insert "* Methods\n")
    (insert "We used approach[fn:1] as described.\n\n")
    (insert "[fn:1] First footnote text.\n")
    (insert "[fn:2] Second footnote text.\n\n")
    (let* ((snap (lambda ()
                   (let ((tree (org-element-parse-buffer)))
                     (list
                      (org-element-map tree 'footnote-reference
                        (lambda (fn)
                          (list (org-element-property :label fn)
                                (org-element-property :footnote-begin fn))))
                      (org-element-map tree 'headline
                        (lambda (hl)
                          (list (org-element-property :raw-value hl)
                                (org-element-property :level hl)))))))))
      (let ((before (funcall snap)))
        (goto-char (point-max))
        (insert "[fn:3] Third footnote added later.\n")
        (goto-char (point-min))
        (search-forward "approach[fn:1]")
        (end-of-line)
        (insert " See also[fn:3].")
        (let ((after (funcall snap)))
          (let ((html (org-export-as 'html nil nil t '(:with-toc nil))))
            (list before after html
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_export_todo_tag_property_planning_html_latex_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: DeepExportCombo\n")
    (insert "#+TODO: TODO DONE WAIT CANCEL\n")
    (insert "#+FILETAGS: :main:\n\n")
    (insert "* TODO Launch :project:critical:\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "DEADLINE: <2026-06-15 Mon>\n")
    (insert ":PROPERTIES:\n:Effort: 40h\n:Budget: 10000\n:CUSTOM_ID: launch\n:END:\n")
    (insert "Launch plan body.\n\n")
    (insert "** DONE Design :design:\n")
    (insert "CLOSED: [2026-05-20 Tue 14:00]\n")
    (insert ":PROPERTIES:\n:Effort: 10h\n:END:\n")
    (insert "Design body.\n\n")
    (insert "** WAIT Approval :admin:\n")
    (insert "DEADLINE: <2026-06-01 Mon>\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Approval body.\n\n")
    (let* ((snap (lambda ()
                   (let* ((tree (org-element-parse-buffer))
                          (info (org-combine-plists
                                 (org-export--get-buffer-attributes)
                                 (org-export-get-environment)))
                          (headlines (org-element-map tree 'headline #'identity)))
                     (mapcar (lambda (h)
                               (list (org-element-property :raw-value h)
                                     (org-element-property :level h)
                                     (org-export-get-headline-number h info)
                                     (org-export-get-tags h info)
                                     (org-export-get-todo-keyword h info)))
                             headlines)))))
      (let ((before (funcall snap)))
        (goto-char (point-min))
        (search-forward "WAIT Approval")
        (replace-match "TODO Approval")
        (let ((after (funcall snap)))
          (let ((html (org-export-as 'html nil nil t '(:with-toc nil)))
                (latex (org-export-as 'latex nil nil t '(:with-toc nil))))
            (list before after html latex
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_export_separate_scheduled_deadline_html_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-todo-keyword)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: SeparatePlanning\n\n")
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-28 Wed>\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert "Body beta.\n\n")
    (insert "** TODO Gamma\n")
    (insert "Body gamma.\n\n")
    (let* ((snap (lambda ()
                   (let* ((tree (org-element-parse-buffer))
                          (info (org-combine-plists
                                 (org-export--get-buffer-attributes)
                                 (org-export-get-environment)))
                          (headlines (org-element-map tree 'headline #'identity)))
                     (mapcar (lambda (h)
                               (list (org-element-property :raw-value h)
                                     (org-element-property :level h)
                                     (org-export-get-headline-number h info)
                                     (org-export-get-tags h info)
                                     (org-export-get-todo-keyword h info)))
                             headlines)))))
      (let ((before (funcall snap)))
        ;; Edit: change Beta to TODO
        (goto-char (point-min))
        (search-forward "DONE Beta")
        (replace-match "TODO Beta")
        (let ((after (funcall snap)))
          ;; Export HTML
          (let ((html (org-export-as 'html nil nil t '(:with-toc nil))))
            (list before after html
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}
