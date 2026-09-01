//! Complex combo batch 210 — `font` objects / `font-spec` / `font-entity`
//! / `font-xlfd-name` queries and face-font resolution.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx210_font_spec_create_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (font-spec :family "Monospace" :size 12 :weight 'bold :slant 'italic)))
      (list (font-spec-p spec)
            (font-get spec :family)
            (font-get spec :size)
            (font-get spec :weight)
            (font-get spec :slant)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_font_entity_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'font-entity-p)
          (fboundp 'font-xlfd-name)
          (fboundp 'font-face-attributes)
          (boundp 'font-encoding-alist))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_font_list_families() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((families (font-family-list)))
      (list (consp families)
            (member "Monospace" families)
            (member "Serif" families)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_face_font_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"default\" \"default\" normal normal normal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (face-attribute 'default :family frame)
        (face-attribute 'default :foundry frame)
        (face-attribute 'default :width frame)
        (face-attribute 'default :weight frame)
        (face-attribute 'default :slant frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx210_font_get_attributes_from_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (font-spec :family "Courier" :foundry "ADOBE" :size 14
                           :weight 'normal :slant 'normal
                           :width 'normal)))
      (list (font-spec-p spec)
            (font-get spec :family)
            (font-get spec :foundry)
            (font-get spec :size)
            (font-get spec :weight)
            (font-get spec :slant)
            (font-get spec :width)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_internal_char_font_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'internal-char-font)
          (fboundp 'font-info)
          (fboundp 'query-font)
          (boundp 'font-encoding-alist))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_font_open_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'font-open)
          (fboundp 'font-close)
          (fboundp 'fontp)
          (fboundp 'font-name)
          (fboundp 'font-put))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_face_height_attribute_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (let ((h (face-attribute 'default :height frame)))
    (list (or (integerp h) (floatp h))
          (> h 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_font_rescale_factor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'face-font-rescale-alist)
          (consp face-font-rescale-alist)
          (fboundp 'face-font))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx210_font_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (font-spec :family "Monospace" :size 12 :weight 'bold)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Font spec mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (font-spec-p spec)
                             (font-get spec :family)
                             (font-get spec :weight)
                             (face-attribute 'default :family)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
