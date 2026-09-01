//! Oracle parity tests for `read-char`.
//!
//! GNU implements `read-char` in `src/keyboard.c` — reads one character
//! from the minibuffer or input stream.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, eval_oracle_and_neovm};

#[test]
fn oracle_read_char_wrong_number_of_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments read-char 4)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(read-char nil nil nil nil)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_read_char_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp (bad))""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(read-char '(bad))", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
