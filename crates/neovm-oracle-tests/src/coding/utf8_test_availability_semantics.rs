//! Oracle parity tests for GNU UTF-8 converter test primitive availability.
//!
//! GNU implements these in `src/coding.c`, but registers them only when
//! `ENABLE_UTF_8_CONVERTER_TEST` is defined.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_utf8_converter_test_primitives_follow_gnu_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (fboundp 'internal-encode-string-utf-8)
 (condition-case err
     (internal-encode-string-utf-8 "a" nil nil nil nil nil 1)
   (error (cons (car err) (cdr err))))
 (fboundp 'internal-decode-string-utf-8)
 (condition-case err
     (internal-decode-string-utf-8 "a" nil nil nil nil nil 1)
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (void-function internal-encode-string-utf-8) nil (void-function internal-decode-string-utf-8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
