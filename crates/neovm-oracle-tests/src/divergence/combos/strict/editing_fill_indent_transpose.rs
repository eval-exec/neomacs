//! Strict combo oracle probes: complex buffer-editing Lisp — fill, indent,
//! transpose, whitespace normalization, comment-region, sentence/paragraph
//! motion, sort-fields/columns, align-regexp, and message formatting.
//!
//! These reimplemented editing functions are where GNU/Neomacs parity is most
//! fragile.  Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_edt_fill_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"The quick brown fox\\njumps over the lazy\\ndog and keeps\\nrunning on.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog and keeps running on.")
  (let ((fill-column 20))
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_edt_fill_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"aaa bbb ccc ddd\\neee fff ggg hhh\\niii jjj\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa bbb ccc ddd eee fff ggg hhh iii jjj")
  (goto-char 5)
  (let ((fill-column 15))
    (fill-paragraph)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_edt_comment_region_uncomment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"// alpha\\n// beta\\n\" \"alpha\\nbeta\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha\nbeta\n")
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-add 0)
  (comment-region (point-min) (point-max))
  (let ((commented (buffer-string)))
    (uncomment-region (point-min) (point-max))
    (list commented (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_edt_indent_region_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"    foo\\n    bar\\n    baz\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo\nbar\nbaz\n")
  (goto-char 1)
  (let ((indent-line-function (lambda () (insert "    "))))
    (indent-region 1 (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_edt_transpose_words_and_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"beta alpha gamma\\n\" \"(aaa) (bbb)\\n\" \"20\\n10\\n30\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "alpha beta gamma\n")
   (goto-char 1)
   (transpose-words 1)
   (buffer-string))
 (with-temp-buffer
   (insert "(aaa) (bbb)\n")
   (goto-char 1)
   (transpose-sexps 1)
   (buffer-string))
 (with-temp-buffer
   (insert "10\n20\n30\n")
   (goto-char 1)
   (forward-line 1)
   (transpose-lines 1)
   (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_edt_just_one_space_and_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a b    c\" \"xy   z\" \"x y   z\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "a   b    c")
   (goto-char 2)
   (just-one-space)
   (buffer-string))
 (with-temp-buffer
   (insert "x   y   z")
   (goto-char 2)
   (just-one-space 0)
   (buffer-string))
 (with-temp-buffer
   (insert "x   y   z")
   (goto-char 2)
   (cycle-spacing -1)
   (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_edt_format_message_and_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Type ‘C-g’ to quit.\" \"‘quoted’ and ’apostrophe’\" \"Continue (default y): \" \"Save file: \" \"Proceed: \")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-message "Type `%s' to quit." "C-g")
      (format-message "`quoted' and 'apostrophe'")
      (format-prompt "Continue" "y")
      (format-prompt "Save file" nil)
      (format-prompt "Proceed" nil))
"##,
        expect,
    );
}

#[test]
fn div_edt_forward_sentence_and_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (16 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "First sentence.  Second one.\n\nSecond paragraph here.\n\n")
  (goto-char 1)
  (forward-sentence 1)
  (let ((after-sentence (point)))
    (forward-paragraph 1)
    (list after-sentence (point))))
"##,
        expect,
    );
}

#[test]
fn div_edt_sort_fields_and_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1 bar\\n2 baz\\n3 foo\\n\" \"aaa 1\\nbbb 2\\nccc 3\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "3 foo\n1 bar\n2 baz\n")
   (sort-fields 1 (point-min) (point-max))
   (buffer-string))
 (with-temp-buffer
   (insert "ccc 3\naaa 1\nbbb 2\n")
   (sort-columns nil 1 (point-max))
   (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_edt_align_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\t\t= 1\\nfoo\t\t= 2\\nlongername\t= 3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a = 1\nfoo = 2\nlongername = 3\n")
  (align-regexp (point-min) (point-max) "\\(\\s-*\\)=")
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_edt_current_column_and_move_to_column_with_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 8 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc\tdef")
  (let ((c0 (progn (goto-char (point-max)) (current-column))))
    (goto-char 1)
    (move-to-column 6)
    (list c0 (current-column) (point))))
"##,
        expect,
    );
}
