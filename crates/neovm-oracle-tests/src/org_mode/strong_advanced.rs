//! Strong org-mode oracle tests — complex export and advanced features.
//!
//! Tests that exercise export backends, filter chains, and advanced
//! org features that are most likely to expose Neomacs divergences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Export: full document with all features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_full_document() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Full
#+AUTHOR: Test
#+OPTIONS: num:t toc:nil
* TODO [#A] Ch1 :tag:
SCHEDULED: <2024-01-15 Mon>
DEADLINE: <2024-01-19 Fri>
:PROPERTIES:
:CUSTOM_ID: ch1
:END:
Body with *bold* and /italic/.
** S1
| a | b |
| c | d |
* Ch2
- item1
- item2
[fn:1] Footnote.
#+BEGIN_SRC emacs-lisp
(+ 1 2)
#+END_SRC")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (plist-get info :title)
         (plist-get info :author)
         (plist-get info :with-toc)
         (mapcar (lambda (h) (list (org-export-get-headline-number h info)
                             (org-export-get-relative-level h info)
                             (org-export-numbered-headline-p h info)))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: backend definition and inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_backend_inheritance() {
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
// Export: filter chain behavior
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_filter_chain_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"210\" \"20\" \"0\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; Applied in order.
   (org-export-filter-apply-functions
    (list (lambda (v &rest _) (concat "1" v))
          (lambda (v &rest _) (concat "2" v)))
    "0" nil)
   ;; Nil skipped.
   (org-export-filter-apply-functions
    (list #'ignore (lambda (v &rest _) (concat "2" v)))
    "0" nil)
   ;; All skipped.
   (org-export-filter-apply-functions (list #'ignore) "0" nil)
   ;; Empty short-circuits.
   (org-export-filter-apply-functions
    (list (lambda (_ &rest _) "")
          (lambda (v &rest _) (concat "2" v)))
    "0" nil)))"##,
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
// Export: read-attribute edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_read_attribute_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:a \"1\" :b \"2\") nil (:a nil))""#]];
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
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a nil\nP")
        (goto-char (point-min)) (org-element-at-point))))))"##,
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
fn strong_export_node_property() {
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
// Advanced: org-element with CJK content
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
// Advanced: org-element with special characters
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
// Advanced: org-element with complex list nesting
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
// Advanced: org-element with complex table
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
// Advanced: org-element with complex timestamps
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
// Advanced: org-element with citations
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
// Advanced: org-element with clock in logbook
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
// Advanced: org-element with drawers
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
// Advanced: org-element with dynamic blocks
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
// Advanced: org-element with inlinetasks
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
// Advanced: org-element with export snippets
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
// Advanced: org-element with statistics cookies
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
// Advanced: org-element with radio targets
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
// Advanced: org-element with diary sexps
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
// Advanced: org-element with horizontal rules
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn horizontal_rule_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Above\n-----\nBelow\n--------\nEnd")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (length (org-element-map tree 'horizontal-rule #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with line breaks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn line_break_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Line1\\\\\nLine2\\\\\nLine3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (length (org-element-map tree 'line-break #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with macros
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn macro_parsing() {
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
// Advanced: org-element with entities
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn entity_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"alpha\" \"beta\" \"gamma\" \"delta\" \"epsilon\" \"omega\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\alpha \\beta \\gamma \\delta \\epsilon \\omega")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (e) (org-element-property :name e))
                (org-element-map tree 'entity #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with LaTeX fragments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn latex_fragment_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"$x^2$\" \"$E=mc^2$\" \"$$\\\\int_0^1 f(x)dx$$\" \"\\\\(y\\\\)\" \"\\\\[z\\\\]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text $x^2$ and $E=mc^2$ and $$\\int_0^1 f(x)dx$$ and \\(y\\) and \\[z\\].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (f) (substring-no-properties (org-element-property :value f)))
                (org-element-map tree 'latex-fragment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with LaTeX environments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn latex_environment_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\begin{equation}\\nx^2 + y^2 = z^2\\n\\\\end{equation}\\n\" \"\\\\begin{align}\\na &= 1\\nb &= 2\\n\\\\end{align}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\begin{equation}\nx^2 + y^2 = z^2\n\\end{equation}\n\\begin{align}\na &= 1\nb &= 2\n\\end{align}")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (e) (substring-no-properties (org-element-property :value e)))
                (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn target_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<target1>> and <<target2>> and <<target3>>.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (length (org-element-map tree 'target #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with all inline markup types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn all_inline_markup_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text with *bold*, /italic/, _underline_, =verbatim=, ~code~, +strike+.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         (length (org-element-map tree 'strike-through #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with links in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn links_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "https://example.org\n[[https://example.org][link]]\n[[file:path.org]]\n[[id:uuid]]\n[[#custom-id]]\n<https://angular.org>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (l) (org-element-property :type l))
                (org-element-map tree 'link #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with footnotes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn footnote_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2:inline def] and[fn::anon].\n\n[fn:1] Standard def.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (mapcar (lambda (r) (org-element-property :type r))
                 (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element property inheritance chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn property_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 (1 2 3) (\"p\") (\"c\") (\"gc\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((gc (org-element-create 'gc '(:shared 3 :own-gc "gc")))
         (c (org-element-create 'c '(:shared 2 :own-c "c") gc))
         (p (org-element-create 'p '(:shared 1 :own-p "p") c)))
    (list
     (org-element-property-inherited :shared gc)
     (org-element-property-inherited :shared gc 'with-self)
     (org-element-property-inherited :shared gc 'with-self 'accumulate)
     (org-element-property-inherited :own-p gc 'with-self 'accumulate)
     (org-element-property-inherited :own-c gc 'with-self 'accumulate)
     (org-element-property-inherited :own-gc gc 'with-self 'accumulate))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element operations chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn element_operations_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* \\nP1.\\n* \\nP2.\\n\" \"* \\nP1.\\n\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((doc (org-element-create 'org-data nil))
         (h1 (org-element-create 'headline '(:level 1 :raw-value "A")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P1.\n"))))
         (h2 (org-element-create 'headline '(:level 1 :raw-value "B")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P2.\n")))))
    (org-element-adopt doc h1 h2)
    (let ((after-adopt (substring-no-properties (org-element-interpret-data doc))))
      (org-element-extract h2)
      (let ((after-extract (substring-no-properties (org-element-interpret-data doc))))
        (list after-adopt after-extract
              (org-element-property :parent h2))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: element deferred chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((bar nil) bar (bar bar) (bar [org-element-deferred (closure (t) (_) 'bar) nil nil] bar bar) (1 1) (1 2 3))""#
    ]];
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
                              (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: parse-and-interpret round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_interpret_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"*text*\\n\" 1 5 (:parent (bold (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) #(\"/text/\\n\" 1 5 (:parent (italic (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) \"~text~\\n\" \"=text=\\n\" #(\"_text_\\n\" 1 5 (:parent (underline (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) #(\"+target+\\n\" 1 7 (:parent (strike-through (:standard-properties [1 nil 2 8 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 9 9 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 9 9 0 nil first-section nil nil nil 1 9 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 9 9 0 nil org-data nil nil nil 3 9 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"target\" 0 6 (:parent #3))))) #(\"a_b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p nil) #(\"b\" 0 1 (:parent #4))))) 2 3 (:parent (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) #(\"a_{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p t) #(\"b\" 0 1 (:parent #4))))) 3 4 (:parent (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) #(\"a^b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p nil) #(\"b\" 0 1 (:parent #4))))) 2 3 (:parent (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) #(\"a^{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p t) #(\"b\" 0 1 (:parent #4))))) 3 4 (:parent (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) #(\"\\\\alpha text\\n\" 7 11 (:parent (paragraph (:standard-properties [1 1 1 12 12 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 12 12 0 nil first-section nil nil nil 1 12 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 12 12 0 nil org-data nil nil nil 3 12 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) (entity (:standard-properties [1 nil nil nil 8 1 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p nil)) #(\"text\" 0 4 (:parent #3))))) #(\"\\\\alpha{}text\\n\" 8 12 (:parent (paragraph (:standard-properties [1 1 1 13 13 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 13 13 0 nil first-section nil nil nil 1 13 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 13 13 0 nil org-data nil nil nil 3 13 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) (entity (:standard-properties [1 nil nil nil 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p t)) #(\"text\" 0 4 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "*text*") (funcall f "/text/") (funcall f "~text~")
     (funcall f "=text=") (funcall f "_text_") (funcall f "+target+")
     (funcall f "a_b") (funcall f "a_{b}") (funcall f "a^b") (funcall f "a^{b}")
     (funcall f "\\alpha text") (funcall f "\\alpha{}text"))))"##,
        expect,
    );
}

#[test]
fn link_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[[https://orgmode.org]]\\n\" #(\"[[https://orgmode.org][Org mode]]\\n\" 23 31 (:parent (link (:standard-properties [1 nil 24 32 34 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 34 34 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 34 34 0 nil first-section nil nil nil 1 34 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 34 34 0 nil org-data nil nil nil 3 34 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)] :type \"https\" :type-explicit-p t :path \"//orgmode.org\" :format bracket :raw-link \"https://orgmode.org\" :application nil :search-option nil) #(\"Org mode\" 0 8 (:parent #3))))) \"[[file:todo.org::*task]]\\n\" \"[[id:aaaa]]\\n\" \"[[#id]]\\n\" \"https://orgmode.org\\n\" \"<https://orgmode.org>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "[[https://orgmode.org]]")
     (funcall f "[[https://orgmode.org][Org mode]]")
     (funcall f "[[file:todo.org::*task]]")
     (funcall f "[[id:aaaa]]")
     (funcall f "[[#id]]")
     (funcall f "https://orgmode.org")
     (funcall f "<https://orgmode.org>"))))"##,
        expect,
    );
}

#[test]
fn footnote_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"Text[fn:1]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 11 11 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 11 11 0 nil first-section nil nil nil 1 11 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 11 11 0 nil org-data nil nil nil 3 11 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil nil nil 11 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"1\" :type standard))))) #(\"Text[fn:label]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 15 15 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 15 15 0 nil first-section nil nil nil 1 15 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 15 15 0 nil org-data nil nil nil 3 15 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"label\" :type standard))))) #(\"Text[fn:label:def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #4))))) 14 17 (:parent (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #3))))) #(\"Text[fn::def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label nil :type inline) #(\"def\" 0 3 (:parent #4))))) 9 12 (:parent (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label nil :type inline) #(\"def\" 0 3 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "Text[fn:1]") (funcall f "Text[fn:label]")
     (funcall f "Text[fn:label:def]") (funcall f "Text[fn::def]"))))"##,
        expect,
    );
}

#[test]
fn block_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (#(\"#+begin_center\\nText\\n#+end_center\\n\" 15 20 (:parent (paragraph (:standard-properties [16 16 16 21 21 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (center-block (:standard-properties [1 1 16 21 33 0 nil top-comment nil nil nil 16 21 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 33 33 0 nil first-section nil nil nil 1 33 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 33 33 0 nil org-data nil nil nil 3 33 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"Text\\n\" 0 5 (:parent #3))))) #(\"#+begin_quote\\nText\\n#+end_quote\\n\" 14 19 (:parent (paragraph (:standard-properties [15 15 15 20 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (quote-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil 15 20 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"Text\\n\" 0 5 (:parent #3))))) \"#+begin_example\\nTest\\n#+end_example\\n\" \"#+begin_export HTML\\n<p>Text</p>\\n#+end_export\\n\" #(\"#+begin_verse\\nTest\\n#+end_verse\\n\" 14 19 (:parent (verse-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Test\\n\" 0 5 (:parent #3))))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-src-preserve-indentation t)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "#+BEGIN_CENTER\nText\n#+END_CENTER")
     (funcall f "#+BEGIN_QUOTE\nText\n#+END_QUOTE")
     (funcall f "#+BEGIN_EXAMPLE\nTest\n#+END_EXAMPLE")
     (funcall f "#+BEGIN_EXPORT HTML\n<p>Text</p>\n#+END_EXPORT")
     (funcall f "#+BEGIN_VERSE\nTest\n#+END_VERSE"))))"##,
        expect,
    );
}

