//! Oracle parity for propertize, functionp, narrow/widen, indirect-function edge cases.
//! GNU src/fns.c, src/data.c, src/editfns.c, src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- propertize ---

#[test]
fn oracle_propertize_returns_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(stringp (propertize "hello" 'face 'bold))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_propertize_preserves_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal (propertize "hello" 'a 1 'b 2) "hello")"#,
        expect,
    );
    // equal by default ignores text properties
    assert_ok_eq("t", &o, &n);
}

// --- functionp ---

#[test]
fn oracle_functionp_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(functionp 'car)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_functionp_non_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(functionp 42)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- narrow-to-region / widen ---

#[test]
fn oracle_narrow_to_region_changes_point_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 5)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*nrw*")) (erase-buffer) (insert "abcdef") (narrow-to-region 2 5) (list (point-min) (point-max)))"#,
        expect,
    );
    assert_ok_eq("(2 5)", &o, &n);
}

#[test]
fn oracle_widen_restores_full_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*nrw2*")) (erase-buffer) (insert "abcdef") (narrow-to-region 2 5) (widen) (point-max))"#,
        expect,
    );
    assert_ok_eq("7", &o, &n);
}

// --- indirect-function ---

#[test]
fn oracle_indirect_function_resolves_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defalias 'my-indirect-fn (symbol-function '1+)) (functionp (indirect-function 'my-indirect-fn)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- macrop ---

#[test]
fn oracle_macrop_subr_is_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(macrop 'car)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- define-key ---

#[test]
fn oracle_define_key_returns_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK forward-char""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq km (make-sparse-keymap)) (define-key km "a" 'forward-char) (lookup-key km "a"))"#,
        expect,
    );
    assert_ok_eq("forward-char", &o, &n);
}

// --- lookup-key ---

#[test]
fn oracle_lookup_key_missing_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq km (make-sparse-keymap)) (lookup-key km "x"))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}
