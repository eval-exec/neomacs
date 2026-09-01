//! Oracle parity for format, predicates, and identity deep edge cases.
//! GNU src/editfns.c, src/data.c, src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- format ---

#[test]
fn oracle_format_string_and_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"x=42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%s=%d" "x" 42)"#, expect);
    assert_ok_eq("\"x=42\"", &o, &n);
}

#[test]
fn oracle_format_zero_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"00007\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%05d" 7)"#, expect);
    assert_ok_eq("\"00007\"", &o, &n);
}

#[test]
fn oracle_format_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"3.14\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%.2f" 3.14159)"#, expect);
    assert_ok_eq("\"3.14\"", &o, &n);
}

// --- null ---

#[test]
fn oracle_null_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(null nil)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_null_empty_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(null '())"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_null_non_nil_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(null t)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- atom ---

#[test]
fn oracle_atom_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(atom 'x)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_atom_cons_is_not_atom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(atom '(a))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_atom_vector_is_atom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(atom [1 2])"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- listp ---

#[test]
fn oracle_listp_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(listp '(a b))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_listp_nil_is_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(listp nil)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_listp_non_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(listp 42)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- consp ---

#[test]
fn oracle_consp_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(consp '(a . b))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_consp_nil_is_not_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(consp nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- nlistp ---

#[test]
fn oracle_nlistp_not_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nlistp 42)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_nlistp_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nlistp '(a))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- identity ---

#[test]
fn oracle_identity_returns_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(eq (identity 'foo) 'foo)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_identity_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(identity nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_identity_ignore_always_strict_runtime_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((cell (list 'x))
      (vec (vector 'y))
      (str (copy-sequence "abc")))
  (list
   ;; GNU src/fns.c `identity' returns the exact same Lisp object.
   (eq (identity cell) cell)
   (eq (identity vec) vec)
   (eq (identity str) str)
   ;; GNU lisp/subr.el defines `ignore' and `always' as Lisp varargs.
   (ignore)
   (ignore 1 nil 'x cell vec str)
   (always)
   (always nil 1 'x cell vec str)
   (condition-case err
       (identity)
     (error (list (car err) (cdr err))))
   (condition-case err
       (identity 1 2)
     (error (list (car err) (cdr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil nil t t (wrong-number-of-arguments (identity 0)) (wrong-number-of-arguments (identity 2)))""#
    ]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "(t t t nil nil t t (wrong-number-of-arguments (identity 0)) (wrong-number-of-arguments (identity 2)))",
        &o,
        &n,
    );
}
