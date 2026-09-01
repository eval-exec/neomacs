//! Delta-strict combo tests for org-mode edit/navigation edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all beginning-of-line edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_beginning_of_line_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "Some text\nSome other text")
      (goto-char (point-max)) (org-beginning-of-line) (bolp))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1\n** H2")
      (goto-char (point-min)) (org-overview) (org-beginning-of-line)
      (= (line-beginning-position) 1))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_special_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* TODO [#A] Headline")
      (goto-char (point-max))
      (let ((org-special-ctrl-a/e t))
        (list (progn (org-beginning-of-line) (looking-at-p "Headline"))
              (progn (org-beginning-of-line) (bolp))
              (progn (org-beginning-of-line) (looking-at-p "Headline")))))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_special_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- [ ] Item")
      (goto-char (point-max))
      (let ((org-special-ctrl-a/e t))
        (list (progn (org-beginning-of-line) (looking-at-p "Item"))
              (progn (org-beginning-of-line) (bolp)))))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_reversed_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* TODO Headline")
      (goto-char (point-max))
      (let ((org-special-ctrl-a/e 'reversed)
            (this-command last-command))
        (list (progn (org-beginning-of-line) (bolp))
              (progn (org-beginning-of-line) (looking-at-p "Headline")))))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_reversed_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- [X] Item")
      (goto-char (point-max))
      (let ((org-special-ctrl-a/e 'reversed)
            (this-command last-command))
        (list (progn (org-beginning-of-line) (bolp))
              (progn (org-beginning-of-line) (looking-at-p "Item")))))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_at_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "[[https://orgmode.org]]")
       (goto-char (point-max)) (org-beginning-of-line) (bolp))
     (with-temp-buffer (org-mode) (insert "[[https://orgmode.org]]")
       (goto-char (point-max))
       (let ((org-special-ctrl-a/e t)) (org-beginning-of-line)) (bolp))
     (with-temp-buffer (org-mode) (insert "[[http<point>://orgmode.org]]")
       (goto-char (point-min)) (search-forward "http")
       (visual-line-mode) (org-beginning-of-line) (bolp)))))"##,
        expect,
    );
}

#[test]
fn delta_beginning_of_line_single_asterisk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "*")
       (goto-char (point-max))
       (let ((org-special-ctrl-a/e t)) (org-beginning-of-line) t))
     (with-temp-buffer (org-mode) (insert "*")
       (goto-char (point-max))
       (let ((org-special-ctrl-a/e nil)) (org-beginning-of-line) t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all end-of-line edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_end_of_line_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "Some text\nSome other text")
      (goto-char (point-min)) (org-end-of-line) (eolp))))"##,
        expect,
    );
}

