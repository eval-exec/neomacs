//! Complex combo batch 177 — `image-type-available-p` matrix expanded,
//! `image-size` for display variants, image-cache, image-transforms.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx177_image_type_available_p_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((png t) (jpeg t) (jpg nil) (gif t) (tiff t) (xpm t) (xbm t) (svg t) (imagemagick nil) (webp t) (pbm t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (t) (list t (image-type-available-p t)))
            '(png jpeg jpg gif tiff xpm xbm svg imagemagick webp pbm))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx177_image_create_with_image_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t image xpm \"test.xpm\" center)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "test.xpm" 'xpm nil :ascent 'center)))
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
fn div_cx177_image_cache_eviction_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'image-cache-eviction-delay)
          (integerp image-cache-eviction-delay)
          (fboundp 'clear-image-cache))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx177_image_transforms_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'image-transforms-p)
          (when (fboundp 'image-transforms-p) (image-transforms-p)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx177_image_size_query_with_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t :err :err)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil)))
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
fn div_cx177_image_mask_p_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t heuristic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil :mask 'heuristic)))
      (list (imagep img)
            (plist-get (cdr img) :mask)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx177_image_animate_predicate() {
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
fn div_cx177_image_flush_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil)))
      (list (fboundp 'image-flush)
            (imagep img)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx177_create_image_with_data_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t png)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((fake-data (unibyte-string #x89 #x50 #x4e #x47 #x0d #x0a #x1a #x0a)))
      (let ((img (create-image fake-data 'png t)))
        (list (imagep img)
              (plist-get (cdr img) :type))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx177_image_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((img (create-image "non-existent.png" 'png nil)))
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
                             (imagep (get-text-property 7 'display))
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
