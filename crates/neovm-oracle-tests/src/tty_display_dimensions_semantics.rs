//! Oracle parity tests for GNU TTY display dimension primitives.
//!
//! GNU implements these in `src/term.c`: an initial terminal frame reports
//! 80x25 from `tty_display_dimension`, not zero dimensions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_tty_display_pixel_dimensions_match_initial_terminal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (tty-display-pixel-width)
 (tty-display-pixel-height)
 (condition-case err
     (tty-display-pixel-width "not-a-frame")
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (80 25 80)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
