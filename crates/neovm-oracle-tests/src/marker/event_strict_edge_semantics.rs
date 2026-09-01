//! Oracle parity for marker + event operations.
//! GNU src/marker.c, src/keyboard.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_make_marker_creates_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(markerp (make-marker))", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_set_marker_returns_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mk*")) (erase-buffer) (insert "0123456789") (markerp (set-marker (make-marker) 5)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_copy_marker_preserves_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mk2*")) (erase-buffer) (insert "0123456789") (let* ((m (set-marker (make-marker) 3)) (c (copy-marker m))) (eq (marker-position m) (marker-position c))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_marker_insertion_type_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(marker-insertion-type (make-marker))",
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_set_marker_nil_detaches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((m (make-marker))) (set-marker m 10 (current-buffer)) (set-marker m nil) (marker-position m)))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_single_key_description_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(single-key-description ?a)"#, expect);
    assert_ok_eq("\"a\"", &o, &n);
}