#[test]
fn inline_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"call_test()\\n\" \"call_test(x=2)\\n\" \"src_emacs-lisp{(+ 1 1)}\\n\" \"@@backend:contents@@\\n\" \"\\\\command{}\\n\" \"$x$\\n\" \"$$x+y$$\\n\" \"\\\\(x+y\\\\)\\n\" \"\\\\[x+y\\\\]\\n\" \"[0/1]\\n\" \"[66%]\\n\" \"<<target>>\\n\" #(\"<<<some text>>>\\n\" 3 12 (:parent (radio-target (:standard-properties [1 nil 4 13 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)] :value \"some text\") #(\"some text\" 0 9 (:parent #3))))) \"{{{test}}}\\n\" \"{{{test(arg1,arg2)}}}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "call_test()") (funcall f "call_test(x=2)")
     (funcall f "src_emacs-lisp{(+ 1 1)}") (funcall f "@@backend:contents@@")
     (funcall f "\\command{}") (funcall f "$x$") (funcall f "$$x+y$$")
     (funcall f "\\(x+y\\)") (funcall f "\\[x+y\\]")
     (funcall f "[0/1]") (funcall f "[66%]")
     (funcall f "<<target>>") (funcall f "<<<some text>>>")
     (funcall f "{{{test}}}") (funcall f "{{{test(arg1,arg2)}}}"))))"##,
        expect,
    );
}

