//! Complex combo batch 120 — `text-property-search` API + `text-property-any`
//! / `not-all` + `next-char-property-change` and `previous-char-property-change`
//! interactions with overlays.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx120_next_char_property_change_overlay_pure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 5 8 11 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov1 (make-overlay 2 5))
        (ov2 (make-overlay 8 11)))
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (list (next-char-property-change 1)
          (next-char-property-change 3)
          (next-char-property-change 6)
          (next-char-property-change 10)
          (next-char-property-change 15))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_previous_char_property_change_overlay_pure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 11 8 2 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov1 (make-overlay 2 5))
        (ov2 (make-overlay 8 11)))
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (list (previous-char-property-change 16)
          (previous-char-property-change 12)
          (previous-char-property-change 9)
          (previous-char-property-change 4)
          (previous-char-property-change 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_next_single_property_change_text_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7 11 14 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 3 7 'face 'bold)
  (put-text-property 11 14 'face 'bold)
  (list (next-single-property-change 1 'face)
        (next-single-property-change 3 'face)
        (next-single-property-change 7 'face)
        (next-single-property-change 11 'face)
        (next-single-property-change 14 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx120_text_property_search_forward_return_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma delta epsilon")
      (put-text-property 1 5 'cat :greek)
      (put-text-property 7 10 'cat :greek)
      (put-text-property 13 17 'cat :greek)
      (let ((m (text-property-search-forward 'cat :greek t)))
        (list (prop-match-beginning m)
              (prop-match-end m)
              (prop-match-value m))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_text_property_search_forward_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma")
      (put-text-property 1 5 'cat :x)
      (let ((m (text-property-search-forward 'cat :y t)))
        (list m)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_text_property_search_backward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma delta epsilon")
      (put-text-property 7 10 'cat :x)
      (put-text-property 13 17 'cat :x)
      (goto-char (point-max))
      (let ((m (text-property-search-backward 'cat :x t)))
        (list (prop-match-beginning m)
              (prop-match-end m)
              (prop-match-value m))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_get_char_property_with_overlay_priority_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold bold bold italic italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'priority 0)
    (list (get-text-property 1 'face)
          (get-text-property 3 'face)
          (get-char-property 1 'face)
          (get-char-property 3 'face)
          (get-char-property 5 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_get_char_property_with_overlay_priority_high() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'priority 10)
    (list (get-char-property 1 'face)
          (get-char-property 3 'face)
          (get-char-property 5 'face)
          (get-char-property 7 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx120_text_property_any_with_range_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 15 nil 5 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (put-text-property 5 10 'p :vowel)
  (put-text-property 15 20 'p :vowel)
  (put-text-property 25 27 'p :consonant)
  (list (text-property-any 1 27 'p :vowel)
        (text-property-any 11 27 'p :vowel)
        (text-property-any 21 27 'p :vowel)
        (text-property-not-all 1 27 'p nil)
        (text-property-not-all 1 4 'p nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx120_next_property_change_skip_over_unrelated_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 8 15 nil 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (put-text-property 5 8 'face 'bold)
  (put-text-property 8 12 'weight 'heavy)
  (put-text-property 15 18 'face 'italic)
  (list (next-single-property-change 1 'face)
        (next-single-property-change 6 'face)
        (next-single-property-change 8 'face)
        (next-single-property-change 18 'face)
        (next-single-property-change 1 'weight)))
"##,
        expect,
    );
}

#[test]
fn div_cx120_property_change_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Property change mega test buffer content")
  (put-text-property 1 8 'face 'bold)
  (put-text-property 12 18 'face 'italic)
  (let ((m (set-marker (make-marker) 15))
        (ov (make-overlay 5 22)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 30)
    (let ((state (list (next-single-char-property-change 1 'face)
                       (next-char-property-change 1)
                       (previous-char-property-change 28)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1)
                       (get-char-property 5 'face)
                       (get-char-property 14 'face))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
