//! Complex combo batch 241 — `display` property variants deep:
//! `(space ...)`, `(height ...)`, `(raise ...)`, `(slice ...)`,
//! `(left-fringe ...)`, `(right-fringe ...)`, integer/float width.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx241_display_space_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((space :width 20) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before  after")
  (put-text-property 7 8 'display '(space :width 20))
  (list (get-text-property 7 'display)
        (get-text-property 1 'display)
        (get-text-property 9 'display)))
"##,
        expect,
    );
}

#[test]
fn div_cx241_display_integer_and_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (42 \"REPLACED\" (image :type xpm :file \"fake.xpm\") #(\"AAA BBB CCC\" 0 2 (display 42) 4 6 (display \"REPLACED\") 8 10 (display (image :type xpm :file \"fake.xpm\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAA BBB CCC")
  (put-text-property 1 3 'display 42)
  (put-text-property 5 7 'display "REPLACED")
  (put-text-property 9 11 'display '(image :type xpm :file "fake.xpm"))
  (list (get-text-property 1 'display)
        (get-text-property 5 'display)
        (get-text-property 9 'display)
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx241_display_height_and_raise() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 13 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "text with height")
  (put-text-property 6 12 'display '(height 2.0))
  (put-text-property 13 18 'display '(raise 0.5))
  (list (get-text-property 6 'display)
        (get-text-property 13 'display)
        (get-text-property 1 'display)))
"##,
        expect,
    );
}

#[test]
fn div_cx241_display_left_right_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((left-fringe right-arrow) (right-fringe left-arrow help-echo \"tip\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "fringe test")
  (put-text-property 1 2 'display '(left-fringe right-arrow))
  (put-text-property 7 8 'display '(right-fringe left-arrow help-echo "tip"))
  (list (get-text-property 1 'display)
        (get-text-property 7 'display)))
"##,
        expect,
    )
}

#[test]
fn div_cx241_display_slice_for_image() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((slice 0 0 50 50 (image :type png :file \"fake.png\" :scale default)) slice)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((img (create-image "fake.png" 'png nil)))
  (with-temp-buffer
    (insert "image slice test")
    (put-text-property 1 6 'display (list 'slice 0 0 50 50 img))
    (list (get-text-property 1 'display)
          (car (get-text-property 1 'display)))))
"##,
        expect,
    )
}

#[test]
fn div_cx241_display_with_space_width_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((space :width 10) (space :width 5 :height 2) (space :align-to 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAA BBB CCC")
  (put-text-property 4 5 'display '(space :width 10))
  (put-text-property 8 9 'display '(space :width 5 :height 2))
  (put-text-property 10 11 'display '(space :align-to 20))
  (list (get-text-property 4 'display)
        (get-text-property 8 'display)
        (get-text-property 10 'display)))
"##,
        expect,
    )
}

#[test]
fn div_cx241_display_chain_across_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TEXT\" \"OVERLAY\" \"OVERLAY\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 4 'display "TEXT")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'display "OVERLAY"))
  (list (get-char-property 1 'display)
        (get-char-property 3 'display)
        (get-char-property 5 'display)
        (get-char-property 7 'display)))
"##,
        expect,
    )
}

#[test]
fn div_cx241_display_wrap_prefix_line_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"LP> \" \"WP> \" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line one\nline two\nline three\n")
  (put-text-property 1 8 'line-prefix "LP> ")
  (put-text-property 1 8 'wrap-prefix "WP> ")
  (list (get-text-property 1 'line-prefix)
        (get-text-property 1 'wrap-prefix)
        (get-text-property 11 'line-prefix)
        (get-text-property 11 'wrap-prefix)))
"##,
        expect,
    )
}

#[test]
fn div_cx241_display_invisible_with_display_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"visible hidden visib\" 0 6 (display \"V\")) #(\"visible hidden visib\" 0 6 (display \"V\")) \"V\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "visible hidden visible")
  (add-to-invisibility-spec 'neo-cx241-h)
  (let ((ov (make-overlay 9 14)))
    (overlay-put ov 'invisible 'neo-cx241-h))
  (put-text-property 1 7 'display "V")
  (list (buffer-substring 1 21)
        (filter-buffer-substring 1 21)
        (get-text-property 1 'display)))
"##,
        expect,
    )
}

#[test]
fn div_cx241_display_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Display property mega test buffer content")
  (put-text-property 1 6 'face 'bold)
  (put-text-property 8 14 'display "REPLACED")
  (put-text-property 16 20 'display '(space :width 10))
  (let ((m (set-marker (make-marker) 18))
        (ov (make-overlay 4 20)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'display "OVERLAY")
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 25)
    (let ((state (list (get-char-property 1 'display)
                       (get-char-property 8 'display)
                       (get-char-property 10 'display)
                       (get-char-property 16 'display)
                       (get-char-property 1 'face)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
