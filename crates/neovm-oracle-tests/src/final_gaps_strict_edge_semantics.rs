//! Oracle parity for final uncovered subrs: buffer-swap-text,
//! copy-keymap, lsh, key-description vector.
//! GNU src/buffer.c, src/keymap.c, src/data.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_copy_keymap_is_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(keymapp (copy-keymap (make-sparse-keymap)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_copy_keymap_returns_copy_not_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let* ((orig (make-sparse-keymap))
                  (cpy (copy-keymap orig)))
             (not (eq orig cpy)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_lsh_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(lsh 1 3)", expect);
    assert_ok_eq("8", &o, &n);
}

#[test]
fn oracle_lsh_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(lsh 16 -2)", expect);
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_key_description_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(stringp (key-description [?\C-a]))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_lsh_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(lsh 42 0)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_nreverse_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [3 2 1]""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nreverse [1 2 3])"#, expect);
    assert_ok_eq("[3 2 1]", &o, &n);
}
