//! Oracle parity tests for GNU `value<` boundary ordering.
//!
//! GNU implements `value<` in `src/fns.c:value_cmp`.  Lists are compared
//! lexicographically, and `nil` has explicit list-order semantics: `nil`
//! precedes any cons, while any cons follows `nil`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_value_lt_orders_nil_against_lists_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/fns.c:value_cmp has explicit cross-type branches for
    // NIL-vs-cons and cons-vs-NIL.  These are not type mismatches.
    let form = r#"
(list
 (condition-case err
     (value< nil '(a))
   (error (list (car err) (cdr err))))
 (condition-case err
     (value< '(a) nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (value< nil nil)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
