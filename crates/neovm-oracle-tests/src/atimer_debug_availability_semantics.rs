//! Oracle parity tests for GNU timer debug primitive availability.
//!
//! GNU implements `debug-timer-check` in `src/atimer.c`, but both the DEFUN
//! body and `defsubr` registration are guarded by `ENABLE_CHECKING`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_debug_timer_check_follows_gnu_checking_build_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (fboundp 'debug-timer-check)
 (condition-case err
     (debug-timer-check)
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![r#""OK (nil (void-function debug-timer-check))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
