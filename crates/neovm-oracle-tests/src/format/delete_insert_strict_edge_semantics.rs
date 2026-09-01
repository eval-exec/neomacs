//! Oracle parity for format edges + delete/insert strict edges.
//! GNU src/editfns.c, src/cmds.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_format_c_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%c" 65)"#, expect);
    assert_ok_eq("\"A\"", &o, &n);
}

#[test]
fn oracle_format_x_hex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ff\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%x" 255)"#, expect);
    assert_ok_eq("\"ff\"", &o, &n);
}

#[test]
fn oracle_format_percent_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"100%\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "100%%")"#, expect);
    assert_ok_eq("\"100%\"", &o, &n);
}

#[test]
fn oracle_format_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"x=42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%s=%d" "x" 42)"#, expect);
    assert_ok_eq("\"x=42\"", &o, &n);
}

#[test]
fn oracle_insert_integer_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ii*")) (erase-buffer) (insert 65) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"A\"", &o, &n);
}

#[test]
fn oracle_insert_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"HI!\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*im*")) (erase-buffer) (insert 72 73 ?!) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"HI!\"", &o, &n);
}
