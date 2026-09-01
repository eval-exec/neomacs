//! Strict combo oracle probes, batch 22: display metrics (pixel/mm/screens/
//! color-cells), fontset listing, default face :box/:underline/:overline/
//! :stipple/:inherit attributes, image-type availability, and X display
//! introspection.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f7_display_metrics_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25 nil nil 1 3 0 nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (display-pixel-width)
      (display-pixel-height)
      (display-mm-width)
      (display-mm-height)
      (display-screens)
      (display-planes)
      (display-color-cells)
      (display-graphic-p)
      (display-color-p)
      (display-grayscale-p))
"##,
        expect,
    );
}

#[test]
fn div_f7_fontset_and_default_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1 \"-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default\" unspecified \"default\" 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length (fontset-list))
      (car (fontset-list))
      (face-attribute 'default :font nil 'default)
      (face-attribute 'default :family nil 'default)
      (face-attribute 'default :height nil 'default))
"##,
        expect,
    );
}

#[test]
fn div_f7_face_box_underline_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil bold italic t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (face-attribute 'default :underline nil 'default)
      (face-attribute 'default :overline nil 'default)
      (face-attribute 'default :box nil 'default)
      (face-attribute 'default :stipple nil 'default)
      (face-attribute 'default :inherit nil 'default)
      (face-attribute 'bold :weight nil 'default)
      (face-attribute 'italic :slant nil 'default)
      (face-attribute 'underline :underline nil 'default))
"##,
        expect,
    );
}

#[test]
fn div_f7_image_type_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function image-types)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (image-type-available-p 'png)
      (image-type-available-p 'jpeg)
      (image-type-available-p 'svg)
      (image-type-available-p 'xpm)
      (image-type-available-p 'xbm)
      (image-type-available-p 'gif)
      (sort (delq nil (mapcar #'symbol-name (image-types))) #'string<))
"##,
        expect,
    );
}

#[test]
fn div_f7_x_display_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (x-display-screens) (error (car err)))
      (condition-case err (x-display-pixel-width) (error (car err)))
      (condition-case err (x-server-version) (error (car err)))
      (condition-case err (x-server-vendor) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_f7_category_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t category-table \"ASCII\\nASCII graphic characters 32-126 (ISO646 IRV:1983[4/0])\" \"a\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (standard-category-table)))
  (list (char-table-p ct)
        (char-table-subtype ct)
        (category-docstring ?a)
        (category-set-mnemonics (make-category-set "a"))
        (condition-case err (modify-category-entry ?a ?a)
          (error (car err)))))
"##,
        expect,
    );
}