#[test]
fn table_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"| a | b |\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3)]) #(\"b\" 0 1 (:parent #3)))) 12 13 (:parent (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) #6)] :type standard) #3 (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"d\" 0 1 (:parent #7))))]) #(\"c\" 0 1 (:parent #3)))) 16 17 (:parent (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) #6)] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))) #3)]) #(\"d\" 0 1 (:parent #3))))) #(\"| a | b |\\n|---+---|\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3)]) #(\"b\" 0 1 (:parent #3)))) 22 23 (:parent (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) #6)] :type standard) #3 (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"d\" 0 1 (:parent #7))))]) #(\"c\" 0 1 (:parent #3)))) 26 27 (:parent (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) #6)] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))) #3)]) #(\"d\" 0 1 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "| a | b |\n| c | d |")
     (funcall f "| a | b |\n|---+---|\n| c | d |"))))"##,
        expect,
    );
}

#[test]
fn timestamp_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (string-match "<2012-03-29 .* 16:40>" (funcall f "<2012-03-29 thu. 16:40>"))
     (string-match "\\[2012-03-29 .* 16:40\\]" (funcall f "[2012-03-29 thu. 16:40]"))
     (string-match "<2012-03-29 .* 16:40-16:41>" (funcall f "<2012-03-29 thu. 16:40-16:41>"))
     (string-match "<2012-03-29 .* \\+1y>" (funcall f "<2012-03-29 thu. +1y>"))
     (equal "<%%(diary-float t 4 2)>\n" (funcall f "<%%(diary-float t 4 2)>"))))"##,
        expect,
    );
}

