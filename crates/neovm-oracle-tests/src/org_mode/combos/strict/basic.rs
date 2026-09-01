//! Complex combo and strict oracle tests for org-mode.
//!
//! These tests exercise multiple org subsystems together, testing
//! edge cases, error conditions, and intricate interactions that
//! simpler per-function tests miss.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Deep nesting and recursion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_deep_headline_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) 10 96 (\"L1\" \"L2\" \"L3\" \"L4\" \"L5\" \"L6\" \"L7\" \"L8\" \"L9\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* L1\n** L2\n*** L3\n**** L4\n***** L5\n****** L6\n******* L7\n******** L8\n********* L9\n********** L10")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; All levels present.
         (mapcar (lambda (h) (org-element-property :level h)) headlines)
         ;; Deepest headline.
         (apply #'max (mapcar (lambda (h) (org-element-property :level h)) headlines))
         ;; Outline path at deepest.
         (goto-char (point-max))
         (org-get-outline-path))))))"##,
        expect,
    );
}

#[test]
fn combo_deeply_nested_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\n#+BEGIN_QUOTE\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n#+END_QUOTE\n#+END_CENTER")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Element types.
         (org-element-type (org-element-at-point))
         ;; Source block inside nested blocks.
         (progn (search-forward "(+ 1 2)")
                (org-element-type (org-element-at-point)))
         ;; Lineage at source block.
         (mapcar #'org-element-type
                 (org-element-lineage (org-element-context) nil t))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mixed content stress tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_mixed_content_full_document() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Full Document Test
#+AUTHOR: Test Author
#+DATE: 2024-01-15
#+OPTIONS: num:t toc:t
#+FILETAGS: :test:org:

* TODO [#A] Section 1 :important:
:PROPERTIES:
:CUSTOM_ID: sec1
:EFFORT: 2h
:END:
DEADLINE: <2024-01-20 Sat>

Paragraph with *bold*, /italic/, _underline_, +strike+, =verbatim=, and ~code~.

Also [[https://orgmode.org][a link]] and [fn:1].

#+BEGIN_QUOTE
Quoted text with *markup*.
#+END_QUOTE

| Name | Value |
|------+-------|
| A    |     1 |
| B    |     2 |
#+TBLFM: @3$2=vsum(@1$2..@2$2)

** DONE Subsection 1.1 :tag1:tag2:
CLOSED: [2024-01-16 Wed 10:00]
:LOGBOOK:
CLOCK: [2024-01-16 Wed 09:00]--[2024-01-16 Wed 10:00] =>  1:00
:END:

#+BEGIN_SRC emacs-lisp :results output
(message \"hello\")
#+END_SRC

** TODO Subsection 1.2
SCHEDULED: <2024-01-18 Fri +1w>

- [ ] Task 1
- [X] Task 2
  - [ ] Sub-task 2.1
  - [X] Sub-task 2.2
- [ ] Task 3

* WAIT Section 2 :waiting:
#+BEGIN_COMMENT
This section is under development.
#+END_END

<<target>> See [[#sec1][Section 1]].

[fn:1] Footnote definition with *bold* text.
")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Document structure.
         (length (org-element-map tree 'headline #'identity))
         (length (org-element-map tree 'section #'identity))
         ;; Inline markup.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         (length (org-element-map tree 'strike-through #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         ;; Links and references.
         (length (org-element-map tree 'link #'identity))
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))
         ;; Blocks.
         (length (org-element-map tree 'quote-block #'identity))
         (length (org-element-map tree 'src-block #'identity))
         (length (org-element-map tree 'comment-block #'identity))
         ;; Tables.
         (length (org-element-map tree 'table #'identity))
         (length (org-element-map tree 'table-row #'identity))
         (length (org-element-map tree 'table-cell #'identity))
         ;; Lists.
         (length (org-element-map tree 'plain-list #'identity))
         (length (org-element-map tree 'item #'identity))
         ;; Planning.
         (length (org-element-map tree 'planning #'identity))
         ;; Clock.
         (length (org-element-map tree 'clock #'identity))
         ;; Property drawers.
         (length (org-element-map tree 'property-drawer #'identity))
         ;; Keywords.
         (length (org-element-map tree 'keyword #'identity))
         ;; Targets.
         (length (org-element-map tree 'target #'identity))
         ;; Export headline numbers.
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge cases: empty, minimal, boundary
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (let* ((tree (org-element-parse-buffer)))
        (list
         (org-element-type tree)
         (org-element-contents tree)
         (org-element-map tree t #'identity)))))"##,
        expect,
    );
}

#[test]
fn combo_single_character() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "x")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (org-element-type tree)
         (length (org-element-map tree t #'identity))
         (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

#[test]
fn combo_only_headline_no_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (\"\" \"\" \"\") (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* \n** \n*** ")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         (length headlines)
         (mapcar (lambda (h) (org-element-property :raw-value h)) headlines)
         (mapcar (lambda (h) (org-element-property :level h)) headlines))))))"##,
        expect,
    );
}

#[test]
fn combo_only_blank_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n\n\n\n\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (org-element-type tree)
         (length (org-element-map tree t #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element property contracts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_headline_property_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Headline :tag1:tag2:\nBody text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity))))
        (list
         ;; All expected properties.
         (org-element-property :level hl)
         (org-element-property :todo-keyword hl)
         (org-element-property :priority hl)
         (org-element-property :tags hl)
         (substring-no-properties (org-element-property :raw-value hl))
         ;; :begin and :end are positions.
         (numberp (org-element-property :begin hl))
         (numberp (org-element-property :end hl))
         ;; :post-blank is a number.
         (numberp (org-element-property :post-blank hl))
         ;; :parent is set.
         (org-element-type (org-element-property :parent hl))
         ;; :contents-begin and :contents-end.
         (numberp (org-element-property :contents-begin hl))
         (numberp (org-element-property :contents-end hl))))))"##,
        expect,
    );
}

