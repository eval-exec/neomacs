//! Complex combo batch 433 — 18 org-mode parity probes: org-mode
//! activation, org-element parse, org-export basics, org-hide,
//! org-table, org-link, org-heading, org-todo, org-tags,
//! org-priority, org-clock, org-src, org-babel, org-list,
//! org-footnote, org-capture, org-agenda.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// org-mode activation and basic buffer properties.
#[test]
fn div_cx433_org_mode_activate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (org-mode org-mode t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (list major-mode
          (derived-mode-p 'org-mode)
          (boundp 'org-mode-map))))"##,
        expect,
    );
}

/// org-element: parsing org document structure.
#[test]
fn div_cx433_org_element_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (org-data t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
  (with-temp-buffer
    (insert "* Heading 1\n** Subheading\nSome text\n")
    (let ((tree (org-element-parse-buffer)))
      (list (org-element-type tree)
            (> (length tree) 0)))))"##,
        expect,
    );
}

/// org-export: basic export to ASCII.
#[test]
fn div_cx433_org_export_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"1 Hello\\n=======\\n\\n  World\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox)
  (with-temp-buffer
    (insert "* Hello\nWorld\n")
    (let ((org-export-with-toc nil))
      (org-export-as 'ascii nil nil t nil))))"##,
        expect,
    );
}

/// org-hide: hide/reveal subtree.
#[test]
fn div_cx433_org_hide_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* H1\n** H2\ntext\n")
    (outline-hide-subtree)
    (outline-invisible-p 1)))"##,
        expect,
    );
}

/// org-table: creating and manipulating org tables.
#[test]
fn div_cx433_org_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \" 2 \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| a | b |\n| 1 | 2 |\n")
    (org-table-goto-line 2)
    (org-table-get-field 2)))"##,
        expect,
    );
}

/// org-link: inserting and resolving org links.
#[test]
fn div_cx433_org_link_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-link-any-regexp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ol)
  (with-temp-buffer
    (org-mode)
    (insert "[[https://example.com][Example]]")
    (beginning-of-line)
    (looking-at org-link-any-regexp)
    (match-string-no-properties 0)))"##,
        expect,
    );
}

/// org-heading: heading todo and level.
#[test]
fn div_cx433_org_heading_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Test heading\n")
    (org-back-to-heading)
    (list (org-get-todo-state)
          (org-outline-level))))"##,
        expect,
    );
}

/// org-tags: tag operations on headings.
#[test]
fn div_cx433_org_tags_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"tag1\" \"tag2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* heading :tag1:tag2:\n")
    (org-back-to-heading)
    (org-get-tags)))"##,
        expect,
    );
}

/// org-priority: priority cookies.
#[test]
fn div_cx433_org_priority_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2000""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* [#A] high priority\n")
    (org-back-to-heading)
    (org-get-priority (thing-at-point 'line t))))"##,
        expect,
    );
}

/// org-clock: clock operations.
#[test]
fn div_cx433_org_clock_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-clock)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO task\n")
    (list (fboundp 'org-clock-in)
          (fboundp 'org-clock-out))))"##,
        expect,
    );
}

/// org-src: source block detection.
#[test]
fn div_cx433_org_src_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-src-in-org-buffer-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-src)
  (with-temp-buffer
    (org-mode)
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (org-src-in-org-buffer-p)))"##,
        expect,
    );
}

/// org-babel: executing source blocks.
#[test]
fn div_cx433_org_babel_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further code blocks\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core)
  (with-temp-buffer
    (org-mode)
    (insert "#+begin_src emacs-lisp :results value\n(+ 1 2)\n#+end_src\n")
    (goto-char 1)
    (org-babel-next-src-block)
    (condition-case e
        (org-babel-execute-src-block)
      (error (car e)))))"##,
        expect,
    );
}

/// org-list: list structure detection.
#[test]
fn div_cx433_org_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- item1\n- item2\n  - subitem\n")
    (let ((struct (org-list-struct)))
      (list (listp struct) (> (length struct) 0)))))"##,
        expect,
    );
}

/// org-footnote: footnote detection.
#[test]
fn div_cx433_org_footnote_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (\"1\" 18 45 \"Footnote definition\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "Text[fn:1] more\n\n[fn:1] Footnote definition\n")
    (list (condition-case e (org-footnote-at-reference-p) (error (car e)))
          (condition-case e (org-footnote-at-definition-p) (error (car e))))))"##,
        expect,
    );
}

/// org-capture: capture template functions.
#[test]
fn div_cx433_org_capture_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-capture)
  (list (boundp 'org-capture-templates)
        (fboundp 'org-capture)))"##,
        expect,
    );
}

/// org-agenda: agenda function availability.
#[test]
fn div_cx433_org_agenda_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda)
  (list (boundp 'org-agenda-files)
        (fboundp 'org-agenda-list)))"##,
        expect,
    );
}
