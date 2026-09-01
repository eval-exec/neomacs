//! Oracle parity tests for `equal` on propertized strings.
//!
//! GNU src/fns.c `internal_equal`: for strings, compares only the
//! character content via `!NILP(Fstring_equal(...))`, ignoring
//! text properties entirely.  Two propertized strings with different
//! faces but identical text are `equal`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_equal_propertized_same_content_different_face_is_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal
     (propertize "hello" 'face 'bold)
     (propertize "hello" 'face 'italic))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_equal_propertized_different_content_is_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal
     (propertize "abc" 'face 'bold)
     (propertize "xyz" 'face 'italic))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_equal_unpropertized_vs_propertized_same_content_is_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal "hello" (propertize "hello" 'face 'bold))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}
