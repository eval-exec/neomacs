//! Oracle parity tests for GNU `subr.el` `merge-ordered-lists`.
//!
//! GNU uses a C3-inspired merge.  The fallback `error-function` is part of the
//! public contract: it receives the unresolved tail, must return the head of
//! one remaining list, and its returned candidate is used to break cycles.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_merge_ordered_lists_gnu_subr_examples() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (merge-ordered-lists
  '((B A) (C A) (D B) (E D C))
  (lambda (_) (error "cycle")))
 (merge-ordered-lists
  '((E D C) (B A) (C A) (D B))
  (lambda (_) (error "cycle")))
 (condition-case err
     (merge-ordered-lists
      '((E C D) (B A) (A C) (D B))
      (lambda (_) (error "cycle")))
   (error err)))
"#;

    let expect = expect_test::expect![[r#""OK ((E D B C A) (E D C B A) (error \"cycle\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_merge_ordered_lists_error_function_observes_unresolved_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (seen)
  (list
   (merge-ordered-lists
    '((A B) (B A) (C B))
    (lambda (remaining)
      (setq seen (copy-tree remaining))
      (caar remaining)))
   seen))
"#;

    let expect = expect_test::expect![[r#""OK ((C A B) ((A B) (B A) (B)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_merge_ordered_lists_rejects_invalid_error_candidate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(condition-case err
    (merge-ordered-lists
     '((A B) (B A))
     (lambda (_) 'not-a-head))
  (error err))
"#;

    let expect = expect_test::expect![[
        r#""OK (error \"Invalid candidate returned by error-function: not-a-head\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_merge_ordered_lists_uses_eql_and_avoids_mutating_outer_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((one-a (string-to-number "1"))
       (one-b (string-to-number "1"))
       (nan-a (/ 0.0 0.0))
       (nan-b (/ 0.0 0.0))
       (lists (list (list one-a nan-a 'tail)
                    nil
                    (list one-b nan-b))))
  (list
   (merge-ordered-lists lists)
   lists
   (eq (cadr lists) nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1 -0.0e+NaN tail) ((1 -0.0e+NaN tail) nil (1 -0.0e+NaN)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
