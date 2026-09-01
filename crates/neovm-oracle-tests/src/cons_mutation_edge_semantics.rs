//! Oracle parity tests for cons mutation operations.
//!
//! GNU src/fns.c + src/data.c: `setcar`, `setcdr`, `nconc`, `delq`,
//! and `nreverse` mutate cons cells in-place.  These mutations are
//! visible to all references — eq-based identity is preserved.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// setcar / setcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_setcar_mutates_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 99 t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((cell (cons 1 2)))
    (let ((ref cell))
      (setcar cell 99)
      (list (car cell) (car ref) (eq cell ref)))))"#,
        expect,
    );
    assert_ok_eq("(99 99 t)", &oracle, &neovm);
}

#[test]
fn oracle_setcdr_mutates_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 99 t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((cell (cons 1 2)))
    (let ((ref cell))
      (setcdr cell 99)
      (list (cdr cell) (cdr ref) (eq cell ref)))))"#,
        expect,
    );
    assert_ok_eq("(99 99 t)", &oracle, &neovm);
}

#[test]
fn oracle_setcar_returns_new_car_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK z""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(setcar (cons 'a 'b) 'z)"#, expect);
    assert_ok_eq("z", &oracle, &neovm);
}

#[test]
fn oracle_setcar_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument consp nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(setcar nil 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

// ---------------------------------------------------------------------------
// nconc
// ---------------------------------------------------------------------------

#[test]
fn oracle_nconc_concatenates_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(nconc (list 1 2) (list 3 4))"#, expect);
    assert_ok_eq("(1 2 3 4)", &oracle, &neovm);
}

#[test]
fn oracle_nconc_tail_shares_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((tail (list 3 4)))
    (let ((result (nconc (list 1 2) tail)))
      (eq (nthcdr 2 result) tail))))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_nconc_nil_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(nconc nil (list 1 2) nil (list 3 4))"#,
        expect,
    );
    assert_ok_eq("(1 2 3 4)", &oracle, &neovm);
}

#[test]
fn oracle_nconc_no_args_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(nconc)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

// ---------------------------------------------------------------------------
// delq
// ---------------------------------------------------------------------------

#[test]
fn oracle_delq_removes_by_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a c d)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(delq 'b '(a b c b d))"#, expect);
    assert_ok_eq("(a c d)", &oracle, &neovm);
}

#[test]
fn oracle_delq_returns_list_when_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(delq 'x '(a b c))"#, expect);
    assert_ok_eq("(a b c)", &oracle, &neovm);
}

// ---------------------------------------------------------------------------
// nreverse
// ---------------------------------------------------------------------------

#[test]
fn oracle_nreverse_reverses_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((lst (list 1 2 3 4)))
    (let ((ref lst))
      (nreverse lst)
      (eq ref lst))))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_nreverse_singleton() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(nreverse '(a))"#, expect);
    assert_ok_eq("(a)", &oracle, &neovm);
}
