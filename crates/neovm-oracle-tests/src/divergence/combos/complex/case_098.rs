//! Complex combo batch 98 — image / svg / png / fringe bitmap / cursor
//! API availability, `image-size` queries, and `image-type-available-p`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx98_image_type_availability_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (image-type-available-p 'png)
          (image-type-available-p 'jpeg)
          (image-type-available-p 'svg)
          (image-type-available-p 'xpm)
          (image-type-available-p 'xbm)
          (image-type-available-p 'gif)
          (image-type-available-p 'tiff))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_image_metadata_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t image png \"non-existent.png\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil)))
      (list (imagep img)
            (car img)
            (plist-get (cdr img) :type)
            (plist-get (cdr img) :file)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_image_size_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.xpm" 'xpm nil :ascent 'center)))
      (list (imagep img)
            (plist-get (cdr img) :type)
            (plist-get (cdr img) :ascent)
            (image-size img)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_fringe_bitmap_availability_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((left-arrow 3) (right-arrow 4) (up-arrow 5) (down-arrow 6) (filled-square 20) (hollow-square 21) (left-triangle 10) (right-triangle 11) (question-mark 1) (exclamation-mark 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (b)
              (list b (fringe-bitmap-p b)))
            '(left-arrow right-arrow up-arrow down-arrow
              filled-square hollow-square left-triangle right-triangle
              question-mark exclamation-mark))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_fringe_bitmaps_at_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 5 6 'before-string
                     (propertize " " 'display
                                 `((left-fringe right-arrow))))
  (put-text-property 7 8 'before-string
                     (propertize " " 'display
                                 `((right-fringe left-arrow help-echo "tip"))))
  (list (get-text-property 5 'display)
        (get-text-property 7 'display)))
"##,
        expect,
    );
}

#[test]
fn div_cx98_svg_creation_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t image svg)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'svg)
      (let ((image (svg-create 100 100)))
        (svg-rectangle image 10 10 80 80 :fill "red")
        (let ((xml (svg-image image)))
          (list (imagep xml)
                  (car xml)
                  (plist-get (cdr xml) :type)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_image_scale_factor_and_multipliers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil :scale 2)))
      (list (imagep img)
            (plist-get (cdr img) :scale)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_image_format_available_p_per_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'image-transforms-p)
          (fboundp 'image-supported-file-p)
          (fboundp 'image-flush)
          (fboundp 'image-mask-p)
          (fboundp 'image-animate))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_put_text_property_with_image_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((image :type png :file \"non-existent.png\" :scale default) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before PIC after")
  (put-text-property 8 11 'display
                     (create-image "non-existent.png" 'png nil))
  (list (get-text-property 8 'display)
        (get-text-property 1 'display)
        (get-text-property 12 'display)))
"##,
        expect,
    );
}

#[test]
fn div_cx98_image_animated_p_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t :err)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.gif" 'gif nil)))
      (list (imagep img)
            (condition-case err (image-animated-p img) (error :err))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx98_image_with_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Image test buffer content")
        (put-text-property 1 5 'face 'bold)
        (put-text-property 7 11 'display img)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 3 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1)
                             (text-properties-at 6)
                             (get-text-property 7 'display))))
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
