//! Oracle parity tests for GNU window tree primitive argument semantics.
//!
//! GNU implements these in `src/window.c`; invalid window arguments are checked
//! before any tree mutation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_window_tree_primitives_validate_window_arguments_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (combine-windows "not-a-window" nil)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (uncombine-window "not-a-window")
   (error (cons (car err) (cdr err))))
 (condition-case err
     (window-discard-buffer-from-window (current-buffer) "not-a-window")
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument window-valid-p \"not-a-window\") (wrong-type-argument window-valid-p \"not-a-window\") (error \"Not a live window\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
