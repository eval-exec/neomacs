//! Oracle parity tests for `char-after`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_char_after_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 97""#]];
    let (oracle_at_point, neovm_at_point) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc") (goto-char 1) (char-after))"#,
        expect,
    );
    assert_ok_eq("97", &oracle_at_point, &neovm_at_point);

    let expect = expect_test::expect![[r#""OK 98""#]];
    let (oracle_pos, neovm_pos) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc") (char-after 2))"#,
        expect,
    );
    assert_ok_eq("98", &oracle_pos, &neovm_pos);
}

#[test]
fn oracle_prop_char_after_nil_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_nonpositive, neovm_nonpositive) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc") (char-after 0))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle_nonpositive, &neovm_nonpositive);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_end, neovm_end) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc") (char-after 4))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle_end, &neovm_end);
}

#[test]
fn oracle_prop_char_after_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p \"x\")""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(char-after "x")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_char_after_bignum_position_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs editfns.c:Fchar_after uses buffer.c:fix_position for explicit
    // integer POS, so huge bignums saturate and then fall outside BEGV..ZV.
    let form = r#"(with-temp-buffer
  (insert "abc")
  (char-after 1000000000000000000000000000000))"#;
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_char_after_position_lookup(
        a in b'a'..=b'z',
        b in b'a'..=b'z',
        first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let left = a as char;
        let right = b as char;
        let pos = if first { 1 } else { 2 };
        let expected_code = if first { a } else { b };
        let form = format!(
            r#"(progn (erase-buffer) (insert "{}{}") (char-after {}))"#,
            left, right, pos
        );
        let expected = expected_code.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
