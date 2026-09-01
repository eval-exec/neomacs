//! Strong export-backends oracle tests — test export with different backends.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn eb_html_export_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 368 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* Heading\nBody text")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "<h1>" html)
          (string-match-p "Body text" html)
          (string-match-p "Test" html)))))"##,
        expect,
    );
}

#[test]
fn eb_latex_export_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 41 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-latex)
  (with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* Heading\nBody text")
  (let ((latex (org-export-as 'latex nil nil t)))
    (list (string-match-p "\\\\section" latex)
          (string-match-p "Body text" latex)
          (string-match-p "Test" latex)))))"##,
        expect,
    );
}

#[test]
fn eb_ascii_export_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 23 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-ascii)
  (with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* Heading\nBody text")
  (let ((ascii (org-export-as 'ascii nil nil t)))
    (list (string-match-p "Heading" ascii)
          (string-match-p "Body text" ascii)
          (string-match-p "Test" ascii)))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_with_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 283)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+OPTIONS: toc:nil num:nil\n* H1\n** H2\nBody")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "<h1>" html)
          (string-match-p "<h2>" html)
          (string-match-p "Body" html)))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_with_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+ATTR_HTML: :id myid :class myclass\n* Heading\nBody")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "myid" html)
          (string-match-p "myclass" html))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_link_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "See [[https://example.com][web]] and [[file:test.png][img]]")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "example.com" html)
          (string-match-p "test.png" html))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_inline_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "*bold* /italic/ =code= ~verbatim~ _underlined_ +strikethrough+")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "<b>" html)
          (string-match-p "<i>" html)
          (string-match-p "<code>" html)
          (string-match-p "<span" html))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "<table" html)
          (string-match-p "<tr" html)
          (string-match-p "<td" html))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_src_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "<pre" html)
          (string-match-p "(+ 1 2)" html))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_footnote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Footnote content")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "Footnote content" html)
          (string-match-p "fn" html))))"##,
        expect,
    );
}

#[test]
fn eb_html_export_image() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My image\n[[file:test.png]]")
  (let ((html (org-export-as 'html nil nil t)))
    (list (string-match-p "test.png" html)
          (string-match-p "My image" html))))"##,
        expect,
    );
}

#[test]
fn eb_latex_export_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-latex)
  (with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (let ((latex (org-export-as 'latex nil nil t)))
    (list (string-match-p "tabular" latex)
          (string-match-p "a" latex))))"##,
        expect,
    );
}

#[test]
fn eb_latex_export_src_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-latex)
  (with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let ((latex (org-export-as 'latex nil nil t)))
    (list (string-match-p "verbatim" latex)
          (string-match-p "(+ 1 2)" latex))))"##,
        expect,
    );
}

#[test]
fn eb_ascii_export_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-ascii)
  (with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (let ((ascii (org-export-as 'ascii nil nil t)))
    (list (string-match-p "a" ascii)
          (string-match-p "1" ascii))))"##,
        expect,
    );
}

#[test]
fn eb_export_backend_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (html)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (let ((be (org-export-get-backend 'html)))
    (list (org-export-backend-name be))))"##,
        expect,
    );
}

#[test]
fn eb_export_with_filters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) (\"H1\" \"H2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\n** H2\nBody")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (headlines (org-element-map tree 'headline
                      (lambda (h) (org-element-property :raw-value h)))))
    (list (plist-get info :title) headlines))))"##,
        expect,
    );
}

#[test]
fn eb_export_plist_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H\nBody")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title))))"##,
        expect,
    );
}

#[test]
fn eb_export_collect_all_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"T\" 0 1 (:parent (#(\"T\" 0 1 (:parent #4)))))) (#(\"A\" 0 1 (:parent (#(\"A\" 0 1 (:parent #4)))))) \"e\" (#(\"d\" 0 1 (:parent (#(\"d\" 0 1 (:parent #4)))))) \"en\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+AUTHOR: A\n#+EMAIL: e\n#+DATE: d\n#+DESCRIPTION: desc\n#+KEYWORDS: kw\n#+LANGUAGE: en\n#+SELECT_TAGS: export\n#+EXCLUDE_TAGS: noexport\n#+OPTIONS: toc:2 num:t")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title)
          (plist-get info :author)
          (plist-get info :email)
          (plist-get info :date)
          (plist-get info :language))))"##,
        expect,
    );
}

