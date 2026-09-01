//! Oracle parity tests for `buffer-substring`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_buffer_substring_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"bcd\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abcdef") (buffer-substring 2 5))"#,
        expect,
    );
    assert_ok_eq("\"bcd\"", &oracle, &neovm);
}

#[test]
fn oracle_prop_buffer_substring_error_kinds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p \"x\")""#]];
    let (type_oracle, type_neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(buffer-substring "x" 1)"#, expect);
    assert_err_kind(&type_oracle, &type_neovm, "wrong-type-argument");

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 0 1)""#]];
    let (range_oracle, range_neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc") (buffer-substring 0 1))"#,
        expect,
    );
    assert_err_kind(&range_oracle, &range_neovm, "args-out-of-range");
}

#[test]
fn oracle_prop_buffer_substring_bignum_start_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs buffer.c:validate_region uses fix_position for START/END,
    // so a huge bignum START is saturated before args-out-of-range is signaled.
    let form = r#"(progn
  (erase-buffer)
  (insert "abc")
  (buffer-substring 1000000000000000000000000000000 1))"#;
    let expect = expect_test::expect![
        r#""ERR (args-out-of-range #<buffer *scratch*> 1000000000000000000000000000000 1)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_buffer_substring_valid_range_parity(
        start in 1usize..8usize,
        end in 1usize..8usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));
        prop_assume!(start <= end);

        let form = format!(
            r#"(progn (erase-buffer) (insert "abcdef") (buffer-substring {} {}))"#,
            start, end
        );
        assert_oracle_parity(&form);
    }
}
