//! Divergence tests: image, pixmaps, SVG, image-cache deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_image_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'image-type-available-p)
  (fboundp 'image-type-from-file-name)
  (fboundp 'image-type-from-file-header)
  (listp image-types))"#,
        expect,
    );
}

#[test]
fn divergence_image_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'create-image)
  (fboundp 'find-image)
  (fboundp 'image-size)
  (fboundp 'image-mask-p))"#,
        expect,
    );
}

#[test]
fn divergence_image_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'image-property)
  (fboundp 'setf)
  (fboundp 'image-plist)
  (fboundp 'image-multi-frame-p))"#,
        expect,
    );
}

#[test]
fn divergence_image_transforms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'image-transforms-p)
  (fboundp 'image-compute-rotation)
  (fboundp 'image-compute-scaling)
  (featurep 'image))"#,
        expect,
    );
}

#[test]
fn divergence_image_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'image-cache-eviction-delay)
  (fboundp 'clear-image-cache)
  (fboundp 'image-cache-size)
  (boundp 'image-cache-size))"#,
        expect,
    );
}

#[test]
fn divergence_svg_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'svg-create)
  (fboundp 'svg-image)
  (fboundp 'svg-rectangle)
  (fboundp 'svg-circle)
  (fboundp 'svg-line)
  (featurep 'svg))"#,
        expect,
    );
}

#[test]
fn divergence_svg_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'svg-text)
  (fboundp 'svg-node)
  (fboundp 'svg-gradient)
  (fboundp 'svg-embed))"#,
        expect,
    );
}

#[test]
fn divergence_image_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'image-mode)
  (fboundp 'image-toggle-display)
  (fboundp 'image-forward-hscroll)
  (fboundp 'image-backward-hscroll)
  (fboundp 'image-scroll-up)
  (featurep 'image-mode))"#,
        expect,
    );
}

#[test]
fn divergence_image_dired() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'image-dired-display-thumb)
  (fboundp 'image-dired-dired-display-image)
  (featurep 'image-dired))"#,
        expect,
    );
}

#[test]
fn divergence_image_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'imagemagick-types)
  (fboundp 'imagemagick-filter-types)
  (fboundp 'imagemagick-registered-p)
  (featurep 'imagemagick))"#,
        expect,
    );
}