#[test]
fn strict_paragraph_property_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Paragraph with *bold* and /italic/.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (para (car (org-element-map tree 'paragraph #'identity))))
        (list
         ;; Type.
         (org-element-type para)
         ;; Positions.
         (numberp (org-element-property :begin para))
         (numberp (org-element-property :end para))
         (numberp (org-element-property :post-blank para))
         ;; Parent.
         (org-element-type (org-element-property :parent para))
         ;; Contents contain objects.
         (mapcar #'org-element-type (org-element-contents para))))))"##,
        expect,
    );
}

#[test]
fn strict_link_property_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "See [[https://orgmode.org][Org mode]] for details.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (link (car (org-element-map tree 'link #'identity))))
        (list
         ;; Type.
         (org-element-type link)
         ;; Link type.
         (org-element-property :type link)
         ;; Path.
         (org-element-property :path link)
         ;; Has description.
         (org-element-contents link)
         ;; Parent is paragraph.
         (org-element-type (org-element-property :parent link))))))"##,
        expect,
    );
}

#[test]
fn strict_timestamp_property_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (timestamp active 2024 1 15 14 30 cumulate 1 week all 3 day)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2024-01-15 Mon 14:30 +1w -3d>\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (ts (car (org-element-map tree 'timestamp #'identity))))
        (list
         ;; Type.
         (org-element-type ts)
         ;; Timestamp type.
         (org-element-property :type ts)
         ;; Date components.
         (org-element-property :year-start ts)
         (org-element-property :month-start ts)
         (org-element-property :day-start ts)
         (org-element-property :hour-start ts)
         (org-element-property :minute-start ts)
         ;; Repeater.
         (org-element-property :repeater-type ts)
         (org-element-property :repeater-value ts)
         (org-element-property :repeater-unit ts)
         ;; Warning.
         (org-element-property :warning-type ts)
         (org-element-property :warning-value ts)
         (org-element-property :warning-unit ts))))))"##,
        expect,
    );
}