#[test]
fn eb_export_element_map_with_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"H1\" \"H2\" \"H3\") \"H1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let* ((tree (org-element-parse-buffer))
         (all (org-element-map tree 'headline
                (lambda (h) (org-element-property :raw-value h))))
         (first (org-element-map tree 'headline
                  (lambda (h) (org-element-property :raw-value h))
                  nil 'first-match)))
    (list all first)))"##,
        expect,
    );
}

#[test]
fn eb_export_element_map_no_recurse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"H2a\" \"H3\" \"H2b\") (\"H2a\" \"H3\" \"H2b\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\n** H2b")
  (let* ((tree (org-element-parse-buffer))
         (h1 (car (org-element-map tree 'headline (lambda (h) h))))
         (direct (org-element-map (org-element-contents h1) 'headline
                   (lambda (h) (org-element-property :raw-value h))
                   nil nil nil t))
         (recursive (org-element-map (org-element-contents h1) 'headline
                      (lambda (h) (org-element-property :raw-value h)))))
    (list direct recursive)))"##,
        expect,
    );
}

#[test]
fn eb_export_element_map_with_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let* ((tree (org-element-parse-buffer))
         (result (org-element-map tree 'headline
                   (lambda (h info)
                     (list (org-element-property :raw-value h)
                           (plist-get info :first-match)))
                   nil 'first-match)))
    result)"##,
        expect,
    );
}

#[test]
fn eb_export_element_map_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n- item\n| tbl |\n#+BEGIN_SRC\n(+ 1)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (types (org-element-map tree (lambda (el) (org-element-type el)))))
    types)"##,
        expect,
    );
}

#[test]
fn sb_export_element_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n** Sub\nBody\n- item\n| tbl |")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (children (mapcar 'org-element-type (org-element-contents h))))
    children)"##,
        expect,
    );
}

#[test]
fn sb_export_element_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n** Sub\nBody")
  (let* ((tree (org-element-parse-buffer))
         (para (car (org-element-map tree 'paragraph (lambda (p) p))))
         (parent (org-element-property :parent para))
         (grandparent (org-element-property :parent parent)))
    (list (org-element-type para)
          (org-element-type parent)
          (org-element-type grandparent)))"##,
        expect,
    );
}

#[test]
fn sb_export_element_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n- item\n| tbl |")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (p (car (org-element-map tree 'paragraph (lambda (p) p))))
         (pl (car (org-element-map tree 'plain-list (lambda (l) l)))))
    (list (org-element-type-p h 'headline)
          (org-element-type-p h 'paragraph)
          (org-element-type-p p 'paragraph)
          (org-element-type-p p 'headline)
          (org-element-type-p pl 'plain-list)))"##,
        expect,
    );
}

#[test]
fn sb_export_element_property_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+FILETAGS: :global:\n* Parent :local:\n** Child")
  (goto-char (point-min))
  (search-forward "Child")
  (let* ((tree (org-element-parse-buffer))
         (child (car (org-element-map tree 'headline
                       (lambda (h) (when (string= (org-element-property :raw-value h) "Child") h))))))
    (list (org-element-property :tags child)))"##,
        expect,
    );
}

#[test]
fn sb_export_element_ancestor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (search-forward "Body")
  (let* ((para (org-element-at-point))
         (h3 (org-element-property :parent para))
         (h2 (org-element-property :parent h3))
         (h1 (org-element-property :parent h2)))
    (list (org-element-type para)
          (org-element-type h3)
          (org-element-type h2)
          (org-element-type h1)))"##,
        expect,
    );
}

#[test]
fn sb_export_element_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (search-forward "Body")
  (let* ((para (org-element-at-point))
         (lineage (org-element-lineage para)))
    (mapcar 'org-element-type lineage))"##,
        expect,
    );
}

#[test]
fn sb_export_element_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P\n** T1\n*** S1\n**** SS1")
  (goto-char (point-min))
  (search-forward "SS1")
  (list (org-get-outline-path)
        (org-current-level)
        (org-get-heading t t t t))"##,
        expect,
    );
}
