//! Oracle parity tests for GNU list-tail helper edge semantics.
//!
//! GNU implements `last`, `butlast`, and `nbutlast` in `lisp/subr.el`.
//! The ordinary cases are broadly covered elsewhere; these tests focus on
//! negative/zero N, improper-list errors, and destructive return behavior.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_last_butlast_nbutlast_n_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((lst '(a b c d)))
  (list
   (last lst -2)
   (last lst 0)
   (last lst 1)
   (last lst 4)
   (last lst 5)
   (butlast lst -2)
   (butlast lst 0)
   (butlast lst 1)
   (butlast lst 4)
   (butlast lst 5)
   (let ((copy (copy-sequence lst)))
     (list (nbutlast copy -2) copy))
   (let ((copy (copy-sequence lst)))
     (list (nbutlast copy 0) copy))
   (let ((copy (copy-sequence lst)))
     (list (nbutlast copy 4) copy))
   (let ((copy (copy-sequence lst)))
     (list (nbutlast copy 5) copy))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil (d) (a b c d) (a b c d) (a b c d) (a b c d) (a b c) nil nil ((a b c d) (a b c d)) ((a b c d) (a b c d)) (nil (a b c d)) (nil (a b c d)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_list_tail_helpers_improper_list_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dotted '(a b . c)))
  (list
   (condition-case err (last dotted) (error err))
   (condition-case err (last dotted 1) (error err))
   (condition-case err (butlast dotted) (error err))
   (condition-case err (butlast dotted 0) (error err))
   (condition-case err (nbutlast (copy-sequence dotted)) (error err))
   (condition-case err (nbutlast (copy-sequence dotted) 0) (error err))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((b . c) (b . c) (wrong-type-argument listp c) (a b . c) (wrong-type-argument listp c) (wrong-type-argument listp c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_nbutlast_destructive_identity_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((lst (list 'a 'b 'c 'd)))
  (let ((ret (nbutlast lst 2)))
    (list ret lst (eq ret lst) (cdr (cdr lst)))))
"#;

    let expect = expect_test::expect![[r#""OK ((a b) (a b) t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
