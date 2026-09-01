//! Ultra-strict combo tests for org-mode edge cases and deep interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with deeply nested inline markup
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_deeply_nested_inline_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text with *bold* and /italic/ and _underline_ and =verbatim= and ~code~ and +strike+.
Also *bold with /italic inside* and /italic with *bold inside/.
Nested *bold with _underline_ and /italic/ inside*.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; All inline markup types.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         (length (org-element-map tree 'strike-through #'identity))
         ;; Nested markup.
         (let ((bold (car (org-element-map tree 'bold #'identity))))
           (mapcar #'org-element-type
                   (org-element-contents bold)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex table with mixed content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_table_mixed_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| *Header1* | /Header2/ | _Header3_ |
|-----------+-----------+-----------|
| *bold*    | /italic/  | _under_   |
| =verbatim= | ~code~   | +strike+  |
| [[https://orgmode.org][link]] | [[#id][ref]] | plain     |
|-----------+-----------+-----------|
| Sum       |           |           |")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Table structure.
         (length (org-element-map tree 'table-row #'identity))
         (length (org-element-map tree 'table-cell #'identity))
         ;; Inline markup in cells.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         (length (org-element-map tree 'strike-through #'identity))
         ;; Links in cells.
         (length (org-element-map tree 'link #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex list with mixed content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_list_mixed_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- *Bold* item with /italic/ and [[https://orgmode.org][link]]
- [ ] Checkbox item with =verbatim=
- [X] Checked item with ~code~
- [-] Partial item with +strike+
- tag :: Description with _underline_
  - Nested item with *bold*
  1. Ordered inside unordered
  2. Another ordered
- Back to top")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; List structure.
         (length (org-element-map tree 'item #'identity))
         (length (org-element-map tree 'plain-list #'identity))
         ;; Inline markup in items.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         (length (org-element-map tree 'strike-through #'identity))
         (length (org-element-map tree 'underline #'identity))
         ;; Links in items.
         (length (org-element-map tree 'link #'identity))
         ;; Checkboxes.
         (mapcar (lambda (i) (org-element-property :checkbox i))
                 (org-element-map tree 'item #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex headline with all features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_headline_all_features() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Headline with *bold* and /italic/ :tag1:tag2:
SCHEDULED: <2024-01-15 Mon +1w -3d>
DEADLINE: <2024-01-19 Fri -2d>
CLOSED: [2024-01-14 Sun 10:30]
:PROPERTIES:
:CUSTOM_ID: myid
:EFFORT: 2h
:CATEGORY: work
:END:
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
:END:
Body with [[https://orgmode.org][link]] and [fn:1].

[fn:1] Footnote definition.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity))))
        (list
         ;; Headline properties.
         (org-element-property :todo-keyword hl)
         (org-element-property :priority hl)
         (org-element-property :tags hl)
         ;; Planning.
         (let ((planning (org-element-map tree 'planning #'identity nil t)))
           (list (org-element-property :scheduled planning)
                 (org-element-property :deadline planning)
                 (org-element-property :closed planning)))
         ;; Property drawer.
         (length (org-element-map tree 'property-drawer #'identity))
         ;; Logbook.
         (length (org-element-map tree 'drawer #'identity))
         ;; Clocks.
         (length (org-element-map tree 'clock #'identity))
         ;; Links.
         (length (org-element-map tree 'link #'identity))
         ;; Footnotes.
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))
         ;; Inline markup in title.
         (mapcar #'org-element-type
                 (org-element-contents
                  (org-element-map tree 'headline #'identity nil t)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex source block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_source_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SRC emacs-lisp -n -r :results output :exports code :noweb yes
(message \"hello\")
(+ 1 2) ;; (ref:calc)
#+END_SRC

#+BEGIN_SRC python :results value :session py
def hello():
    return \"world\"
#+END_SRC

#+BEGIN_SRC shell :results output
echo \"hello\"
#+END_SRC")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (src-blocks (org-element-map tree 'src-block #'identity)))
        (list
         ;; Number of source blocks.
         (length src-blocks)
         ;; Languages.
         (mapcar (lambda (b) (org-element-property :language b)) src-blocks)
         ;; Switches.
         (mapcar (lambda (b) (org-element-property :switches b)) src-blocks)
         ;; Parameters.
         (mapcar (lambda (b) (org-element-property :parameters b)) src-blocks)
         ;; Values (code content).
         (mapcar (lambda (b) (org-element-property :value b)) src-blocks)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex export block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_export_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_EXPORT html
<p>HTML content with <b>bold</b> and <i>italic</i>.</p>
#+END_EXPORT

#+BEGIN_EXPORT latex
\\textbf{Bold} and \\textit{italic}.
\\begin{equation}
x^2 + y^2 = z^2
\\end{equation}
#+END_EXPORT

#+BEGIN_EXPORT ascii
Plain text content.
#+END_EXPORT")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (export-blocks (org-element-map tree 'export-block #'identity)))
        (list
         ;; Number of export blocks.
         (length export-blocks)
         ;; Types.
         (mapcar (lambda (b) (org-element-property :type b)) export-blocks)
         ;; Values.
         (mapcar (lambda (b) (org-element-property :value b)) export-blocks)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex example block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_example_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_EXAMPLE
Example text
  with indentation
    and more indentation
#+END_EXAMPLE

#+BEGIN_EXAMPLE -n -r
Numbered example
  with switches
#+END_EXAMPLE")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (example-blocks (org-element-map tree 'example-block #'identity)))
        (list
         ;; Number of example blocks.
         (length example-blocks)
         ;; Switches.
         (mapcar (lambda (b) (org-element-property :switches b)) example-blocks)
         ;; Values.
         (mapcar (lambda (b) (org-element-property :value b)) example-blocks)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex quote block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_quote_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_QUOTE
Quoted text with *bold* and /italic/.
Also with [[https://orgmode.org][link]].
#+END_QUOTE

#+BEGIN_QUOTE
Another quote.
#+END_QUOTE")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (quote-blocks (org-element-map tree 'quote-block #'identity)))
        (list
         ;; Number of quote blocks.
         (length quote-blocks)
         ;; Inline markup in quotes.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'link #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex center block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_center_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER
Centered text with *bold* and /italic/.
#+END_CENTER

#+BEGIN_CENTER
Another centered paragraph.
#+END_CENTER")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (center-blocks (org-element-map tree 'center-block #'identity)))
        (list
         ;; Number of center blocks.
         (length center-blocks)
         ;; Inline markup in center blocks.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex verse block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_verse_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_VERSE
Verse line 1
Verse line 2
  with indentation
Verse line 3
#+END_VERSE")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (verse-blocks (org-element-map tree 'verse-block #'identity)))
        (list
         ;; Number of verse blocks.
         (length verse-blocks)
         ;; Types.
         (mapcar #'org-element-type verse-blocks)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex comment block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_comment_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_COMMENT
This is a comment block.
It can contain multiple lines.
#+END_COMMENT

# This is a line comment.
# Another line comment.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Comment blocks.
         (length (org-element-map tree 'comment-block #'identity))
         ;; Line comments.
         (length (org-element-map tree 'comment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex fixed-width scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_fixed_width_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert ": Fixed width line 1
: Fixed width line 2
:   with indentation
: Fixed width line 3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Fixed-width elements.
         (length (org-element-map tree 'fixed-width #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'fixed-width #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex keyword scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_keyword_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test Document
#+AUTHOR: Test Author
#+EMAIL: test@example.org
#+DATE: 2024-01-15
#+DESCRIPTION: A test document
#+KEYWORDS: test org mode
#+LANGUAGE: en
#+OPTIONS: H:3 num:t toc:t
#+CATEGORY: test
#+FILETAGS: :test:org:
#+STARTUP: overview
#+TODO: TODO WAIT | DONE CANCEL
#+TAGS: @work @home @errand
#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS
#+CONSTANTS: pi=3.14 c=299792458
#+LINK: orgmode https://orgmode.org
#+PRIORITIES: A C B")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (keywords (org-element-map tree 'keyword #'identity)))
        (list
         ;; Number of keywords.
         (length keywords)
         ;; Keyword keys.
         (mapcar (lambda (k) (org-element-property :key k)) keywords)
         ;; Keyword values (first 5).
         (mapcar (lambda (k) (org-element-property :value k))
                 (take 5 keywords))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex affiliated keyword scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_affiliated_keyword_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CAPTION: A caption with *bold*
#+NAME: my-element
#+ATTR_HTML: :class my-class :id my-id
| a | b |
| c | d |

#+CAPTION: Another caption
#+ATTR_LATEX: :float t :options [htbp]
#+BEGIN_SRC emacs-lisp
(+ 1 2)
#+END_SRC")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Keywords found.
         (length (org-element-map tree 'keyword #'identity))
         ;; Elements with affiliated keywords.
         (length (org-element-map tree 'table #'identity))
         (length (org-element-map tree 'src-block #'identity))
         ;; Caption on table.
         (org-element-property :caption
           (car (org-element-map tree 'table #'identity)))
         ;; Name on table.
         (org-element-property :name
           (car (org-element-map tree 'table #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex paragraph scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_paragraph_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Paragraph 1 with *bold* and /italic/ and _underline_.

Paragraph 2 with [[https://orgmode.org][link]] and [fn:1].

Paragraph 3 with $x^2$ and \\alpha and {{{macro}}}.

[fn:1] Footnote definition.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Paragraphs.
         (length (org-element-map tree 'paragraph #'identity))
         ;; Inline markup.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         ;; Links.
         (length (org-element-map tree 'link #'identity))
         ;; LaTeX fragments.
         (length (org-element-map tree 'latex-fragment #'identity))
         ;; Entities.
         (length (org-element-map tree 'entity #'identity))
         ;; Macros.
         (length (org-element-map tree 'macro #'identity))
         ;; Footnotes.
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex section scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_section_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1
Paragraph 1.
** H2
Paragraph 2.
*** H3
Paragraph 3.
** H2b
Paragraph 4.
* H1b
Paragraph 5.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Sections.
         (length (org-element-map tree 'section #'identity))
         ;; Headlines.
         (length (org-element-map tree 'headline #'identity))
         ;; Paragraphs.
         (length (org-element-map tree 'paragraph #'identity))
         ;; Hierarchy.
         (mapcar (lambda (h) (list (org-element-property :level h)
                             (substring-no-properties
                              (org-element-property :raw-value h))))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex org-data scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_org_data_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test
* H1
Body 1
* H2
Body 2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Root type.
         (org-element-type tree)
         ;; Has contents.
         (org-element-contents tree)
         ;; Child types.
         (mapcar #'org-element-type (org-element-contents tree))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex anonymous node scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_anonymous_node_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil anonymous anonymous dummy dummy nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Anonymous node type.
   (org-element-type '((dummy)))
   (org-element-type '((dummy)) t)
   (org-element-type '("string") t)
   ;; Not anonymous.
   (org-element-type '(dummy))
   (org-element-type '(dummy) t)
   ;; Invalid.
   (org-element-type '(1 2) t)
   (org-element-type nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex type-p scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_type_p_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil (foo) (foo bar) nil t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Single type.
   (org-element-type-p '(foo) 'foo)
   (org-element-type-p '(foo) 'bar)
   ;; List of types.
   (org-element-type-p '(foo) '(foo))
   (org-element-type-p '(foo) '(foo bar))
   (org-element-type-p '(foo) '(bar baz))
   ;; Plain text.
   (org-element-type-p "string" 'plain-text)
   ;; Anonymous.
   (org-element-type-p '((foo)) 'anonymous)
   ;; Invalid.
   (org-element-type-p nil 'foo)
   (org-element-type-p 1 'foo)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex class scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_class_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (element element object object element object object element element element object object object)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Regular elements.
   (org-element-class '(paragraph nil) nil)
   (org-element-class '(headline nil) nil)
   ;; Regular objects.
   (org-element-class '(bold nil) nil)
   (org-element-class '(italic nil) nil)
   ;; Special types.
   (org-element-class '(org-data nil) nil)
   ;; Plain text.
   (org-element-class "text" nil)
   ;; Secondary string.
   (org-element-class '("secondary " "string") nil)
   ;; Pseudo elements.
   (org-element-class '(foo nil) nil)
   (org-element-class '(foo nil) '(center-block nil))
   (org-element-class '(foo nil) '(org-data nil))
   ;; Pseudo objects.
   (org-element-class '(foo nil) '(bold nil))
   (org-element-class '(foo nil) '(paragraph nil))
   (org-element-class '(foo nil) '("secondary"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex property-raw scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_property_raw_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; No properties.
   (dolist (element `( nil
                       (headline nil)
                       (headline nil (headline))
                       "string"))
     (list (org-element-property-raw :begin element)
           (org-element-property-raw :begin element 'default)))
   ;; Only non-standard properties.
   (dolist (element `((headline (:begin1 1))
                      (headline (:begin1 1) (headline))
                      ,(propertize "string" :begin1 1)))
     (list (org-element-property-raw :begin element)
           (org-element-property-raw :begin1 element)))
   ;; Only standard properties.
   (dolist (element `((headline (:standard-properties ,(make-vector 10 'test)))
                      (headline (:standard-properties ,(make-vector 10 'test)) (headline))))
     (list (org-element-property-raw :begin element)
           (org-element-property-raw :begin1 element)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Ultra: org-element with complex deferred scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ultra_complex_deferred_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Resolve :deferred property.
   (let ((el (org-element-create
              'dummy
              `(:deferred
                ,(org-element-deferred-create
                  t (lambda (el) (org-element-put-property el :foo 'bar) nil))))))
     (list (org-element-property :foo el)
           (org-element-property :foo2 el)))
   ;; Deferred value.
   (let ((el (org-element-create
              'dummy
              `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (org-element-property :foo el))
   ;; Auto-undefer.
   (let ((el (org-element-create
              'dummy
              `(:foo ,(org-element-deferred-create t (lambda (_) 'bar))))))
     (list (org-element-property :foo el)
           (org-element-property-raw :foo el)))
   ;; Force undefer.
   (let ((el (org-element-create
              'dummy
              `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (list (org-element-property :foo el)
           (org-element-property-raw :foo el)
           (org-element-property :foo el nil 'force)
           (org-element-property-raw :foo el)))
   ;; Deferred alias.
   (let ((el (org-element-create
              'dummy
              `( :foo 1
                 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el)
           (org-element-property :bar el)))
   ;; Deferred list.
   (let ((el (org-element-create
              'dummy
              `(:foo ,(org-element-deferred-create-list
                       (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))
   ;; Deferred with side effects (retry).
   (let ((el (org-element-create
              'dummy
              `(:foo ,(org-element-deferred-create
                       nil (lambda (el)
                             (org-element-put-property el :foo 1)
                             (throw :org-element-deferred-retry nil)))))))
     (org-element-property :foo el))
   ;; Recursive undefer.
   (let ((el (org-element-create
              'dummy
              `(:foo ,(org-element-deferred-create
                       nil (lambda (el)
                             (org-element-deferred-create
                              nil (lambda (_) 1)))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}
