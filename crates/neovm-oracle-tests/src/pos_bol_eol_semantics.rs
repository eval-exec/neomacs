//! Oracle parity tests for `pos-bol` and `pos-eol`.
//!
//! GNU implements both in `src/editfns.c` — return position of beginning/end
//! of line at a given position.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_pos_bol_at_buffer_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-pos-bol*"))
  (erase-buffer)
  (pos-bol))"#,
        expect,
    );
    assert_ok_eq("1", &oracle, &neovm);
}

#[test]
fn oracle_pos_eol_at_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-pos-eol*"))
  (erase-buffer)
  (pos-eol))"#,
        expect,
    );
    assert_ok_eq("1", &oracle, &neovm);
}

#[test]
fn oracle_pos_bol_wrong_type_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp a)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(pos-bol 'a)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