#[test]
fn delta_end_of_line_special_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* TODO Headline :tag:")
      (goto-char (point-min))
      (let ((org-special-ctrl-a/e t))
        (list (progn (org-end-of-line) (looking-back "Headline" nil))
              (progn (org-end-of-line) (eolp)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all fill-element edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_fill_element_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "|a|")
      (goto-char (point-min)) (org-fill-element) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_paragraph_with_line_break() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"some \\\\\\\\\\nlong text\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "some \\\\\nlong\ntext")
      (goto-char (point-min))
      (let ((fill-column 20)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_at_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A B\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "A\nB")
      (goto-char (point-max))
      (let ((fill-column 20)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- A B\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- A\n  B")
      (goto-char (point-min))
      (let ((fill-column 20)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"  # A B\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "  # A\n  # B")
      (goto-char (point-min))
      (let ((fill-column 20)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_comment_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK \"#+BEGIN_COMMENT\\nSome text\\n#+END_COMMENT\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+BEGIN_COMMENT\nSome\ntext\n#+END_COMMENT")
      (goto-char (point-min)) (forward-line)
      (let ((fill-column 20)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_affiliated_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+NAME: para\\nSome\\ntext.\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+NAME: para\nSome\ntext.")
      (goto-char (point-min))
      (let ((fill-column 20)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_fill_element_n_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"123456789 {{{n}}}.\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "123456789 {{{n}}}.")
      (goto-char (point-min))
      (let ((fill-column 10)) (org-fill-element)) (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all indent-line edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_indent_line_diary_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "%%(org-calendar-holiday)")
      (org-indent-line) (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_footnote_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "[fn:1] fn")
      (let ((org-adapt-indentation t)) (org-indent-line))
      (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H")
      (org-indent-line) (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "")
      (org-indent-line) (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn indent_line_with_adapt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H\nA")
      (goto-char (point-max))
      (let ((org-adapt-indentation t)) (org-indent-line))
      (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_without_adapt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H\nA")
      (goto-char (point-max))
      (let ((org-adapt-indentation nil)) (org-indent-line))
      (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_preserves_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H\nAB")
      (goto-char (point-min)) (forward-line) (forward-char)
      (let ((org-adapt-indentation t)) (org-indent-line))
      (looking-at "B"))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H\n - A")
      (goto-char (point-min)) (forward-line)
      (let ((org-adapt-indentation t)) (org-indent-line))
      (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_latex_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\begin{equation}\n 1+1=2\n\\end{equation}")
      (goto-char (point-min)) (forward-line)
      (org-indent-line) (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_blank_at_list_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H\n- A\n  - AA\n")
      (goto-char (point-max))
      (let ((org-adapt-indentation t)) (org-indent-line))
      (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn indent_line_after_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert " Paragraph\n")
      (goto-char (point-max))
      (org-indent-line) (org-get-indentation))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_property_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"* H\\n:PROPERTIES:\\n:key:      value\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:key: value\n:END:")
      (goto-char (point-min)) (forward-line 2)
      (let ((org-property-format "%-10s %s")) (org-indent-line))
      (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_indent_line_property_empty_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\\n:PROPERTIES:\\n:key:\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:key:\n:END:")
      (goto-char (point-min)) (forward-line 2)
      (let ((org-property-format "%-10s %s")) (org-indent-line))
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all return edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_return_regular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Para\\n<point>graph\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "Para<point>graph")
      (goto-char (+ 4 (point-min))) (org-return) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_return_with_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"  Para\\n  <point>graph\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "  Para<point>graph")
      (goto-char (+ 6 (point-min))) (org-return t) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_return_on_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a |\n| b |")
      (goto-char (point-min)) (forward-char 2) (org-return) (looking-at "b"))))"##,
        expect,
    );
}

#[test]
fn delta_return_on_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H :tag:\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H :tag:")
      (goto-char (point-min)) (search-forward ":tag") (org-return) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_return_before_headline_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* TODO H :tag:\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* TODO H :tag:")
      (goto-char (point-min)) (forward-char 2) (org-return) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_return_at_bol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n* h\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* h")
      (goto-char (point-min)) (org-return) (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all meta-return edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_meta_return_in_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* a\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "a")
      (goto-char (point-min)) (org-meta-return) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_meta_return_in_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- \\n- a\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- a")
      (goto-char (point-min)) (org-meta-return) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_meta_return_in_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"|   |\\n| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row) 6 7 (face org-table) 7 8 (face org-table rear-nonsticky t display (space :relative-width 1)) 8 9 (face org-table) 9 10 (face org-table display (space :relative-width 1.001)) 10 11 (face org-table) 11 12 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a |")
      (goto-char (point-min)) (forward-char 2) (org-meta-return) (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all entry-blocked-p edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_entry_blocked_children_not_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** TODO two")
      (goto-char (point-min)) (org-entry-blocked-p))))"##,
        expect,
    );
}

#[test]
fn delta_entry_blocked_all_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** DONE two")
      (goto-char (point-min)) (org-entry-blocked-p))))"##,
        expect,
    );
}

#[test]
fn delta_entry_blocked_no_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (with-temp-buffer (org-mode) (insert "* Blocked\n** TODO one")
      (goto-char (point-min)) (org-entry-blocked-p))))"##,
        expect,
    );
}

#[test]
fn delta_entry_blocked_done_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (with-temp-buffer (org-mode) (insert "* DONE Blocked\n** TODO one")
      (goto-char (point-min)) (org-entry-blocked-p))))"##,
        expect,
    );
}

