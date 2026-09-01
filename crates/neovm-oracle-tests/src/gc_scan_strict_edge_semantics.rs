//! Oracle parity for garbage-collect, scan-lists, scan-sexps.
//! GNU src/alloc.c, src/syntax.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- garbage-collect ---

#[test]
fn oracle_garbage_collect_returns_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(consp (garbage-collect))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_garbage_collect_has_conses_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(not (null (assq 'conses (garbage-collect))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- scan-lists ---

#[test]
fn oracle_scan_lists_forward_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*sl1*")) (erase-buffer) (insert "(a (b) c)") (goto-char 1) (scan-lists 1 1 0))"#,
        expect,
    );
    assert_ok_eq("10", &o, &n);
}

#[test]
fn oracle_scan_lists_backward_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*sl2*")) (erase-buffer) (insert "(a b)") (goto-char 6) (scan-lists 6 -1 0))"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_scan_lists_no_paren_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*sl3*")) (erase-buffer) (insert "hello") (goto-char 1) (condition-case nil (scan-lists 1 1 0) (error nil)))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

// --- scan-sexps ---

#[test]
fn oracle_scan_sexps_forward_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*ss1*")) (erase-buffer) (insert "(a b c)") (goto-char 1) (scan-sexps 1 1))"#,
        expect,
    );
    assert_ok_eq("8", &o, &n);
}

#[test]
fn oracle_scan_sexps_backward_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*ss2*")) (erase-buffer) (insert "(a b)") (goto-char 6) (scan-sexps 6 -1))"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);
}
