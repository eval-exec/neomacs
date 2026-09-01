//! Oracle parity tests for GNU OpenType font primitive availability.
//!
//! GNU implements `font-drive-otf` and `font-otf-alternates` in `src/font.c`,
//! but the studied registration block keeps both `defsubr` calls under `#if 0`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_font_otf_helpers_follow_gnu_primitive_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (fboundp 'font-drive-otf)
 (condition-case err
     (font-drive-otf nil nil nil nil nil nil)
   (error (cons (car err) (cdr err))))
 (fboundp 'font-otf-alternates)
 (condition-case err
     (font-otf-alternates nil nil nil)
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (void-function font-drive-otf) nil (void-function font-otf-alternates))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