#[test]
fn delta_entry_blocked_ordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (list
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO one\n** TODO two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO one\n** DONE two")
       (goto-char (point-min)) (org-entry-blocked-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all find-olp edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_find_olp_basic() {
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

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all map-entries edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_map_entries_full_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
      (goto-char (point-min)) (org-map-entries #'point))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_level_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
      (goto-char (point-min))
      (let (org-odd-levels-only) (org-map-entries #'point "LEVEL=1")))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_todo_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1\n* TODO H2\n* DONE H3")
      (goto-char (point-min))
      (org-map-entries #'point "TODO=\"TODO\""))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_tag_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1 :no:\n* H2 :yes:")
      (goto-char (point-min)) (org-map-entries #'point "yes"))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_priority_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* [#A] H1\n* [#B] H2")
      (goto-char (point-min))
      (org-map-entries #'point "PRIORITY=\"A\""))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_property_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n:PROPERTIES:\n:TEST: 1\n:END:\n* H2\n:PROPERTIES:\n:TEST: 2\n:END:")
      (goto-char (point-min)) (org-map-entries #'point "TEST=1"))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_and_criteria() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1 :no:\n** H2 :yes:\n* H3 :yes:")
      (goto-char (point-min))
      (let (org-odd-levels-only (org-use-tag-inheritance nil))
        (org-map-entries #'point "yes+LEVEL=1")))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_or_criteria() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :maybe:")
      (goto-char (point-min))
      (let (org-odd-levels-only) (org-map-entries #'point "yes|no")))))"##,
        expect,
    );
}

#[test]
fn delta_map_entries_and_tag_criteria() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :yes:no:")
      (goto-char (point-min))
      (let (org-odd-levels-only) (org-map-entries #'point "yes&no")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all edit-headline edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_edit_headline_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* B\" \"* \" \"* A\" \"* TODO B\" \"* [#A] B\" \"* TODO [#A] B\" \"* B :tag:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min)) (org-edit-headline "") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min)) (org-edit-headline "A") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* TODO A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* [#A] A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* TODO [#A] A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* A :tag:")
       (goto-char (point-min))
       (let ((org-tags-column 4)) (org-edit-headline "B")) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all insert-heading edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_insert_heading_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (org-insert-heading) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_insert_heading_at_bol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* P\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "P")
      (goto-char (point-min)) (org-insert-heading) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_insert_heading_at_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* \\n* H\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H")
      (goto-char (point-min)) (org-insert-heading) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_insert_heading_level_depends_on_above() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"** H\\nP\\n** \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "** H\nP")
      (goto-char (point-max)) (org-insert-heading) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_insert_heading_with_blank() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n* \\n\\n* H1\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1")
      (goto-char (point-min))
      (let ((org-blank-before-new-entry '((heading . t))))
        (org-insert-heading)) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_insert_heading_empty_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* \\n* \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* ")
      (goto-char (point-min)) (org-insert-heading) (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all kill-line edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_kill_line_at_beginning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "abc")
      (goto-char (point-min)) (org-kill-line) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_kill_line_in_middle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ab\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "abc")
      (goto-char (+ 2 (point-min))) (org-kill-line) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_kill_line_no_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n123\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "abc\n123")
      (goto-char (point-min)) (org-kill-line) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_kill_line_special_ctrl_k() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* A :tag:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* AB :tag:")
      (goto-char (point-min)) (forward-char 3)
      (let ((org-special-ctrl-k t) (org-tags-column 0))
        (org-kill-line)) (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all sort-entries edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_sort_entries_alphabetical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\n* abc\\n* def\\n* xyz\\n\" \"\\n* xyz\\n* def\\n* abc\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min)) (org-sort-entries nil ?a) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min)) (org-sort-entries nil ?A) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn delta_sort_entries_numerical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n* 1\\n* 2\\n* 10\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "\n* 10\n* 1\n* 2\n")
      (goto-char (point-min)) (org-sort-entries nil ?n) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn delta_sort_entries_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\n* [#A] h2\\n* [#B] h3\\n* [#C] h1\\n\" \"\\n* [#C] h1\\n* [#B] h3\\n* [#A] h2\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min)) (org-sort-entries nil ?p) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min)) (org-sort-entries nil ?P) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all mark-element edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_mark_element_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "Paragraph")
      (goto-char (point-min))
      (org-mark-element) (list (bobp) (= (mark) (point-max))))))"##,
        expect,
    );
}

#[test]
fn delta_mark_element_between_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "P1\n\nParagraph\n\nP2")
      (goto-char (point-min)) (forward-line 2)
      (org-mark-element)
      (list (looking-at "Paragraph")
            (org-with-point-at (mark) (looking-at "P2"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all mark-subtree edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_mark_subtree_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* Headline\n** Sub-headline\nBody")
      (goto-char (point-min)) (forward-line 2) (org-mark-subtree)
      (list (region-beginning) (region-end)))))"##,
        expect,
    );
}

#[test]
fn delta_mark_subtree_with_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* Headline\n** Sub-headline\nBody")
      (goto-char (point-min)) (forward-line 2) (org-mark-subtree 1)
      (list (region-beginning) (region-end)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all collect-keywords edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_collect_keywords_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"TITLE\" \"My Title\") (\"AUTHOR\" \"Me\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+TITLE: My Title\n#+AUTHOR: Me\nBody")
      (goto-char (point-min)) (org-collect-keywords '("TITLE" "AUTHOR")))))"##,
        expect,
    );
}

#[test]
fn delta_collect_keywords_not_in_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+begin_example\n#+foo: bar\n#+end_example")
      (goto-char (point-min)) (org-collect-keywords '("FOO")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all shiftright-heading edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_shiftright_heading_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO a1\\n** a2\\n* DONE b1\\n\" 0 9 (org-todo-head \"TODO\")) #(\"* TODO a1\\n** a2\\n* b1\\n\" 0 9 (org-todo-head \"TODO\") 16 20 (org-todo-head nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (list
     (with-temp-buffer (org-mode) (insert "* a1\n** a2\n* DONE b1\n")
       (goto-char (point-min)) (org-shiftright) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* a1\n** a2\n* DONE b1\n")
       (goto-char (point-min))
       (let ((org-loop-over-headlines-in-active-region 'start-level))
         (transient-mark-mode 1) (push-mark (point) t t)
         (search-forward "* DONE b1") (org-shiftright))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Delta: org-element with all toggle-heading edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delta_toggle_heading_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Item\" \"Heading\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min)) (org-toggle-heading) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-toggle-heading) (buffer-string)))))"##,
        expect,
    );
}
