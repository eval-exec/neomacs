/// Batch 493: image-create, image-type, image-animate, image-mask, image-data.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx493_image_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (image-type-available-p 'png) (image-type-available-p 'jpeg) (image-type-available-p 'xpm))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-size (list 'image :type 'png :data "") t)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_meta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-metadata (list 'image :type 'png :data ""))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_type_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-type-header-regexps)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_type_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-type-from-file-header "/nonexistent")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_imagemagick_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (imagemagick-types)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_file_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-size "nonexistent" t)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_create_image() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (image :type png :file \"\" :scale default :data \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (create-image "" 'png nil :data "")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_extension() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK png""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-type-from-file-name "test.png")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_crop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-crop nil nil)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_cut() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-cut nil nil nil)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-rotate nil 90)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_flip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-flip nil nil)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_threshold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-threshold nil 128)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx493_image_gaussian() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (image-gaussian-blur nil 5)
  (error (car e)))
"##,
        expect,
    );
}