#[test]
fn strict_src_block_property_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SRC emacs-lisp -n -r :results output :exports code\n(+ 1 2)\n#+END_SRC\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (src (car (org-element-map tree 'src-block #'identity))))
        (list
         ;; Type.
         (org-element-type src)
         ;; Language.
         (org-element-property :language src)
         ;; Switches.
         (org-element-property :switches src)
         ;; Parameters.
         (org-element-property :parameters src)
         ;; Value (code content).
         (org-element-property :value src)
         ;; Positions.
         (numberp (org-element-property :begin src))
         (numberp (org-element-property :end src))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-map edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_map_with_first_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n* H2\n* H3\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; FIRST-MATCH = t: single result.
         (org-element-property :raw-value
           (org-element-map tree 'headline #'identity nil t))
         ;; FIRST-MATCH = nil: list of results.
         (mapcar (lambda (h) (org-element-property :raw-value h))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

#[test]
fn strict_map_with_no_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\n*bold* and /italic/\n#+END_CENTER\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Without no-recursion: finds objects inside.
         (length (org-element-map tree 'bold #'identity))
         ;; With no-recursion on center-block: skips contents.
         (length (org-element-map tree 'bold #'identity nil nil 'center-block))))))"##,
        expect,
    );
}

#[test]
fn strict_map_with_affiliated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CAPTION: *bold* caption\n| a | b |\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Without affiliated: no bold found.
         (length (org-element-map tree 'bold #'identity))
         ;; With affiliated: finds bold in caption.
         (length (org-element-map tree 'bold #'identity nil nil nil nil t))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-interpret-data round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_interpret_roundtrip_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Simple paragraph.\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Simple paragraph.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (interpreted (org-element-interpret-data tree)))
        ;; Round-trip: parse then interpret should preserve content.
        (substring-no-properties interpreted)))))"##,
        expect,
    );
}

#[test]
fn strict_interpret_roundtrip_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"* H1\\nBody 1\\n** H2\\nBody 2\\n* H3\\nBody 3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nBody 1\n** H2\nBody 2\n* H3\nBody 3\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (interpreted (org-element-interpret-data tree)))
        (substring-no-properties interpreted)))))"##,
        expect,
    );
}

#[test]
fn strict_interpret_roundtrip_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n|---+---|\\n| c | d |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n|---+---|\n| c | d |\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (interpreted (org-element-interpret-data tree)))
        (substring-no-properties interpreted)))))"##,
        expect,
    );
}

#[test]
fn strict_interpret_roundtrip_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"- Item 1\\n- Item 2\\n  - Sub 2.1\\n  - Sub 2.2\\n- Item 3\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Item 1\n- Item 2\n  - Sub 2.1\n  - Sub 2.2\n- Item 3\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (interpreted (org-element-interpret-data tree)))
        (substring-no-properties interpreted)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: multi-file-like structures
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_multiple_top_level_sections() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Preamble paragraph.\n\n* H1\nBody 1\n\n* H2\nBody 2\n\n** H2.1\nBody 2.1\n\n* H3\nBody 3\n\nEpilogue.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Sections before, between, and after headlines.
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

#[test]
fn combo_interleaved_blocks_and_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n* H1\n#+BEGIN_QUOTE\nQuoted\n#+END_QUOTE\n** H2\n#+BEGIN_EXAMPLE\nExample\n#+END_EXAMPLE\n* H3\n#+BEGIN_CENTER\nCentered\n#+END_CENTER")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Block types in order.
         (org-element-map tree '(src-block quote-block example-block center-block)
           (lambda (el) (org-element-type el)))
         ;; Headlines.
         (mapcar (lambda (h) (org-element-property :raw-value h))
                 (org-element-map tree 'headline #'identity))
         ;; Each block has correct parent.
         (org-element-map tree '(src-block quote-block example-block center-block)
           (lambda (el) (org-element-type (org-element-property :parent el))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: error conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_incomplete_block_no_crash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SRC\nNo end block\n\n* H\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Should not crash; tree is still valid.
         (org-element-type tree)
         ;; Headline after incomplete block is still found.
         (length (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

#[test]
fn strict_malformed_link_no_crash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text with [[incomplete link and [[valid][link]].\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (org-element-type tree)
         ;; Valid link is still found.
         (length (org-element-map tree 'link #'identity))))))"##,
        expect,
    );
}

