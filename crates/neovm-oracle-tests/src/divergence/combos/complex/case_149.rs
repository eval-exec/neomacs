//! Complex combo batch 149 — `image-dired` / `image-mode` / `image-wrap`
//! / `image-animated` / `imagemagick` availability and rendering hooks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx149_image_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'image-mode)
      (list (fboundp 'image-mode)
            (fboundp 'image-toggle-animation)
            (boundp 'image-animate-loop)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_dired_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'image-dired)
      (list (fboundp 'image-dired)
            (boundp 'image-dired-dir)
            (boundp 'image-dired-thumbnail-storage)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_type_available_p_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((png t) (jpeg t) (gif t) (tiff t) (xpm t) (xbm t) (svg t) (imagemagick nil) (webp t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (t) (list t (image-type-available-p t)))
            '(png jpeg gif tiff xpm xbm svg imagemagick webp))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_create_with_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t image png \"test.png\" 90)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "test.png" 'png nil :ascent 90)))
      (list (imagep img)
            (car img)
            (plist-get (cdr img) :type)
            (plist-get (cdr img) :file)
            (plist-get (cdr img) :ascent)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_size_query_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t :err :err)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "fake.xpm" 'xpm nil)))
      (list (imagep img)
            (condition-case err
                (image-size img)
              (error :err))
            (condition-case err
                (image-size img t)
              (error :err))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_imagemagick_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'imagemagick-types)
          (fboundp 'imagemagick-register-type)
          (boundp 'imagemagick-enabled-types))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_animate_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t :err)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "fake.gif" 'gif nil)))
      (list (imagep img)
            (condition-case err (image-animated-p img) (error :err))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_wrap_display_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((image :type png :file \"fake.png\" :scale default) nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before image after")
  (let ((img (create-image "fake.png" 'png nil)))
    (put-text-property 7 12 'display img)
    (list (get-text-property 7 'display)
          (get-text-property 1 'display)
          (imagep (get-text-property 7 'display)))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_transform_avail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'image-transforms-p)
          (fboundp 'image-rotate)
          (fboundp 'image-flip)
          (fboundp 'image-increase-size))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_refresh() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'image-flush)
          (fboundp 'image-refresh)
          (boundp 'image-cache-eviction-delay))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_svg_create_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t image svg)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'svg)
      (let ((svg (svg-create 100 100)))
        (svg-circle svg 50 50 30 :fill "red")
        (svg-rectangle svg 10 10 80 40 :stroke "blue")
        (let ((img (svg-image svg)))
          (list (imagep img)
                (car img)
                (plist-get (cdr img) :type)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx149_image_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "fake.png" 'png nil)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Image mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (put-text-property 7 12 'display img)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (imagep img)
                             (get-text-property 7 'display)
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
