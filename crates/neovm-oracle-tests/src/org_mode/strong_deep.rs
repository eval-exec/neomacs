//! Strong org-mode oracle tests — export options and advanced features.
//!
//! Tests that exercise export options, backend inheritance, and
//! advanced org features that are most likely to expose Neomacs
//! divergences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Export: all 24+ options parsed correctly
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_all_options_parsed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) (#(\"Author\" 0 6 (:parent (#(\"Author\" 0 6 (:parent #4)))))) \"e@e.org\" 3 t t t t t t t t t t t t t t t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test\n#+AUTHOR: Author\n#+EMAIL: e@e.org\n#+DATE: 2024-01-15\n#+OPTIONS: H:3 num:t toc:t \\n:t timestamp:t author:t creator:t d:t email:t *:t e:t ::t f:t pri:t -:t ^:t toc:t |:t tags:t tasks:t <:t todo:t inline:nil stat:t title:t\n#+CATEGORY: test\n* H\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list (plist-get info :title) (plist-get info :author)
              (plist-get info :email) (plist-get info :headline-levels)
              (plist-get info :section-numbers) (plist-get info :with-toc)
              (plist-get info :with-timestamps) (plist-get info :with-author)
              (plist-get info :with-email) (plist-get info :with-emphasize)
              (plist-get info :with-entities) (plist-get info :with-footnotes)
              (plist-get info :with-priority) (plist-get info :with-special-strings)
              (plist-get info :with-sub-superscript) (plist-get info :with-tables)
              (plist-get info :with-tags) (plist-get info :with-tasks)
              (plist-get info :with-todo-keywords) (plist-get info :with-inlinetasks)
              (plist-get info :with-statistics-cookies) (plist-get info :with-title))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: headline numbers at all levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_headline_numbers_all_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+OPTIONS: num:t H:3\n* Ch1\n** S1\n*** SS1\n*** SS2\n** S2\n*** SS3\n* Ch2\n** S3\n*** SS4\n** S4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-relative-level h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-numbered-headline-p h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-low-level-p h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: footnote numbers with nested refs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_footnote_numbers_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2] and[fn:3].\n* H1\nBody[fn:4].\n** H2\nBody[fn:5:nested[fn:6]].\n\n[fn:1] Def 1.\n[fn:2] Def 2 with *bold*.\n[fn:3] Def 3 with [[link]].\n[fn:4] Def 4.\n[fn:6] Deeply nested.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (r) (org-export-get-footnote-number r info))
                 (org-element-map tree 'footnote-reference #'identity))
         (mapcar (lambda (r) (org-export-footnote-first-reference-p r info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: tags and categories at all levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_tags_categories_all_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CATEGORY: work\n* H1 :tag1:\n** H2 :tag2:\n*** H3 :tag3:\n** H2b :tag1:tag2:\n* H1b :tag3:\n** H2c :tag1:tag3:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (h) (org-export-get-tags h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-category h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: first/last sibling detection
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_sibling_detection_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n** H3\n** H4\n* H5")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hls (org-element-map tree 'headline #'identity)))
        (list (mapcar #'org-export-first-sibling-p hls)
              (mapcar #'org-export-last-sibling-p hls))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: filter chain behavior
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_filter_chain_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"210\" \"20\" \"0\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   (org-export-filter-apply-functions
    (list (lambda (v &rest _) (concat "1" v))
          (lambda (v &rest _) (concat "2" v)))
    "0" nil)
   (org-export-filter-apply-functions
    (list #'ignore (lambda (v &rest _) (concat "2" v)))
    "0" nil)
   (org-export-filter-apply-functions (list #'ignore) "0" nil)
   (org-export-filter-apply-functions
    (list (lambda (_ &rest _) "")
          (lambda (v &rest _) (concat "2" v)))
    "0" nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: backend inheritance chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_backend_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((parent) t nil ((lambda (h c i) (format \"C: %s\" (org-element-property :raw-value h))) (lambda (s c i) c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'parent
      '((headline . (lambda (h c i) (format "P: %s" (org-element-property :raw-value h))))
        (section . (lambda (s c i) c))
        (paragraph . (lambda (p c i) c))
        (plain-text . (lambda (t i) t))))
    (org-export-define-derived-backend 'child 'parent
      :translate-alist '((headline . (lambda (h c i) (format "C: %s" (org-element-property :raw-value h))))))
    (list
     (org-export-derived-backend-p 'child 'parent)
     (org-export-derived-backend-p 'child 'child)
     (org-export-derived-backend-p 'parent 'child)
     (let ((all (org-export-get-all-transcoders 'child)))
       (list (cdr (assq 'headline all))
             (cdr (assq 'section all)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: read-attribute edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_read_attribute_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:a \"1\" :b \"2\") nil (:a nil :b nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a 1 :b 2\nP")
        (goto-char (point-min)) (org-element-at-point)))
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "P")
        (goto-char (point-min)) (org-element-at-point)))
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a nil :b nil\nP")
        (goto-char (point-min)) (org-element-at-point))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: caption handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_caption_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Long caption\" 0 12 (:parent (#(\"Long caption\" 0 12 (:parent #4)))))) ((#(\"Long caption\" 0 12 (:parent (#(\"Long caption\" 0 12 (:parent #5)))))) (#(\"short\" 0 5 (:parent (#(\"short\" 0 5 (:parent #5))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "#+CAPTION: Long caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (org-export-get-caption table)))
     (with-temp-buffer (org-mode)
       (insert "#+CAPTION[short]: Long caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (list (org-export-get-caption table)
               (org-export-get-caption table t)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: optional title
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_optional_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-optional-title)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Doc Title\n* H\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment))))
             (hl (car (org-element-map tree 'headline #'identity))))
        (org-export-get-optional-title hl info)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: node property access
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_node_property_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"myid\" \"2h\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:CUSTOM_ID: myid\n:EFFORT: 2h\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity))))
        (list (org-export-get-node-property :CUSTOM_ID hl)
              (org-export-get-node-property :EFFORT hl))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: tag filtering (exclude/select)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_tag_filtering_both() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"H1\" \"H2\" \"H3\") (\"H1\" \"H2\" \"H3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Exclude tags.
     (let ((org-export-exclude-tags '("noexport")))
       (with-temp-buffer (org-mode)
         (insert "* H1 :noexport:\nBody1\n* H2\nBody2\n* H3 :noexport:\nBody3")
         (goto-char (point-min))
         (let* ((tree (org-element-parse-buffer))
                (info (org-combine-plists
                       (org-export--get-export-attributes)
                       (org-export-get-environment)
                       (org-export--collect-tree-properties tree (org-export-get-environment)))))
           (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                   (org-element-map tree 'headline #'identity)))))
     ;; Select tags.
     (let ((org-export-select-tags '("export")))
       (with-temp-buffer (org-mode)
         (insert "* H1 :export:\nBody1\n* H2\nBody2\n* H3 :export:\nBody3")
         (goto-char (point-min))
         (let* ((tree (org-element-parse-buffer))
                (info (org-combine-plists
                       (org-export--get-export-attributes)
                       (org-export-get-environment)
                       (org-export--collect-tree-properties tree (org-export-get-environment)))))
           (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                   (org-element-map tree 'headline #'identity))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: footnote edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_footnote_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2:inline] and[fn::anon].\n\n[fn:1] Standard.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (r) (org-element-property :type r))
                 (org-element-map tree 'footnote-reference #'identity))
         (mapcar (lambda (r) (org-export-get-footnote-number r info))
                 (org-element-map tree 'footnote-reference #'identity))
         (mapcar (lambda (r) (org-export-footnote-first-reference-p r info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: CJK content parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cjk_content_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* 日本語見出し\n本文の段落です。\n** 中文标题\n这是一个段落。\n* 한국어 제목\n한국어 단락입니다.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                 (org-element-map tree 'headline #'identity))
         (length (org-element-map tree 'paragraph #'identity))
         (substring-no-properties (org-element-interpret-data tree))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: special characters parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn special_chars_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Title with \\alpha and \\beta\nPara with $x^2$ and \\[E=mc^2\\].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'entity #'identity))
         (length (org-element-map tree 'latex-fragment #'identity))
         (length (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: complex list nesting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn complex_list_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Item 1\n  - Sub 1.1\n    - Sub-sub\n  - Sub 1.2\n- Item 2\n  1. Ordered 1\n  2. Ordered 2\n- tag :: desc")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'item #'identity))
         (length (org-element-map tree 'plain-list #'identity))
         (mapcar (lambda (l) (org-element-property :type l))
                 (org-element-map tree 'plain-list #'identity))
         (mapcar (lambda (i) (org-element-property :checkbox i))
                 (org-element-map tree 'item #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: complex table structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn complex_table_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| *H1* | /H2/ |\n|------+------|\n| *a*  | /b/  |\n| c    | d    |")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'table-row #'identity))
         (length (org-element-map tree 'table-cell #'identity))
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: complex timestamps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn complex_timestamp_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (6 (active inactive active active-range active-range diary) (nil nil nil daterange timerange nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2024-01-15 Mon>\n[2024-01-15 Mon]\n<2024-01-15 Mon 14:30>\n<2024-01-15 Mon>--<2024-01-16 Tue>\n<2024-01-15 Mon 14:30-15:30>\n<%%(diary-float t 4 2)>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (ts (org-element-map tree 'timestamp #'identity)))
        (list
         (length ts)
         (mapcar (lambda (t) (org-element-property :type t)) ts)
         (mapcar (lambda (t) (org-element-property :range-type t)) ts))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: citations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn citation_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[cite:@k1] [cite/style:@k2] [cite:pre @k3] [cite:@k4 post] [cite:@a;@b;@c]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'citation #'identity))
         (length (org-element-map tree 'citation-reference #'identity))
         (mapcar (lambda (c) (org-element-property :style c))
                 (org-element-map tree 'citation #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: clock in logbook
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn clock_in_logbook_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 (closed closed) (\"1:30\" \"2:00\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task\n:LOGBOOK:\nCLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:30] =>  1:30\nCLOCK: [2024-01-14 Sun 14:00]--[2024-01-14 Sun 16:00] =>  2:00\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (clocks (org-element-map tree 'clock #'identity)))
        (list
         (length clocks)
         (mapcar (lambda (c) (org-element-property :status c)) clocks)
         (mapcar (lambda (c) (org-element-property :duration c)) clocks))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: drawers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn drawer_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:KEY: val\n:END:\n:LOGBOOK:\nNote\n:END:\n:MYDRAWER:\nContent\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'property-drawer #'identity))
         (length (org-element-map tree 'drawer #'identity))
         (mapcar (lambda (d) (org-element-property :drawer-name d))
                 (org-element-map tree 'drawer #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: dynamic blocks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn dynamic_block_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"clocktable\" \"myblock\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: clocktable :scope file\n#+END:\n#+BEGIN: myblock :param val\nContent\n#+END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (b) (org-element-property :block-name b))
                (org-element-map tree 'dynamic-block #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: inlinetasks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn inlinetask_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil) (org-inlinetask-min-level 15))
    (with-temp-buffer (org-mode)
      (insert "* Regular\n*************** TODO Inline :tag:\nBody\n*************** END\n* Another")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'headline #'identity))
         (length (org-element-map tree 'inlinetask #'identity))
         (mapcar (lambda (i) (list (org-element-property :todo-keyword i)
                             (org-element-property :tags i)))
                 (org-element-map tree 'inlinetask #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: export snippets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn export_snippet_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"html\" \"<b>bold</b>\") (\"latex\" \"\\\\textbf{bold}\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "@@html:<b>bold</b>@@ and @@latex:\\textbf{bold}@@.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (s)
                  (list (org-element-property :back-end s)
                        (substring-no-properties (org-element-property :value s))))
                (org-element-map tree 'export-snippet #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: statistics cookies
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn statistics_cookie_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[1/3]\" \"[50%]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H [1/3]\n** S1\n** S2\n** S3\n* H2 [50%]\n** A\n** B")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (c) (substring-no-properties (org-element-property :value c)))
                (org-element-map tree 'statistics-cookie #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: radio targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn radio_target_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<<radio1>>> and <<<radio2>>>.\n<<<radio with \\alpha entity>>>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (length (org-element-map tree 'radio-target #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: diary sexps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn diary_sexp_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "%%(org-anniversary 1956 5 14) Arthur is %d\n%%(diary-float t 4 2)")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (length (org-element-map tree 'diary-sexp #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: horizontal rules and line breaks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn horizontal_rules_and_line_breaks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Line1\\\\\nLine2\n-----\nLine3\\\\\nLine4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'horizontal-rule #'identity))
         (length (org-element-map tree 'line-break #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: macros with arguments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn macro_with_args_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"{{{greet}}}\" \"{{{greet(Beautiful)}}}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: greet Hello\n{{{greet}}} World {{{greet(Beautiful)}}}.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (m) (substring-no-properties (org-element-property :value m)))
                (org-element-map tree 'macro #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: entities in headlines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn entities_in_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* \\alpha title\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'entity #'identity))
         (mapcar (lambda (e) (org-element-property :name e))
                 (org-element-map tree 'entity #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: LaTeX fragments and environments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn latex_fragments_and_environments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text $x^2$ and $E=mc^2$ and $$\\int_0^1 f(x)dx$$.\n\\begin{equation}\nx^2 + y^2 = z^2\n\\end{equation}")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (mapcar (lambda (f) (substring-no-properties (org-element-property :value f)))
                 (org-element-map tree 'latex-fragment #'identity))
         (mapcar (lambda (e) (substring-no-properties (org-element-property :value e)))
                 (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: links in headlines and tables
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn links_in_headlines_and_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* [[https://orgmode.org][Org mode]]\n| [[https://orgmode.org][link]] | plain |")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'link #'identity))
         (length (org-element-map tree 'table-cell #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: footnotes in lists and tables
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn footnotes_in_lists_and_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2) (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "- Item 1[fn:1]\n- Item 2\n\n[fn:1] Note.")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer)))
         (list (length (org-element-map tree 'footnote-reference #'identity))
               (length (org-element-map tree 'item #'identity)))))
     (with-temp-buffer (org-mode)
       (insert "| cell[fn:1] | other |\n\n[fn:1] Note.")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer)))
         (list (length (org-element-map tree 'footnote-reference #'identity))
               (length (org-element-map tree 'table-cell #'identity))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: overlapping inline markup
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn overlapping_inline_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "before *bold /italic/ bold* after")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (mapcar (lambda (b) (substring-no-properties
                         (org-element-interpret-data b)))
                 (org-element-map tree 'bold #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: empty/minimal buffers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn empty_buffer_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (org-data nil ((org-data (:standard-properties [1 1 1 1 1 0 nil org-data nil nil nil nil 1 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (let* ((tree (org-element-parse-buffer)))
        (list (org-element-type tree)
              (org-element-contents tree)
              (org-element-map tree t #'identity))))))"##,
        expect,
    );
}

#[test]
fn single_star_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (org-data nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "*")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list (org-element-type tree)
              (org-element-map tree 'headline
                (lambda (h) (substring-no-properties (org-element-property :raw-value h)))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: narrowing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nBody1\n* H2\nBody2\n* H3\nBody3")
      (narrow-to-region 1 20)
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: at-point edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn at_point_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (paragraph center-block center-block headline paragraph)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\nText\n#+END_CENTER\n\n* H\nBody")
      (list
       (progn (goto-char (point-min)) (forward-line 1)
              (org-element-type (org-element-at-point)))
       (progn (goto-char (point-min))
              (org-element-type (org-element-at-point)))
       (progn (goto-char (point-min)) (forward-line 3)
              (org-element-type (org-element-at-point)))
       (progn (goto-char (point-min)) (forward-line 4)
              (org-element-type (org-element-at-point)))
       (progn (goto-char (point-min)) (forward-line 5)
              (org-element-type (org-element-at-point)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: context edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn context_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic link paragraph)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text *bold* /italic/ and [[link]].")
      (list
       (progn (goto-char (point-min)) (search-forward "bold")
              (org-element-type (org-element-context)))
       (progn (goto-char (point-min)) (search-forward "italic")
              (org-element-type (org-element-context)))
       (progn (goto-char (point-min)) (search-forward "link")
              (org-element-type (org-element-context)))
       (progn (goto-char (point-min)) (search-forward "Text")
              (org-element-type (org-element-context)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: secondary string parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn secondary_string_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (plain-text bold plain-text italic))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Title with *bold* and /italic/")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity)))
             (title (org-element-property :title hl)))
        (list (listp title)
              (mapcar #'org-element-type title))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element set with keep-props
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_set_keep_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bar bar2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((result (org-element-set
                 (org-element-create 'dummy '(:foo bar))
                 (org-element-create 'dummy '(:foo2 bar2))
                 '(:foo))))
    (list (org-element-property :foo result)
          (org-element-property :foo2 result))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element uniq
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_uniq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-uniq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((p1 (org-element-create 'paragraph nil "p1"))
         (p2 (org-element-create 'paragraph nil "p2"))
         (h1 (org-element-create 'headline '(:level 1)))
         (list (list p1 p2 h1 p1 p2 h1 p1)))
    (list (length list)
          (length (org-element-uniq list))
          (mapcar #'org-element-type (org-element-uniq list)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element secondary-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_secondary_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:title :foo nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* Headline *object*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (o) (org-element-secondary-p o))
         nil t))
     (org-element-secondary-p
      (let* ((el (org-element-create 'd '(:secondary (:foo))))
             (ch (org-element-create "str" `(:parent ,el))))
        (org-element-put-property el :foo (list ch)) ch))
     (with-temp-buffer (org-mode) (insert "Para *obj*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (o) (org-element-type (org-element-secondary-p o)))
         nil t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element deferred
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_deferred_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (cl-assertion-failed (listp args))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (let ((el (org-element-create 'd
              `(:deferred ,(org-element-deferred-create t
                            (lambda (el) (org-element-put-property el :foo 'bar) nil))))))
     (list (org-element-property :foo el) (org-element-property :foo2 el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (org-element-property :foo el))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create t (lambda (_) 'bar))))))
     (list (org-element-property :foo el) (org-element-property-raw :foo el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (list (org-element-property :foo el)
           (org-element-property-raw :foo el)
           (org-element-property :foo el nil 'force)
           (org-element-property-raw :foo el)))
   (let ((el (org-element-create 'd `( :foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el) (org-element-property :bar el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create-list
                              (list 1 2 (org-element-deferred-create nil (lambda _) 3)))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element map with various options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_map_various_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nPara *bold* /italic/.\n** H2\nMore text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree t #'identity))
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (mapcar #'org-element-type
                 (org-element-map tree '(bold italic) #'identity))
         (org-element-property :raw-value
           (org-element-map tree 'headline #'identity nil t))
         (length (org-element-map tree 'bold #'identity nil nil 'paragraph))
         (length (org-element-map tree 'bold #'identity nil nil nil nil t))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element ast-map
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_ast_map_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((anon plain-text plain-text bold) (anon plain-text plain-text) (bold bold) (bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-ast-map
    (org-element-create 'anon nil "a" "b" (org-element-create 'bold))
    t #'org-element-type)
   (let ((b (org-element-create 'bold)))
     (org-element-ast-map
      (org-element-create 'anon nil "a" "b" b)
      t #'org-element-type (list b)))
   (org-element-ast-map
    (org-element-create 'd `(:foo ,(org-element-create 'bold))
      (org-element-create 'bold))
    'bold #'org-element-type nil nil nil '(:foo))
   (org-element-ast-map
    (org-element-create 'd `(:secondary (:foo) :foo ,(org-element-create 'bold))
      (org-element-create 'bold))
    'bold #'org-element-type nil nil nil nil 'no-secondary)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element properties-mapc
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_properties_mapc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'd
             `( :foo ,(org-element-deferred-create t (lambda (_) 1))
                :bar 2))))
    (list
     (catch :found
       (org-element-properties-mapc
        (lambda (_ val _) (when (org-element-deferred-p val) (throw :found t)))
        el))
     (catch :found
       (org-element-properties-mapc
        (lambda (prop val _) (when (and (eq prop :foo) (eq 1 val)) (throw :found t)))
        el 'undefer)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element properties-map
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_properties_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 nil) (1 2 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'd '(:foo 1 :bar 2 :baz 3))))
    (list
     (org-element-properties-map #'identity el)
     (org-element-properties-map (lambda (p v) (unless (eq p :baz) v)) el)
     (org-element-properties-map
      (lambda (p v n) (if (eq p :baz) (1+ (org-element-property-raw :baz n)) v)) el))))"##,
        expect,
    );
}
