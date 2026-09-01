//! Oracle parity tests for GNU image feature-gated primitives.
//!
//! GNU registers `imagemagick-types` in `src/image.c` only under
//! `HAVE_IMAGEMAGICK`, and `lookup-image` only under `GLYPH_DEBUG`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_image_primitives_follow_gnu_build_feature_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (fboundp 'imagemagick-types)
 (condition-case err
     (imagemagick-types)
   (error (cons (car err) (cdr err))))
 (fboundp 'lookup-image)
 (condition-case err
     (lookup-image nil)
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (void-function imagemagick-types) nil (void-function lookup-image))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
