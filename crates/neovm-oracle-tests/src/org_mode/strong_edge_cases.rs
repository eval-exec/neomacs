//! Strong org-mode oracle tests — edge cases where Neomacs may diverge.
//!
//! These tests target areas where Neomacs (Rust) and GNU Emacs (C)
//! are most likely to produce different results:
//! - Buffer content after editing operations
//! - Text properties and overlays
//! - Element parsing edge cases
//! - Export output

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Edge case: empty/minimal buffers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_empty_buffer_parse() {
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
fn strong_single_newline_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (org-data 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list (org-element-type tree)
              (length (org-element-map tree t #'identity)))))))"##,
        expect,
    );
}

#[test]
fn strong_single_star_parse() {
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
// Edge case: overlapping inline markup
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_overlapping_markup_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 (\"*bold /italic/ bold* \"))""#]];
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
                 (org-element-map tree 'bold #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: links in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_links_in_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[[https://orgmode.org][Org mode]]\" (\"//orgmode.org\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* [[https://orgmode.org][Org mode]]\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity))))
        (list
         (substring-no-properties (org-element-property :raw-value hl))
         (org-element-map (org-element-property :title hl)
           'link (lambda (l) (org-element-property :path l))))))))"##,
        expect,
    );
}

#[test]
fn strong_links_in_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| [[https://orgmode.org][link]] | plain |")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'link #'identity))
         (length (org-element-map tree 'table-cell #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: footnotes in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Item 1[fn:1]\n- Item 2\n\n[fn:1] Note.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))
         (length (org-element-map tree 'item #'identity))))))"##,
        expect,
    );
}

#[test]
fn strong_footnote_in_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| cell[fn:1] | other |\n\n[fn:1] Note.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))
         (length (org-element-map tree 'table-cell #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: entities in various contexts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entities_in_headline() {
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
// Edge case: LaTeX in various contexts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_latex_in_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"$x^2$\" \"$$E=mc^2$$\" \"\\\\(y\\\\)\" \"\\\\[z\\\\]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text $x^2$ and $$E=mc^2$$ and \\(y\\) and \\[z\\].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (f) (substring-no-properties (org-element-property :value f)))
                (org-element-map tree 'latex-fragment #'identity))))))"##,
        expect,
    );
}

#[test]
fn strong_latex_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\begin{equation}\\nx^2 + y^2 = z^2\\n\\\\end{equation}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\begin{equation}\nx^2 + y^2 = z^2\n\\end{equation}")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (e) (substring-no-properties (org-element-property :value e)))
                (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: macros with arguments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_macro_with_args() {
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
// Edge case: export snippets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_snippets() {
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
// Edge case: radio targets and targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_targets_and_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<my-target>> and <<<my-radio>>> and [[my-target]].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'target #'identity))
         (length (org-element-map tree 'radio-target #'identity))
         (length (org-element-map tree 'link #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: statistics cookies
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_statistics_cookies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[1/3]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H [1/3]\n** S1\n** S2\n** S3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (c) (substring-no-properties (org-element-property :value c)))
                (org-element-map tree 'statistics-cookie #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: inlinetasks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_inlinetask_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil) (org-inlinetask-min-level 15))
    (with-temp-buffer (org-mode)
      (insert "*************** TODO Inline :tag:\nBody\n*************** END")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'inlinetask #'identity))
         (mapcar (lambda (i)
                   (list (org-element-property :todo-keyword i)
                         (org-element-property :tags i)))
                 (org-element-map tree 'inlinetask #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: drawers with various content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_drawer_parse() {
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
// Edge case: dynamic blocks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_dynamic_block_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"clocktable\" \"myblock\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: clocktable :scope file :maxlevel 2\n#+END:\n#+BEGIN: myblock :param val\nContent\n#+END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (mapcar (lambda (b) (org-element-property :block-name b))
                (org-element-map tree 'dynamic-block #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: diary sexps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_diary_sexp_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "%%(org-anniversary 1956 5 14) Arthur is %d years old\n%%(diary-float t 4 2)")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (length (org-element-map tree 'diary-sexp #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: horizontal rules and line breaks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_horizontal_rules_and_line_breaks() {
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
// Edge case: citations with various styles
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_citations_various_styles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[cite:@key] [cite/style:@key] [cite:pre @key] [cite:@key post] [cite:@a;@b;@c]")
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
// Edge case: export with all option types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_options_parsed() {
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
        (list (plist-get info :title)
              (plist-get info :author)
              (plist-get info :email)
              (plist-get info :headline-levels)
              (plist-get info :section-numbers)
              (plist-get info :with-toc))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: org-element-at-point in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_at_point_edge_cases() {
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
       ;; Inside center block.
       (progn (goto-char (point-min)) (forward-line 1)
              (org-element-type (org-element-at-point)))
       ;; At center block boundary.
       (progn (goto-char (point-min))
              (org-element-type (org-element-at-point)))
       ;; At blank line between blocks.
       (progn (goto-char (point-min)) (forward-line 3)
              (org-element-type (org-element-at-point)))
       ;; At headline.
       (progn (goto-char (point-min)) (forward-line 4)
              (org-element-type (org-element-at-point)))
       ;; In body.
       (progn (goto-char (point-min)) (forward-line 5)
              (org-element-type (org-element-at-point)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: context in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_context_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic link paragraph)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text *bold* /italic/ and [[link]].")
      (list
       ;; On bold.
       (progn (goto-char (point-min)) (search-forward "bold")
              (org-element-type (org-element-context)))
       ;; On italic.
       (progn (goto-char (point-min)) (search-forward "italic")
              (org-element-type (org-element-context)))
       ;; On link.
       (progn (goto-char (point-min)) (search-forward "link")
              (org-element-type (org-element-context)))
       ;; On plain text.
       (progn (goto-char (point-min)) (search-forward "Text")
              (org-element-type (org-element-context)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: export with exclude/select tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_tag_filtering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil)
        (org-export-exclude-tags '("noexport")))
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
// Edge case: export first/last sibling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_sibling_detection() {
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
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         (mapcar #'org-export-first-sibling-p headlines)
         (mapcar #'org-export-last-sibling-p headlines))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: element secondary string parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_secondary_string_parsing() {
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
        (list
         (listp title)
         (mapcar #'org-element-type title))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge case: element parse with narrowing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_parse_with_narrowing() {
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
// Edge case: element set with keep-props
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_set_keep_props() {
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
// Edge case: element uniq
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_uniq() {
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
// Edge case: org-element-cache-map
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cache_map_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (org-data headline section drawer paragraph headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* headline\n:DRAWER:\nparagraph\n:END:\n* headline 2")
      (goto-char (point-min))
      (org-element-cache-map #'car :granularity 'element))))"##,
        expect,
    );
}
