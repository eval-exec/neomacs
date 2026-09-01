//! Complex combo batch 314 — `text-property` interval engine ultimate:
//! `next`/`previous`-property-change, `text-property-any`/`not-all`,
//! `text-property-search-forward`/`backward`, sticky front/rear props,
//! `set-text-properties` clear and re-add, `add-text-properties` merge.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx314_next_previous_property_change_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8 12 18 nil 18 18 12 8 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (put-text-property 3 8 'face 'bold)
  (put-text-property 12 18 'face 'italic)
  (list (next-property-change 1)
        (next-property-change 3)
        (next-property-change 8)
        (next-property-change 12)
        (next-property-change 18)
        (previous-property-change 26)
        (previous-property-change 20)
        (previous-property-change 15)
        (previous-property-change 10)
        (previous-property-change 5)))
"##,
        expect,
    )
}

#[test]
fn div_cx314_text_property_any_not_all_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 15 nil nil 5 5 nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (put-text-property 5 10 'p :one)
  (put-text-property 15 20 'p :two)
  (list (text-property-any 1 26 'p :one)
        (text-property-any 1 26 'p :two)
        (text-property-any 11 26 'p :one)
        (text-property-any 1 26 'p :missing)
        (text-property-not-all 1 26 'p nil)
        (text-property-not-all 5 10 'p nil)
        (text-property-not-all 11 15 'p nil)
        (text-property-not-all 1 26 'p :val)))
"##,
        expect,
    )
}

#[test]
fn div_cx314_text_property_search_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil 13 17 :y)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma alpha beta gamma")
      (put-text-property 1 5 'cat :x)
      (put-text-property 7 10 'cat :x)
      (put-text-property 13 17 'cat :y)
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
    )
}

#[test]
fn div_cx314_sticky_front_rear_insertion_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abcdXefYghij\" 2 4 (p :core rear-nonsticky t front-sticky nil) 5 7 (p :core rear-nonsticky t front-sticky nil)) :core nil :core nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 3 7 'front-sticky nil)
  (put-text-property 3 7 'rear-nonsticky t)
  (put-text-property 3 7 'p :core)
  (goto-char 5)
  (insert "X")
  (goto-char 8)
  (insert "Y")
  (list (buffer-string)
        (get-text-property 3 'p)
        (get-text-property 5 'p)
        (get-text-property 6 'p)
        (get-text-property 8 'p)
        (get-text-property 9 'p)))
"##,
        expect,
    )
}

#[test]
fn div_cx314_set_text_properties_clear_re_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((face italic) (face italic) (face italic) nil 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 6 '(face bold weight heavy))
  (set-text-properties 1 6 nil)
  (add-text-properties 1 6 '(face italic))
  (list (text-properties-at 1)
        (text-properties-at 3)
        (text-properties-at 5)
        (text-properties-at 6)
        (next-single-property-change 1 'face)))
"##,
        expect,
    )
}

#[test]
fn div_cx314_char_property_search_overlay_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold bold bold italic italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 3 8)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'priority 0)
    (list (get-text-property 1 'face)
          (get-text-property 3 'face)
          (get-char-property 1 'face)
          (get-char-property 3 'face)
          (get-char-property 5 'face)
          (get-char-property 8 'face))))
"##,
        expect,
    )
}

#[test]
fn div_cx314_next_single_char_property_change_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7 9 13 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 3 7 'face 'bold)
  (let ((ov (make-overlay 9 13)))
    (overlay-put ov 'face 'italic)
    (list (next-single-char-property-change 1 'face)
          (next-single-char-property-change 3 'face)
          (next-single-char-property-change 7 'face)
          (next-single-char-property-change 9 'face)
          (next-single-char-property-change 13 'face))))
"##,
        expect,
    )
}

#[test]
fn div_cx314_remove_text_properties_specific_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((color blue face bold) (color blue face bold) nil nil (color blue face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 6 '(face bold weight heavy color blue))
  (remove-text-properties 1 6 '(weight nil))
  (list (text-properties-at 1)
        (text-properties-at 3)
        (text-properties-at 6)
        (remove-text-properties 1 6 nil)
        (text-properties-at 1)))
"##,
        expect,
    )
}

#[test]
fn div_cx314_buffer_substring_with_without_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"alpha beta gamma delt\" 0 4 (face bold) 6 9 (face italic) 12 16 (face underline)) \"alpha beta gamma delt\" (face bold) (face bold) (face italic) (face underline) 21 21)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha beta gamma delta")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 7 10 'face 'italic)
  (put-text-property 13 17 'face 'underline)
  (let* ((with-props (buffer-substring 1 22))
         (no-props (buffer-substring-no-properties 1 22)))
    (list with-props no-props
          (text-properties-at 0 with-props)
          (text-properties-at 3 with-props)
          (text-properties-at 6 with-props)
          (text-properties-at 12 with-props)
          (length with-props) (length no-props))))
"##,
        expect,
    )
}

#[test]
fn div_cx314_textprop_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Text property interval mega test buffer content here")
  (put-text-property 1 10 'face 'bold)
  (put-text-property 12 20 'face 'italic)
  (put-text-property 25 35 'face 'underline)
  (let ((m (set-marker (make-marker) 15))
        (ov (make-overlay 5 30)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 38)
    (let ((state (list (next-property-change 1)
                       (next-single-char-property-change 1 'face)
                       (previous-property-change 35)
                       (text-property-any 1 35 'face 'bold)
                       (text-property-not-all 1 35 'face nil)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1)
                       (get-char-property 14 'face))))
      (undo)
      (widen()
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    )
}
