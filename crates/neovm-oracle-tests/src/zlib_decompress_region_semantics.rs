//! Oracle parity tests for GNU `zlib-decompress-region` validation semantics.
//!
//! GNU implements this in `src/decompress.c`: it calls `validate_region`
//! before checking whether the current buffer is unibyte.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_zlib_decompress_region_validates_region_before_unibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (zlib-available-p)
 (condition-case err
     (zlib-decompress-region "x" "y")
   (error (cons (car err) (cdr err))))
 (condition-case err
     (with-temp-buffer
       (zlib-decompress-region 1 1))
   (error (cons (car err) (cdr err))))
 (condition-case err
     (with-temp-buffer
       (set-buffer-multibyte nil)
       (insert "bad")
       (zlib-decompress-region 1 4))
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t (wrong-type-argument integer-or-marker-p \"x\") (error \"This function can be called only in unibyte buffers\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
