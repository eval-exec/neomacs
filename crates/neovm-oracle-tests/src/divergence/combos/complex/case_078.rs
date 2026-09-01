//! Complex combo batch 78 — abbrev / completion / fill / indent / case
//! region operations: abbrev expansion, completion-styles, `complete-symbol`,
//! `fill-region`, `indent-region`, `upcase-region`/`downcase-region`/`capitalize-region`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx78_upcase_downcase_capitalize_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLd\" \"hello worlD\" \"Hello World\" \"HELLO WORLD\" \"hello world\" \"Hello World\" \"Hello World Foo Bar\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer (insert "hello world") (upcase-region 1 11) (buffer-string))
 (with-temp-buffer (insert "HELLO WORLD") (downcase-region 1 11) (buffer-string))
 (with-temp-buffer (insert "hello world") (capitalize-region 1 11) (buffer-string))
 (upcase "hello world")
 (downcase "HELLO WORLD")
 (capitalize "hello world")
 (upcase-initials "hello world foo bar"))
"##,
        expect,
    );
}

#[test]
fn div_cx78_upcase_word_downcase_word_capitalize_word_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"HELLO world foo bar\" \"hello WORLD FOO BAR\" \"Hello World foo bar\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "hello world foo bar")
   (goto-char 1)
   (upcase-word 1)
   (buffer-string))
 (with-temp-buffer
   (insert "HELLO WORLD FOO BAR")
   (goto-char 1)
   (downcase-word 1)
   (buffer-string))
 (with-temp-buffer
   (insert "hello world foo bar")
   (goto-char 1)
   (capitalize-word 2)
   (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx78_fill_region_with_fill_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"This is a long line of text\\nthat should be wrapped at the\\nfill column boundary for\\ntesting purposes.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "This is a long line of text that should be wrapped at the fill column boundary for testing purposes.")
  (let ((fill-column 30))
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx78_fill_paragraph_with_fill_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"    This is a paragraph that has a fill\\n    prefix applied to it that should\\n    also be wrapped properly at the\\n    column boundary.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "    This is a paragraph that has a fill prefix applied to it that should also be wrapped properly at the column boundary.")
  (goto-char 1)
  (let ((fill-column 40))
    (fill-paragraph))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx78_indent_region_with_tab_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"    line1\\n    line2\\n    line3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "line1\nline2\nline3\n")
      (let ((indent-tabs-mode nil))
        (indent-rigidly (point-min) (point-max) 4)
        (buffer-string)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx78_abbrev_define_and_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t foo \"forward\" \"backward\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table)))
      (define-abbrev table "foo" "forward" nil)
      (define-abbrev table "bar" "backward" nil)
      (list (abbrev-table-p table)
            (abbrev-symbol "foo" table)
            (abbrev-expansion "foo" table)
            (abbrev-expansion "bar" table)
            (abbrev-expansion "missing" table)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx78_completion_styles_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"alp\" \"alpha\" t nil (\"alpha\" \"alphabet\" \"alpine\") (\"beta\") t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "alphabet" "alpine" "beta" "gamma" "delta")))
  (list
   (try-completion "al" coll)
   (try-completion "alph" coll)
   (try-completion "alphabet" coll)
   (try-completion "z" coll)
   (all-completions "al" coll)
   (all-completions "b" coll)
   (test-completion "alpha" coll)
   (test-completion "alp" coll)))
"##,
        expect,
    );
}

#[test]
fn div_cx78_completion_with_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"apple1\") (\"banana2\") \"apple1\" 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("apple1" "apple2" "banana1" "banana2" "cherry1" "cherry2")))
  (list
   (all-completions "a" coll (lambda (s) (string-match-p "1$" s)))
   (all-completions "b" coll (lambda (s) (string-match-p "2$" s)))
   (try-completion "app" coll (lambda (s) (string-match-p "1$" s)))
   (length (all-completions "" coll))))
"##,
        expect,
    );
}

#[test]
fn div_cx78_completion_with_alist_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"alpha\" (\"alpha\" \"alphabet\") (\"alpha\" . 1) (\"alphabet\" . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '(("alpha" . 1) ("alphabet" . 2) ("beta" . 3))))
  (list
   (try-completion "al" coll)
   (all-completions "al" coll)
   (assoc "alpha" coll)
   (assoc "alphabet" coll)))
"##,
        expect,
    );
}

#[test]
fn div_cx78_indent_to_column_with_tabs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"x\t\t\t\" \"x           \" 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((indent-tabs-mode t)
        (tab-width 4))
    (insert "x")
    (indent-to 12)
    (let ((with-tabs (buffer-string)))
      (erase-buffer)
      (let ((indent-tabs-mode nil))
        (insert "x")
        (indent-to 12))
      (list with-tabs (buffer-string) (current-column)))))
"##,
        expect,
    );
}

#[test]
fn div_cx78_move_to_column_with_tabs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 5 4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((tab-width 4))
    (insert "abc\tdef\tghi")
    (goto-char 1)
    (move-to-column 8)
    (let ((p1 (point)))
      (move-to-column 4)
      (let ((p2 (point)))
        (list p1 p2 (current-column) (point))))))
"##,
        expect,
    );
}

#[test]
fn div_cx78_completion_table_case_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha\" \"Alpha\" 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("Alpha" "ALPHA" "alpha" "Beta"))
      (completion-ignore-case t))
  (list
   (try-completion "a" coll)
   (try-completion "A" coll)
   (length (all-completions "a" coll))
   (length (all-completions "A" coll))))
"##,
        expect,
    );
}

#[test]
fn div_cx78_case_region_indent_fill_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "this is line one\nthis is line two\nthis is line three")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 20))
        (ov (make-overlay 5 35)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 50)
    (let ((fill-column 15))
      (upcase-region 5 25)
      (fill-region (point-min) (point-max))
      (let ((state (list (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (point-min) (point-max)
                         (text-properties-at 1)))))
        (undo) (undo)
        (widen)
        (list state
              (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
