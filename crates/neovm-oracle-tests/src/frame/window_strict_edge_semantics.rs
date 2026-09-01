//! Oracle parity for frame/window operations.
//! GNU src/frame.c, src/window.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_framep_on_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(framep (selected-frame))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_framep_on_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(framep nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_windowp_on_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(windowp (selected-window))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_windowp_on_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(windowp nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_selected_frame_is_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(framep (selected-frame))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_selected_window_is_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(windowp (selected-window))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_frame_root_frame_returns_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(framep (frame-root-frame))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_frame_id_returns_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(integerp (frame-id (selected-frame)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
