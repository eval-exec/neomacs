//! Complex combo batch 364 — `auto-fill`/`fill`/`justify` ultimate:
//! fill-region with fill-column/prefix, auto-fill-mode, justify-current-line
//! with full/left/right/center, fill-individual-paragraphs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx364_fill_region_with_fill_column_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "This is a long line of text that should be wrapped at the fill column boundary for testing purposes.")
  (let ((fill-column 30))
    (fill-region (point-min) (point-max))
    (buffer-string))
"##,
        expect,
    )
}

#[test]
fn div_cx364_fill_region_with_fill_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"    This is a paragraph with a fill\\n    prefix that should be wrapped\\n    at the column boundary.  Second\\n    line of the same paragraph\\n    continues here.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "    This is a paragraph with a fill prefix that should be wrapped at the column boundary.\n    Second line of the same paragraph continues here.")
  (goto-char 1)
  (let ((fill-column 35)
        (fill-prefix "    "))
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    )
}

#[test]
fn div_cx364_auto_fill_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'auto-fill-mode)
      (boundp 'auto-fill-function)
      (boundp 'comment-auto-fill-only-comments))
"##,
        expect,
    )
}

#[test]
fn div_cx364_fill_paragraph_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"This is a paragraph that is\\nlong enough to require\\nwrapping at a reasonable fill\\ncolumn setting like 30\\ncharacters or so.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "This is a paragraph that is long enough to require wrapping at a reasonable fill column setting like 30 characters or so.")
  (goto-char 1)
  (let ((fill-column 30))
    (fill-paragraph)
    (buffer-string)))
"##,
        expect,
    )
}

#[test]
fn div_cx364_justify_current_line_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"This is a paragraph of text.\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "This is a paragraph of text.\n")
      (let ((fill-column 30))
        (fill-region (point-min) (point-max))
        (justify-current-line 'full t)
        (buffer-string)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx364_center_line_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\t       short line\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "short line\n")
      (goto-char 1)
      (let ((fill-column 40))
        (center-line)
        (buffer-string)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx364_center_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"\t\tline one\\n\t\tline two\\n\t       line three\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "line one\nline two\nline three\n")
      (let ((fill-column 40))
        (center-region (point-min) (point-max))
        (buffer-string)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx364_fill_individual_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"First paragraph here\\nthat is long enough\\nto wrap at 20.\\n\\nSecond paragraph\\nalso long enough to\\nwrap at 20 chars.\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "First paragraph here that is long enough to wrap at 20.\n\nSecond paragraph also long enough to wrap at 20 chars.\n")
      (let ((fill-column 20))
        (fill-individual-paragraphs (point-min) (point-max))
        (buffer-string)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx364_fill_column_indicator_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'fill-column)
      (integerp fill-column)
      (> fill-column 0)
      (fboundp 'set-fill-column)
      (fboundp 'set-fill-prefix))
"##,
        expect,
    )
}

#[test]
fn div_cx364_fill_justify_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "This is a long line that should be wrapped at the fill column boundary for mega testing purposes in the oracle test suite.")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 20))
        (ov (make-overlay 5 30)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (let ((fill-column 25))
      (fill-region (point-min) (point-max))
      (let ((state (list (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