#[test]
fn keyword_comment_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#+keyword: value\\n\" \"# Comment\\n\" \"#+begin_comment\\nTest\\n#+end_comment\\n\" \": Test\\n\" \"-----\\n\" \"\\\\begin{equation}\\n1+1=2\\n\\\\end{equation}\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "#+KEYWORD: value") (funcall f "# Comment")
     (funcall f "#+BEGIN_COMMENT\nTest\n#+END_COMMENT")
     (funcall f ": Test") (funcall f "-------")
     (funcall f "\\begin{equation}\n1+1=2\n\\end{equation}"))))"##,
        expect,
    );
}

#[test]
fn citation_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[cite:@key]\\n\" \"[cite/style:@key]\\n\" #(\"[cite:pre @key]\\n\" 6 10 (:parent (citation-reference (:standard-properties [7 nil nil nil 15 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (citation (:standard-properties [1 nil 7 15 16 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)]) #6)] :style nil) #3)] :key \"key\" :prefix (#(\"pre \" 0 4 (:parent #3))))))) #(\"[cite:@key post]\\n\" 10 15 (:parent (citation-reference (:standard-properties [7 nil nil nil 16 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (citation (:standard-properties [1 nil 7 16 17 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 17 17 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 17 17 0 nil first-section nil nil nil 1 17 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 17 17 0 nil org-data nil nil nil 3 17 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)]) #6)] :style nil) #3)] :key \"key\" :suffix (#(\" post\" 0 5 (:parent #3))))))) \"[cite:@a;@b;@c]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "[cite:@key]") (funcall f "[cite/style:@key]")
     (funcall f "[cite:pre @key]") (funcall f "[cite:@key post]")
     (funcall f "[cite:@a;@b;@c]"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: export options parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn export_options_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) (#(\"Author\" 0 6 (:parent (#(\"Author\" 0 6 (:parent #4)))))) \"e@e.org\" 3 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test\n#+AUTHOR: Author\n#+EMAIL: e@e.org\n#+DATE: 2024-01-15\n#+OPTIONS: H:3 num:t toc:t\n#+CATEGORY: test\n* H\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list (plist-get info :title) (plist-get info :author)
              (plist-get info :email) (plist-get info :headline-levels)
              (plist-get info :section-numbers) (plist-get info :with-toc))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: export headline features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn export_headline_features() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TODO\" \"DONE\" nil) (65 66 nil) ((\"tag1\") nil nil) ((1) (2) (3)) (1 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] H1 :tag1:\n* DONE [#B] H2\n* Normal H3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment))))
             (hls (org-element-map tree 'headline #'identity)))
        (list
         (mapcar (lambda (h) (org-element-property :todo-keyword h)) hls)
         (mapcar (lambda (h) (org-element-property :priority h)) hls)
         (mapcar (lambda (h) (org-element-property :tags h)) hls)
         (mapcar (lambda (h) (org-export-get-headline-number h info)) hls)
         (mapcar (lambda (h) (org-export-get-relative-level h info)) hls))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: export sibling detection
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn export_sibling_detection() {
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
// Advanced: export tag filtering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn export_tag_filtering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil) (org-export-exclude-tags '("noexport")))
    (with-temp-buffer (org-mode)
      (insert "* H1 :noexport:\nBody1\n* H2\nBody2\n* H3 :noexport:\nBody3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: map-entries with various matchers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn map_entries_various_matchers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 11) (1) (6) (11) (1) (1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
       (goto-char (point-min)) (org-map-entries #'point))
     (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
       (goto-char (point-min)) (let (org-odd-levels-only) (org-map-entries #'point "LEVEL=1")))
     (with-temp-buffer (org-mode) (insert "* H1\n* TODO H2\n* DONE H3")
       (goto-char (point-min)) (org-map-entries #'point "TODO=\"TODO\""))
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n* H2 :yes:")
       (goto-char (point-min)) (org-map-entries #'point "yes"))
     (with-temp-buffer (org-mode) (insert "* [#A] H1\n* [#B] H2")
       (goto-char (point-min)) (org-map-entries #'point "PRIORITY=\"A\""))
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:TEST: 1\n:END:\n* H2\n:PROPERTIES:\n:TEST: 2\n:END:")
       (goto-char (point-min)) (org-map-entries #'point "TEST=1")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: entry-blocked-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn entry_blocked_p_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (list
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** TODO two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** DONE two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* DONE Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: find-olp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn find_olp_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n* Headline\n** COMMENT headline2\n** TODO headline3\n*** [#A] headline4 :tags:\n** [#A]headline5\n** [0%] headline6\n** headline7 [100%]\n** headline8 [1/5] :some:more:tags:\n* Test")
      (goto-char (point-min))
      (list
       (org-find-olp '("Headline") t)
       (org-find-olp '("Headline" "headline2") t)
       (org-find-olp '("Headline" "headline3") t)
       (org-find-olp '("Headline" "headline3" "headline4") t)
       (org-find-olp '("Headline" "headline6") t)
       (org-find-olp '("Headline" "headline7") t)
       (org-find-olp '("Headline" "headline8") t)))))"##,
        expect,
    );
}
