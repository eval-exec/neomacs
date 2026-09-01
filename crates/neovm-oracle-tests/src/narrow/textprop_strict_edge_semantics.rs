//! Oracle parity for narrow/widen + text property operations.
//!
//! GNU src/editfns.c, src/textprop.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_narrow_and_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"234\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*nw*")) (erase-buffer) (insert "0123456789") (narrow-to-region 3 6) (prog1 (buffer-string) (widen)))"#,
        expect,
    );
    assert_ok_eq("\"234\"", &o, &n);
}

#[test]
fn oracle_widen_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"0123456789\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*nw2*")) (erase-buffer) (insert "0123456789") (narrow-to-region 4 8) (widen) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"0123456789\"", &o, &n);
}

#[test]
fn oracle_narrowed_point_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 9)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*nw3*")) (erase-buffer) (insert "0123456789") (narrow-to-region 5 9) (prog1 (list (point-min) (point-max)) (widen)))"#,
        expect,
    );
    assert_ok_eq("(5 9)", &o, &n);
}

#[test]
fn oracle_remove_text_properties_returns_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*rtp*")) (erase-buffer) (insert "hello") (put-text-property 1 3 'face 'bold) (remove-text-properties 1 3 '(face nil)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_next_property_change_after_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*npc*")) (erase-buffer) (insert "abcdef") (put-text-property 2 4 'x 'y) (next-property-change 1))"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_get_text_property_on_propertized() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK y""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*gtp*")) (erase-buffer) (insert "abcdef") (put-text-property 1 4 'x 'y) (get-text-property 2 'x))"#,
        expect,
    );
    assert_ok_eq("y", &o, &n);
}

#[test]
fn oracle_put_text_property_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mtp*")) (erase-buffer) (insert "abcdef") (put-text-property 1 3 'a 1) (put-text-property 1 3 'b 2) (list (get-text-property 2 'a) (get-text-property 2 'b)))"#,
        expect,
    );
    assert_ok_eq("(1 2)", &o, &n);
}

#[test]
fn oracle_text_properties_at_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*tpea*")) (erase-buffer) (insert "hello") (text-properties-at 2))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}
