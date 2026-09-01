//! Oracle parity for fset, set-marker, defalias, copy-alist interaction.
//! GNU src/data.c, src/marker.c, src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- fset / symbol-function / fboundp interaction ---

#[test]
fn oracle_fset_and_fboundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (fset 'nvm--fs-test (symbol-function '1+)) (fboundp 'nvm--fs-test))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_fset_overwrites() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 41""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defalias 'nvm--fs-ow (symbol-function '1+)) (defalias 'nvm--fs-ow (symbol-function '1-)) (funcall 'nvm--fs-ow 42))"#,
        expect,
    );
    assert_ok_eq("41", &o, &n);
}

// --- defalias ---

#[test]
fn oracle_defalias_creates_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defalias 'nvm--da-test (symbol-function '1+)) (fboundp 'nvm--da-test))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- set-marker ---

#[test]
fn oracle_set_marker_returns_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*sm*")) (erase-buffer) (insert "abcdef") (markerp (set-marker (make-marker) 3)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- copy-alist ---

#[test]
fn oracle_copy_alist_is_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((99) 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq orig '((a . 1) (b . 2))) (setq cp (copy-alist orig)) (setcdr (assq 'a orig) '(99)) (list (cdr (assq 'a orig)) (cdr (assq 'a cp))))"#,
        expect,
    );
    assert_ok_eq("((99) 1)", &o, &n);
}

// --- mapcar ---

#[test]
fn oracle_mapcar_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(mapcar 'identity '(1 2 3))"#, expect);
    assert_ok_eq("(1 2 3)", &o, &n);
}

// --- copy-sequence with cons ---

#[test]
fn oracle_copy_sequence_cons_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq orig (cons 1 (cons 2 nil))) (setq cp (copy-sequence orig)) (setcar orig 99) (car cp))"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);
}

// --- eq vs equal ---

#[test]
fn oracle_eq_identical_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eq 'foo 'foo)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_equal_distinct_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal (make-string 3 ?a) (make-string 3 ?a))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- eql vs eq for numbers ---

#[test]
fn oracle_eql_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eql 1 1)"#, expect);
    assert_ok_eq("t", &o, &n);
}
