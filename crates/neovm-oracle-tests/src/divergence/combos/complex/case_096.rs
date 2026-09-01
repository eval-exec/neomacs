//! Complex combo batch 96 — buffer substring / narrowing / filter-buffer
//! substrate: `filter-buffer-substring`, `format-format-string`, indenting
//! across overlays, text-property-search variants.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx96_filter_buffer_substring_with_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Visible hidden visibl\" \"Visible hidden visibl\" \"Visible hidden visibl\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Visible hidden visible")
  (add-to-invisibility-spec 'neo-cx96-h)
  (let ((ov (make-overlay 9 14)))
    (overlay-put ov 'invisible 'neo-cx96-h))
  (list (buffer-substring 1 22)
        (filter-buffer-substring 1 22)
        (filter-buffer-substring 1 22 t)))
"##,
        expect,
    );
}

#[test]
fn div_cx96_filter_buffer_substring_with_display_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Text with display pro\" \"Text with display pro\" \"Text with display pro\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Text with display prop")
  (let ((ov (make-overlay 5 11)))
    (overlay-put ov 'display "[DISPLAY]"))
  (list (buffer-substring 1 22)
        (filter-buffer-substring 1 22)
        (filter-buffer-substring 1 22 t)))
"##,
        expect,
    );
}

#[test]
fn div_cx96_format_mode_line_per_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "mode-line test")
      (let ((ml (format-mode-line mode-line-format)))
        (list (stringp ml)
              (> (length ml) 0)
              (format-mode-line "%b %p"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx96_buffer_substring_with_text_properties_vs_without() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"alpha beta gamma delt\" 0 4 (face bold) 6 9 (face italic) 12 16 (face underline)) \"alpha beta gamma delt\" (face bold) (face bold) (face italic) 21 21)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha beta gamma delta")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 7 10 'face 'italic)
  (put-text-property 13 17 'face 'underline)
  (let* ((with-props (buffer-substring 1 22))
         (no-props (buffer-substring-no-properties 1 22))
         (props-1 (text-properties-at 0 with-props))
         (props-3 (text-properties-at 2 with-props))
         (props-7 (text-properties-at 6 with-props)))
    (list with-props no-props props-1 props-3 props-7
          (length with-props) (length no-props))))
"##,
        expect,
    );
}

#[test]
fn div_cx96_text_property_search_backwards_with_value_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil 19 23 :y)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma alpha beta gamma")
      (put-text-property 1 5 'cat :x)
      (put-text-property 7 10 'cat :x)
      (put-text-property 13 17 'cat :x)
      (put-text-property 19 23 'cat :y)
      (let ((fwd (text-property-search-forward 'cat :x t)))
        (goto-char (point-max))
        (let ((bwd (text-property-search-backward 'cat :y t)))
          (list (and fwd (prop-match-beginning fwd))
                (and fwd (prop-match-end fwd))
                (and fwd (prop-match-value fwd))
                (and bwd (prop-match-beginning bwd))
                (and bwd (prop-match-end bwd))
                (and bwd (prop-match-value bwd))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx96_indent_rigidly_with_tabs_across_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\tline1\\n\tline2\\n\tline3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\n")
  (let ((indent-tabs-mode t)
        (tab-width 4))
    (indent-rigidly (point-min) (point-max) 4)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx96_indent_rigidly_with_spaces_across_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"      line1\\n      line2\\n      line3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\n")
  (let ((indent-tabs-mode nil))
    (indent-rigidly (point-min) (point-max) 6)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx96_buffer_substring_with_overlay_text_props_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"ABCDEFGH\" 0 3 (face bold)) \"ABCDEFGH\" (face bold) (face bold) nil 8 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGH")
  (put-text-property 1 4 'face 'bold)
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'display "[REP]"))
  (let* ((sub (buffer-substring 1 9))
         (no-props (buffer-substring-no-properties 1 9)))
    (list sub no-props
          (text-properties-at 0 sub)
          (text-properties-at 2 sub)
          (text-properties-at 5 sub)
          (length sub) (length no-props))))
"##,
        expect,
    );
}

#[test]
fn div_cx96_buffer_substring_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 38)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Header content for buffer substring mega")
  (put-text-property 1 6 'face 'bold)
  (put-text-property 8 14 'display "XX")
  (add-to-invisibility-spec 'neo-cx96-h)
  (let ((m (set-marker (make-marker) 18))
        (invis-ov (make-overlay 20 30))
        (face-ov (make-overlay 5 25)))
    (overlay-put invis-ov 'invisible 'neo-cx96-h)
    (overlay-put face-ov 'face 'italic)
    (overlay-put face-ov 'priority 5)
    (narrow-to-region 3 40)
    (let ((state (list (buffer-substring 1 38)
                       (buffer-substring-no-properties 1 38)
                       (filter-buffer-substring 1 38)
                       (marker-position m)
                       (overlay-start invis-ov) (overlay-end invis-ov)
                       (overlay-start face-ov) (overlay-end face-ov)
                       (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start invis-ov) (overlay-end invis-ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx96_buffer_formatted_substring_no_text_props_in_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"field1\tfield2\tfield3\tfield\" 0 5 (field a) 7 12 (field b) 14 19 (field c) 21 26 (field d)) \"field1\tfield2\tfield3\tfield\" (field a) (field b) (field c) (field d))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "field1\tfield2\tfield3\tfield4")
  (put-text-property 1 6 'field 'a)
  (put-text-property 8 13 'field 'b)
  (put-text-property 15 20 'field 'c)
  (put-text-property 22 27 'field 'd)
  (list (buffer-substring 1 27)
        (buffer-substring-no-properties 1 27)
        (text-properties-at 0 (buffer-substring 1 27))
        (text-properties-at 7 (buffer-substring 1 27))
        (text-properties-at 14 (buffer-substring 1 27))
        (text-properties-at 21 (buffer-substring 1 27))))
"##,
        expect,
    );
}

#[test]
fn div_cx96_add_text_properties_vs_set_text_properties_idempotent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((b 2 a 1) (b 2 a 1) nil (b 2 a 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 4 '(a 1 b 2))
  (let ((first-add (text-properties-at 1)))
    (add-text-properties 1 4 '(a 1 b 2))
    (let ((second-add (text-properties-at 1)))
      (set-text-properties 3 8 nil)
      (let ((after-clear-3 (text-properties-at 3))
            (after-clear-2 (text-properties-at 2)))
        (list first-add second-add after-clear-3 after-clear-2)))))
"##,
        expect,
    );
}
