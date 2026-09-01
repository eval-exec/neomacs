//! Oracle parity tests for GNU terminal frame hit testing.
//!
//! GNU `tty-frame-at` checks `FIXNUMP` for both coordinates and returns nil
//! for non-fixnum values instead of signaling a type error.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_tty_frame_at_non_fixnum_coordinates_return_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (tty-frame-at nil nil)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (tty-frame-at "0" 0)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (tty-frame-at 0 "0")
   (error (cons (car err) (cdr err))))
 (condition-case err
     (tty-frame-at 1.0 0)
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