#[test]
fn strict_malformed_table_no_crash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n| incomplete\n| c | d |\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (org-element-type tree)
         (length (org-element-map tree 'table-row #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: org-element-adopt / extract / set chains
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_adopt_extract_set_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Alpha\\nPara 1.\\n* Beta\\nPara 2.\\n* Gamma\\nPara 3.\\n\" \"* Alpha\\nPara 1.\\n* Gamma\\nPara 3.\\n\" \"* Alpha\\nNew para.\\n* Gamma\\nPara 3.\\n\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((doc (org-element-create 'org-data nil))
         (h1 (org-element-create
              'headline
              '(:level 1 :raw-value "Alpha" :title ("Alpha"))
              (org-element-create
               'section nil
               (org-element-create 'paragraph nil "Para 1.\n"))))
         (h2 (org-element-create
              'headline
              '(:level 1 :raw-value "Beta" :title ("Beta"))
              (org-element-create
               'section nil
               (org-element-create 'paragraph nil "Para 2.\n"))))
         (h3 (org-element-create
              'headline
              '(:level 1 :raw-value "Gamma" :title ("Gamma"))
              (org-element-create
               'section nil
               (org-element-create 'paragraph nil "Para 3.\n")))))
    ;; Adopt all three.
    (org-element-adopt doc h1 h2 h3)
    (let ((after-adopt (org-element-interpret-data doc)))
      ;; Extract middle.
      (org-element-extract h2)
      (let ((after-extract (org-element-interpret-data doc)))
        ;; Set h1's paragraph.
        (let* ((sec (car (org-element-contents h1)))
               (para (car (org-element-contents sec))))
          (org-element-set para (org-element-create 'paragraph nil "New para.\n")))
        (list (substring-no-properties after-adopt)
              (substring-no-properties after-extract)
              (substring-no-properties (org-element-interpret-data doc))
              ;; h2 has no parent after extract.
              (org-element-property :parent h2))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: property inheritance chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_property_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 2 (1 2 3) (\"p\") (\"c\") (\"gc\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  ;; Build a 3-level hierarchy with properties at each level.
  (let* ((grandchild (org-element-create 'grandchild '(:shared 3 :own-gc "gc")))
         (child (org-element-create 'child '(:shared 2 :own-c "c") grandchild))
         (parent (org-element-create 'parent '(:shared 1 :own-p "p") child)))
    (list
     ;; At grandchild: own value wins.
     (org-element-property-inherited :shared grandchild 'with-self)
     ;; At grandchild: without self, get parent's.
     (org-element-property-inherited :shared grandchild)
     ;; Accumulate all.
     (org-element-property-inherited :shared grandchild 'with-self 'accumulate)
     ;; Only parent has :own-p.
     (org-element-property-inherited :own-p grandchild 'with-self 'accumulate)
     ;; Only child has :own-c.
     (org-element-property-inherited :own-c grandchild 'with-self 'accumulate)
     ;; Only grandchild has :own-gc.
     (org-element-property-inherited :own-gc grandchild 'with-self 'accumulate))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: export with all option types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_export_all_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Options Test
#+AUTHOR: Author Name
#+EMAIL: author@example.org
#+DATE: 2024-01-15
#+DESCRIPTION: A test document
#+KEYWORDS: test org mode
#+LANGUAGE: en
#+OPTIONS: H:2 num:t \\n:t timestamp:t author:t creator:t d:t email:t \
*:t e:t ::t f:t pri:t -:t ^:t toc:t |:t tags:t tasks:t <:t todo:t \
inline:nil stat:t title:t
#+CATEGORY: test
#+FILETAGS: :test:org:
* Section 1
** Subsection 1.1
Content.
* Section 2
Content.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Title.
         (plist-get info :title)
         ;; Author.
         (plist-get info :author)
         ;; Email.
         (plist-get info :email)
         ;; Headline level.
         (plist-get info :headline-levels)
         ;; Section numbers.
         (plist-get info :section-numbers)
         ;; With timestamps.
         (plist-get info :with-timestamps)
         ;; With author.
         (plist-get info :with-author)
         ;; With email.
         (plist-get info :with-email)
         ;; With emphasis.
         (plist-get info :with-emphasize)
         ;; Headline numbers.
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: list operations combo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_list_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Item 1\n- Item 2\n  - Sub 2.1\n  - Sub 2.2\n    - Sub-sub 2.2.1\n- Item 3\n  - Sub 3.1\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (items (org-element-map tree 'item #'identity))
             (lists (org-element-map tree 'plain-list #'identity)))
        (list
         ;; Number of items.
         (length items)
         ;; Number of lists.
         (length lists)
         ;; Item structure.
         (mapcar (lambda (item)
                   (list (org-element-property :bullet item)
                         (org-element-property :level item)))
                 items)
         ;; Each item's parent is a plain-list.
         (every (lambda (item) (eq 'plain-list (org-element-type (org-element-property :parent item)))) items))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: footnote nesting and references
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_footnote_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2] and[fn:3].\n\n[fn:1] Definition 1 with *bold*.\n\n[fn:2] Definition 2 with [[https://orgmode.org][link]].\n\n[fn:3] Inline def[fn:nested:inner footnote].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Footnote references.
         (length (org-element-map tree 'footnote-reference #'identity))
         ;; Footnote definitions.
         (length (org-element-map tree 'footnote-definition #'identity))
         ;; Footnote numbers.
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))
         ;; First reference check.
         (mapcar (lambda (ref) (org-export-footnote-first-reference-p ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: clock and logbook combo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_clock_logbook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (3 (closed closed closed) (\"1:30\" \"1:00\" \"2:00\") ((2024 1 15 9 0) (2024 1 15 11 0) (2024 1 14 14 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task\n:LOGBOOK:\nCLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:30] =>  1:30\nCLOCK: [2024-01-15 Mon 11:00]--[2024-01-15 Mon 12:00] =>  1:00\nCLOCK: [2024-01-14 Sun 14:00]--[2024-01-14 Sun 16:00] =>  2:00\n:END:\nBody text.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (clocks (org-element-map tree 'clock #'identity)))
        (list
         ;; Number of clocks.
         (length clocks)
         ;; Clock statuses.
         (mapcar (lambda (c) (org-element-property :status c)) clocks)
         ;; Clock durations.
         (mapcar (lambda (c) (org-element-property :duration c)) clocks)
         ;; Clock values (timestamps).
         (mapcar (lambda (c)
                   (let ((ts (org-element-property :value c)))
                     (list (org-element-property :year-start ts)
                           (org-element-property :month-start ts)
                           (org-element-property :day-start ts)
                           (org-element-property :hour-start ts)
                           (org-element-property :minute-start ts))))
                 clocks))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: dynamic blocks and column view
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_dynamic_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: clocktable :scope file :maxlevel 2\n#+END:\n* Task\n:LOGBOOK:\nCLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Dynamic block found.
         (length (org-element-map tree 'dynamic-block #'identity))
         ;; Block name.
         (org-element-property :block-name
           (org-element-map tree 'dynamic-block #'identity nil t))
         ;; Clock found.
         (length (org-element-map tree 'clock #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: entities and LaTeX
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_entities_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Entity: \\alpha and \\beta.\n\nLaTeX inline: $x^2 + y^2 = z^2$.\n\nLaTeX display: $$E = mc^2$$.\n\nLaTeX env:\n\\begin{equation}\n\\int_0^1 f(x) dx\n\\end{equation}\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Entities.
         (length (org-element-map tree 'entity #'identity))
         (mapcar (lambda (e) (org-element-property :name e))
                 (org-element-map tree 'entity #'identity))
         ;; LaTeX fragments.
         (length (org-element-map tree 'latex-fragment #'identity))
         ;; LaTeX environments.
         (length (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: sparse tree and occur
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_sparse_tree_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"Alpha\" \"Beta\" \"Gamma\" \"Delta\" \"Epsilon\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO Alpha\nBody\n* DONE Beta\nBody\n* TODO Gamma\nBody\n* DONE Delta\nBody\n* TODO Epsilon\nBody")
      (goto-char (point-min))
      (org-match-sparse-tree nil "TODO")
      (let ((visible nil))
        (org-element-map (org-element-parse-buffer) 'headline
          (lambda (h)
            (let ((title (org-element-property :raw-value h)))
              (when (org-element-property :begin h)
                (push title visible)))))
        (nreverse visible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: refiling targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_refile_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Project A\" \"Design\" \"UI\" \"Implementation\" \"Project B\" \"Testing\" \"Unit tests\" \"Integration tests\" \"Archive\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 3))))
    (with-temp-buffer (org-mode)
      (insert "* Project A\n** Design\n*** UI\n** Implementation\n* Project B\n** Testing\n*** Unit tests\n*** Integration tests\n* Archive :ARCHIVE:")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r))
              (org-refile-get-targets)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: tag inheritance and matching
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_tag_inheritance_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-use-tag-inheritance t))
    (with-temp-buffer (org-mode)
      (insert "* Project :project:\n** Design :design:\n*** UI :ui:\n**** Wireframes\n** Development :dev:\n*** Backend :backend:\n*** Frontend :frontend:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; Tags at each level.
         (mapcar (lambda (h) (list (org-element-property :raw-value h)
                             (org-element-property :tags h)))
                 headlines)
         ;; Tag matcher: find :project: tagged.
         (length (org-map-entries #'point "project"))
         ;; Tag matcher: find :dev: tagged.
         (length (org-map-entries #'point "dev"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: export backend chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_export_backend_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((parent) t ((lambda (h c i) (format \"CHILD: %s\\n%s\" (org-element-property :raw-value h) c)) (lambda (s c i) c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    ;; Define parent backend.
    (org-export-define-backend 'parent
      '((headline . (lambda (h c i) (format "PARENT: %s\n%s" (org-element-property :raw-value h) c)))
        (section . (lambda (s c i) c))
        (paragraph . (lambda (p c i) c))
        (plain-text . (lambda (t i) t))))
    ;; Define derived backend.
    (org-export-define-derived-backend 'child 'parent
      :translate-alist
      '((headline . (lambda (h c i) (format "CHILD: %s\n%s" (org-element-property :raw-value h) c)))))
    (list
     ;; Derived check.
     (org-export-derived-backend-p 'child 'parent)
     (org-export-derived-backend-p 'child 'child)
     ;; Transcoders.
     (let ((all (org-export-get-all-transcoders 'child)))
       (list (cdr (assq 'headline all))
             (cdr (assq 'section all)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex: org-cite citations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_citations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Simple [cite:@key1].\n\nMultiple [cite:@a;@b;@c].\n\nWith style [cite/style:@key].\n\nWith prefix [cite:common-prefix;@key].\n\nWith suffix [cite:@key;common-suffix].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Citations.
         (length (org-element-map tree 'citation #'identity))
         ;; References.
         (length (org-element-map tree 'citation-reference #'identity))
         ;; Styles.
         (mapcar (lambda (c) (org-element-property :style c))
                 (org-element-map tree 'citation #'identity))
         ;; Keys.
         (mapcar (lambda (r) (org-element-property :key r))
                 (org-element-map tree 'citation-reference #'identity))))))"##,
        expect,
    );
}
