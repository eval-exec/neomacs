//! Org export/structure parity: HTML/LaTeX/ASCII body export (no headlines,
//! so anchors stay deterministic), element table-cell interpret, fill, and
//! org-table-to-lisp.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_ascii_list_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"1. first\\n2. second\\n\\n x  y \\n------\\n 9  8 \\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ox-ascii)
  (with-temp-buffer (org-mode)
    (insert "1. first\n2. second\n\n| x | y |\n|---+---|\n| 9 | 8 |\n")
    (let ((org-ascii-text-width 72)) (org-export-as 'ascii nil nil t))))"##,
        expect,
    );
}

#[test]
fn org_element_table_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"a\" 0 1 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 14 15 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 29 29 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 29 29 0 nil first-section nil nil nil 1 29 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 29 29 0 nil org-data nil nil nil 3 29 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [15 15 16 28 29 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"1\" 0 1 (:parent #11))) (table-cell (:standard-properties [20 nil 21 22 24 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"2\" 0 1 (:parent #11))) (table-cell (:standard-properties [24 nil 25 26 28 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"3\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))) (table-cell (:standard-properties [10 nil 11 12 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3))))) #(\"b\" 0 1 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 14 15 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 29 29 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 29 29 0 nil first-section nil nil nil 1 29 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 29 29 0 nil org-data nil nil nil 3 29 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [15 15 16 28 29 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"1\" 0 1 (:parent #11))) (table-cell (:standard-properties [20 nil 21 22 24 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"2\" 0 1 (:parent #11))) (table-cell (:standard-properties [24 nil 25 26 28 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"3\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3 (table-cell (:standard-properties [10 nil 11 12 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))))]) #(\"b\" 0 1 (:parent #3))))) #(\"c\" 0 1 (:parent (table-cell (:standard-properties [10 nil 11 12 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 14 15 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 29 29 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 29 29 0 nil first-section nil nil nil 1 29 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 29 29 0 nil org-data nil nil nil 3 29 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [15 15 16 28 29 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"1\" 0 1 (:parent #11))) (table-cell (:standard-properties [20 nil 21 22 24 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"2\" 0 1 (:parent #11))) (table-cell (:standard-properties [24 nil 25 26 28 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"3\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))) #3)]) #(\"c\" 0 1 (:parent #3))))) #(\"1\" 0 1 (:parent (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [15 15 16 28 29 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 29 29 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 29 29 0 nil first-section nil nil nil 1 29 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 29 29 0 nil org-data nil nil nil 3 29 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 14 15 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11))) (table-cell (:standard-properties [10 nil 11 12 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11)))) #6)] :type standard) #3 (table-cell (:standard-properties [20 nil 21 22 24 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"2\" 0 1 (:parent #7))) (table-cell (:standard-properties [24 nil 25 26 28 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"3\" 0 1 (:parent #7))))]) #(\"1\" 0 1 (:parent #3))))) #(\"2\" 0 1 (:parent (table-cell (:standard-properties [20 nil 21 22 24 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [15 15 16 28 29 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 29 29 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 29 29 0 nil first-section nil nil nil 1 29 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 29 29 0 nil org-data nil nil nil 3 29 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 14 15 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11))) (table-cell (:standard-properties [10 nil 11 12 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11)))) #6)] :type standard) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"1\" 0 1 (:parent #7))) #3 (table-cell (:standard-properties [24 nil 25 26 28 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"3\" 0 1 (:parent #7))))]) #(\"2\" 0 1 (:parent #3))))) #(\"3\" 0 1 (:parent (table-cell (:standard-properties [24 nil 25 26 28 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [15 15 16 28 29 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 29 29 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 29 29 0 nil first-section nil nil nil 1 29 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 29 29 0 nil org-data nil nil nil 3 29 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 14 15 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11))) (table-cell (:standard-properties [10 nil 11 12 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11)))) #6)] :type standard) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"1\" 0 1 (:parent #7))) (table-cell (:standard-properties [20 nil 21 22 24 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"2\" 0 1 (:parent #7))) #3)]) #(\"3\" 0 1 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "| a | b | c |\n| 1 | 2 | 3 |\n")
    (org-element-map (org-element-parse-buffer) 'table-cell
      (lambda (c) (org-element-interpret-data (org-element-contents c))))))"##,
        expect,
    );
}

#[test]
fn org_fill_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"aaaa bbbb cccc dddd\\neeee ffff gggg hhhh\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode) (setq fill-column 20)
    (insert "aaaa bbbb cccc dddd eeee ffff gggg hhhh")
    (fill-paragraph) (buffer-string)))"##,
        expect,
    );
}

#[test]
fn org_html_inline_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"<p>\\nSome <b>bold</b>, <i>italic</i>, <code>code</code>, <code>verbatim</code>, <del>strike</del> and a <a href=\\\"https://x.org\\\">link</a>.\\n</p>\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ox-html)
  (with-temp-buffer (org-mode)
    (insert "Some *bold*, /italic/, =code=, ~verbatim~, +strike+ and a [[https://x.org][link]].\n")
    (org-export-as 'html nil nil t)))"##,
        expect,
    );
}

#[test]
fn org_html_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"<ul class=\\\"org-ul\\\">\\n<li>one</li>\\n<li>two\\n<ul class=\\\"org-ul\\\">\\n<li>nested</li>\\n</ul></li>\\n<li>three</li>\\n</ul>\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ox-html)
  (with-temp-buffer (org-mode)
    (insert "- one\n- two\n  - nested\n- three\n")
    (org-export-as 'html nil nil t)))"##,
        expect,
    );
}

#[test]
fn org_html_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"<table border=\\\"2\\\" cellspacing=\\\"0\\\" cellpadding=\\\"6\\\" rules=\\\"groups\\\" frame=\\\"hsides\\\">\\n\\n\\n<colgroup>\\n<col  class=\\\"org-right\\\" />\\n\\n<col  class=\\\"org-right\\\" />\\n</colgroup>\\n<thead>\\n<tr>\\n<th scope=\\\"col\\\" class=\\\"org-right\\\">a</th>\\n<th scope=\\\"col\\\" class=\\\"org-right\\\">b</th>\\n</tr>\\n</thead>\\n<tbody>\\n<tr>\\n<td class=\\\"org-right\\\">1</td>\\n<td class=\\\"org-right\\\">2</td>\\n</tr>\\n</tbody>\\n</table>\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ox-html)
  (with-temp-buffer (org-mode)
    (insert "| a | b |\n|---+---|\n| 1 | 2 |\n")
    (org-export-as 'html nil nil t)))"##,
        expect,
    );
}

#[test]
fn org_latex_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"Text with \\\\textbf{bold} and \\\\emph{italic} and \\\\(x^2\\\\) math.\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ox-latex)
  (with-temp-buffer (org-mode)
    (insert "Text with *bold* and /italic/ and $x^2$ math.\n")
    (org-export-as 'latex nil nil t)))"##,
        expect,
    );
}

#[test]
fn org_table_to_lisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") hline (\"3\" \"4\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "| 1 | 2 |\n|---+---|\n| 3 | 4 |\n")
    (goto-char (point-min)) (org-table-to-lisp)))"##,
        expect,
    );
}
